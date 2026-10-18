//! T1253 — revisión ADVERSARIAL (Buffy, turno 1268): casos que el test de
//! Deep NO cubre, ejecutados sobre SU diff copiado a este worktree.
//!
//! Cubre: token maestro por query-string (debe 403, no autentica la clase
//! operador), dev_mode (DevBypassDocumentado → 200), trailing slash y
//! variante de mayúsculas (404, sin bypass del gate), límites degenerados
//! (negativo / no numérico / desbordado → 400 sin panic), excerpt con
//! emoji multibyte y CRLF (300 chars Unicode, no bytes), excerpt vacío,
//! y coherencia sum(n) == len(rows) del resumen por contrato.

mod support;

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use tylluan_kernel::transport::http::{HttpState, build_router};

const MASTER: &str = "master-buffy-rev-token";

fn authed_app(state: Arc<HttpState>) -> axum::Router {
    build_router(state)
}

async fn status_of(app: &axum::Router, method: &str, uri: &str, bearer: Option<&str>) -> StatusCode {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let resp = app.clone().oneshot(req.body(Body::empty()).unwrap()).await.unwrap();
    resp.status()
}

async fn get_json(app: &axum::Router, uri: &str, bearer: &str) -> serde_json::Value {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {bearer}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "GET {uri}");
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).expect("valid JSON body")
}

/// (2) El token maestro POR QUERY-STRING autentica pero NO satisface la clase
/// operador (E1, turno 1237): ambos endpoints deben devolver 403 — no 200.
#[tokio::test(flavor = "multi_thread")]
async fn query_string_master_token_is_rejected_on_operator_reads() {
    let state = support::test_state_custom("buffy_rev_qtoken", Some(MASTER.to_string()), false).await;
    let app = authed_app(state);

    for path in ["/api/v1/decide/shadow", "/api/v1/silva/conflicted"] {
        let uri = format!("{path}?token={MASTER}");
        let status = status_of(&app, "GET", &uri, None).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "query-string master token must NOT open {path} (got {status})"
        );
        // Misma negación vía cabecera alternativa Authorization en query.
        let uri = format!("{path}?Authorization={MASTER}");
        let status = status_of(&app, "GET", &uri, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "Authorization= query on {path}");
    }
}

/// dev_mode=true instala DevBypass — el gate operador lo acepta POR DISEÑO
/// (mismo trato que el resto de rutas operador). Fijado aquí para que un
/// cambio futuro de política lo note.
#[tokio::test(flavor = "multi_thread")]
async fn dev_mode_bypass_is_accepted_by_design() {
    let state = support::test_state("buffy_rev_devmode").await; // dev_mode=true, sin token
    let app = authed_app(state);

    for path in ["/api/v1/decide/shadow", "/api/v1/silva/conflicted"] {
        let status = status_of(&app, "GET", path, None).await;
        assert_eq!(status, StatusCode::OK, "dev_mode DevBypass opens {path}");
    }
}

/// Trailing slash y variante de mayúsculas NO deben servir el endpoint ni
/// eludir el gate: 404 (ruta no registrada) — nunca 200 con datos.
#[tokio::test(flavor = "multi_thread")]
async fn trailing_slash_and_case_variants_do_not_bypass() {
    let state = support::test_state_custom("buffy_rev_slash", Some(MASTER.to_string()), false).await;
    let app = authed_app(state);

    // Con el token maestro legítimo igualmente no existen: 404, no datos.
    for uri in [
        "/api/v1/decide/shadow/",
        "/api/v1/silva/conflicted/",
        "/api/v1/Decide/shadow",
        "/api/v1/decide/Shadow",
    ] {
        let status = status_of(&app, "GET", uri, Some(MASTER)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} must 404, got {status}");
    }
}

/// Límites degenerados: negativo, no numérico y desbordado → 400 del
/// extractor Query (consistente con el resto de GETs con Query del código),
/// sin panic y sin 500.
#[tokio::test(flavor = "multi_thread")]
async fn degenerate_limits_reject_cleanly() {
    let state = support::test_state_custom("buffy_rev_limits", Some(MASTER.to_string()), false).await;
    let app = authed_app(state);

    for base in ["/api/v1/decide/shadow", "/api/v1/silva/conflicted"] {
        for bad in ["-1", "abc", "1e3", "500.5", "18446744073709551616"] {
            let uri = format!("{base}?limit={bad}");
            let status = status_of(&app, "GET", &uri, Some(MASTER)).await;
            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{uri} must be 400, got {status}"
            );
        }
    }
}

