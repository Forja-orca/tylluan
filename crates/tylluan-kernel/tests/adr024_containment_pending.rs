//! ADR-024 — contención de ejecución de procesos por HTTP: **tests de regresión
//! escritos en ROJO**, para que aprobar sea solo activar.
//!
//! Origen: S-BUFFY-1 (decisión de José 2026-10-26: la seguridad no está
//! congelada). Cada control del ADR
//! (`docs/reference/adr/ADR024_process_execution_containment.md`, §4) tiene un
//! caso aquí que **falla hoy** y debe pasar con el arreglo descrito. Todos
//! llevan `#[ignore = "ADR-024 <ítem>: pendiente de aprobación"]`: la suite
//! normal queda en verde y CI no se rompe mientras José no decida.
//!
//! Cómo se ejecutan (uno a uno, con la salida roja pegada al informe):
//!
//! ```text
//! CARGO_TARGET_DIR=/e/cargo-target/buffy cargo test -p tylluan-kernel \
//!   --test adr024_containment_pending -- --ignored --nocapture
//! ```
//!
//! Aislamiento (regla de la máquina): la config se desvía a un `tylluan.toml`
//! temporal con `TylluanConfig::set_config_path_override`, así que **nunca** se
//! escribe el `tylluan.toml` del repo ni el de producción. No se arranca ningún
//! kernel ni se abre ningún puerto; los handlers se conducen in-process con el
//! router real (`api_v1_routes`). Los tests se serializan porque tocan
//! variables de proceso (`APPDATA`/`USERPROFILE`) y el override de config.
//!
//! Nota sobre el alcance de A1 y C3: A1 se comprueba por el **rechazo en el
//! alta** (el argv con wrapper de shell no debe aceptarse) y C3 por la **clase
//! de ruta** (`operator_class_required`, `auth.rs`), que es donde el ADR ubica
//! su test. La matriz completa de comportamiento de C3 (agente real + router
//! con middleware de auth) necesita un harness de config con `acl.roles`
//! sembrado; se deja anotada en el informe como paso siguiente, no simulada
//! aquí para no fingir cobertura.

mod support;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;
use tylluan_kernel::config::TylluanConfig;
use tylluan_kernel::transport::http::api_v1::api_v1_routes;
use tylluan_kernel::transport::http::auth::operator_class_required;
use tylluan_kernel::transport::http::HttpState;

const MASTER: &str = "adr024-master-token";

/// Serializa los tests: tocan variables de proceso y un `OnceLock` global.
static GUARD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Directorio temporal propio por propósito (nunca el repo).
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tylluan_adr024_{tag}_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("crear directorio temporal");
    dir
}

/// Desvía la config del kernel a un fichero temporal. Idempotente: el
/// `OnceLock` del override se queda con el primero, que es el temporal.
fn isolate_config() -> PathBuf {
    let path = scratch("config").join("tylluan.toml");
    if !path.exists() {
        // TylluanConfig completa el resto con sus defaults de serde.
        std::fs::write(&path, "# ADR-024: config aislada de test\n")
            .expect("escribir tylluan.toml temporal");
    }
    TylluanConfig::set_config_path_override(path.clone());
    path
}

async fn adr024_state() -> Arc<HttpState> {
    support::test_state_custom("adr024", Some(MASTER.to_string()), false).await
}

fn app(state: Arc<HttpState>) -> axum::Router {
    axum::Router::new().merge(api_v1_routes()).with_state(state)
}

async fn post_json(
    app: axum::Router,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::AUTHORIZATION, format!("Bearer {MASTER}"))
        .body(Body::from(body.to_string()))
        .expect("construir request");
    let res = app.oneshot(req).await.expect("respuesta del router");
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20)
        .await
        .expect("leer body");
    let parsed = serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        json!({ "raw": String::from_utf8_lossy(&bytes).to_string() })
    });
    (status, parsed)
}

// ---------------------------------------------------------------------------
// A4 — alta/edición de MCP externo: confirmación humana + auditoría del diff
// ---------------------------------------------------------------------------

