//! T-DSH-21 — matriz de audit de aprobaciones de plan (D-13 + D-16 juntos).
//!
//! Rutas de aprobación contra el router real (`/mcp`, JSON-RPC `tools/call`):
//!   (a) `approve_action` directo,
//!   (b) ruta de intención de `tylluan_do` (`approve action for plan <id>`, D-16),
//!   (c) REST de aprobación de plan — se comprueba si existe.
//!
//! Casos por ruta: autor (autoaprobación), otro agente, operador (token maestro),
//! anónimo aprobando plan ajeno, y anónimo autoaprobándose.
//!
//! Contrato observado (D-13): autoaprobación sin operador → rechazo
//! `self_approval`, plan intacto y **cero** filas de audit; aprobación válida →
//! **exactamente una** fila con el aprobador REAL en `agent_id` y
//! `human_intervention = 1` solo si aprobó el operador (una aprobación de otro
//! agente deja `human_intervention = 0`: la fila dice lo que pasó, no que
//! intervino un humano). D-26: una aprobaci?n del operador sin `agent_id`
//! declarado se registra como `operator`, no como `anonymous` (la compuerta
//! y qui?n puede aprobar no cambian).
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test approve_audit_matrix

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;
use tylluan_kernel::security::grants;
use tylluan_kernel::transport::http::build_router;

const MASTER: &str = "master-t21-token";
const AUTHOR: &str = "t21-author";
const OTHER: &str = "t21-other";
const ANON: &str = "anonymous";
const OPERATOR: &str = "operator";

