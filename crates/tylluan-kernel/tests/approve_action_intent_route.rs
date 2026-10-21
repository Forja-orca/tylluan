//! D-16 — segunda superficie de aprobación: la ruta de intent de `tylluan_do`.
//!
//! El propio kernel, en modo plan, emite esta instrucción:
//!
//! ```text
//! To approve, call: tylluan_do(intent='approve action for plan <plan_id>')
//!                   or approve_action(requestId='<plan_id>', approved=true).
//! ```
//!
//! D-13 puso la compuerta de auto-aprobación en `approve_action`. Si la ruta de
//! intent resuelve planes por su cuenta (o no existe y el cliente acaba en otro
//! sitio), la compuerta se elude o la instrucción es mentira. Esta prueba fija
//! el contrato de la SEGUNDA superficie con los mismos tres casos observables y
//! sin duplicar la lógica: el intent debe terminar en el mismo `approve_action`
//! (una sola compuerta, un solo audit).
//!
//! 1. El AUTOR del plan pide aprobarlo por intent, sin credencial de operador:
//!    rechazo con el token `self_approval`; el plan NO se consume; sin audit.
//! 2. Un ANÓNIMO (sin `agent_id`) sobre un plan anónimo, sin operador: mismo
//!    rechazo; el plan sigue pendiente.
//! 3. El OPERADOR (token maestro en cabecera) lo aprueba por intent: el plan se
//!    consume y el audit nombra al aprobador real (`agent_id` del que aprueba,
//!    `human_intervention=1`).
//!
//! El plan fixture apunta a un guild inexistente a propósito: se especifica la
//! compuerta de aprobación, no la ejecución del guild (hermético, sin spawn de
//! Python), igual que en `approve_action_self_approval.rs`.
//!
//! Run: cargo test -p tylluan-kernel --test approve_action_intent_route

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::build_router;

const MASTER_TOKEN: &str = "master-d16-token";
const AUTHOR: &str = "agent-author";
const OPERATOR_AS: &str = "operator-jose";

/// `tylluan_do(intent = 'approve action for plan <id>')` sobre el router real.
async fn intent_call(
    app: &axum::Router,
    bearer: &str,
    intent: &str,
    agent_id: Option<&str>,
) -> (StatusCode, String) {
    let mut arguments = serde_json::json!({ "intent": intent });
    if let Some(aid) = agent_id {
        arguments["agent_id"] = serde_json::json!(aid);
    }
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "tylluan_do", "arguments": arguments }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn is_refused_with(status: StatusCode, body: &str, token: &str) -> bool {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let mcp_error = parsed["result"]["isError"].as_bool() == Some(true)
        || parsed["error"].is_object();
    let refused = status == StatusCode::FORBIDDEN || (status == StatusCode::OK && mcp_error);
    refused && body.contains(token)
}

async fn plan_is_pending(id: &str) -> bool {
    grants::get_plan(id).await.is_some()
}

