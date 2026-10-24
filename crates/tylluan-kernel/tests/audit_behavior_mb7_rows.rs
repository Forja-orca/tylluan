//! M-BUFFY-7 — contraparte CONDUCTORA de `audit_callsite_static_tripwire.rs`.
//!
//! De las 5 llamadas `let _ = log_audit_entry(...)` con `let _ =` de T-DSH-67,
//! esta binario cubre por conducto las dos que no tenían ningún test que
//! fallara si la escritura desaparece:
//!
//!   fila 2 — herramientas soberanas que no son `tylluan_do` → guild='kernel'
//!            (handlers.rs, rama Ok del handle_kernel_tool): se ejercita con
//!            `ponder` contra `/mcp` real y se espera la fila en la DB.
//!   fila 4 — llamadas a MCP externos → guild='external_mcp:<server>'
//!            (handler_do/external_mcp.rs): se levanta un servidor MCP falso
//!            en 127.0.0.1 (initialize + tools/list + tools/call), se registra
//!            en config+registry, y se dispara `tylluan_do` con un intent sin
//!            guild_hint que resuelve a NO_GUILD_MATCH y cae en la fase 1 de
//!            `try_external_mcp_dispatch` (el intent contiene el nombre del
//!            servidor). Se espera la fila con intent/guild/tool/agent exactos.
//!
//! Filas 1, 3 y 5 ya tienen cobertura conductora previa (approve_audit_matrix,
//! test D-28 de lib y cola_d34_acceptance respectivamente) + tripwire estático.
//!
//! Convención del repo: UN solo `#[test]` en este binario, porque se usa la
//! seam de env `TYLLUAN_AUDIT_DB` (variable global por proceso).
//!
//! Run: cargo test -p tylluan-kernel --test audit_behavior_mb7_rows

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;
use tylluan_kernel::transport::http::build_router;

const MASTER: &str = "mb7-master-token";
const SERVER: &str = "mb7srv";
const FAKE_TOOL: &str = "mb7_probe";
const AGENT: &str = "mb7-agent";

type AuditRow = (String, String, String, String, String); // intent, guild, tool, agent, status

