//! Tests de F9-0: esquema, reclamos atomicos, caducidad, reconstruccion y carga.

use super::*;
use serde_json::json;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

fn store() -> (tempfile::TempDir, WorkspaceStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = WorkspaceStore::open(dir.path().join("workspace.db")).expect("abrir workspace.db");
    (dir, store)
}

#[test]
fn events_are_append_only_by_trigger() {
    let (_dir, store) = store();
    store
        .append_event(1, "a0", "t1", "created", &json!({"x": 1}))
        .unwrap();
    let conn = store.conn().unwrap();
    let update = conn.execute("UPDATE events SET kind = 'failed' WHERE seq = 1", []);
    assert!(update.is_err(), "UPDATE sobre events debe abortar");
    let delete = conn.execute("DELETE FROM events WHERE seq = 1", []);
    assert!(delete.is_err(), "DELETE sobre events debe abortar");
    let kind: String = conn
        .query_row("SELECT kind FROM events WHERE seq = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "created");
    drop(conn);
    assert_eq!(store.event_count().unwrap(), 1);
}

#[test]
fn second_claim_overlapping_paths_is_rejected() {
    let (_dir, store) = store();
    let now = 1_000_000;
    store
        .create_task(now, &TaskSpec::new("t1", "crates/tylluan-kernel/src/**"))
        .unwrap();
    store
        .create_task(now, &TaskSpec::new("t2", "crates/tylluan-kernel/src/workspace/**"))
        .unwrap();
    store
        .create_task(now, &TaskSpec::new("t3", "docs/**"))
        .unwrap();
    store
        .create_task(now, &TaskSpec::new("t4", "crates/tylluan-kernel/src/**"))
        .unwrap();

    let first = store
        .claim("a0", "t1", "crates/tylluan-kernel/src/**", now)
        .unwrap();
    assert!(matches!(first, ClaimOutcome::Claimed(_)), "primer reclamo: {first:?}");

    // Rutas solapadas (glob contenido) => rechazo con conflicto.
    let overlap = store
        .claim("a1", "t2", "crates/tylluan-kernel/src/workspace/**", now)
        .unwrap();
    match overlap {
        ClaimOutcome::Rejected(ClaimRejection::Conflict { task_id, .. }) => {
            assert_eq!(task_id, "t1", "debe señalar el reclamo que estorba");
        }
        other => panic!("se esperaba Conflict, llego {other:?}"),
    }

    // Mismo glob exacto en otra tarea => tambien rechazo (transaccion/indice).
    let same_glob = store
        .claim("a2", "t4", "crates/tylluan-kernel/src/**", now)
        .unwrap();
    assert!(
        matches!(
            same_glob,
            ClaimOutcome::Rejected(ClaimRejection::Conflict { .. })
        ),
        "mismo glob debe chocar: {same_glob:?}"
    );

    // La misma tarea no se puede reclamar dos veces.
    let again = store
        .claim("a1", "t1", "crates/tylluan-kernel/src/**", now)
        .unwrap();
    assert!(matches!(
        again,
        ClaimOutcome::Rejected(ClaimRejection::NotQueued(TaskStatus::Claimed))
    ));

    // Rutas disjuntas si se pueden reclamar.
    let disjoint = store.claim("a3", "t3", "docs/**", now).unwrap();
    assert!(matches!(disjoint, ClaimOutcome::Claimed(_)), "docs no solapa: {disjoint:?}");

    assert_eq!(store.active_claims().unwrap().len(), 2);
}

#[test]
fn overlap_helper_is_conservative_and_symmetric() {
    assert!(globs_overlap("crates/a/**", "crates/a/src/**"));
    assert!(globs_overlap("crates/a/src/**", "crates/a/**"));
    assert!(globs_overlap("crates/*/src/**", "crates/a/src/lib.rs"));
    assert!(!globs_overlap("crates/a/**", "crates/b/**"));
    assert!(!globs_overlap("docs/**", "crates/**"));
    assert!(globs_overlap("src/*.rs", "src/main.rs"));
    assert!(globs_overlap("src/**", "src/**"));
}

#[test]
fn expired_lease_requeues_clean_task_and_sends_dirty_one_to_review() {
    let (_dir, store) = store();
    let now = 1_000_000;
    store.create_task(now, &TaskSpec::new("clean", "src/clean/**")).unwrap();
    store.create_task(now, &TaskSpec::new("dirty", "src/dirty/**")).unwrap();
    assert!(matches!(
        store.claim("a0", "clean", "src/clean/**", now).unwrap(),
        ClaimOutcome::Claimed(_)
    ));
    assert!(matches!(
        store.claim("a0", "dirty", "src/dirty/**", now).unwrap(),
        ClaimOutcome::Claimed(_)
    ));
    store
        .set_dirty_paths("dirty", "src/dirty/changed.rs")
        .unwrap();

    let expired = store.expire_leases(now + LEASE_MS + 1).unwrap();
    assert_eq!(expired, 2);
    assert_eq!(store.task_status("clean").unwrap(), Some(TaskStatus::Queued));
    assert_eq!(store.task_status("dirty").unwrap(), Some(TaskStatus::Review));
    assert!(store.active_claims().unwrap().is_empty());

    let released = store.events_of_kind("expired").unwrap();
    assert_eq!(released.len(), 2);
    assert_eq!(store.events_of_kind("requeued").unwrap().len(), 1);
    assert_eq!(store.events_of_kind("review").unwrap().len(), 1);

    // Una tarea limpia caducada se puede volver a reclamar.
    assert!(matches!(
        store.claim("a1", "clean", "src/clean/**", now + LEASE_MS + 2).unwrap(),
        ClaimOutcome::Claimed(_)
    ));
}

#[test]
fn heartbeat_extends_the_lease_until_it_is_actually_expired() {
    let (_dir, store) = store();
    let now = 1_000_000;
    store.create_task(now, &TaskSpec::new("t1", "src/**")).unwrap();
    let claim = match store.claim("a0", "t1", "src/**", now).unwrap() {
        ClaimOutcome::Claimed(c) => c,
        other => panic!("{other:?}"),
    };
    assert_eq!(claim.expires_at, now + LEASE_MS);
    // Latido a mitad del arriendo: la caducidad se mueve.
    let beat_at = now + LEASE_MS / 2;
    assert!(store.heartbeat("a0", "t1", beat_at).unwrap());
    assert_eq!(store.events_of_kind("heartbeat").unwrap().len(), 1);
    // Aun no ha caducado en `now + LEASE_MS` (se extendio).
    assert_eq!(store.expire_leases(now + LEASE_MS).unwrap(), 0);
    assert_eq!(store.expire_leases(beat_at + LEASE_MS).unwrap(), 1);
    assert!(!store.heartbeat("a0", "t1", beat_at).unwrap());
}

#[test]
fn tasks_table_is_rebuildable_from_events() {
    let (_dir, store) = store();
    let now = 1_000_000;
    for (id, glob) in [("t1", "src/a/**"), ("t2", "src/b/**"), ("t3", "src/c/**")] {
        store.create_task(now, &TaskSpec::new(id, glob)).unwrap();
    }
    assert!(matches!(store.claim("a0", "t1", "src/a/**", now).unwrap(), ClaimOutcome::Claimed(_)));
    assert!(matches!(store.claim("a0", "t2", "src/b/**", now).unwrap(), ClaimOutcome::Claimed(_)));
    assert!(store.release("a0", "t2", now + 10).unwrap());
    assert!(matches!(store.claim("a1", "t3", "src/c/**", now).unwrap(), ClaimOutcome::Claimed(_)));
    store.set_dirty_paths("t3", "src/c/x.rs").unwrap();
    store.expire_agent_leases("a1", now + LEASE_MS + 1).unwrap();

    let before = [
        store.task_status("t1").unwrap(),
        store.task_status("t2").unwrap(),
        store.task_status("t3").unwrap(),
    ];
    assert_eq!(before, [Some(TaskStatus::Claimed), Some(TaskStatus::Queued), Some(TaskStatus::Review)]);
    let rebuilt = store.rebuild_tasks_from_events().unwrap();
    assert_eq!(rebuilt, 3);
    let after = [
        store.task_status("t1").unwrap(),
        store.task_status("t2").unwrap(),
        store.task_status("t3").unwrap(),
    ];
    assert_eq!(before, after, "tasks materializada != replay de events");
}

/// Prueba de carga de F9-0: 4 agentes x 200 tareas con rutas solapadas.
/// Comprueba 0 dobles reclamos, 0 solapes activos y p95 del reclamo < 200 ms.
#[test]
fn four_agents_two_hundred_tasks_load_test() {
    let (_dir, store) = store();
    let now = 1_000_000;
    let tasks = 200usize;
    let groups = 20usize;
    for i in 0..tasks {
        let group = i % groups;
        store
            .create_task(
                now,
                &TaskSpec::new(format!("t{i:03}"), format!("crates/g{group:02}/**")),
            )
            .unwrap();
    }

    let store = Arc::new(store);
    let barrier = Arc::new(Barrier::new(4));
    let mut handles = Vec::with_capacity(4);
    for agent_idx in 0..4usize {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            let agent = format!("a{agent_idx}");
            barrier.wait();
            // El 10 % de agentes «muertos» se aproxima con 4 agentes marcando
            // a a3: solo puja por el 10 % de las tareas (i % 10 == 3) y despues
            // se le deja de latir.
            let mut durations = Vec::with_capacity(tasks);
            for i in 0..tasks {
                if agent_idx == 3 && i % 10 != 3 {
                    continue;
                }
                let task_id = format!("t{i:03}");
                let glob = format!("crates/g{:02}/**", i % groups);
                let started = Instant::now();
                let outcome = store.claim(&agent, &task_id, &glob, now).expect("claim no debe fallar");
                durations.push(started.elapsed());
                assert!(
                    matches!(outcome, ClaimOutcome::Claimed(_) | ClaimOutcome::Rejected(_)),
                    "resultado inesperado: {outcome:?}"
                );
            }
            durations
        }));
    }
    let mut all_durations = Vec::with_capacity(tasks * 4);
    for handle in handles {
        all_durations.extend(handle.join().expect("thread de carga"));
    }
    all_durations.sort_unstable();
    let idx = (all_durations.len() * 95 / 100).min(all_durations.len() - 1);
    let p95 = all_durations[idx];
    println!(
        "F9-0: {} intentos de reclamo, p95 = {} ms (max = {} ms)",
        all_durations.len(),
        p95.as_millis(),
        all_durations.last().unwrap().as_millis()
    );
    assert!(
        p95.as_millis() < 200,
        "p95 del reclamo = {} ms, limite 200 ms",
        p95.as_millis()
    );

    // 0 solapes y 0 tareas reclamadas dos veces entre las reclamaciones activas.
    let actives = store.active_claims().unwrap();
    assert!(!actives.is_empty(), "deberia haber reclamaciones activas");
    for (i, a) in actives.iter().enumerate() {
        for b in &actives[i + 1..] {
            assert_ne!(a.task_id, b.task_id, "misma tarea reclamada dos veces");
            assert!(
                !globs_overlap(&a.path_glob, &b.path_glob),
                "solape activo: {} ({}) vs {} ({})",
                a.path_glob,
                a.agent_id,
                b.path_glob,
                b.agent_id
            );
        }
    }
    let claimed_events = store.events_of_kind("claimed").unwrap().len();
    assert_eq!(
        claimed_events,
        actives.len(),
        "cada reclamo concedido debe tener exactamente un evento claimed"
    );
    let claimed_distinct: std::collections::BTreeSet<&str> =
        actives.iter().map(|c| c.task_id.as_str()).collect();
    assert_eq!(
        claimed_distinct.len(),
        actives.len(),
        "0 tareas reclamadas dos veces"
    );

    // 10 % de agentes «muertos»: con 4 agentes se marca uno entero (a3) como
    // muerto tras la carrera; sus tareas deben volver a la cola y poder
    // reclamarse otra vez. Se publica cuantas eran (fraccion real observada).
    let expired = store
        .expire_agent_leases("a3", now + LEASE_MS + 1)
        .expect("caducar agente muerto");
    println!(
        "F9-0: agente muerto a3 tenia {expired} tareas ({} % de las {} activas)",
        expired * 100 / actives.len().max(1),
        actives.len()
    );
    let expired_events = store.events_of_kind("expired").unwrap();
    assert_eq!(expired_events.len(), expired);
    for (task_id, _) in &expired_events {
        let status = store.task_status(task_id).unwrap();
        assert!(
            matches!(status, Some(TaskStatus::Queued) | Some(TaskStatus::Review)),
            "tarea caducada {task_id} quedo en {status:?}"
        );
    }
    // Reclamo de una tarea caducada por un agente vivo.
    if let Some((task_id, _)) = expired_events.first() {
        let glob = store.task_glob(task_id).unwrap().expect("glob de la tarea");
        let reclaim = store
            .claim("a0", task_id, &glob, now + LEASE_MS + 2)
            .expect("reclamo tras caducidad");
        assert!(
            matches!(reclaim, ClaimOutcome::Claimed(_)),
            "una tarea caducada debe poder reclamarse: {reclaim:?}"
        );
    }
}
