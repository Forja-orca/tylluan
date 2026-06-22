//! Verifies that `api_v1_routes()` does not panic on construction.
//!
//! If Axum encounters a duplicate route at build time it panics,
//! which would crash the kernel at startup. This test catches that
//! before it reaches production.

use std::panic::catch_unwind;

#[test]
fn test_api_v1_routes_construction_does_not_panic() {
    let result = catch_unwind(|| {
        let _ = tylluan_kernel::transport::http::api_v1::api_v1_routes();
    });
    assert!(result.is_ok(), "api_v1_routes() panicked! This means there is a duplicate route or another router construction error that would crash the kernel at startup.");
}
