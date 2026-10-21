//! T-DSH-22 — integración de los escritores `untrusted_content` de federación y A2A.
//!
//! D-18 declaró el hueco (IMPL_D-18, sección de huecos): `ingest_from_peer`
//! (`api_federation.rs`) y el nodo `a2a_result` (`a2a.rs`) marcan desconfianza,
//! pero no tenían test de integración propio. Este fichero los ejercita contra
//! el router real (`build_router`) y comprueba:
//!   1. lo que entra de un peer/agente externo queda con `untrusted_content=true`
//!      y `untrusted_reason`;
//!   2. un nodo local legítimo NO se marca;
//!   3. el banner aparece en las rutas que PUEDEN llevar ese contenido:
//!      `tylluan_think` (federación y A2A, verificadas); bootstrap y `@inbox` no
//!      llevan estos tipos de nodo (se documenta con asserts de ausencia);
//!   4. un peer no puede enviar `untrusted_content=false` para saltarse la marca.
//!   5. (D-27) si el peer manda `true` con un `untrusted_reason` inventado, el
//!      motivo lo fija el kernel: un peer no puede hacerse pasar por otro origen.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test untrusted_writers_integration

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::federation::FederationPeer;

const MASTER: &str = "master-t22-token";
const PEER: &str = "atlas-t22";
const PEER_TOKEN: &str = "peer-t22-token";
const SPACE: &str = "t22-space";
const BANNER: &str = "[CONTENIDO NO CONFIABLE";

fn peer() -> FederationPeer {
    FederationPeer {
        name: PEER.to_string(),
        url: "http://127.0.0.1:5999".to_string(),
        auth_token: PEER_TOKEN.to_string(),
        shared_secret: "t22-shared-secret-long-enough".to_string(),
        last_sync: None,
        approved: true,
        added_at: 0,
        ed25519_pubkey: String::new(), // peer legacy: ruta ChaCha (harness de federation_audit)
        external_address: String::new(),
        permissions: serde_json::from_str(
            r#"{"sync_spaces":[{"space":"t22-space","direction":"pull"}]}"#,
        )
        .unwrap(),
    }
}

fn fed_node(id: &str, content: &str, extra_meta: serde_json::Value) -> serde_json::Value {
    let mut meta = serde_json::json!({ "space": SPACE });
    if let Some(extra) = extra_meta.as_object() {
        for (k, v) in extra {
            meta[k] = v.clone();
        }
    }
    serde_json::json!({
        "id": id,
        "node_type": "document",
        "content": content,
        "metadata": meta.to_string(),
        "weight": 1.0,
        "protected": false,
        "conflicted": false,
        "topic_key": null,
        "created_at": null,
        "updated_at": null,
        "last_touched": "2026-10-21T00:00:00Z",
        "valid_from": null,
        "valid_until": null,
        "shareable": true
    })
}

/// Identidad de firma del lado peer (el kernel no la verifica aquí porque el
/// peer de test no publica pubkey; se genera una vez por proceso).
fn identity_for_signing() -> &'static tylluan_link::identity::NodeIdentity {
    static ID: std::sync::OnceLock<tylluan_link::identity::NodeIdentity> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        tylluan_link::identity::NodeIdentity::load_or_generate(
            &std::env::temp_dir().join(format!("tylluan_t22_peer_id_{}", std::process::id())),
        )
        .expect("peer identity")
    })
}