async fn mcp_call(
    app: &axum::Router,
    bearer: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> (StatusCode, String) {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
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

fn mcp_error(body: &str) -> bool {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    parsed["result"]["isError"].as_bool() == Some(true) || parsed["error"].is_object()
}

/// Poll de la DB de audit (la escritura es fire-and-forget: fila 2 usa
/// `tokio::spawn`, fila 4 `spawn_blocking`). Cualquier error de lectura se
/// reintenta hasta el deadline: la tabla puede no existir antes de la primera
/// escritura. Devuelve (intent, guild, tool_name, agent_id, success).
async fn wait_row(db: &std::path::Path, sql: &str, param: &str) -> Option<AuditRow> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut last_err = String::new();
    while std::time::Instant::now() < deadline {
        if let Ok(conn) = tylluan_kernel::config::open_db(db) {
            match conn.query_row(sql, rusqlite::params![param], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            }) {
                Ok(row) => return Some(row),
                Err(e) => last_err = e.to_string(),
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    eprintln!("MB7: fila '{param}' no encontrada; ultimo error de lectura: {last_err}");
    None
}

// ─── Servidor MCP falso (HTTP Streamable, solo loopback) ────────────────────

fn fake_mcp_app() -> axum::Router {
    axum::Router::new().route("/mcp", axum::routing::post(fake_mcp_handler))
}

async fn fake_mcp_handler(
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> (StatusCode, axum::response::Json<serde_json::Value>) {
    let id = body.get("id").cloned().unwrap_or(serde_json::Value::Null);
    let method = body.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "mb7-fake", "version": "0.0.1" }
        }),
        "tools/list" => json!({
            "tools": [{
                "name": FAKE_TOOL,
                "description": "mb7 probe tool zqxwjv",
                "inputSchema": { "type": "object", "properties": {} }
            }]
        }),
        "tools/call" => json!({
            "content": [{ "type": "text", "text": "mb7 fake ok" }],
            "isError": false
        }),
        _ => json!({}),
    };
    if id.is_null() {
        // notificaciones (p. ej. notifications/initialized): respuesta vacía
        (StatusCode::ACCEPTED, axum::response::Json(json!({})))
    } else {
        (
            StatusCode::OK,
            axum::response::Json(json!({ "jsonrpc": "2.0", "id": id, "result": result })),
        )
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn mb7_rows_2_and_4_write_audit_rows() {
    // Seam de audit: DB temporal única de este proceso (un solo #[test] por binario).
    let db = std::env::temp_dir().join(format!("tylluan_mb7_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&db);
    // SAFETY: edición 2024 exige unsafe; este binario tiene UN SOLO #[test],
    // así que no hay concurrencía sobre el entorno del proceso.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &db) };

    let state = support::test_state_custom("mb7", Some(MASTER.into()), false)
        .await;
    let jwt = state.oauth.sign_jwt("mb7-jti", 300);

    // Servidor MCP falso en loopback.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, fake_mcp_app()).await;
    });
    let url = format!("http://{addr}/mcp");

    // Config: servidor externo activo (misma caché que lee try_external_mcp_dispatch).
    {
        let mut cfg = state.config.write().await;
        cfg.external_mcp.push(tylluan_kernel::config::ExternalMcpConfig {
            name: SERVER.to_string(),
            command: None,
            args: None,
            cwd: None,
            url: Some(url.clone()),
            sse_url: None,
            post_url: None,
            env: None,
            headers: None,
            timeout_ms: Some(5000),
            active: true,
        });
    }

    // Registry: registrar + conectar (initialize + tools/list del servidor falso).
    {
        let srv = state.server.clone().expect("HttpState.server presente en harness");
        let tylluan_server = srv.write().await;
        tylluan_server
            .registry
            .write()
            .await
            .register_http_mcp(SERVER, &url, Default::default(), Some(5000));
        tylluan_server
            .registry
            .write()
            .await
            .ensure_guild_running(SERVER)
            .await
            .expect("el MCP falso debe conectar (initialize + tools/list)");
    }

    let app = build_router(state.clone());
    let mut failures: Vec<String> = Vec::new();

    // ── Fila 2: tool soberana no-tylluan_do → guild='kernel', tool='ponder' ──
    let (st2, body2) = mcp_call(&app, &jwt, "ponder", json!({ "thought": "mb7 fila2" })).await;
    if st2 != StatusCode::OK || mcp_error(&body2) {
        failures.push(format!(
            "FILA 2: ponder no devolvio exito (status={st2}): {}",
            body2.chars().take(400).collect::<String>()
        ));
    }
    match wait_row(
        &db,
        "SELECT intent, guild, tool_name, agent_id, status FROM guild_audit_log \
         WHERE tool_name = ?1 AND guild = 'kernel' LIMIT 1",
        "ponder",
    )
    .await
    {
        None => failures.push(
            "FILA 2: handlers.rs no escribio la fila de audit para una tool \
             soberana que no es tylluan_do (guild='kernel', tool='ponder'); \
             si la llamada `let _ = handler_do::log_audit_entry(\"\", \"kernel\"...)` \
             se elimino, esta linea es la prueba."
                .to_string(),
        ),
        Some((intent, guild, tool, agent, status)) => {
            println!("MB7 fila2 OK: intent={intent:?} guild={guild:?} tool={tool:?} agent={agent:?} status={status:?}");
        }
    }

    // ── Fila 4: MCP externo → guild='external_mcp:mb7srv', intent exacto ──
    // Sin guild_hint: resolve_guild_name debe fallar (gibberish, umbral 0.25)
    // y la fase 1 de try_external_mcp_dispatch debe ver el nombre del servidor
    // en el intent (intent_lower.contains(nombre)).
    let unique = format!(
        "{SERVER} zqxwjv flim {} {}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    let (st4, body4) = mcp_call(
        &app,
        &jwt,
        "tylluan_do",
        json!({ "intent": unique, "agent_id": AGENT }),
    )
    .await;
    match wait_row(
        &db,
        "SELECT intent, guild, tool_name, agent_id, status FROM guild_audit_log \
         WHERE intent = ?1 LIMIT 1",
        &unique,
    )
    .await
    {
        None => failures.push(format!(
            "FILA 4: external_mcp.rs no dejo fila de audit para el intent \
             '{unique}' (status tylluan_do={st4}, body={}); si la llamada \
             `let _ = log_audit_entry(...)` de do_external_call se elimino, \
             esta linea es la prueba",
            body4.chars().take(400).collect::<String>()
        )),
        Some((intent, guild, tool, agent, status)) => {
            let expected_guild = format!("external_mcp:{SERVER}");
            if intent != unique {
                failures.push(format!("FILA 4: intent en la fila ({intent:?}) != esperado ({unique:?})"));
            }
            if guild != expected_guild {
                failures.push(format!(
                    "FILA 4: guild en la fila ({guild:?}) != esperado ({expected_guild:?})"
                ));
            }
            if tool != FAKE_TOOL {
                failures.push(format!(
                    "FILA 4: tool en la fila ({tool:?}) != esperado ({FAKE_TOOL:?})"
                ));
            }
            if agent != AGENT {
                failures.push(format!(
                    "FILA 4: agent en la fila ({agent:?}) != esperado ({AGENT:?})"
                ));
            }
            if status != "ok" {
                failures.push(format!("FILA 4: la llamada al MCP falso deberia registrar status='ok' (got {status:?})"));
            }
            println!("MB7 fila4 OK: intent={intent:?} guild={guild:?} tool={tool:?} agent={agent:?} status={status:?}");
        }
    }

    assert!(
        failures.is_empty(),
        "M-BUFFY-7: la auditoria de T-DSH-67 no sobrevive en estas rutas:\n  - {}",
        failures.join("\n  - ")
    );
}
