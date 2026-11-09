//! D-163: alta/baja de proveedores externos y **refresco en caliente**.
//!
//! Origen: la revisión estática de Antigravity (T3762-A) sospechó que
//! `list_external_providers_handler` leía una configuración vieja porque
//! `reload()` pudiera cambiar el `Arc`. La medición empírica (T3778) demostró
//! que **no**: `reload()` escribe dentro del mismo `Arc<RwLock<TylluanConfig>>`
//! (`config.rs:2917`). Este test blinda ese comportamiento en CI para que no
//! pueda romperse en silencio:
//!
//! 1. `POST` con ETag → el proveedor queda en disco **y** la lista ya lo ve.
//! 2. `DELETE` con ETag → la lista vuelve a estar vacía en el acto.
//!
//! **Mutación**: si alguien quita `TylluanConfig::reload()` de
//! `create_external_provider_handler` o de `delete_external_provider_handler`,
//! el paso 2 (o el 4) falla. No hay caché que valga: la respuesta es la prueba.
//!
//! El test escribe su propia config en un directorio temporal
//! (`set_config_path_override`) para no tocar el `tylluan.toml` del checkout.

mod support;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use std::sync::Arc;
use tower::ServiceExt;

use tylluan_kernel::config::TylluanConfig;
use tylluan_kernel::transport::http::{build_router, HttpState};

const TOKEN: &str = "d163-external-providers-token";
const HOST: &str = "127.0.0.1:47201";

/// Config mínima propia del test. Sin proveedores: el alta tiene que ser la que
/// los ponga.
fn config_temporal() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tylluan_d163_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("tylluan.toml");
    std::fs::write(
        &path,
        r#"[nexus]
host = "127.0.0.1"
port = 47201
dev_mode = false
allow_hf_download = false

[security]
encrypt_at_rest = false
"#,
    )
    .expect("config temporal");
    path
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn call(
    state: &Arc<HttpState>,
    method: Method,
    path: &str,
    body: &str,
) -> Resp {
    let app = build_router(state.clone());
    let res = app
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", HOST)
                .header("authorization", format!("Bearer {TOKEN}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .expect("router must not panic");
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(!text.contains(TOKEN), "response leaked the master token");
    Resp { status, headers, body: text }
}

fn etag_of(resp: &Resp) -> String {
    resp.headers
        .get("x-config-etag")
        .and_then(|v| v.to_str().ok())
        .expect("GET /api/v1/config debe traer x-config-etag")
        .to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d163_provider_create_and_delete_are_visible_immediately() {
    // El override va ANTES de construir el estado: `load_cached()` es un
    // OnceLock por proceso y este binario de test es su propio proceso.
    let cfg_path = config_temporal();
    TylluanConfig::set_config_path_override(cfg_path.clone());
    let state = support::test_state_custom("d163", Some(TOKEN.to_string()), false).await;

    // Lista inicial vacía.
    let inicial = call(&state, Method::GET, "/api/v1/external-providers", "").await;
    assert_eq!(inicial.status, StatusCode::OK, "GET inicial: {}", inicial.body);
    assert!(!inicial.body.contains("prueba-d163"), "no debería existir aún: {}", inicial.body);

    // ETag y alta.
    let cfg = call(&state, Method::GET, "/api/v1/config", "").await;
    assert_eq!(cfg.status, StatusCode::OK, "GET /config: {}", cfg.body);
    let etag = etag_of(&cfg);

    let alta = call(
        &state,
        Method::POST,
        "/api/v1/external-providers",
        &format!(
            r#"{{"name":"prueba-d163","base_url":"https://api.example.com","api_key_env":"D163_API_KEY","models":["m1"],"etag":"{etag}"}}"#
        ),
    )
    .await;
    assert_eq!(alta.status, StatusCode::CREATED, "POST: {}", alta.body);

    // EL PUNTO DEL TEST: la lista lo ve EN EL ACTO, sin reiniciar.
    let tras_alta = call(&state, Method::GET, "/api/v1/external-providers", "").await;
    assert_eq!(tras_alta.status, StatusCode::OK);
    assert!(
        tras_alta.body.contains("prueba-d163"),
        "la lista no refleja el alta (¿se quitó reload()?): {}",
        tras_alta.body
    );

    // Baja y comprobación inmediata.
    let cfg2 = call(&state, Method::GET, "/api/v1/config", "").await;
    let etag2 = etag_of(&cfg2);
    let baja = call(
        &state,
        Method::DELETE,
        &format!("/api/v1/external-providers/prueba-d163?etag={etag2}"),
        "",
    )
    .await;
    assert_eq!(baja.status, StatusCode::OK, "DELETE: {}", baja.body);

    let tras_baja = call(&state, Method::GET, "/api/v1/external-providers", "").await;
    assert_eq!(tras_baja.status, StatusCode::OK);
    assert!(
        !tras_baja.body.contains("prueba-d163"),
        "la lista sigue mostrando el proveedor borrado: {}",
        tras_baja.body
    );
}
