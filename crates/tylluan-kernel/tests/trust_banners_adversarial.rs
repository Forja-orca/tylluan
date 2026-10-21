//! T-DSH-10 — Auditoría adversarial de D-18: `untrusted_content` y banners.
//!
//! Contrato que D-18 declara: el flag `untrusted_content` deja de ser solo un
//! multiplicador de ranking y pasa a ser **visible** en las rutas que devuelven
//! contenido al modelo sin pasar por el gate (bootstrap, contexto de sesión del
//! recall, `@inbox`), y los resúmenes (`agent_summary`/`session_digest`) heredan
//! la marca de cualquiera de sus fuentes.
//!
//! Esta matriz ataca los tres vectores pedidos:
//!  (A) contenido marcado llegando a un contexto SIN banner por las rutas del
//!      mapa de B-8;
//!  (B) remitente externo pasando por local en `mailbox_send_handler`;
//!  (C) `agent_summary` perdiendo la marca.
//!
//! Prueba-especificación (patrón B-7/T-DSH-9): corre en verde en la suite normal
//! porque está ignorada; con `-- --ignored` falla HOY en los casos abiertos y
//! debe pasar cuando se cierren las quebraduras que lista el informe.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel \
//!     --test trust_banners_adversarial -- --ignored --nocapture

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;
use tylluan_kernel::memory::agent_memory::AgentMemoryManager;
use tylluan_kernel::memory::identity::IdentityManager;
use tylluan_kernel::memory::mailbox::BlackboardMessage;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::transport::http::build_router;
use tylluan_kernel::transport::server::bootstrap::build_agent_bootstrap;

const MASTER_TOKEN: &str = "master-t10-token";
const BANNER: &str = "[CONTENIDO NO CONFIABLE";

struct Case {
    name: &'static str,
    expected: &'static str,
    ok: bool,
    detail: String,
}