async fn receive(app: &axum::Router, nodes: &[serde_json::Value]) -> (StatusCode, String) {
    let envelopes: Vec<tylluan_link::identity::SignedEnvelope> = nodes
        .iter()
        .map(|n| tylluan_link::identity::sign_node(identity_for_signing(), n).unwrap())
        .collect();
    let plain = serde_json::to_vec(&envelopes).unwrap();
    let encrypted =
        tylluan_kernel::federation::encrypt_payload(&plain, peer().encryption_key()).unwrap();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/federation/sync/receive")
        .header(header::AUTHORIZATION, format!("Bearer {PEER_TOKEN}"))
        .header("content-type", "application/octet-stream")
        .body(Body::from(encrypted))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn mcp_call(
    app: &axum::Router,
    tool: &str,
    arguments: serde_json::Value,
) -> (StatusCode, String) {
    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
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

async fn post_memory_write(app: &axum::Router, content: &str) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/memory/write")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "content": content }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

async fn node_meta(
    state: &std::sync::Arc<tylluan_kernel::transport::http::HttpState>,
    id: &str,
) -> Option<serde_json::Value> {
    let node = state.silva.get_node(id).await.ok().flatten()?;
    serde_json::from_str(&node.metadata).ok()
}

/// `message/send` devuelve `{"result":{"id":"a2a_<hex>","state":"working"}}`.
fn task_id_of(resp: &str) -> String {
    serde_json::from_str::<serde_json::Value>(resp)
        .ok()
        .and_then(|v| v["result"]["id"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// El `a2a_result` lo escribe un spawn en background tras ejecutar el intent.
async fn wait_node_meta(
    state: &std::sync::Arc<tylluan_kernel::transport::http::HttpState>,
    id: &str,
) -> Option<serde_json::Value> {
    for _ in 0..60 {
        if let Some(m) = node_meta(state, id).await {
            return Some(m);
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

#[tokio::test(flavor = "multi_thread")]
async fn untrusted_writers_federation_and_a2a() {
    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();
    let state = support::test_state_custom("t22", Some(MASTER.to_string()), false).await;
    state.peer_db.insert(&peer()).unwrap();
    state.config.write().await.federation_peers.push(peer());
    let app = tylluan_kernel::transport::http::build_router(state.clone());

    let mut failures: Vec<String> = Vec::new();
    let mut table: Vec<String> = Vec::new();

    // ── F1: nodo federado normal queda marcado con flag + reason ────────────
    let (st, body) = receive(
        &app,
        &[fed_node(
            "t22-fed-1",
            "FEDERATED-T22 contenido externo",
            serde_json::json!({}),
        )],
    )
    .await;
    let meta = node_meta(&state, "t22-fed-1").await;
    let ok_mark = meta
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    let ok_reason = meta
        .as_ref()
        .and_then(|m| m["untrusted_reason"].as_str())
        .map(|r| r == format!("federation_peer:{PEER}"))
        .unwrap_or(false);
    if st != StatusCode::OK || !ok_mark || !ok_reason {
        failures.push(format!(
            "F1 federado: status={st} body={body} mark={ok_mark} reason={ok_reason} meta={meta:?}"
        ));
    }
    table.push(format!(
        "| federation ingest | nodo de peer normal | flag=true + reason=federation_peer:{PEER} | status={st} mark={ok_mark} reason={ok_reason} |"
    ));

    // ── F2: nodo local legítimo NO se marca ─────────────────────────────────
    let (st2, body2) =
        post_memory_write(&app, "LOCAL-T22 contenido legitimo de la flota").await;
    let local_id = body2
        .split("\"node_id\":\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("")
        .to_string();
    let local_meta = node_meta(&state, &local_id).await;
    let local_marked = local_meta
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    if st2 != StatusCode::OK || local_marked || local_id.is_empty() {
        failures.push(format!(
            "F2 local: status={st2} id='{local_id}' marcado={local_marked}"
        ));
    }
    table.push(format!(
        "| federation/local | nodo local legitimo (memory/write) | NO marcado | status={st2} marcado={local_marked} |"
    ));

    // ── F3: peer intenta saltarse la marca con untrusted_content=false ──────
    let (st3, body3) = receive(
        &app,
        &[fed_node(
            "t22-fed-false",
            "FEDERATED-T22 intento de bypass",
            serde_json::json!({"untrusted_content": false, "untrusted_reason": "fake"}),
        )],
    )
    .await;
    let meta3 = node_meta(&state, "t22-fed-false").await;
    let bypass_mark = meta3
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    let bypass_reason = meta3
        .as_ref()
        .and_then(|m| m["untrusted_reason"].as_str())
        .map(|r| r == format!("federation_peer:{PEER}"))
        .unwrap_or(false);
    if st3 != StatusCode::OK || !bypass_mark || !bypass_reason {
        failures.push(format!(
            "F3 bypass: status={st3} body={body3} mark={bypass_mark} reason={bypass_reason} meta={meta3:?}"
        ));
    }
    table.push(format!(
        "| federation ingest | peer envia untrusted_content=false | forzado a true + reason del kernel | status={st3} mark={bypass_mark} reason_reescrito={bypass_reason} |"
    ));

    // -- F4 (D-27): peer envia true + motivo falso -> el motivo lo fija el kernel --
    let (st4, body4) = receive(
        &app,
        &[fed_node(
            "t22-fed-forged",
            "FEDERATED-T22 reason falsificado",
            serde_json::json!({"untrusted_content": true, "untrusted_reason": "forged by peer"}),
        )],
    )
    .await;
    let meta4 = node_meta(&state, "t22-fed-forged").await;
    let forged_mark = meta4
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    let forged_reason = meta4
        .as_ref()
        .and_then(|m| m["untrusted_reason"].as_str())
        .unwrap_or("");
    let kernel_reason = format!("federation_peer:{PEER}");
    if st4 != StatusCode::OK || !forged_mark || forged_reason != kernel_reason {
        failures.push(format!(
            "F4 motivo forjado (D-27): status={st4} body={body4} mark={forged_mark} reason='{forged_reason}' esperado='{kernel_reason}' meta={meta4:?}"
        ));
    }
    table.push(format!(
        "| federation ingest | peer envia true + motivo falso | flag=true y motivo del kernel (D-27) | status={st4} reason='{forged_reason}' |"
    ));

    // ── F5: banner en tylluan_think ─────────────────────────────────────────
    let (st5, body5) = mcp_call(
        &app,
        "tylluan_think",
        serde_json::json!({ "query": "FEDERATED-T22 contenido externo" }),
    )
    .await;
    let banner_think = body5.contains(BANNER) && body5.contains(&format!("federation_peer:{PEER}"));
    if !banner_think {
        failures.push(format!(
            "F5 think: sin banner de federacion (status={st5}): {}",
            body5.chars().take(300).collect::<String>()
        ));
    }
    table.push(format!(
        "| tylluan_think | contenido federado marcado | banner con origen federation_peer | banner={banner_think} |"
    ));

    // ── F6: bootstrap y @inbox no llevan estos tipos (banner N/A) ───────────
    let boot = tylluan_kernel::transport::server::bootstrap::build_agent_bootstrap(
        state.silva.clone(),
        "agent-t22",
    )
    .await
    .to_string();
    let boot_has = boot.contains("FEDERATED-T22");
    let (st6, inbox) = mcp_call(
        &app,
        "tylluan_recall",
        serde_json::json!({ "query": "@inbox" }),
    )
    .await;
    let inbox_has = inbox.contains("FEDERATED-T22");
    if boot_has || inbox_has {
        failures.push(format!(
            "F6: bootstrap_has={boot_has} inbox_has={inbox_has} — si el contenido llega debe llevar banner"
        ));
    }
    table.push(format!(
        "| bootstrap / @inbox | contenido federado | NO llevan nodos federados (N/A, no hay ruta) | bootstrap_has={boot_has} inbox_status={st6} inbox_has={inbox_has} |"
    ));

    // ── A1: A2A message/send → a2a_result marcado ───────────────────────────
    let a2a_body = serde_json::json!({
        "jsonrpc": "2.0", "id": 7, "method": "message/send",
        "params": { "agent_id": "a2a-t22", "intent": "whoami" }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/a2a")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(a2a_body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    let a2a_resp = String::from_utf8_lossy(&bytes).to_string();
    let a2a_ok = status == StatusCode::OK && !a2a_resp.contains("\"error\"");
    // message/send responde `result.id` y ejecuta en background: el nodo
    // `a2a:<task>:result` aparece después del intent, así que se espera.
    let task_id = task_id_of(&a2a_resp);
    let a2a_node_id = if task_id.is_empty() {
        String::new()
    } else {
        format!("a2a:{task_id}:result")
    };
    let a2a_meta = if a2a_node_id.is_empty() {
        None
    } else {
        wait_node_meta(&state, &a2a_node_id).await
    };
    let a2a_mark = a2a_meta
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    let a2a_reason = a2a_meta
        .as_ref()
        .and_then(|m| m["untrusted_reason"].as_str())
        .map(|r| r == "a2a external agent: a2a-t22")
        .unwrap_or(false);
    if !a2a_ok || !a2a_mark || !a2a_reason {
        failures.push(format!(
            "A1 a2a: status={status} ok={a2a_ok} task='{task_id}' mark={a2a_mark} reason={a2a_reason} resp={}",
            a2a_resp.chars().take(300).collect::<String>()
        ));
    }
    table.push(format!(
        "| A2A message/send | resultado de agente externo | nodo a2a_result con flag=true + reason='a2a external agent: a2a-t22' | status={status} task='{task_id}' mark={a2a_mark} reason={a2a_reason} |"
    ));

    // ── A2: el payload no puede inyectar la marca en false ─────────────────
    let a2a2_body = serde_json::json!({
        "jsonrpc": "2.0", "id": 8, "method": "message/send",
        "params": {
            "agent_id": "a2a-t22",
            "intent": "whoami",
            "untrusted_content": false,
            "metadata": "{\"untrusted_content\":false}"
        }
    });
    let req = Request::builder()
        .method("POST")
        .uri("/a2a")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(a2a2_body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    let a2a2_resp = String::from_utf8_lossy(&bytes).to_string();
    let task2 = task_id_of(&a2a2_resp);
    let a2a2_meta = if task2.is_empty() {
        None
    } else {
        wait_node_meta(&state, &format!("a2a:{task2}:result")).await
    };
    let a2a2_mark = a2a2_meta
        .as_ref()
        .map(|m| m["untrusted_content"] == true)
        .unwrap_or(false);
    if !a2a2_mark {
        failures.push(format!(
            "A2 a2a payload false: el nodo no quedo marcado (task='{task2}') resp={}",
            a2a2_resp.chars().take(300).collect::<String>()
        ));
    }
    table.push(format!(
        "| A2A message/send | payload intenta untrusted_content=false | sigue marcado true | task='{task2}' mark={a2a2_mark} |"
    ));

    // ── A3: banner del resultado A2A en las rutas que pueden llevarlo ───────
    // El nodo `a2a_result` no sale con una query genérica; se busca por su
    // contenido real con `tylluan_recall` (ruta 1 del mapa B-8) y con
    // `tylluan_think`. Se documenta qué ruta lo surfacea y si lo hace CON banner.
    let a2a_content = if a2a_node_id.is_empty() {
        String::new()
    } else {
        state
            .silva
            .get_node(&a2a_node_id)
            .await
            .ok()
            .flatten()
            .map(|n| n.content)
            .unwrap_or_default()
    };
    let a2a_query: String = a2a_content
        .split_whitespace()
        .take(10)
        .collect::<Vec<_>>()
        .join(" ");
    let (st7, body7) = mcp_call(
        &app,
        "tylluan_think",
        serde_json::json!({ "query": a2a_query }),
    )
    .await;
    let think_has = body7.contains("a2a external agent");
    let think_banner = think_has && body7.contains(BANNER);
    if think_has && !think_banner {
        failures.push(format!(
            "A3 think: el contenido A2A sale SIN banner: {}",
            body7.chars().take(300).collect::<String>()
        ));
    }
    table.push(format!(
        "| tylluan_think | resultado A2A marcado (query = contenido) | banner con origen a2a external agent | surfacing={think_has} banner={think_banner} status={st7} |"
    ));

    let (st8, body8) = mcp_call(
        &app,
        "tylluan_recall",
        serde_json::json!({ "query": a2a_query }),
    )
    .await;
    let recall_has = body8.contains("a2a external agent");
    let recall_banner = recall_has && body8.contains(BANNER);
    if recall_has && !recall_banner {
        failures.push(format!(
            "A3 recall: el contenido A2A sale SIN banner: {}",
            body8.chars().take(300).collect::<String>()
        ));
    }
    table.push(format!(
        "| tylluan_recall | resultado A2A marcado (query = contenido) | banner con origen a2a external agent | surfacing={recall_has} banner={recall_banner} status={st8} |"
    ));

    println!("\n=== MATRIZ T-DSH-22 ===");
    println!("| ruta | caso | esperado | real |");
    for line in &table {
        println!("{line}");
    }

    assert!(
        failures.is_empty(),
        "T-DSH-22 — {} caso(s) incumplido(s):\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}
