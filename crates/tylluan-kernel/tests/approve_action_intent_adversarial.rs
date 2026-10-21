//! T-DSH-9 — Auditoría adversarial de D-16: ruta de intent `tylluan_do`
//! → `approve_action` (`plan_id_from_approve_intent` en
//! `transport/server/handler_do/mod.rs`).
//!
//! Contrato observado (post-D-16): si el intent es corto (≤ 60 chars) y
//! contiene el literal `approve action for plan <id>` (o la variante corta
//! `approve plan <id>`), la llamada se enruta a `approve_action` con
//! `requestId=<id>` y `approved=true`. El parser busca con `split_once` sobre
//! el intent ya *lowercased*, así que:
//!
//!   * el literal puede aparecer en CUALQUIER posición (no anclado al inicio);
//!   * el id se filtra a `[A-Za-z0-9_-]` ASCII tras `trim_start()`,
//!     truncando en el primer carácter fuera de ese conjunto;
//!   * un id vacío no enruta.
//!
//! Matriz de 15 intents (5 dimensiones pedidas: mayúsculas, espacios,
//! prefijos, ids con `-`/`_`, unicode) que NO deberían aprobar un plan, más
//! filas de control y un caso extra de suplantación de `agent_id`.
//!
//! Fixture: cada plan lo firma `plan-author`; el llamante declara
//! `agent_id = attacker-agent` (ni autor ni operador). El guild del plan no
//! existe a propósito: la ejecución falla, pero el plan se consume igual, que
//! es la señal observable de que la ruta llegó a `approve_action`. Un
//! `requestId` sin plan devuelve `Action not found.` (handlers.rs), segunda
//! señal de que hubo ruteo.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel \
//!     --test approve_action_intent_adversarial -- --nocapture
//!
//! Requiere el patch de D-16 aplicado: sin él la ruta no existe y la fila de
//! control falla (las adversarias "no aprueban" por ausencia de ruta, no por
//! la compuerta).

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::build_router;

const MASTER_TOKEN: &str = "master-t9-token";
const AUTHOR: &str = "plan-author";
const ATTACKER: &str = "attacker-agent";
const PROBE_GUILD: &str = "t9_probe_guild"; // inexistente a propósito
const PROBE_TOOL: &str = "t9_tool";

#[derive(Clone, Copy, PartialEq)]
enum Expect {
    /// Debe aprobarse (forma documentada / variante benigna / charset válido).
    Approved,
    /// NO debe aprobarse (vector adversarial o superficie no documentada).
    NotApproved,
}

struct Row {
    label: &'static str,
    intent: String,
    plan_id: Option<&'static str>,
    expect: Expect,
}

