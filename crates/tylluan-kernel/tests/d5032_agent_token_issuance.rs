//! D5032 (F1 de D5014): emision de tokens por agente por HTTP.
//!
//! Que hay detras (medido antes de escribir este test, sobre `7e14dfd2`): el
//! resolutor de tokens de agente ya estaba en `main` desde D225 — `acl_token_sha256`,
//! `lookup_acl_entry`, `agent_token_kind` y `resolve_agent_id_for_token` en
//! `transport/http/auth.rs`. Lo que no existia era el **emisor**: `git grep -i
//! 'operator/tokens'` solo devolvian dos comentarios y `tylluan-cli` no tenia
//! ningun subcomando. O sea, dar un token de agente significaba editar a mano
//! `tylluan.toml` y pegar ahi el valor en claro.
//!
//! Este test va contra el router real (`build_router`) con `dev_mode = false` y
//! token maestro, igual que `b20_auth_route_matrix.rs`. Cada criterio del DoD es
//! una asercion:
//!
//! * (a) el token emitido autentica **en el acto** (el `reload()` escribio dentro
//!   del mismo `Arc`, no hace falta reiniciar) y su identidad es `AgentToken` con
//!   el `agent_id` vinculado;
//! * (b) el token en claro no aparece en `tylluan.toml`, ni en sus copias de
//!   seguridad, ni en `config_audit.jsonl`, ni en `GET /api/v1/config`;
//! * (c) sin credencial de operador no se emite (401/403), y un token de agente
//!   tampoco puede emitir (la ruta es clase Operador en
//!   `auth::operator_class_required`);
//! * mutaciones que deben caer: guardar el token en claro, no escribir
//!   `token_agent_bindings`, o quitar la entrada del gate de operador.
//!
//! Lo que NO comprueba (y esta dicho en el informe): expiracion. `ttl` se rechaza
//! porque el resolutor todavia no la aplica (F2 de D5014).
mod support;

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;

use tylluan_kernel::config::TylluanConfig;
use tylluan_kernel::transport::http::auth::{
    acl_token_sha256, agent_token_kind, resolve_agent_id_for_token, resolve_role_for_token,
    AclAuthKind,
};
use tylluan_kernel::transport::http::{build_router, HttpState};

const MAESTRO: &str = "d5032-operador-maestro";
const HOST: &str = "127.0.0.1:47215";

/// Config propia del test: `dev_mode = false` y roles reales. Sin
/// `[security.acl.tokens]`: el alta tiene que ser la que los ponga.
fn config_temporal() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tylluan_d5032_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir d5032");
    let path = dir.join("tylluan.toml");
    std::fs::write(
        &path,
        r#"[nexus]
host = "127.0.0.1"
port = 47215
dev_mode = false
allow_hf_download = false

[security]
encrypt_at_rest = false

[security.acl]
default_role = "admin"

[security.acl.roles]
admin = ["*"]
writer = ["knowledge", "coloquio"]
"#,
    )
    .expect("config temporal d5032");
    path
}