/// Filas de audit (agent_id, human_intervention) para un intent dado.
fn audit_rows_for_intent(
    db_path: &std::path::Path,
    intent: &str,
) -> Vec<(String, i64)> {
    let Ok(conn) = tylluan_kernel::config::open_db(db_path) else {
        return Vec::new();
    };
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS guild_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            guild TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            agent_id TEXT NOT NULL DEFAULT '',
            intent TEXT,
            status TEXT NOT NULL DEFAULT 'ok',
            result_preview TEXT,
            prev_hash TEXT NOT NULL DEFAULT '',
            hash TEXT NOT NULL
        );
        ALTER TABLE guild_audit_log ADD COLUMN latency_ms INTEGER;
        ALTER TABLE guild_audit_log ADD COLUMN human_intervention INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE guild_audit_log ADD COLUMN system_snapshot TEXT;",
    )
    .ok();
    let Ok(mut stmt) = conn
        .prepare("SELECT agent_id, human_intervention FROM guild_audit_log WHERE intent = ?1")
    else {
        return Vec::new();
    };
    stmt.query_map([intent], |row| {
        Ok((row.get::<_, String>(0).unwrap_or_default(), row.get::<_, i64>(1).unwrap_or(0)))
    })
    .and_then(|r| r.collect::<Result<Vec<_>, _>>())
    .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn d16_intent_route_reuses_the_same_approval_gate() {
    let audit_db = std::env::temp_dir().join(format!(
        "tylluan_d16_audit_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; this test binary contains
    // exactly one #[test], so no concurrent env access is possible.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    grants::init();
    grants::init_plan_store();

    let state = support::test_state_custom("d16", Some(MASTER_TOKEN.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("d16-jti", 300);
    let app = build_router(state);

    let mut failures: Vec<String> = Vec::new();

    // ── Caso 1: el autor aprueba su propio plan por intent ──────────────────
    let plan_self = "d16planself";
    grants::store_plan(
        plan_self,
        "d16_probe",
        "d16_tool",
        &serde_json::json!({}),
        AUTHOR,
        "D-16 intent caso-autor",
    )
    .await;
    let (st, body) = intent_call(
        &app,
        &jwt,
        &format!("approve action for plan {plan_self}"),
        Some(AUTHOR),
    )
    .await;
    if !is_refused_with(st, &body, "self_approval") {
        failures.push(format!(
            "CASO 1 (autor aprueba por intent sin operador): esperado rechazo \
             `self_approval`, obtenido HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if !plan_is_pending(plan_self).await {
        failures.push(
            "CASO 1: el plan fue consumido por la ruta de intent sin operador \
             (debe seguir pendiente)"
                .to_string(),
        );
    }

    // ── Caso 2: anónimo sobre plan anónimo, por intent ──────────────────────
    let plan_anon = "d16plananon";
    grants::store_plan(
        plan_anon,
        "d16_probe",
        "d16_tool",
        &serde_json::json!({}),
        "anonymous",
        "D-16 intent caso-anonimo",
    )
    .await;
    let (st, body) = intent_call(
        &app,
        &jwt,
        &format!("approve action for plan {plan_anon}"),
        None,
    )
    .await;
    if !is_refused_with(st, &body, "self_approval") {
        failures.push(format!(
            "CASO 2 (anónimo aprueba su propio plan por intent): esperado rechazo \
             `self_approval`, obtenido HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if !plan_is_pending(plan_anon).await {
        failures.push("CASO 2: el plan anónimo fue consumido sin operador".to_string());
    }

    // Los rechazos no deben dejar filas de audit (pasó por la compuerta de
    // D-13, no por la ejecución). Margen corto por el spawn_blocking del audit.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    for intent in ["D-16 intent caso-autor", "D-16 intent caso-anonimo"] {
        let rows = audit_rows_for_intent(&audit_db, intent);
        if !rows.is_empty() {
            failures.push(format!(
                "CASO 1-2: existe(n) {} fila(s) de audit para un intent rechazado \
                 ('{intent}'): {rows:?}",
                rows.len()
            ));
        }
    }

    // ── Caso 3: el operador aprueba por intent ──────────────────────────────
    let (st, body) = intent_call(
        &app,
        MASTER_TOKEN,
        &format!("approve action for plan {plan_anon}"),
        Some(OPERATOR_AS),
    )
    .await;
    if st != StatusCode::OK
        || is_refused_with(st, &body, "self_approval")
        || is_refused_with(st, &body, "operator_required")
    {
        failures.push(format!(
            "CASO 3 (operador aprueba por intent): esperado HTTP 200, obtenido \
             HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if plan_is_pending(plan_anon).await {
        failures.push(
            "CASO 3: el plan debería estar consumido tras la aprobación del operador \
             por la ruta de intent (si sigue pendiente, el intent no llegó a \
             approve_action)"
                .to_string(),
        );
    }

    // ── Caso 4: el audit nombra al aprobador real ───────────────────────────
    {
        let mut found: Option<(String, i64)> = None;
        for _ in 0..50 {
            let rows = audit_rows_for_intent(&audit_db, "D-16 intent caso-anonimo");
            if let Some(row) = rows.into_iter().find(|(_, hitl)| *hitl == 1) {
                found = Some(row);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        match found {
            None => failures.push(
                "CASO 4: la aprobación del operador por intent no dejó fila de audit \
                 con human_intervention=1"
                    .to_string(),
            ),
            Some((who, _)) if who != OPERATOR_AS => failures.push(format!(
                "CASO 4: el audit atribuye la aprobación a '{who}', no al aprobador \
                 real ('{OPERATOR_AS}')"
            )),
            Some(_) => {}
        }
    }

    assert!(
        failures.is_empty(),
        "D-16 (ruta de intent de approve_action) — {} caso(s) sin cumplir:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}