/// (1)+(4) Sombra: ninguna clave de state/preguntas/instructions en la
/// respuesta; EXACTAMENTE los 11 campos congelados; y coherencia del
/// resumen: sum(n por contrato) == len(rows) y n == n_agreed+n_disagreed+n_null.
#[tokio::test(flavor = "multi_thread")]
async fn shadow_no_question_text_and_summary_coherent() {
    let state = support::test_state_custom("buffy_rev_leak", Some(MASTER.to_string()), false).await;
    {
        let g = state.silva.conn_lock();
        let conn = g.lock().await;
        for (contract, agreed) in [("c_a", Some(1i64)), ("c_a", Some(0)), ("c_b", None)] {
            conn.execute(
                "INSERT INTO decision_shadow_log
                 (ts, contract, request_hash, rules_value, provider, latency_ms, agreed, mode)
                 VALUES ('2026-10-18T00:00:00Z', ?1, ?2, 'v', 'rules', 1, ?3, 'shadow')",
                rusqlite::params![contract, format!("h-{contract}-{agreed:?}"), agreed],
            )
            .expect("seed");
        }
    }
    let app = authed_app(state);
    let v = get_json(&app, "/api/v1/decide/shadow?limit=50", MASTER).await;

    let body = v.to_string();
    for banned in [
        "state",
        "question",
        "instructions",
        "model_probs_json",
        "model_latency_ms",
    ] {
        assert!(
            !body.contains(&format!("\"{banned}\"")),
            "response must not carry the key {banned:?}: {body}"
        );
    }

    let rows = v["rows"].as_array().unwrap();
    let summary = v["summary"].as_array().unwrap();
    let sum_n: i64 = summary.iter().map(|s| s["n"].as_i64().unwrap()).sum();
    assert_eq!(sum_n as usize, rows.len(), "summary n adds up to the window");
    for s in summary {
        let n = s["n"].as_i64().unwrap();
        let acc = s["n_agreed"].as_i64().unwrap()
            + s["n_disagreed"].as_i64().unwrap()
            + s["n_null"].as_i64().unwrap();
        assert_eq!(n, acc, "per-contract n = agreed+disagreed+null");
    }
    assert!(v["total"].as_i64().unwrap() >= sum_n, "total >= window");
}

/// (4) Excerpt: emoji multibyte (4 bytes/carácter) — el tope es de 300
/// CARACTERES Unicode, no bytes; CRLF y controles → espacio; contenido
/// vacío → excerpt vacío sin panic; y ningún nodo limpio se cuela.
#[tokio::test(flavor = "multi_thread")]
async fn conflicted_excerpt_unicode_crlf_empty() {
    let state = support::test_state("buffy_rev_unicode").await;
    // 400 emojis 👍 (U+1F44D, 4 bytes) — un tope en bytes cortaría distinto.
    state
        .silva
        .upsert_node("t_emoji", "fact", &"👍".repeat(400), "{}")
        .await
        .unwrap();
    state.silva.mark_conflicted("t_emoji", true).await.unwrap();
    state
        .silva
        .upsert_node("t_crlf", "fact", "linea1\r\nlinea2\u{0007}bell\u{200b}fin", "{}")
        .await
        .unwrap();
    state.silva.mark_conflicted("t_crlf", true).await.unwrap();
    state
        .silva
        .upsert_node("t_empty", "fact", "", "{}")
        .await
        .unwrap();
    state.silva.mark_conflicted("t_empty", true).await.unwrap();
    let app = authed_app(state);

    let v = get_json(&app, "/api/v1/silva/conflicted?limit=50", MASTER).await;
    let nodes = v["nodes"].as_array().unwrap();
    let by_id = |id: &str| {
        nodes
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("{id} present in the window"))
            .clone()
    };

    let emoji_node = by_id("t_emoji");
    let emoji = emoji_node["content_excerpt"].as_str().unwrap();
    assert_eq!(
        emoji.chars().count(),
        300,
        "300 Unicode CHARS not bytes (400×4-byte emoji)"
    );
    assert!(emoji.starts_with('👍'), "no byte-level mangling of the first char");

    let crlf_node = by_id("t_crlf");
    let crlf = crlf_node["content_excerpt"].as_str().unwrap();
    assert!(!crlf.contains('\r') && !crlf.contains('\n'), "CRLF sanitized to spaces");
    assert!(
        !crlf.chars().any(|c| c.is_control()),
        "no control chars survive sanitize_excerpt"
    );

    let empty_node = by_id("t_empty");
    let empty = empty_node["content_excerpt"].as_str().unwrap();
    assert_eq!(empty, "", "empty content → empty excerpt, no panic");

    assert_eq!(v["total_conflicted"], 3);
}
