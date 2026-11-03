//! F9-3 (ADR-026 §6): arnés de aceptación del intermediario de espacio de trabajo.
//!
//! SOLO usa la API de F9-0 (claim/release/expire_leases) y F9-1 (commit por rutas y
//! parche de recuperación): no depende de F9-2, que está pendiente de decisión.
//!
//! 4 agentes simulados (hilos, sin LLM) sobre un repo git temporal; 200 tareas en 20
//! grupos de glob `gNN/**` con solapes a propósito; un agente muere a mitad de edición
//! en 1 de cada 10 repeticiones (≈10 % de agente-repeticiones) y su tarea se caduca con
//! `recovery_patch`. Escritor git serializado (un solo escritor, como el broker real).
//!
//! Invariantes:
//! - 0 tareas reclamadas dos veces (detección real con HashSet),
//! - 0 solapes entre reclamos activos,
//! - 0 escrituras perdidas (cada marcador exacto declarado aparece en el árbol o en parche),
//! - 0 commits con ficheros fuera de la ruta reclamada,
//! - 0 errores `index.lock`,
//! - p95 del reclamo < 200 ms (se publica).
//!
//! `TYLLUAN_F9_REPS` fija las repeticiones (por defecto 5; 50 para la evidencia completa).
//! Marcado `#[ignore]` para no cargar CI: se lanza a mano con
//! `cargo test -p tylluan-kernel --test workspace_acceptance -- --ignored --nocapture`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};

use tylluan_kernel::workspace::git_writer::{
    commit_paths, path_matches_glob, recovery_patch,
};
use tylluan_kernel::workspace::{ClaimOutcome, TaskSpec, WorkspaceStore};

fn run_git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} fallo: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn init_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.email", "f93@example.invalid"]);
    run_git(&repo, &["config", "user.name", "F9-3"]);
    run_git(&repo, &["config", "commit.gpgsign", "false"]);
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-q", "-m", "base"]);
    repo
}