/// **ADR-024 A4 (rojo hoy).** Dar de alta un servidor MCP por stdio **sin**
/// confirmación explícita debe ser 400: el kernel no puede registrar ni
/// arrancar un proceso a partir de un body HTTP sin que alguien confirme el
/// comando. Hoy el handler registra, intenta arrancar y persiste: 201.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "ADR-024 A4: pendiente de aprobación"]
async fn a4_external_mcp_add_without_confirmation_is_rejected() {
    let _g = GUARD.lock().await;
    isolate_config();
    let app = app(adr024_state().await);

    let (status, body) = post_json(
        app,
        "/api/v1/mcp/external",
        json!({
            "name": "adr024-a4",
            "command": "C:\\tylluan-no-existe\\adr024.exe",
            "args": ["--flag"]
        }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "ADR-024 A4: sin confirmación explícita del comando el alta debe ser 400; \
         hoy responde {status} y el cuerpo es {body}"
    );
    assert!(
        body.to_string().to_lowercase().contains("confirm"),
        "ADR-024 A4: el 400 debe explicar que falta la confirmación del comando; cuerpo: {body}"
    );
}

// ---------------------------------------------------------------------------
// D1 — descubrimiento de MCP: propuesta, no registro automático
// ---------------------------------------------------------------------------

/// **ADR-024 D1 (rojo hoy).** Un `mcpServers` encontrado en un perfil de
/// terceros debe volver como **propuesta** (`discovered = 0`, lista de
/// propuestas), no registrarse solo. Hoy `discover` importa el server y lo
/// persiste en config (`discovered = 1`) sin intervención humana.
///
/// El perfil de terceros es sintético: `APPDATA`/`USERPROFILE` se desvían a un
/// temporal, así que el escaneo no lee los ficheros reales de la máquina.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "ADR-024 D1: pendiente de aprobación"]
async fn d1_discover_proposes_without_registering() {
    let _g = GUARD.lock().await;
    isolate_config();
    let root = scratch("discover");
    let appdata = root.join("appdata");
    let profile = root.join("profile");
    std::fs::create_dir_all(appdata.join("Claude")).expect("appdata/Claude");
    std::fs::create_dir_all(&profile).expect("profile");
    std::fs::write(
        appdata.join("Claude").join("claude_desktop_config.json"),
        json!({
            "mcpServers": {
                "adr024-ghost": {
                    "command": "C:\\tylluan-no-existe\\ghost.exe",
                    "args": ["--serve"]
                }
            }
        })
        .to_string(),
    )
    .expect("escribir perfil sintético");

    // SAFETY (set_var): edition-2024 unsafe op; el test está serializado por
    // GUARD y este binario no tiene otros consumidores de estas variables.
    unsafe {
        std::env::set_var("APPDATA", &appdata);
        std::env::set_var("USERPROFILE", &profile);
    }

    let app = app(adr024_state().await);
    let (status, body) = post_json(app, "/api/v1/mcp/external/discover", json!({})).await;

    assert_eq!(status, StatusCode::OK, "discover responde 200; cuerpo: {body}");
    assert_eq!(
        body.get("discovered").and_then(|v| v.as_u64()),
        Some(0),
        "ADR-024 D1: discover no debe registrar nada sin aprobación; hoy discovered = {:?} \
         (servers = {:?})",
        body.get("discovered"),
        body.get("servers")
    );
    assert!(
        body.get("proposals").map(|p| p.is_array()).unwrap_or(false)
            || body
                .get("servers")
                .map(|s| s.as_array().map(|a| a.is_empty()).unwrap_or(false))
                .unwrap_or(false),
        "ADR-024 D1: la respuesta debe traer propuestas (o una lista vacía de registrados); \
         cuerpo: {body}"
    );
}

// ---------------------------------------------------------------------------
// B1 — `POST /api/v1/config` deja de ser camino para sembrar procesos
// ---------------------------------------------------------------------------

/// **ADR-024 B1 (rojo hoy).** Guardar por API un TOML con
/// `[[external_mcp]].command`, `[services]` o `[wake].command` debe ser 400 y
/// **no** reescribir el fichero: esas claves se editan a mano, no por HTTP.
/// Hoy se escribe tal cual (200) y el proceso queda sembrado para el siguiente
/// arranque.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "ADR-024 B1: pendiente de aprobación"]
async fn b1_config_api_rejects_process_execution_keys() {
    let _g = GUARD.lock().await;
    let config_path = isolate_config();
    let app = app(adr024_state().await);

    let poisoned = r#"
[[external_mcp]]
name = "adr024-b1"
command = "C:\\tylluan-no-existe\\b1.exe"
args = ["--serve"]
active = true
"#;

    let (status, body) = post_json(app, "/api/v1/config", json!({ "content": poisoned })).await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "ADR-024 B1: sembrar `[[external_mcp]].command` por la API debe ser 400; hoy {status} \
         (cuerpo: {body})"
    );
    let on_disk = std::fs::read_to_string(&config_path).unwrap_or_default();
    assert!(
        !on_disk.contains("adr024-b1"),
        "ADR-024 B1: el fichero no debe reescribirse cuando se rechaza; contiene: {on_disk}"
    );
}

