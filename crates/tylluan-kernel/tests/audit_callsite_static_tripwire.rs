//! M-BUFFY-7 — tripwires estáticos de las 5 llamadas `log_audit_entry` con
//! `let _ =` de T-DSH-67 (filas 1-5). Cada test compila el archivo fuente con
//! `include_str!` y exige que la llamada siga viva (línea real, sin comentar).
//!
//! Si alguien elimina, comenta o "limpia" una de esas llamadas, este binario
//! falla en CI con un mensaje que nombra la fila exacta de T-DSH-67.
//! La contraparte conductora (filas 2 y 4, escritura real en la DB de audit)
//! vive en `tests/audit_behavior_mb7_rows.rs`.
//!
//! Cobertura completa de las 5 filas:
//!   fila 1 approve_action ....... row1_approve_action_audit_call_is_live
//!                                 (+ conductora existente: approve_audit_matrix)
//!   fila 2 tools kernel ......... row2_kernel_tool_audit_call_is_live
//!                                 (+ conductora nueva: audit_behavior_mb7_rows)
//!   fila 3 outcome tylluan_do ... row3_outcome_audit_call_is_live
//!                                 (+ conductora existente: D-28, lib tests)
//!   fila 4 external MCP ......... row4_external_mcp_audit_call_is_live
//!                                 (+ conductora nueva: audit_behavior_mb7_rows)
//!   fila 5 silva/edge D-34 ...... row5_silva_edge_audit_call_is_live
//!                                 (+ conductora existente: cola_d34_acceptance)
//!
//! Run: cargo test -p tylluan-kernel --test audit_callsite_static_tripwire

const HANDLERS_RS: &str = include_str!("../src/transport/server/handlers.rs");
const OUTCOME_RS: &str = include_str!("../src/transport/server/handler_do/outcome.rs");
const EXTERNAL_MCP_RS: &str = include_str!("../src/transport/server/handler_do/external_mcp.rs");
const API_V1_MOD_RS: &str = include_str!("../src/transport/http/api_v1/mod.rs");
const AUDIT_RS: &str = include_str!("../src/transport/server/handler_do/audit.rs");
const CI_YML: &str = include_str!("../../../.github/workflows/ci.yml");

/// `true` si `needle` aparece en una línea viva (no comentada) de `src`.
fn live_line(src: &str, needle: &str) -> bool {
    src.lines().any(|line| {
        let t = line.trim_start();
        !t.starts_with("//") && !t.starts_with('*') && t.contains(needle)
    })
}

#[test]
fn row1_approve_action_audit_call_is_live() {
    assert!(
        live_line(HANDLERS_RS, "let _ = handler_do::audit::log_audit_entry("),
        "M-BUFFY-7: la fila 1 de T-DSH-67 (audit de approve_action, \
         handlers.rs) desapareció o quedó comentada. Si el cambio es intencional, \
         elimina/ajusta este tripwire JUNTO con la llamada y dilo en el commit."
    );
}

#[test]
fn row2_kernel_tool_audit_call_is_live() {
    assert!(
        live_line(HANDLERS_RS, "let _ = handler_do::log_audit_entry(\"\", \"kernel\""),
        "M-BUFFY-7: la fila 2 de T-DSH-67 (audit de tools soberanas que no \
         son tylluan_do, handlers.rs, guild='kernel') desapareció o quedó \
         comentada. Si el cambio es intencional, elimina/ajusta este tripwire \
         JUNTO con la llamada y dilo en el commit."
    );
}

#[test]
fn row3_outcome_audit_call_is_live() {
    assert!(
        live_line(OUTCOME_RS, "let _ = log_audit_entry(&audit_intent, &audit_guild"),
        "M-BUFFY-7: la fila 3 de T-DSH-67 (audit de dispatch de tylluan_do en \
         post_process_outcome, outcome.rs) desapareció o quedó comentada. El \
         test D-28 de lib la verifica por conducto; si retiras la llamada, \
         fallarán ambos."
    );
}

#[test]
fn row4_external_mcp_audit_call_is_live() {
    assert!(
        live_line(EXTERNAL_MCP_RS, "let _ = log_audit_entry(&audit_intent, &audit_guild"),
        "M-BUFFY-7: la fila 4 de T-DSH-67 (audit de llamadas a MCP externos, \
         external_mcp.rs) desapareció o quedó comentada. Si el cambio es \
         intencional, elimina/ajusta este tripwire JUNTO con la llamada."
    );
}

#[test]
fn row5_silva_edge_audit_call_is_live() {
    assert!(
        live_line(API_V1_MOD_RS, "let _ = crate::transport::server::handler_do::audit::log_audit_entry("),
        "M-BUFFY-7: la fila 5 de T-DSH-67 (excepción de operador-guild D-34 \
         en POST /api/v1/silva/edge, api_v1/mod.rs) desapareció o quedó \
         comentada. cola_d34_acceptance verifica por conducto la escritura."
    );
}

#[test]
fn writer_log_audit_entry_is_live() {
    assert!(
        live_line(AUDIT_RS, "pub(crate) fn log_audit_entry("),
        "M-BUFFY-7: la función escritora log_audit_entry (handler_do/audit.rs) \
         desapareció o fue renombrada: todas las filas de T-DSH-67 dependen de ella."
    );
}

#[test]
fn ci_runs_the_behavior_tripwires() {
    assert!(
        CI_YML.contains("--test audit_behavior_mb7_rows"),
        "M-BUFFY-7: ci.yml dejó de ejecutar el test conductora \
         audit_behavior_mb7_rows — sin esa línea las filas 2 y 4 de T-DSH-67 \
         dejan de quedar blindadas en CI."
    );
    assert!(
        CI_YML.contains("--test audit_callsite_static_tripwire"),
        "M-BUFFY-7: ci.yml dejó de ejecutar audit_callsite_static_tripwire."
    );
}