/// `tylluan_do(intent=<intent>)` sobre el router real (MCP tools/call).
async fn do_call(
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

/// Señal de que la petición llegó a `approve_action` sin depender del consumo
/// del plan (id equivocado, plan inexistente, rechazo por compuerta).
fn routed_to_approve(body: &str) -> bool {
    body.contains("Plan guild")
        || body.contains("Action not found.")
        || body.contains("self_approval")
        || body.contains("operator_required")
        || body.contains("request_id")
}

async fn store_probe_plan(id: &str, author: &str) {
    grants::store_plan(
        id,
        PROBE_GUILD,
        PROBE_TOOL,
        &serde_json::json!({}),
        author,
        "T-DSH-9 fixture",
    )
    .await;
}

/// Extrae el texto del primer `content[].text` de la respuesta MCP, plano y
/// recortado, para la columna "respuesta real" del informe.
fn response_text(body: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let text = parsed["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c["text"].as_str())
        .map(|s| s.to_string())
        .or_else(|| parsed["error"]["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| body.to_string());
    let flat = text.replace(['\n', '\r'], " ");
    let flat = flat.trim();
    if flat.chars().count() > 90 {
        format!("{}…", flat.chars().take(90).collect::<String>())
    } else {
        flat.to_string()
    }
}

#[tokio::test(flavor = "multi_thread")]
// Prueba-especificación (mismo patrón que el B-7 de Buffy): corre en verde en
// la suite normal porque está ignorada; con `-- --ignored` falla HOY por la
// razón correcta (9 filas no conformes) y debe pasar cuando se endurezca el
// parser. No rompe CI mientras el hallazgo no esté decidido.
#[ignore = "T-DSH-9 X1: abierto — agent_id declarativo permite al autor aprobar su plan; requiere binding token->agent_id (decisión de José). F1,F3 ya corregidos"]
async fn d16_intent_route_adversarial_matrix() {
    // Aislar el audit store: una aprobación real escribe en guild_audit_log.
    // Nunca debe tocar la base de producción.
    let audit_db = std::env::temp_dir().join(format!(
        "tylluan_t9_audit_{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&audit_db);
    // SAFETY (set_var): edition-2024 unsafe op; este binario contiene un único
    // #[test], no hay acceso concurrente al entorno.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &audit_db) };

    grants::init();
    grants::init_plan_store();

    let state = support::test_state_custom("t9", Some(MASTER_TOKEN.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("t9-jti", 300);
    let app = build_router(state);

    // ── Fixtures de plan ────────────────────────────────────────────────────
    // Ids con el mismo alfabeto que la producción (12 hex uuid v4 recortado),
    // más uno con '-' y '_' para la dimensión de charset.
    // Un plan por fila: si dos filas comparten plan, la primera que lo consume
    // invalida la medición de la siguiente.
    let documented = "a10000010001";
    let upper = "a20000020002";
    let r03 = "b30000030003";
    let r04 = "b40000040004";
    let r05 = "b50000050005";
    let r06 = "b60000060006";
    let r07 = "b70000070007";
    let r08 = "b80000080008";
    let r09 = "b90000090009";
    let r10 = "c10000100001";
    let hyphen_underscore = "ab-cd_ef1001";
    let r14 = "b14000140004";
    let shared = "deadbeef0003"; // filas 12-13: no deben consumirlo
    let spoof_id = "e10000130003";
    for id in [
        documented,
        upper,
        r03,
        r04,
        r05,
        r06,
        r07,
        r08,
        r09,
        r10,
        hyphen_underscore,
        r14,
        shared,
        spoof_id,
    ] {
        store_probe_plan(id, AUTHOR).await;
    }

    // ── 15 intents que NO deberían aprobar (más 2 controles) ───────────────
    let rows = vec![
        // Control positivo: la forma documentada por el kernel en modo plan.
        Row {
            label: "01 CONTROL documentada",
            intent: format!("approve action for plan {documented}"),
            plan_id: Some(documented),
            expect: Expect::Approved,
        },
        // Control benigno: el parser lowercasea; con ids hex minúsculas el
        // efecto es el mismo id. No es un bypass, pero se mide.
        Row {
            label: "02 MAYÚSCULAS",
            intent: format!(
                "APPROVE ACTION FOR PLAN {}",
                upper.to_uppercase()
            ),
            plan_id: Some(upper),
            expect: Expect::Approved,
        },
        // ── Negaciones / menciones (el literal aparece como substring) ──────
        Row {
            label: "03 negación 'reject'",
            intent: format!("reject approve action for plan {r03}"),
            plan_id: Some(r03),
            expect: Expect::NotApproved,
        },
        Row {
            label: "04 negación 'no'",
            intent: format!("no approve action for plan {r04}"),
            plan_id: Some(r04),
            expect: Expect::NotApproved,
        },
        Row {
            label: "05 negación 'do not'",
            intent: format!("do not approve action for plan {r05}"),
            plan_id: Some(r05),
            expect: Expect::NotApproved,
        },
        Row {
            label: "06 'cancel'",
            intent: format!("cancel approve action for plan {r06}"),
            plan_id: Some(r06),
            expect: Expect::NotApproved,
        },
        Row {
            label: "07 'disapprove'",
            intent: format!("disapprove action for plan {r07}"),
            plan_id: Some(r07),
            expect: Expect::NotApproved,
        },
        Row {
            label: "08 mención 'explain'",
            intent: format!("explain approve action for plan {r08}"),
            plan_id: Some(r08),
            expect: Expect::NotApproved,
        },
        Row {
            label: "09 JSON pegado",
            intent: format!("{{\"intent\":\"approve action for plan {r09}\"}}"),
            plan_id: Some(r09),
            expect: Expect::NotApproved,
        },
        Row {
            label: "10 variante corta 'approve plan'",
            intent: format!("approve plan {r10}"),
            plan_id: Some(r10),
            expect: Expect::NotApproved, // superficie no documentada por el kernel
        },
        // ── Dimensión ids con '-' / '_' (debe funcionar: charset permitido) ─
        Row {
            label: "11 id con '-' y '_'",
            intent: format!("approve action for plan {hyphen_underscore}"),
            plan_id: Some(hyphen_underscore),
            expect: Expect::Approved,
        },
        // ── Dimensión prefijos de id ───────────────────────────────────────
        Row {
            label: "12 id truncado (prefijo)",
            intent: "approve action for plan dead".to_string(),
            plan_id: Some(shared),
            expect: Expect::NotApproved,
        },
        // ── Dimensión unicode (homoglifo cirílico 'а' U+0430) ──────────────
        Row {
            label: "13 homoglifo cirílico",
            intent: "approve action for plan de\u{0430}dbeef0003".to_string(),
            plan_id: Some(shared),
            expect: Expect::NotApproved,
        },
        // ── Dimensión espacios (prefijo partido: no debe enrutar) ──────────
        Row {
            label: "14 espacios dobles en prefijo",
            intent: format!("approve  action for plan {r14}"),
            plan_id: Some(r14),
            expect: Expect::NotApproved,
        },
        // ── Sin id: el parser debe devolver None ───────────────────────────
        Row {
            label: "15 sin id",
            intent: "approve action for plan".to_string(),
            plan_id: None,
            expect: Expect::NotApproved,
        },
    ];

    let mut failures: Vec<String> = Vec::new();
    let mut table: Vec<(String, String, usize, bool, bool, String, &'static str)> = Vec::new();

    for row in &rows {
        let (st, body) = do_call(&app, &jwt, &row.intent, Some(ATTACKER)).await;
        let consumed = match row.plan_id {
            Some(id) => grants::get_plan(id).await.is_none(),
            None => false,
        };
        let routed = consumed || routed_to_approve(&body);
        let ok = match row.expect {
            Expect::Approved => consumed,
            Expect::NotApproved => !consumed,
        };
        if !ok {
            failures.push(format!(
                "{}: esperado {:?}, plan consumido={consumed} (routed={routed}) · HTTP {} · {}",
                row.label,
                row.expect as u8,
                st.as_u16(),
                response_text(&body)
            ));
        }
        table.push((
            row.label.to_string(),
            row.intent.clone(),
            row.intent.chars().count(),
            consumed,
            routed,
            response_text(&body),
            match row.expect {
                Expect::Approved => "aprobado",
                Expect::NotApproved => "NO aprobado",
            },
        ));
    }

    // ── Caso extra: suplantación de agent_id (identidad declarativa) ────────
    // El autor real de `spoof_id` es plan-author; el llamante declara
    // agent_id="innocent-agent". El gate self_approval de D-13 compara contra
    // ese string declarado, no contra la credencial autenticada.
    let (st, body) = do_call(
        &app,
        &jwt,
        &format!("approve action for plan {spoof_id}"),
        Some("innocent-agent"),
    )
    .await;
    let spoof_consumed = grants::get_plan(spoof_id).await.is_none();
    table.push((
        "X1 spoof agent_id".to_string(),
        format!("approve action for plan {spoof_id}"),
        0,
        spoof_consumed,
        routed_to_approve(&body),
        response_text(&body),
        "NO aprobado",
    ));
    if spoof_consumed {
        failures.push(format!(
            "X1 spoof agent_id: declarando 'innocent-agent' se consumió un plan cuyo autor \
             es '{AUTHOR}' (bypass del gate D-13 por identidad declarativa) · HTTP {} · {}",
            st.as_u16(),
            response_text(&body)
        ));
    }

    println!("\n=== T-DSH-9 · matriz adversarial D-16 (15 intents + spoof) ===");
    println!("| # | longitud | consumo | ruta | esperado | respuesta real |");
    println!("|---|---|---|---|---|---|");
    for (label, _intent, len, consumed, routed, resp, expected) in &table {
        println!(
            "| {label} | {len} | {} | {} | {expected} | {resp} |",
            if *consumed { "CONSUMIDO" } else { "pendiente" },
            if *routed { "ROUTE" } else { "no-route" },
        );
    }
    println!("=== fin matriz ===\n");

    assert!(
        failures.is_empty(),
        "T-DSH-9: {} fila(s) con comportamiento no conforme:\n  - {}",
        failures.len(),
        failures.join("\n  - ")
    );
}
