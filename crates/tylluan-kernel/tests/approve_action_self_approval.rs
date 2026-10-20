//! D-13 (especificación ejecutable, escrita ANTES de la implementación — Buffy, COLA 2, 2026-10-21).
//!
//! `approve_action` (HITL del plan mode) debe rechazar la autoaprobación sin
//! credencial de operador. Hoy NO comprueba ni quién aprueba ni si el aprobador
//! es operador: cualquier cliente MCP autenticado puede aprobar su propio plan
//! y el kernel lo registra en el audit como `human_intervention=true` — una
//! mentira que contamina el ASR y las métricas de fricción.
//!
//! Contrato observable que Deep debe cumplir en D-13 (los cuatro casos del
//! encargo de la cola B-7):
//!
//! 1. El AUTOR de un plan aprueba su propio plan sin operador: rechazo con
//!    el token de error `self_approval`; el plan NO se consume y NO se
//!    escribe audit. (Hoy: se aprueba y se ejecuta.)
//! 2. OTRO agente aprueba sin operador: rechazo (token `operator_required`);
//!    plan intacto, sin audit. (Hoy: se aprueba igualmente - no hay ninguna
//!    comprobación.)
//! 3. OPERADOR (token maestro por CABECERA) aprueba: la aprobación procede
//!    (el plan se consume y la herramienta se ejecuta); HTTP 200.
//! 4. El audit solo registra `human_intervention=true` cuando el aprobador
//!    fue operador.
//!
//! Sobre la superficie del rechazo: la cola describe los casos 1-2 como "403
//! self_approval" / "403". Un `tools/call` MCP devuelve normalmente HTTP 200
//! con `result.isError=true`, así que `assert_refused` acepta CUALQUIERA de
//! las dos superficies (HTTP 403, o 200+isError) siempre que el cuerpo lleve
//! el token de error esperado — lo no negociable es el rechazo machine-readable
//! + que el plan no se consuma + que no haya audit.
//!
//! El plan fixture apunta a un guild inexistente a propósito: lo que especifica
//! esta prueba es la COMPUERTA DE APROBACIÓN, no la ejecución del guild; con un
//! guild inexistente el camino de aprobación es idéntico (se consume plan y se
//! audita) pero el test queda hermético (sin spawn de procesos Python).
//!
//! Corrección del "autor": el identidad del aprobador llega como argumento
//! `agent_id` del tools/call (handlers.rs:23, mismo mecanismo que el resto de
//! herramientas soberanas), y el credencial de operador es el token maestro en
//! la cabecera Authorization (AclAuthKind::MasterToken, auth.rs
//! `operator_gate_allows`). El JWT OAuth autentica pero NO es operador — igual
//! que en producción para cualquier cliente de la flota.
//!
//! Run: cargo test -p tylluan-kernel --test approve_action_self_approval -- --ignored
//!
//! Marcada `#[ignore = "D-13"]` (mismo patrón que la suite OAuth F2): sin
//! `--ignored` la suite normal la salta y CI sigue verde; con `--ignored`
//! FALLA HOY mostrando los casos incumplidos — esa salida es la demostración
//! de que el bug existe. Tras implementar D-13, la misma corrida debe pasar.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::build_router;

const MASTER_TOKEN: &str = "master-d13-token";
const AUTHOR: &str = "agent-author";
const OTHER: &str = "agent-other";

