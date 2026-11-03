//! Experimento de Deep (T2607): ¿muerde el invariante de solapes del arnes F9-3
//! cuando el reclamo se MANTIENE VIVO mientras se comprueba?
//!
//! Minimal y determinista, sin hilos ni suerte de planificacion: A reclama el
//! grupo entero y NO lo suelta; B intenta el subgrupo solapado mientras A sigue
//! activo. Con la guarda `globs_overlap` en pie, B debe ser rechazado. Si la
//! guarda se muta a `false`, B entra y el solape es observable -> el test falla.

use std::sync::Arc;

use tylluan_kernel::workspace::{ClaimOutcome, TaskSpec, WorkspaceStore};

/// Predicado independiente del broker (no llama a `globs_overlap`).
fn test_overlap(a: &str, b: &str) -> bool {
    let a = a.trim_end_matches("/**");
    let b = b.trim_end_matches("/**");
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

#[test]
fn solapes_se_detectan_con_el_reclamo_vivo() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(WorkspaceStore::open(dir.path().join("ws.db")).unwrap());
    let now = chrono::Utc::now().timestamp_millis();
    store
        .create_task(now, &TaskSpec::new("t-root", "g05/**"))
        .unwrap();
    store
        .create_task(now, &TaskSpec::new("t-sub", "g05/s3/**"))
        .unwrap();

    let first = store.claim("a0", "t-root", "g05/**", now).unwrap();
    assert!(
        matches!(first, ClaimOutcome::Claimed(_)),
        "el primer reclamo debe concederse sobre un store vacio"
    );

    let mut granted_sub = false;
    let mut overlaps_seen = 0usize;
    for attempt in 0..50 {
        let ts = now + attempt as i64;
        match store.claim("a1", "t-sub", "g05/s3/**", ts).unwrap() {
            ClaimOutcome::Claimed(_) => {
                granted_sub = true;
                let active = store.active_claims().unwrap();
                for (i, x) in active.iter().enumerate() {
                    for y in active.iter().skip(i + 1) {
                        if test_overlap(&x.path_glob, &y.path_glob) {
                            overlaps_seen += 1;
                        }
                    }
                }
                let _ = store.release("a1", "t-sub", ts);
            }
            ClaimOutcome::Rejected(_) => break,
        }
    }
    let _ = store.release("a0", "t-root", now + 1_000);

    println!("EXP: granted_sub={granted_sub} solapes={overlaps_seen}");
    assert_eq!(
        overlaps_seen, 0,
        "dos reclamos activos con globs solapados convivieron (guarda de solape inoperante)"
    );
}