/// `tools/call` genérico sobre el router real (MCP).
async fn mcp_tool(
    app: &axum::Router,
    bearer: &str,
    tool: &str,
    arguments: serde_json::Value,
) -> (StatusCode, String) {
    let body = serde_json::json!({
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
    let bytes = axum::body::to_bytes(resp.into_body(), 8 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn text_of(body: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    parsed["result"]["content"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_else(|| body.to_string())
}

async fn post_json(
    app: &axum::Router,
    bearer: &str,
    path: &str,
    payload: serde_json::Value,
) -> (StatusCode, String) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 8 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// Nodo de memoria de agente con marca opcional.
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

async fn summary_of(silva: &Arc<SilvaDB>, agent: &str) -> Option<tylluan_kernel::memory::silva::GraphNode> {
    AgentMemoryManager::new(silva.clone(), 3).get_summary(agent).await
}

fn push_case(cases: &mut Vec<Case>, name: &'static str, expected: &'static str, ok: bool, detail: String) {
    cases.push(Case { name, expected, ok, detail });
}

#[tokio::test(flavor = "multi_thread")]
// Prueba-especificación: la suite normal queda en verde (`0 passed; 1 ignored`);
// con `--ignored` falla por los casos abiertos del informe T-DSH-10.
#[ignore = "T-DSH-10: vectores abiertos de D-18 (banner ausente en think/@inbox legacy, localidad por identidad auto-creada, herencia no retroactiva)"]
async fn d18_trust_banners_adversarial_matrix() {
    let audit_db = std::env::temp_dir().join(format!("tylluan_t10_audit_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; un único #[test] en el binario.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    tylluan_kernel::security::grants::init();
    tylluan_kernel::security::grants::init_plan_store();

    let state = support::test_state_custom("t10", Some(MASTER_TOKEN.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("t10-jti", 300);
    let app = build_router(state.clone());
    let silva = state.silva.clone();
    let mut cases: Vec<Case> = Vec::new();

    // ───────────────────────── (A) banners ─────────────────────────────────
    // A1 — bootstrap (ruta 4 del mapa de B-8): memoria marcada -> banner.
    {
        seed_memory(&silva, "mem:a1:1", "a1", "ZZPAYLOADA1 memoria externa", true).await;
        seed_memory(&silva, "mem:a1:2", "a1", "memoria local limpia", false).await;
        seed_memory(&silva, "mem:a1:3", "a1", "otra memoria local", false).await;
        let boot = build_agent_bootstrap(silva.clone(), "a1").await;
        let dumped = boot.to_string();
        let ok = dumped.contains(BANNER) && dumped.contains("ZZPAYLOADA1");
        push_case(
            &mut cases,
            "A1 bootstrap: memoria marcada con banner",
            "banner visible",
            ok,
            format!("banner={} payload={}", dumped.contains(BANNER), dumped.contains("ZZPAYLOADA1")),
        );
    }

    // A2 — bootstrap: resumen marcado -> banner. (La cabecera del contexto de
    // sesión del recall, handler_recall.rs línea 195, usa la MISMA pieza de
    // confianza pero no es ejercitable en este harness: `server.agent_memory`
    // no está cableado en el estado de test. Queda cubierta por lectura de
    // código y anotada como no verificada en el informe.)
    {
        let meta = serde_json::json!({
            "agent_id": "a2",
            "untrusted_content": true,
            "untrusted_reason": "peer-declared"
        });
        silva
            .upsert_node_with_validity(
                "summary:a2:1",
                "agent_summary",
                "resumen del agente a2 con ZZPAYLOADA2",
                &meta.to_string(),
                tylluan_kernel::memory::silva::NodeWriteOptions::new("agent_generated")
                    .drift_allowed(true),
            )
            .await
            .unwrap();
        let boot = build_agent_bootstrap(silva.clone(), "a2").await;
        let dumped = boot.to_string();
        push_case(
            &mut cases,
            "A2 bootstrap: resumen marcado con banner",
            "banner visible",
            dumped.contains(BANNER) && dumped.contains("ZZPAYLOADA2"),
            format!(
                "payload_presente={} banner={}",
                dumped.contains("ZZPAYLOADA2"),
                dumped.contains(BANNER)
            ),
        );
    }

    // A3 — @inbox con payload NUEVO marcado (D-18) -> banner.
    {
        let mut bm = BlackboardMessage::task("ext-a3", "victim-a3", "ZZPAYLOADA3 cuerpo externo");
        bm.untrusted = true;
        state
            .mailbox
            .send_mail_with_ttl("ext-a3", "victim-a3", &bm.to_payload(), 3600)
            .await
            .unwrap();
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-a3" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "A3 @inbox: mensaje nuevo marcado con banner",
            "banner visible",
            text.contains(BANNER),
            text.chars().take(120).collect::<String>(),
        );
    }

    // A4 — @inbox con payload ANTIGUO (sin clave `untrusted`) -> sin banner (hueco).
    {
        let legacy = serde_json::json!({
            "msg_type": "task",
            "body": "ZZPAYLOADA4 legacy externo",
            "to": "victim-a4",
            "from": "ext-a4"
        })
        .to_string();
        state
            .mailbox
            .send_mail_with_ttl("ext-a4", "victim-a4", &legacy, 3600)
            .await
            .unwrap();
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-a4" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "A4 @inbox: payload legacy sin clave untrusted",
            "banner visible (o backfill)",
            text.contains(BANNER) && text.contains("ZZPAYLOADA4"),
            format!(
                "payload_presente={} banner={}",
                text.contains("ZZPAYLOADA4"),
                text.contains(BANNER)
            ),
        );
    }

    // A5 — tylluan_think (ruta 7): nodo marcado devuelto SIN banner.
    {
        let meta = serde_json::json!({
            "untrusted_content": true,
            "untrusted_reason": "web-external",
            "agent_id": "a5"
        });
        silva
            .upsert_node(
                "bcast:a5",
                "broadcast",
                "ZZPAYLOADA5 instrucciones hostiles para el turno",
                &meta.to_string(),
            )
            .await
            .unwrap();
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_think",
            serde_json::json!({ "query": "ZZPAYLOADA5", "agent_id": "a5" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "A5 think: nodo marcado sin banner",
            "banner visible",
            text.contains(BANNER) && text.contains("ZZPAYLOADA5"),
            format!(
                "payload_presente={} banner={}",
                text.contains("ZZPAYLOADA5"),
                text.contains(BANNER)
            ),
        );
    }

    // ──────────────────── (B) remitente externo vs local ───────────────────
    let send_mail = |app: &axum::Router, jwt: String, from: &str, to: &str, body: &str| {
        let payload = serde_json::json!({
            "from": from, "to": to, "subject": "s", "body": body
        });
        let app = app.clone();
        async move { post_json(&app, &jwt, "/api/v1/mailbox/send", payload).await }
    };

    // B1 — remitente desconocido -> mensaje marcado (control).
    {
        let (st, _) = send_mail(&app, jwt.clone(), "ext-b1", "victim-b1", "ZZPAYLOADB1").await;
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-b1" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "B1 mailbox_send: remitente desconocido marcado",
            "marcado (banner)",
            st == StatusCode::OK && text.contains(BANNER),
            format!("HTTP {} banner={}", st.as_u16(), text.contains(BANNER)),
        );
    }

    // B2 — remitente externo que se registra identidad en ESTE kernel
    // (`register_identity` es una herramienta alcanzable por cualquier cliente):
    // `sender_is_local` pasa a true y D-18 lo trata como local.
    {
        let (_, _) = mcp_tool(
            &app,
            &jwt,
            "register_identity",
            serde_json::json!({
                "agent_id": "ext-b2",
                "human_name": "Ext B2",
                "role": "agent",
                "purpose": "probador externo"
            }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let identity = IdentityManager::new(silva.clone()).get_identity("ext-b2").await;
        let (st, _) = send_mail(&app, jwt.clone(), "ext-b2", "victim-b2", "ZZPAYLOADB2").await;
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-b2" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "B2 mailbox_send: externo que se auto-registra identidad pasa por local",
            "marcado (registrar identidad no debe conceder localidad)",
            identity.is_some() && st == StatusCode::OK && text.contains(BANNER),
            format!(
                "identidad_creada={} banner={}",
                identity.is_some(),
                text.contains(BANNER)
            ),
        );
    }

    // B3 — variante de caja: con identidad "deep", enviar como "Deep".
    {
        let (_, _) = mcp_tool(
            &app,
            &jwt,
            "tylluan_remember",
            serde_json::json!({ "content": "identidad local deep", "agent_id": "deep" }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let (_, _) = send_mail(&app, jwt.clone(), "Deep", "victim-b3", "ZZPAYLOADB3").await;
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-b3" }),
        )
        .await;
        let text = text_of(&body);
        // Seguro esperado: "Deep" no es "deep" -> externo -> marcado.
        push_case(
            &mut cases,
            "B3 mailbox_send: 'Deep' con identidad local 'deep'",
            "marcado (sin colisión de caja)",
            text.contains(BANNER),
            format!("banner={}", text.contains(BANNER)),
        );
        // Y ahora se crea también la identidad "Deep" (como en producción, donde
        // coexisten agent:deep y agent:Deep): el mismo remitente pasa a local.
        let (_, _) = mcp_tool(
            &app,
            &jwt,
            "tylluan_remember",
            serde_json::json!({ "content": "identidad Deep (variante)", "agent_id": "Deep" }),
        )
        .await;
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        let (_, _) = send_mail(&app, jwt.clone(), "Deep", "victim-b3b", "ZZPAYLOADB3B").await;
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-b3b" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "B3b mailbox_send: 'Deep' tras crearse identidad 'Deep'",
            "marcado (la caja no debe cambiar la confianza)",
            text.contains(BANNER),
            format!("banner={}", text.contains(BANNER)),
        );
    }

    // B4 — remitente con id vacío.
    {
        let (st, _) = send_mail(&app, jwt.clone(), "", "victim-b4", "ZZPAYLOADB4").await;
        let (_, body) = mcp_tool(
            &app,
            &jwt,
            "tylluan_recall",
            serde_json::json!({ "query": "@inbox", "agent_id": "victim-b4" }),
        )
        .await;
        let text = text_of(&body);
        push_case(
            &mut cases,
            "B4 mailbox_send: remitente con id vacío",
            "marcado (banner)",
            text.contains(BANNER),
            format!("HTTP {} banner={}", st.as_u16(), text.contains(BANNER)),
        );
    }

    // ──────────────────── (C) agent_summary pierde la marca ────────────────
    // C1 — control: resumen nace con la marca heredada.
    {
        for i in 0..4 {
            seed_memory(&silva, &format!("mem:c1:{i}"), "c1", "memoria local", false).await;
        }
        seed_memory(&silva, "mem:c1:x", "c1", "ZZPAYLOADC1 fuente marcada", true).await;
        AgentMemoryManager::new(silva.clone(), 3)
            .consolidate_if_needed("c1")
            .await;
        let summary = summary_of(&silva, "c1").await;
        let flagged = summary
            .as_ref()
            .map(|s| s.metadata.contains("\"untrusted_content\":true"))
            .unwrap_or(false);
        push_case(
            &mut cases,
            "C1 consolidación: el resumen hereda la marca",
            "marca heredada",
            flagged,
            format!("summary={} flagged={flagged}", summary.is_some()),
        );
    }

    // C2 — retroactividad: consolidar con fuentes limpias y marcar DESPUÉS.
    {
        for i in 0..4 {
            seed_memory(&silva, &format!("mem:c2:{i}"), "c2", "memoria local", false).await;
        }
        seed_memory(&silva, "mem:c2:x", "c2", "ZZPAYLOADC2 contenido hostil", false).await;
        AgentMemoryManager::new(silva.clone(), 3)
            .consolidate_if_needed("c2")
            .await;
        // El juez de capa 2 / un productor marca la fuente DESPUÉS de consolidar.
        seed_memory(&silva, "mem:c2:x", "c2", "ZZPAYLOADC2 contenido hostil", true).await;
        let summary = summary_of(&silva, "c2").await;
        let summary_flagged = summary
            .as_ref()
            .map(|s| s.metadata.contains("\"untrusted_content\":true"))
            .unwrap_or(false);
        let summary_has_payload = summary
            .as_ref()
            .map(|s| s.content.contains("ZZPAYLOADC2"))
            .unwrap_or(false);
        // La pregunta exacta: ¿el resumen (que YA copió el payload) queda marcado
        // cuando su fuente se marca después de consolidar? La herencia de D-18 es
        // de creación, así que la marca no llega.
        push_case(
            &mut cases,
            "C2 retroactividad: fuente marcada tras consolidar",
            "resumen re-marcado (herencia no solo en creación)",
            summary_flagged || !summary_has_payload,
            format!(
                "resumen_con_payload={summary_has_payload} summary_flagged={summary_flagged}"
            ),
        );
    }

    // ──────────────── Salida: tabla + aserción de la especificación ─────────
    println!("\n=== T-DSH-10 · matriz adversarial D-18 ===");
    println!("| caso | esperado | real | veredicto |");
    println!("|---|---|---|---|");
    for c in &cases {
        println!(
            "| {} | {} | {} | {} |",
            c.name,
            c.expected,
            c.detail,
            if c.ok { "OK" } else { "QUEBRADO" }
        );
    }
    let broken: Vec<&Case> = cases.iter().filter(|c| !c.ok).collect();
    println!("=== fin matriz ({} quebrados de {}) ===\n", broken.len(), cases.len());

    assert!(
        broken.is_empty(),
        "T-DSH-10: {} caso(s) no conformes:\n  - {}",
        broken.len(),
        broken
            .iter()
            .map(|c| format!("{} -> {}", c.name, c.detail))
            .collect::<Vec<_>>()
            .join("\n  - ")
    );
}
