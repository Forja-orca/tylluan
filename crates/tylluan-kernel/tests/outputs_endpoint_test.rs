//! Integration test: guild outputs ledger HTTP surface (bwc-d0fb0812).
//!
//! One sequential test drives the real `api_v1_routes()` router through the
//! shared harness: empty-store listing, a seeded run round-trip (list +
//! manifest), unknown run 404, and hostile run ids (path traversal shapes)
//! answering 404 without leaking paths. The `TYLLUAN_OUTPUTS_DIR` seam is
//! process-global, so the whole file is a single test that sets the seam
//! once and restores it (cycle-5 lesson: no parallel env-var tests).

mod support;

use std::fs;

use axum::http::StatusCode;
use tower::ServiceExt;

use tylluan_kernel::registry::outputs::{CallRecord, OutputsStore};

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// multi_thread flavor: the shared harness touches blocking-capable code
// (HybridMemory) — same requirement as the other integration tests.
#[tokio::test(flavor = "multi_thread")]
async fn outputs_ledger_http_roundtrip_and_hostile_ids() {
    // Seam: point the store at a fresh temp dir for the whole test.
    let dir = std::env::temp_dir().join(format!(
        "tylluan_outputs_http_{}",
        support::test_counter_next()
    ));
    fs::create_dir_all(&dir).unwrap();
    let old = std::env::var("TYLLUAN_OUTPUTS_DIR").ok();
    // SAFETY (set_var/remove_var): edition-2024 unsafe ops. This binary is
    // one sequential test (env seams are process-global, cycle-5 lesson);
    // no other test in this file touches the variable.
    unsafe { std::env::set_var("TYLLUAN_OUTPUTS_DIR", &dir) };

    let state = support::test_state("outputs").await;
    let app = support::build_test_app(state);

    // 1. Empty store: valid endpoint, zero runs.
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/outputs")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["runs"].as_array().map(Vec::len), Some(0));

    // 2. Seed one run directly through the store (the actor-side wiring is
    //    unit-covered in outputs.rs; here we exercise the HTTP read shape).
    let artifact = dir.join("comfy").join("img_001.png");
    fs::create_dir_all(artifact.parent().unwrap()).unwrap();
    fs::write(&artifact, b"PNGDATA").unwrap();
    let store = OutputsStore::new(dir.clone());
    let record = CallRecord {
        guild: "comfy",
        tool: "txt2img",
        requested_by: "buffy",
        success: true,
        started_unix: unix_now() - 5,
        ended_unix: unix_now(),
        result_json: r#"{"content":[{"type":"text","text":"done"}]}"#,
        run_id_override: Some("feedface00000001".to_string()),
    };
    let manifest = store.index_call(&record).expect("seeded run indexed");

    // List shows the seeded run.
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/outputs")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let runs = v["runs"].as_array().cloned().unwrap_or_default();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0]["run_id"], "feedface00000001");
    assert_eq!(runs[0]["guild"], "comfy");
    assert_eq!(runs[0]["requested_by"], "buffy");

    // Manifest round-trip with the real file entry.
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/outputs/feedface00000001/manifest")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1024 * 1024).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["run_id"], "feedface00000001");
    assert_eq!(v["delivery_status"], "ok");
    assert_eq!(v["files"].as_array().map(Vec::len), Some(1));
    assert_eq!(v["files"][0]["bytes"], 7);
    assert_eq!(v["files"][0]["sha256"].as_str().map(str::len), Some(64));
    assert_eq!(v, serde_json::to_value(&manifest).unwrap());

    // 3. Unknown run: clean 404 JSON.
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .uri("/api/v1/outputs/ffffffffffffffff/manifest")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    // 4. Hostile run ids: traversal shapes must degrade as a client error
    //    (404 from our sanitizer, or 400 if the router itself rejects the
    //    decoded segment — both are graceful; never 2xx, never 5xx) with no
    //    path echo.
    //    (Backslashes are percent-encoded: a raw \ in a URI can be rejected
    //    by http::Uri before reaching the router — the hostile case that
    //    matters is what the handler receives after decoding.)
    for hostile in ["..%2F..%2Fetc%2Fpasswd", "..%5C..%5Cwindows%5Cwin.ini", "%2e%2e%2f"] {
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/api/v1/outputs/{hostile}/manifest"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        assert!(status.is_client_error(), "hostile id {hostile} → {status}");
        let body = axum::body::to_bytes(res.into_body(), 64 * 1024).await.unwrap();
        let text = String::from_utf8_lossy(&body);
        assert!(!text.contains("passwd"));
        assert!(!text.contains("windows"));
    }

    // Restore the seam and clean up.
    match old {
        Some(v) => unsafe { std::env::set_var("TYLLUAN_OUTPUTS_DIR", v) },
        None => unsafe { std::env::remove_var("TYLLUAN_OUTPUTS_DIR") },
    }
    fs::remove_dir_all(dir).ok();
}