// ---------------------------------------------------------------------------
// A1 — sin `cmd /c` para binarios nativos; metacaracteres rechazados
// ---------------------------------------------------------------------------

/// **ADR-024 A1 (rojo hoy).** Un `command` con metacaracteres de shell no debe
/// aceptarse en el alta: hoy viaja a `cmd /c <command>` y el shell del sistema
/// interpreta lo que venga. Debe ser 400 (o, con la ruta absoluta verificada,
/// spawn directo sin intérprete y metacaracteres rechazados).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "ADR-024 A1: pendiente de aprobación"]
async fn a1_shell_metacharacters_in_command_are_rejected() {
    let _g = GUARD.lock().await;
    isolate_config();
    let app = app(adr024_state().await);

    let (status, body) = post_json(
        app,
        "/api/v1/mcp/external",
        json!({
            "name": "adr024-a1",
            "command": "C:\\tylluan-no-existe\\a1.exe | rem adr024-a1",
            "args": []
        }),
    )
    .await;

    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "ADR-024 A1: un command con metacaracteres de shell debe rechazarse al alta; hoy {status} \
         (cuerpo: {body})"
    );
}

// ---------------------------------------------------------------------------
// C3 — endpoint directo de guild: operador para bash/filesystem
// ---------------------------------------------------------------------------

/// **ADR-024 C3 (rojo hoy).** `POST /api/v1/guilds/{guild}/call/{tool}` y su
/// alias `/tools/` deben ser **operator-class** cuando la guild es `bash` o
/// `filesystem`: un agente no puede ejecutar comandos ni leer ficheros por la
/// puerta HTTP directa saltándose `tylluan_do`. Hoy la clase de ruta es la de
/// agente y el ACL solo actúa si `acl.roles` no está vacío.
///
/// Se comprueba la clase de ruta (donde el ADR ubica el test de C3) y no el
/// 403 real: la matriz agente/operador con el router autenticado exige sembrar
/// `acl.roles` y el contexto de auth, que se propone como paso siguiente.
#[test]
#[ignore = "ADR-024 C3: pendiente de aprobación"]
fn c3_direct_guild_endpoint_is_operator_class_for_sensitive_guilds() {
    for (method, path, guild) in [
        ("POST", "/api/v1/guilds/bash/call/execute", "bash"),
        ("POST", "/api/v1/guilds/bash/tools/execute", "bash"),
        ("POST", "/api/v1/guilds/filesystem/call/read_file", "filesystem"),
        ("POST", "/api/v1/guilds/filesystem/tools/write_file", "filesystem"),
    ] {
        assert!(
            operator_class_required(method, path),
            "ADR-024 C3: '{guild}' por endpoint directo debe exigir token de operador; \
             {method} {path} es hoy agent-class"
        );
    }
    // Guarda de la otra cara: las guilds que el dashboard y el juez usan por
    // `/call/` deben seguir siendo agent-class (T-DSH-71).
    for path in [
        "/api/v1/guilds/vision/call/caption_image",
        "/api/v1/guilds/llama_backend/call/generate",
    ] {
        assert!(
            !operator_class_required("POST", path),
            "ADR-024 C3: '{path}' debe seguir siendo agent-class (dashboard de visión y juez)"
        );
    }
}