/// Predicado INDEPENDIENTE del que usa el broker (no llama a globs_overlap):
/// sirve para detectar que dos reclamos activos realmente se pisan aunque la
/// guarda del kernel se mute a `false`.
fn test_overlap(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches("/**");
    let b = b.trim_end_matches("/**");
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn p95(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let idx = ((values.len() as f64) * 0.95).ceil() as usize;
    values[idx.saturating_sub(1).min(values.len() - 1)]
}

#[derive(Default)]
struct RepMetrics {
    claims: usize,
    double_claims: usize,
    rejected: usize,
    overlaps: usize,
    lost_markers: usize,
    commits_outside: usize,
    index_lock_errors: usize,
    commit_errors: usize,
    claim_ms: Vec<u64>,
}

fn one_rep(rep: usize, data_root: &Path) -> RepMetrics {
    let dir = tempfile::tempdir_in(data_root).unwrap();
    let repo = init_repo(dir.path());
    // Pre-crea los directorios con un .keep rastreado: si no, `git status`
    // colapsa un directorio nuevo como "?? gXX/" y commit_paths no ve el
    // fichero del reclamo (bug real de F9-1 detectado por este arnés).
    for g in 0..20 {
        for i in 0..10 {
            let sub = if i == 0 {
                format!("g{g:02}")
            } else {
                format!("g{g:02}/s{i}")
            };
            std::fs::create_dir_all(repo.join(&sub)).unwrap();
            std::fs::write(repo.join(&sub).join(".keep"), "keep\n").unwrap();
        }
    }
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-q", "-m", "dirs"]);
    let store = Arc::new(WorkspaceStore::open(dir.path().join("workspace.db")).unwrap());
    let now = chrono::Utc::now().timestamp_millis();
    for g in 0..20 {
        for i in 0..10 {
            let task = format!("g{g:02}t{i:02}");
            let glob = if i == 0 {
                format!("g{g:02}/**")
            } else {
                format!("g{g:02}/s{i}/**")
            };
            store.create_task(now, &TaskSpec::new(&task, glob)).unwrap();
        }
    }

    let dead_agent = rep % 10 == 3;
    let next = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(4));
    let git_lock = Arc::new(Mutex::new(()));
    let metrics = Arc::new(Mutex::new(RepMetrics::default()));
    let seen_claims = Arc::new(Mutex::new(HashSet::<String>::new()));
    let markers = Arc::new(Mutex::new(HashSet::<String>::new()));
    let dead_done = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for agent_idx in 0..4usize {
        let store = Arc::clone(&store);
        let repo = repo.clone();
        let next = Arc::clone(&next);
        let barrier = Arc::clone(&barrier);
        let git_lock = Arc::clone(&git_lock);
        let metrics = Arc::clone(&metrics);
        let seen_claims = Arc::clone(&seen_claims);
        let markers = Arc::clone(&markers);
        let dead_done = Arc::clone(&dead_done);
        let agent = format!("a{agent_idx}");
        let dies = dead_agent && agent_idx == 3;
        handles.push(std::thread::spawn(move || {
            barrier.wait();
            let deadline = Instant::now() + Duration::from_secs(900);
            loop {
                if Instant::now() > deadline {
                    panic!("F9-3 rep={rep}: sin terminar en 900 s");
                }
                let idx = next.fetch_add(1, Ordering::SeqCst);
                if idx >= 200 {
                    return;
                }
                let group = idx / 10;
                let sub = idx % 10;
                let task = format!("g{group:02}t{sub:02}");
                let glob = if sub == 0 {
                    format!("g{group:02}/**")
                } else {
                    format!("g{group:02}/s{sub}/**")
                };
                // Reintenta hasta que su grupo quede libre: un rechazo por solape no
                // puede consumir la tarea (si no, solo se reclamarian 20 de 200).
                // El p95 mide el INTENTO que concede (no la espera de contencion).
                let (claim, elapsed) = loop {
                    let attempt_started = Instant::now();
                    match store.claim(&agent, &task, &glob, chrono::Utc::now().timestamp_millis()) {
                        Ok(ClaimOutcome::Claimed(c)) => {
                            break (c, attempt_started.elapsed().as_millis() as u64);
                        }
                        Ok(ClaimOutcome::Rejected(_)) => {
                            metrics.lock().unwrap().rejected += 1;
                            if Instant::now() > deadline {
                                panic!("F9-3 rep={rep}: sin poder reclamar {task} en 900 s");
                            }
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(2)),
                    }
                };
                {
                    let active = store.active_claims().unwrap();
                    let mut m = metrics.lock().unwrap();
                    m.claims += 1;
                    m.claim_ms.push(elapsed);
                    for (i, a) in active.iter().enumerate() {
                        for b in active.iter().skip(i + 1) {
                            if test_overlap(&a.path_glob, &b.path_glob) {
                                m.overlaps += 1;
                            }
                        }
                    }
                }
                if !seen_claims.lock().unwrap().insert(claim.task_id.clone()) {
                    metrics.lock().unwrap().double_claims += 1;
                }
                let marker = format!("F93-{rep}-{agent}-{idx}-MARKER");
                markers.lock().unwrap().insert(marker.clone());
                let file = repo
                    .join(glob.replace("/**", ""))
                    .join(format!("{marker}.txt"));
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(&file, &marker).unwrap();

                if dies && dead_done.fetch_add(1, Ordering::SeqCst) == 0 {
                    // Muere a mitad de edición: caduca su arriendo y guarda el
                    // parche de recuperación (no commitea).
                    let _ = store.expire_agent_leases(&agent, chrono::Utc::now().timestamp_millis() + 121_000);
                    {
                        let _g = git_lock.lock().unwrap();
                        let _ = recovery_patch(&store, &repo, &claim);
                    }
                    return;
                }

                // Renueva el arriendo justo antes de git (200 commits pueden tardar).
                let _ = store.heartbeat(
                    &agent,
                    &claim.task_id,
                    chrono::Utc::now().timestamp_millis(),
                );
                let commit_result = {
                    let _g = git_lock.lock().unwrap();
                    commit_paths(&store, &repo, &claim, &marker)
                };
                match commit_result {
                    Ok(_) => {
                        let shown =
                            run_git(&repo, &["show", "--name-only", "--pretty=format:", "HEAD"]);
                        for line in shown.lines().filter(|l| !l.is_empty()) {
                            if !path_matches_glob(&glob, line) {
                                metrics.lock().unwrap().commits_outside += 1;
                            }
                        }
                    }
                    Err(e) => {
                        if e.to_string().contains("index.lock") {
                            metrics.lock().unwrap().index_lock_errors += 1;
                        } else {
                            eprintln!("F93 commit_error task={} glob={} err={e}", claim.task_id, glob);
                            metrics.lock().unwrap().commit_errors += 1;
                        }
                    }
                }
                let _ = store.release(
                    &agent,
                    &claim.task_id,
                    chrono::Utc::now().timestamp_millis(),
                );
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let mut all_text = String::new();
    fn walk(dir: &Path, out: &mut String) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
    walk(&repo, &mut all_text);
    let patches = data_root.join("workspace").join("patches");
    if patches.exists() {
        walk(&patches, &mut all_text);
    }
    let declared = markers.lock().unwrap();
    let mut m = metrics.lock().unwrap();
    m.lost_markers = declared
        .iter()
        .filter(|marker| !all_text.contains(marker.as_str()))
        .count();
    RepMetrics {
        claims: m.claims,
        double_claims: m.double_claims,
        rejected: m.rejected,
        overlaps: m.overlaps,
        lost_markers: m.lost_markers,
        commits_outside: m.commits_outside,
        index_lock_errors: m.index_lock_errors,
        commit_errors: m.commit_errors,
        claim_ms: std::mem::take(&mut m.claim_ms),
    }
}

#[ignore = "F9-3: arnes manual de aceptacion (5 min/rep en Windows); correr con --ignored"]
#[test]
fn f9_acceptance_4_agents_200_tasks_overlapping() {
    let reps: usize = std::env::var("TYLLUAN_F9_REPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let root = tempfile::tempdir().unwrap();
    // Una sola ruta de datos para todo el test (no set_var por repetición con hilos vivos).
    let data_root = root.path().join("data");
    std::fs::create_dir_all(&data_root).unwrap();
    unsafe { std::env::set_var("TYLLUAN_DATA_DIR", &data_root) };
    let mut total_claims = 0usize;
    let mut double_claims = 0usize;
    let mut rejected = 0usize;
    let mut overlaps = 0usize;
    let mut lost = 0usize;
    let mut outside = 0usize;
    let mut locks = 0usize;
    let mut commit_errors = 0usize;
    let mut all_ms = Vec::new();
    for rep in 0..reps {
        let m = one_rep(rep, &data_root);
        total_claims += m.claims;
        double_claims += m.double_claims;
        rejected += m.rejected;
        overlaps += m.overlaps;
        lost += m.lost_markers;
        outside += m.commits_outside;
        locks += m.index_lock_errors;
        commit_errors += m.commit_errors;
        all_ms.extend(m.claim_ms);
    }
    let p95_ms = p95(all_ms);
    println!(
        "F9-3: reps={reps} claims={total_claims} p95={p95_ms}ms dobles={double_claims} \
         solapes={overlaps} perdidos={lost} fuera_de_ruta={outside} index_lock={locks} \
         commit_errors={commit_errors} rechazos_guarda={rejected}"
    );
    assert_eq!(double_claims, 0, "tareas reclamadas dos veces");
    assert!(rejected > 0, "la guarda de solape nunca rechazo: el arnes no muerde");
    assert_eq!(overlaps, 0, "solapes de reclamos activos");
    assert_eq!(lost, 0, "escrituras perdidas");
    assert_eq!(outside, 0, "commits con ficheros fuera de la ruta");
    assert_eq!(locks, 0, "errores index.lock");
    assert_eq!(commit_errors, 0, "commits rechazados");
    assert!(p95_ms < 200, "p95 del reclamo {p95_ms} ms >= 200 ms");
}
