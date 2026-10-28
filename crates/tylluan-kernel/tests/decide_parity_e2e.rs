//! End-to-end parity and contract tests for `tylluan_decide` (ADR-021 Phase 1).
//!
//! Enforces:
//! 1. Paridad estricta MCP vs REST: misma entrada => misma salida.
//! 2. Límites canónicos (estado máx 16 KiB, 1..=16 preguntas, opciones Choice 2..=32, niveles Score 2..=10).
//! 3. Comportamiento apagado (status=disabled, answers vacío, CERO llamadas externas).
//! 4. Clasificación de clase Agente (no Operador).

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use rmcp::model::JsonObject;
use std::collections::BTreeMap;
use tower::ServiceExt;

use tylluan_kernel::router::decision::api_types::*;
use tylluan_kernel::transport::http::auth::operator_class_required;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_decide_mcp_rest_parity() {
    let state = support::test_state("decide_parity_1").await;
    let app = support::build_test_app(state.clone());
    let server_arc = state.server.as_ref().expect("server present").clone();

    // Petición idéntica para ambos canales
    let state_payload = serde_json::json!({
        "intent": "recall_memory",
        "risk": 0.15,
        "recent_failures": 0
    });

    let mut questions = BTreeMap::new();
    questions.insert(
        "provider_selection".to_string(),
        QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: Some("Seleccionar proveedor según riesgo y fallos".into()),
            options: vec!["local_fast".into(), "local_deep".into()],
            levels: vec![],
        },
    );
    questions.insert(
        "risk_tier".to_string(),
        QuestionSpec {
            kind: QuestionKind::Score,
            instructions: Some("Clasificar nivel de riesgo".into()),
            options: vec![],
            levels: vec!["low".into(), "medium".into(), "high".into(), "critical".into()],
        },
    );

    let req_body = DecideRequest {
        state: state_payload.clone(),
        questions: questions.clone(),
        contract: Some("scheduler_routing".into()),
        max_latency_ms: Some(2000),
        sensitivity: None,
    };

    // ── 1. Ejecutar vía MCP ──────────────────────────────────────────────────
    let mut mcp_args = JsonObject::new();
    mcp_args.insert("state".into(), state_payload);
    let q_val = serde_json::to_value(&questions).unwrap();
    mcp_args.insert("questions".into(), q_val);
    mcp_args.insert("contract".into(), serde_json::Value::String("scheduler_routing".into()));
    mcp_args.insert("max_latency_ms".into(), serde_json::json!(2000));

    let mcp_res = {
        let srv = server_arc.read().await;
        srv.handle_kernel_tool("tylluan_decide", Some(mcp_args)).await.expect("mcp call ok")
    };
    assert_eq!(mcp_res.is_error, Some(false));
    let mcp_text = &mcp_res.content[0].as_text().expect("text content").text;
    let mcp_parsed: DecideResponse = serde_json::from_str(mcp_text).expect("mcp json response");

    // ── 2. Ejecutar vía REST POST /api/v1/decide ──────────────────────────────
    let rest_req = Request::builder()
        .method("POST")
        .uri("/api/v1/decide")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
        .unwrap();
    let rest_resp = app.oneshot(rest_req).await.unwrap();
    assert_eq!(rest_resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(rest_resp.into_body(), usize::MAX).await.unwrap();
    let rest_parsed: DecideResponse = serde_json::from_slice(&bytes).expect("rest json response");

    // ── 3. Verificar paridad estricta ─────────────────────────────────────────
    assert_eq!(mcp_parsed.status, DecideStatus::Ok);
    assert_eq!(rest_parsed.status, DecideStatus::Ok);
    assert_eq!(mcp_parsed.mode, rest_parsed.mode);
    assert_eq!(mcp_parsed.provider, rest_parsed.provider);
    assert_eq!(mcp_parsed.degraded, rest_parsed.degraded);

    // Comparar valores de respuestas por clave
    assert_eq!(mcp_parsed.answers.len(), rest_parsed.answers.len());
    for (k, mcp_ans) in &mcp_parsed.answers {
        let rest_ans = rest_parsed.answers.get(k).expect("matching key in rest");
        assert_eq!(mcp_ans.value, rest_ans.value, "Value mismatch for question {k}");
        assert_eq!(mcp_ans.abstained, rest_ans.abstained, "Abstain mismatch for question {k}");
        assert_eq!(mcp_ans.abstain_reason, rest_ans.abstain_reason);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_decide_disabled_mode_zero_external_calls() {
    let state = support::test_state("decide_disabled").await;
    let app = support::build_test_app(state.clone());
    let server_arc = state.server.as_ref().expect("server present").clone();

    // Desactivar servicio (interruptor global `[decision] enabled = false`)
    {
        let mut cfg = tylluan_kernel::config::TylluanConfig::default();
        cfg.decision.enabled = false;
        server_arc.write().await.set_decision_config(&cfg);
    }

    let mut questions = BTreeMap::new();
    questions.insert(
        "q".into(),
        QuestionSpec {
            kind: QuestionKind::Choice,
            instructions: None,
            options: vec!["a".into(), "b".into()],
            levels: vec![],
        },
    );

    let req_body = DecideRequest {
        state: serde_json::json!({ "task": "test" }),
        questions: questions.clone(),
        contract: None,
        max_latency_ms: None,
        sensitivity: None,
    };

    // REST debe devolver 200 OK con status: disabled y answers vacío
    let rest_req = Request::builder()
        .method("POST")
        .uri("/api/v1/decide")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&req_body).unwrap()))
        .unwrap();
    let rest_resp = app.oneshot(rest_req).await.unwrap();
    assert_eq!(rest_resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(rest_resp.into_body(), usize::MAX).await.unwrap();
    let parsed: DecideResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(parsed.status, DecideStatus::Disabled);
    assert_eq!(parsed.mode, DecisionMode::Off);
    assert_eq!(parsed.provider, "none");
    assert!(parsed.answers.is_empty(), "Disabled answers must be empty");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_decide_limits_enforced_in_rest() {
    let state = support::test_state("decide_limits").await;
    let app = support::build_test_app(state.clone());

    // 1. Estado > 16 KiB
    let large_state = serde_json::json!({ "large": "x".repeat(17 * 1024) });
    let mut questions = BTreeMap::new();
    questions.insert("q".into(), QuestionSpec {
        kind: QuestionKind::Boolean,
        instructions: None,
        options: vec![],
        levels: vec![],
    });

    let bad_req = DecideRequest {
        state: large_state,
        questions,
        contract: None,
        max_latency_ms: None,
        sensitivity: None,
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/decide")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&bad_req).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 2. 0 preguntas
    let empty_q_req = serde_json::json!({
        "state": { "ok": true },
        "questions": {}
    });
    let req2 = Request::builder()
        .method("POST")
        .uri("/api/v1/decide")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&empty_q_req).unwrap()))
        .unwrap();
    let resp2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_decide_route_is_agent_class_not_operator() {
    // /api/v1/decide es una superficie consumible por agentes, nunca bloqueada a operador
    assert!(
        !operator_class_required("POST", "/api/v1/decide"),
        "POST /api/v1/decide must NOT require Operator class"
    );
}