struct Resp {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

/// Llamada con el bearer que se quiera: el test necesita comparar maestro,
/// token de agente recien emitido y la huella guardada.
async fn call(state: &Arc<HttpState>, method: Method, path: &str, bearer: Option<&str>, body: &str) -> Resp {
    let app = build_router(state.clone());
    let mut b = Request::builder()
        .method(method)
        .uri(path)
        .header("host", HOST)
        .header("content-type", "application/json");
    if let Some(t) = bearer {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    let res = app
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .expect("el router no debe partirse");
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap_or_default();
    Resp {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).to_string(),
    }
}

/// Todos los ficheros del directorio del config (TOML, temporales y copias de
/// seguridad que escribe `backup_and_prune`).
fn ficheros_del_config(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for entrada in std::fs::read_dir(dir).expect("leer el dir del config") {
        let p = entrada.expect("entrada").path();
        if p.is_file() {
            let nombre = p.file_name().unwrap_or_default().to_string_lossy().to_string();
            let contenido = std::fs::read_to_string(&p).unwrap_or_default();
            out.push((nombre, contenido));
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn d5032_emitir_un_token_de_agente_de_principio_a_fin() {
    // El override va ANTES de construir el estado (`load_cached()` es un OnceLock
    // por proceso). Un solo test por binario a proposito: el override es un
    // OnceLock y no se puede volver a fijar.
    let cfg_path = config_temporal();
    let cfg_dir = cfg_path.parent().unwrap().to_path_buf();
    TylluanConfig::set_config_path_override(cfg_path.clone());
    let state = support::test_state_custom("d5032", Some(MAESTRO.to_string()), false).await;

    // ── (c) sin credencial de operador no se emite nada ───────────────────────
    let cuerpo = r#"{"agent_id":"deep","role":"writer","scope":"read-only"}"#;
    let sin_credencial = call(&state, Method::POST, "/api/v1/acl/tokens", None, cuerpo).await;
    assert!(
        matches!(sin_credencial.status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
        "sin credencial debe ser 401/403, no {}: {}",
        sin_credencial.status,
        sin_credencial.body
    );

    // El ETag es obligatorio: dos emisiones sin ETag se pisarian el config.
    let sin_etag = call(&state, Method::POST, "/api/v1/acl/tokens", Some(MAESTRO), cuerpo).await;
    assert_eq!(
        sin_etag.status,
        StatusCode::PRECONDITION_REQUIRED,
        "sin ETag: {}",
        sin_etag.body
    );
    assert!(
        !std::fs::read_to_string(&cfg_path)
            .expect("leer config")
            .contains("sha256:"),
        "un rechazo no debe haber escrito la ACL"
    );

    // Rol inexistente: se rechaza en vez de heredar `default_role = "admin"`.
    let rol_malo = call(
        &state,
        Method::POST,
        "/api/v1/acl/tokens",
        Some(MAESTRO),
        r#"{"agent_id":"deep","role":"súper-admin"}"#,
    )
    .await;
    assert_eq!(rol_malo.status, StatusCode::BAD_REQUEST, "rol inexistente: {}", rol_malo.body);

    // `ttl` se rechaza: el resolutor no lo aplica y no se emite con caducidad falsa.
    let con_ttl = call(
        &state,
        Method::POST,
        "/api/v1/acl/tokens",
        Some(MAESTRO),
        r#"{"agent_id":"deep","role":"writer","ttl_days":7}"#,
    )
    .await;
    assert_eq!(con_ttl.status, StatusCode::BAD_REQUEST, "con ttl: {}", con_ttl.body);
    assert!(con_ttl.body.contains("ttl_not_supported"), "codigo esperado: {}", con_ttl.body);

    // agent_id con separadores de ruta: rompe `agent_permissions` y las metricas.
    let id_malo = call(
        &state,
        Method::POST,
        "/api/v1/acl/tokens",
        Some(MAESTRO),
        r#"{"agent_id":"../../etc/passwd","role":"writer"}"#,
    )
    .await;
    assert_eq!(id_malo.status, StatusCode::BAD_REQUEST, "agent_id malo: {}", id_malo.body);

    // ── emision valida ────────────────────────────────────────────────────────
    let config_anterior = std::fs::read_to_string(&cfg_path).expect("leer config");
    let etag_actual = {
        let r = call(&state, Method::GET, "/api/v1/config", Some(MAESTRO), "").await;
        assert_eq!(r.status, StatusCode::OK, "GET /config: {}", r.body);
        r.headers
            .get("x-config-etag")
            .and_then(|v| v.to_str().ok())
            .expect("GET /api/v1/config debe traer x-config-etag")
            .to_string()
    };
    let cuerpo_bueno = format!(
        r#"{{"agent_id":"deep","role":"writer","scope":"read-only","etag":"{etag_actual}"}}"#
    );
    let emitido = call(&state, Method::POST, "/api/v1/acl/tokens", Some(MAESTRO), &cuerpo_bueno).await;
    assert_eq!(emitido.status, StatusCode::CREATED, "emision: {}", emitido.body);
    let v: serde_json::Value = serde_json::from_str(&emitido.body).expect("respuesta JSON");
    let token = v["token"].as_str().unwrap_or_default().to_string();
    let huella = v["fingerprint"].as_str().unwrap_or_default().to_string();
    assert!(token.starts_with("tyll-agent-") && token.len() >= 70, "token: {token}");
    assert_eq!(huella, acl_token_sha256(&token), "la huella devuelta es el sha256 del token");
    assert_eq!(v["token_shown_once"], serde_json::json!(true));

    // ── (b) el token en claro no queda en ningun sitio persistente ───────────
    let crudos = ficheros_del_config(&cfg_dir);
    assert!(crudos.iter().any(|(n, _)| n == "tylluan.toml"), "debe existir el config");
    for (nombre, contenido) in &crudos {
        assert!(
            !contenido.contains(&token),
            "el token en claro salio en {}: {}",
            nombre,
            contenido.chars().take(160).collect::<String>()
        );
    }
    let escrito = std::fs::read_to_string(&cfg_path).expect("releer config");
    assert_ne!(escrito, config_anterior, "el config tenia que cambiar");
    assert!(escrito.contains(&format!("\"{huella}\"")), "la huella si va al TOML: {escrito}");
    assert!(escrito.contains("token_agent_bindings"), "la tabla de vinculos: {escrito}");
    assert!(escrito.contains("read-only"), "el scope escrito en agent_permissions: {escrito}");

    let auditoria = tylluan_kernel::config::data_file("config_audit.jsonl");
    if auditoria.exists() {
        let log = std::fs::read_to_string(&auditoria).unwrap_or_default();
        assert!(!log.contains(&token), "el token en claro salio en la auditoria: {log}");
        assert!(log.contains("acl_token_api"), "el asiento de auditoria debe nombrar la via: {log}");
    }

    // `GET /api/v1/config`: ni el token ni la huella (la CLAVE es el secreto, D-100).
    let config_json = call(&state, Method::GET, "/api/v1/config", Some(MAESTRO), "").await;
    assert_eq!(config_json.status, StatusCode::OK);
    assert!(!config_json.body.contains(&token), "el config por HTTP lleva el token");
    assert!(!config_json.body.contains(&huella), "ni la huella: {}", &config_json.body[..config_json.body.len().min(200)]);
    assert!(config_json.body.contains("tokens_count"), "el mapa se omite y se cuenta");

    // ── (a) el token sirve en el acto y su identidad es AgentToken ───────────
    let con_agente = call(&state, Method::GET, "/api/v1/coloquio/channels", Some(&token), "").await;
    assert!(
        !matches!(con_agente.status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
        "el token recien emitido debe autenticar sin reiniciar, fue {}: {}",
        con_agente.status,
        con_agente.body
    );
    {
        let cfg = state.config.read().await;
        let acl = &cfg.security.acl;
        assert_eq!(
            agent_token_kind(&token, acl),
            Some(AclAuthKind::AgentToken),
            "el bearer debe resolverse como AgentToken"
        );
        assert_eq!(resolve_agent_id_for_token(&token, acl), "deep", "la vinculacion hash -> agent_id");
        assert_eq!(resolve_role_for_token(&token, acl), "writer", "el rol pedido, no default_role");
    }

    // Presentar la huella guardada no autentica: quien puede leer `tylluan.toml`
    // no entra por ello (guardia de D225, ahora probada con un token real).
    let replay = call(&state, Method::GET, "/api/v1/coloquio/channels", Some(&huella), "").await;
    assert!(
        matches!(replay.status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
        "la huella guardada no puede ser un bearer: {}",
        replay.status
    );

    // ── (c) un token de agente no emite tokens (clase Operador) ──────────────
    // Se vuelve a leer el ETag vigente para que lo UNICO que pueda rechazar la
    // peticion sea el gate de clase Operador: si alguien quita la entrada de
    // `operator_class_required`, este intento devuelve 201 y el test ruge.
    let etag_nuevo = {
        let r = call(&state, Method::GET, "/api/v1/config", Some(MAESTRO), "").await;
        r.headers
            .get("x-config-etag")
            .and_then(|v| v.to_str().ok())
            .expect("x-config-etag tras emitir")
            .to_string()
    };
    let escalar = call(
        &state,
        Method::POST,
        "/api/v1/acl/tokens",
        Some(&token),
        &format!(r#"{{"agent_id":"otro","role":"admin","etag":"{etag_nuevo}"}}"#),
    )
    .await;
    assert!(
        matches!(escalar.status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN),
        "emitir con un token de agente debe ser 401/403 (clase Operador), fue {}: {}",
        escalar.status,
        escalar.body
    );

    // El config sigue sin llevar el token despues de todos los intentos fallidos.
    let final_ = std::fs::read_to_string(&cfg_path).expect("leer config final");
    assert!(!final_.contains(&token), "el token en claro quedo en el config al final");
    assert_eq!(
        final_.matches(&format!("\"{huella}\"")).count(),
        2,
        "la huella aparece exactamente en tokens y en token_agent_bindings"
    );
}