/// Send an `approve_action` tools/call over the real production router.
async fn approve_call(
    app: &axum::Router,
    bearer: &str,
    request_id: &str,
    agent_id: Option<&str>,
) -> (StatusCode, String) {
    let mut arguments = serde_json::json!({ "requestId": request_id, "approved": true });
    if let Some(aid) = agent_id {
        arguments["agent_id"] = serde_json::json!(aid);
    }
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": "approve_action", "arguments": arguments }
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

/// A refusal is EITHER an HTTP 403 from the gate, OR an MCP-level error
/// (result.isError / jsonrpc error) — but the response MUST carry the
/// machine-readable `token` so a vague failure never counts as a refusal.
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

/// Count audit rows for a given intent (the plan's intent is what
/// `approve_action` logs into the audit's `intent` column).
fn audit_rows_for_intent(db_path: &std::path::Path, intent: &str) -> Vec<i64> {
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
    let Ok(mut stmt) = conn.prepare(
        "SELECT human_intervention FROM guild_audit_log WHERE intent = ?1",
    ) else {
        return Vec::new();
    };
    let rows = stmt
        .query_map([intent], |row| row.get::<_, i64>(0))
        .and_then(|r| r.collect::<Result<Vec<_>, _>>());
    rows.unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "D-13"]
async fn d13_self_approval_requires_operator_credential() {
    // The one env seam for this binary (process-global): audit writes land in a
    // temp file so the repo's ./data/audit.db is never touched.
    let audit_db = std::env::temp_dir().join(format!(
        "tylluan_d13_audit_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; this test binary contains
    // exactly one #[test], so no concurrent env access is possible.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    grants::init(); // como en el arranque real (main.rs): el registro de grants oneshot
    grants::init_plan_store();

    // dev_mode=false on purpose: the DevBypass operator kind must not exist in
    // this scenario, so "operator" can ONLY mean the master token in the
    // Authorization header (exactly the production operator class).
    let state = support::test_state_custom("d13", Some(MASTER_TOKEN.to_string()), false).await;
    // Any authenticated non-operator (a fleet agent's OAuth JWT, scope mcp).
    let jwt = state.oauth.sign_jwt("d13-jti", 300);
    let app = build_router(state);

    let mut failures: Vec<String> = Vec::new();

    // ── Caso 1: el AUTOR aprueba su propio plan sin operador ────────────────
    let plan_self = "d13-plan-self";
    grants::store_plan(
        plan_self,
        "d13_probe",
        "d13_tool",
        &serde_json::json!({}),
        AUTHOR,
        "D-13 caso-autor",
    )
    .await;
    let (st, body) = approve_call(&app, &jwt, plan_self, Some(AUTHOR)).await;
    if !is_refused_with(st, &body, "self_approval") {
        failures.push(format!(
            "CASO 1 (autor aprueba su propio plan sin operador): esperado rechazo con token \
             `self_approval` (HTTP 403 o 200+isError), obtenido HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if !plan_is_pending(plan_self).await {
        failures.push(
            "CASO 1: el plan fue consumido por una autoaprobacion no autorizada \
             (debe seguir pendiente hasta que un operador lo resuelva)"
                .to_string(),
        );
    }

    // ── Caso 2: OTRO agente aprueba sin operador ────────────────────────────
    let plan_other = "d13-plan-other";
    grants::store_plan(
        plan_other,
        "d13_probe",
        "d13_tool",
        &serde_json::json!({}),
        AUTHOR,
        "D-13 caso-otro-agente",
    )
    .await;
    let (st, body) = approve_call(&app, &jwt, plan_other, Some(OTHER)).await;
    if !is_refused_with(st, &body, "operator_required") {
        failures.push(format!(
            "CASO 2 (otro agente aprueba sin operador): esperado rechazo con token \
             `operator_required`, obtenido HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if !plan_is_pending(plan_other).await {
        failures.push(
            "CASO 2: el plan fue consumido por un agente no operador \
             (debe seguir pendiente)"
                .to_string(),
        );
    }

    // ── Audit de los casos 1-2: NO debe existir ninguna fila ────────────────
    // (el audit se escribe fire-and-forget en spawn_blocking: un frame de
    // espera corta para asegurar que un falso negativo no sea solo carrera).
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    for intent in ["D-13 caso-autor", "D-13 caso-otro-agente"] {
        let rows = audit_rows_for_intent(&audit_db, intent);
        if !rows.is_empty() {
            failures.push(format!(
                "CASO 4: existe(n) {n} fila(s) de audit con human_intervention={flags:?} \
                 para intents rechazados ('{intent}') — solo el aprobador operador debe \
                 generar auditoria HITL",
                n = rows.len(),
                flags = rows,
                intent = intent
            ));
        }
    }

    // ── Caso 3: OPERADOR (token maestro en cabecera) aprueba ────────────────
    let (st, body) = approve_call(&app, MASTER_TOKEN, plan_other, None).await;
    let refused_by_gate = is_refused_with(st, &body, "operator_required")
        || is_refused_with(st, &body, "self_approval");
    if st != StatusCode::OK || refused_by_gate {
        failures.push(format!(
            "CASO 3 (operador con token maestro): esperado HTTP 200 aprobando, obtenido \
             HTTP {st}: {}",
            body.chars().take(400).collect::<String>()
        ));
    }
    if plan_is_pending(plan_other).await {
        failures.push(
            "CASO 3: el plan deberia estar consumido tras la aprobacion del operador"
                .to_string(),
        );
    }

    // ── Caso 4 (parte positiva): el audit del operador lleva human_intervention ──
    {
        let mut hitl = false;
        for _ in 0..50 {
            let rows = audit_rows_for_intent(&audit_db, "D-13 caso-otro-agente");
            if rows.contains(&1) {
                hitl = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        if !hitl {
            failures.push(
                "CASO 4: la aprobacion del operador no produjo ninguna fila de audit con \
                 human_intervention=1 (el camino HITL debe quedar registrado)"
                    .to_string(),
            );
        }
    }

    assert!(
        failures.is_empty(),
        "D-13 (self-approval de approve_action) — {} caso(s) sin cumplir:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}
