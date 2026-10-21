//! T-DSH-13 — C2 de T-DSH-10: herencia **retroactiva** de `untrusted_content`.
//!
//! D-18 marcaba los resúmenes solo en su creación. Si la fuente se marca
//! después (federación, A2A, ingesta, juez post-escritura), el `agent_summary`
//! conservaba el texto verbatim sin marca y viajaba por bootstrap/contexto de
//! sesión. El fix propaga la marca al escribir el flag sobre un nodo ya citado.
//!
//! C1 (control): la herencia en creación sigue intacta.
//! C2: resumen creado limpio -> marcar la fuente después -> resumen re-marcado.
//! Control negativo: un resumen que NO cita la fuente no se marca.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test trust_summary_retro

mod support;

use std::sync::Arc;
use tylluan_kernel::memory::agent_memory::AgentMemoryManager;
use tylluan_kernel::memory::silva::{GraphNode, SilvaDB};
use tylluan_kernel::security::trust;

async fn seed_memory(silva: &Arc<SilvaDB>, id: &str, agent: &str, payload: &str, flagged: bool) {
    let meta = if flagged {
        serde_json::json!({
            "agent_id": agent,
            "importance": 0.5,
            "untrusted_content": true,
            "untrusted_reason": "external-feed"
        })
    } else {
        serde_json::json!({ "agent_id": agent, "importance": 0.5 })
    };
    silva
        .upsert_node(id, "agent_memory", &format!("[{agent}] {payload}"), &meta.to_string())
        .await
        .unwrap();
}

async fn summary_of(silva: &Arc<SilvaDB>, agent: &str) -> Option<GraphNode> {
    AgentMemoryManager::new(silva.clone(), 3).get_summary(agent).await
}

fn is_flagged(summary: &Option<GraphNode>) -> bool {
    summary
        .as_ref()
        .map(|s| s.metadata.contains("\"untrusted_content\":true"))
        .unwrap_or(false)
}

#[tokio::test(flavor = "multi_thread")]
async fn untrusted_flag_propagates_retroactively_to_summaries() {
    let audit_db =
        std::env::temp_dir().join(format!("tylluan_t13_audit_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; un único #[test] en el binario.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();

    let state = support::test_state_custom("t13", Some("t13-token".to_string()), false).await;
    let silva = state.silva.clone();
    let mgr = AgentMemoryManager::new(silva.clone(), 3);

    // ── C1 (control): herencia en creación sigue intacta ────────────────────
    for i in 0..4 {
        seed_memory(&silva, &format!("mem:c1:{i}"), "c1", "memoria local", false).await;
    }
    seed_memory(&silva, "mem:c1:x", "c1", "ZZC1 fuente marcada primero", true).await;
    mgr.consolidate_if_needed("c1").await;
    let c1 = summary_of(&silva, "c1").await;
    assert!(c1.is_some(), "C1: la consolidación debe crear el resumen");
    assert!(
        is_flagged(&c1),
        "C1: el resumen debe heredar la marca si la fuente ya estaba marcada: {:?}",
        c1.map(|s| s.metadata)
    );

    // ── C2: resumen limpio y la fuente se marca DESPUÉS ─────────────────────
    for i in 0..4 {
        seed_memory(&silva, &format!("mem:c2:{i}"), "c2", "memoria local", false).await;
    }
    seed_memory(&silva, "mem:c2:x", "c2", "ZZRETRO contenido hostil tardio", false).await;
    mgr.consolidate_if_needed("c2").await;
    let c2_before = summary_of(&silva, "c2").await;
    assert!(c2_before.is_some(), "C2: resumen creado");
    assert!(
        !is_flagged(&c2_before),
        "C2: el resumen debe nacer limpio (aún no hay marca)"
    );
    assert!(
        c2_before
            .as_ref()
            .map(|s| s.content.contains("ZZRETRO"))
            .unwrap_or(false),
        "C2: el resumen ya copió el texto de la fuente"
    );

    // El flag llega DESPUÉS (mismo camino que federación/A2A/ingesta:
    // upsert_node_with_validity con metadata marcada).
    seed_memory(&silva, "mem:c2:x", "c2", "ZZRETRO contenido hostil tardio", true).await;

    let c2_after = summary_of(&silva, "c2").await;
    assert!(
        is_flagged(&c2_after),
        "C2: tras marcar la fuente, el resumen que la cita debe re-marcarse: {:?}",
        c2_after.as_ref().map(|s| s.metadata.clone())
    );

    // El banner debe llegar al contexto (bootstrap) sin perder el texto.
    let boot = tylluan_kernel::transport::server::bootstrap::build_agent_bootstrap(
        silva.clone(),
        "c2",
    )
    .await
    .to_string();
    assert!(
        boot.contains("ZZRETRO"),
        "C2: el resumen sigue en el contexto (control)"
    );
    assert!(
        boot.contains("[CONTENIDO NO CONFIABLE"),
        "C2: el resumen re-marcado debe viajar con banner: {boot}"
    );

    // ── Control negativo: un resumen que NO cita la fuente no se marca ──────
    for i in 0..4 {
        seed_memory(&silva, &format!("mem:c3:{i}"), "c3", "otra memoria limpia", false).await;
    }
    seed_memory(&silva, "mem:c3:x", "c3", "ZZC3 texto distinto", false).await;
    mgr.consolidate_if_needed("c3").await;
    let c3 = summary_of(&silva, "c3").await;
    assert!(c3.is_some(), "C3: resumen creado");
    // Marcar la fuente de c2 no debe tocar el resumen de c3.
    seed_memory(&silva, "mem:c2:x", "c2", "ZZRETRO contenido hostil tardio", true).await;
    let c3_after = summary_of(&silva, "c3").await;
    assert!(
        !is_flagged(&c3_after),
        "negativo: un resumen que no cita la fuente marcada no se toca"
    );

    // La idempotencia del helper: segunda propagación no cuenta de más.
    let extra = trust::propagate_untrusted_to_summaries(&silva, "mem:c2:x").await;
    assert_eq!(extra, 0, "la propagación debe ser idempotente");
}