fn audit_db_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("tylluan_t21_{tag}_{}.db", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

async fn mcp_call(
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
    let bytes = axum::body::to_bytes(resp.into_body(), 4 << 20)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

fn mcp_error(body: &str) -> bool {
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    parsed["result"]["isError"].as_bool() == Some(true) || parsed["error"].is_object()
}

fn refused_with(status: StatusCode, body: &str, token: &str) -> bool {
    let refused = status == StatusCode::FORBIDDEN || (status == StatusCode::OK && mcp_error(body));
    refused && body.contains(token)
}

async fn approve_direct(
    app: &axum::Router,
    bearer: &str,
    plan_id: &str,
    approver: Option<&str>,
) -> (StatusCode, String) {
    let mut args = serde_json::json!({ "requestId": plan_id, "approved": true });
    if let Some(a) = approver {
        args["agent_id"] = serde_json::json!(a);
    }
    mcp_call(app, bearer, "approve_action", args).await
}

async fn approve_by_intent(
    app: &axum::Router,
    bearer: &str,
    plan_id: &str,
    approver: Option<&str>,
) -> (StatusCode, String) {
    let mut args = serde_json::json!({ "intent": format!("approve action for plan {plan_id}") });
    if let Some(a) = approver {
        args["agent_id"] = serde_json::json!(a);
    }
    mcp_call(app, bearer, "tylluan_do", args).await
}

async fn plan_pending(id: &str) -> bool {
    grants::get_plan(id).await.is_some()
}

/// Filas de audit de un intent: (agent_id, human_intervention, preview).
fn audit_rows(db: &std::path::Path, intent: &str) -> Vec<(String, i64, String)> {
    let Ok(conn) = tylluan_kernel::config::open_db(db) else {
        return Vec::new();
    };
    let Ok(mut stmt) = conn.prepare(
        "SELECT agent_id, human_intervention, COALESCE(result_preview,'') \
         FROM guild_audit_log WHERE intent = ?1 ORDER BY id",
    ) else {
        return Vec::new();
    };
    stmt.query_map([intent], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?))
    })
    .map(|it| it.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Espera a que el audit fire-and-forget (`spawn_blocking`) escriba.
async fn wait_rows(db: &std::path::Path, intent: &str, at_least: usize) -> Vec<(String, i64, String)> {
    let mut rows = audit_rows(db, intent);
    for _ in 0..40 {
        if rows.len() >= at_least {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        rows = audit_rows(db, intent);
    }
    rows
}

struct Case {
    name: &'static str,
    plan_id: String,
    author: &'static str,
    approver: Option<&'static str>,
    operator: bool,
    expect: Expect,
}

#[derive(Debug, Clone, Copy)]
enum Expect {
    RefusedSelfApproval,
    Approved { approver: &'static str, human: i64 },
}

struct Results {
    failures: Vec<String>,
}

#[tokio::test(flavor = "multi_thread")]
async fn approval_routes_leave_exactly_one_honest_audit_row() {
    let db = audit_db_path("audit");
    // SAFETY: único #[test] de este binario; el env se fija antes de arrancar
    // el router y aísla el audit de ./data/audit.db.
    unsafe { std::env::set_var("TYLLUAN_AUDIT_DB", &db) };

    grants::init();
    grants::init_plan_store();

    let state = support::test_state_custom("t21", Some(MASTER.to_string()), false).await;
    let jwt = state.oauth.sign_jwt("t21-jti", 300);
    let app = build_router(state);

    let mut results = Results { failures: Vec::new() };
    let mut matrix: Vec<String> = Vec::new();

    for route in ["direct", "intent"] {
        let cases = [
            Case {
                name: "autor-autoaprobacion",
                plan_id: format!("t21-{route}-self"),
                author: AUTHOR,
                approver: Some(AUTHOR),
                operator: false,
                expect: Expect::RefusedSelfApproval,
            },
            Case {
                name: "otro-agente",
                plan_id: format!("t21-{route}-other"),
                author: AUTHOR,
                approver: Some(OTHER),
                operator: false,
                expect: Expect::Approved { approver: OTHER, human: 0 },
            },
            Case {
                name: "operador",
                plan_id: format!("t21-{route}-op"),
                author: AUTHOR,
                approver: None,
                operator: true,
                // D-26: el operador sin `agent_id` declarado se materializa
                // como 'operator' en el audit (antes 'anonymous').
                expect: Expect::Approved { approver: OPERATOR, human: 1 },
            },
            Case {
                name: "anonimo-plan-ajeno",
                plan_id: format!("t21-{route}-anon"),
                author: AUTHOR,
                approver: None,
                operator: false,
                expect: Expect::Approved { approver: ANON, human: 0 },
            },
            Case {
                name: "anonimo-autoaprobacion",
                plan_id: format!("t21-{route}-aself"),
                author: ANON,
                approver: None,
                operator: false,
                expect: Expect::RefusedSelfApproval,
            },
        ];

        for case in cases {
            let intent = format!("D-21 {route} {}", case.name);
            grants::store_plan(
                &case.plan_id,
                "t21_probe",
                "t21_tool",
                &serde_json::json!({}),
                case.author,
                &intent,
            )
            .await;

            let bearer = if case.operator { MASTER } else { jwt.as_str() };
            let (status, body) = if route == "direct" {
                approve_direct(&app, bearer, &case.plan_id, case.approver).await
            } else {
                approve_by_intent(&app, bearer, &case.plan_id, case.approver).await
            };
            let refused = refused_with(status, &body, "self_approval");
            let rows = wait_rows(&db, &intent, 1).await;

            match case.expect {
                Expect::RefusedSelfApproval => {
                    if !refused {
                        results.failures.push(format!(
                            "[{route}/{}] esperado rechazo self_approval; real HTTP {status}: {}",
                            case.name, body.chars().take(200).collect::<String>()
                        ));
                    }
                    if !rows.is_empty() {
                        results.failures.push(format!(
                            "[{route}/{}] rechazo pero escribió {} fila(s) de audit: {rows:?}",
                            case.name,
                            rows.len()
                        ));
                    }
                    if !plan_pending(&case.plan_id).await {
                        results.failures.push(format!(
                            "[{route}/{}] el plan se consumió en un rechazo", case.name
                        ));
                    }
                    matrix.push(format!(
                        "| {route} | {} | rechazo self_approval, 0 filas, plan intacto | refused={refused} rows={} pending={} |",
                        case.name,
                        rows.len(),
                        plan_pending(&case.plan_id).await
                    ));
                }
                Expect::Approved { approver, human } => {
                    // Éxito de la APROBACIÓN = el plan se consume y NO hay
                    // rechazo de compuerta. El plan apunta a un guild inexistente
                    // a propósito (hermético): que la ejecución posterior devuelva
                    // error MCP es lo esperado y no invalida la aprobación.
                    let gate_refused = refused_with(status, &body, "self_approval");
                    if gate_refused {
                        results.failures.push(format!(
                            "[{route}/{}] esperado aprobación; real rechazo de compuerta: {}",
                            case.name, body.chars().take(200).collect::<String>()
                        ));
                    }
                    if plan_pending(&case.plan_id).await {
                        results.failures.push(format!(
                            "[{route}/{}] la aprobación no consumió el plan", case.name
                        ));
                    }
                    let n = rows.len();
                    if n != 1 {
                        results.failures.push(format!(
                            "[{route}/{}] esperado EXACTAMENTE 1 fila de audit; real {n}: {rows:?}",
                            case.name
                        ));
                    } else {
                        let (agent, hi, preview) = &rows[0];
                        if agent != approver {
                            results.failures.push(format!(
                                "[{route}/{}] el audit debe nombrar al aprobador real '{approver}'; real '{agent}'",
                                case.name
                            ));
                        }
                        if *hi != human {
                            results.failures.push(format!(
                                "[{route}/{}] human_intervention esperado {human}; real {hi} \
                                 (una aprobación sin operador NO es intervención humana)",
                                case.name
                            ));
                        }
                        if approver != ANON && !preview.contains(&format!("[plan author: {}", case.author)) {
                            results.failures.push(format!(
                                "[{route}/{}] la fila debería conservar al autor del plan en el preview; real '{preview}'",
                                case.name
                            ));
                        }
                    }
                    matrix.push(format!(
                        "| {route} | {} | 1 fila, agente='{approver}', human_intervention={human} | rows={n} {:?} |",
                        case.name, rows
                    ));
                }
            }

            // Segunda aprobación del mismo plan: no puede añadir otra fila
            // (el plan ya está consumido; el segundo intento debe fallar).
            if matches!(case.expect, Expect::Approved { .. }) {
                let _ = if route == "direct" {
                    approve_direct(&app, bearer, &case.plan_id, case.approver).await
                } else {
                    approve_by_intent(&app, bearer, &case.plan_id, case.approver).await
                };
                let rows2 = audit_rows(&db, &intent);
                if rows2.len() != 1 {
                    results.failures.push(format!(
                        "[{route}/{}] la segunda aprobación cambió las filas de audit: {}",
                        case.name,
                        rows2.len()
                    ));
                }
            }
        }
    }

    // (c) ¿Existe una ruta REST de aprobación de plan?
    let mut rest_found: Vec<String> = Vec::new();
    for path in [
        "/api/v1/plans/t21-rest/approve",
        "/api/v1/plan/t21-rest/approve",
        "/api/v1/approvals/t21-rest/approve",
    ] {
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        if resp.status() != StatusCode::NOT_FOUND {
            rest_found.push(format!("{path} -> HTTP {}", resp.status()));
        }
        matrix.push(format!("| REST | probe {path} | no existe (404) | HTTP {} |", resp.status()));
    }
    if !rest_found.is_empty() {
        results.failures.push(format!(
            "hay una ruta REST de aprobación de plan que la matriz no cubre: {rest_found:?}"
        ));
    }

    println!("\n=== MATRIZ T-DSH-21 ===");
    println!("| ruta | caso | esperado | real |");
    for line in &matrix {
        println!("{line}");
    }

    assert!(
        results.failures.is_empty(),
        "T-DSH-21 — {} caso(s) con audit/compuerta incoherente:\n  - {}",
        results.failures.len(),
        results.failures.join("\n  - ")
    );
}
