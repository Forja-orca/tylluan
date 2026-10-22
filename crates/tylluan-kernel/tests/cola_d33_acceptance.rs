//! T-DSH-40 — aceptación ejecutable de D-33 (el juez de la capa 2 deja de morir
//! en silencio).
//!
//! D-33 (cola de Deep, 2026-10-23) tiene dos defectos medidos por Buffy:
//!  (1) con un backend externo compatible con OpenAI (varios modelos cargados,
//!      p. ej. LM Studio), la petición del guild `llama_backend` va **sin el
//!      campo `model`** y el backend responde 400;
//!  (2) `write_gate.rs` registra el error del juez con `debug!` y sale: la capa 2
//!      queda muerta sin que nada lo muestre (sin contador/estado visible).
//!
//! Este fichero levanta un **backend OpenAI-compatible falso** en el propio test
//! (`TcpListener` en 127.0.0.1:0, sin red externa) que:
//!   - responde `GET /v1/models` con dos modelos;
//!   - responde `400` a `POST /v1/chat/completions` si el cuerpo NO trae `model`;
//!   - responde `200` con una completion válida (`SAFE`) si lo trae.
//!
//! Tests (D-33 cerrado: los tres fijan el comportamiento DESEADO):
//!   - el guild `llama_backend` manda `model` (resuelto de `GET /v1/models`) y
//!     el backend lo acepta: el juez puede emitir veredictos;
//!   - un backend que devuelve 400 siempre deja el fallo VISIBLE en
//!     `components.judge.by_kind` como `backend_error`.
//!
//! Run:
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d33_acceptance
//!   rustup run 1.99.1 cargo test -p tylluan-kernel --test cola_d33_acceptance -- --ignored
//!
//! Sin red externa, sin puertos fijos, sin `cfg(unix)`/`cfg(windows)` en ninguna
//! función; `src/` intacto.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rmcp::model::CallToolRequestParam;
use tylluan_kernel::config::TimeoutsConfig;
use tylluan_kernel::memory::silva::SilvaDB;
use tylluan_kernel::registry::guild_process::{GuildLauncher, GuildProcess};
use tylluan_kernel::security::judge_telemetry::judge_telemetry;
use tylluan_kernel::security::write_gate::spawn_write_gate_judge;

#[derive(Clone, Copy)]
enum Behavior {
    /// Backend OpenAI-compatible de D-33: 400 si falta `model`, 200 si viene.
    OpenAiCompatible,
    /// Devuelve 400 a todo (para probar la visibilidad del fallo del juez).
    Always400,
}

#[derive(Clone)]
struct Recorded {
    method: String,
    path: String,
    body: String,
}

struct FakeServer {
    base: String,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl FakeServer {
    fn post_bodies_to(&self, path_suffix: &str) -> Vec<String> {
        let guard = self.requests.lock().unwrap();
        guard
            .iter()
            .filter(|r| r.method == "POST" && r.path.ends_with(path_suffix))
            .map(|r| r.body.clone())
            .collect()
    }
}

fn read_http_request(stream: &mut TcpStream) -> Option<(String, String, String)> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..pos]).to_string();
            let mut lines = head.lines();
            let request_line = lines.next()?;
            let mut parts = request_line.split_whitespace();
            let method = parts.next()?.to_string();
            let path = parts.next()?.to_string();
            let content_length = head.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                if k.eq_ignore_ascii_case("content-length") {
                    v.trim().parse::<usize>().ok()
                } else {
                    None
                }
            });
            let body_start = pos + 4;
            let mut body = buf[body_start..].to_vec();
            if let Some(len) = content_length {
                while body.len() < len {
                    let n = stream.read(&mut tmp).ok()?;
                    if n == 0 {
                        break;
                    }
                    body.extend_from_slice(&tmp[..n]);
                }
                body.truncate(len);
            }
            return Some((method, path, String::from_utf8_lossy(&body).to_string()));
        }
        if buf.len() > 1 << 20 {
            return None;
        }
    }
}

fn write_http_response(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let resp = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn start_fake(behavior: Behavior) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1:0");
    let port = listener.local_addr().expect("local_addr").port();
    let requests: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
    let reqs = requests.clone();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut stream) = conn else { continue };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let Some((method, path, body)) = read_http_request(&mut stream) else {
                continue;
            };
            let (status, response) = match behavior {
                Behavior::Always400 => (400, r#"{"error":{"message":"model required"}}"#.to_string()),
                Behavior::OpenAiCompatible => {
                    if method == "GET" && path.ends_with("/models") {
                        (
                            200,
                            r#"{"object":"list","data":[{"id":"fake-model-A","object":"model"},{"id":"fake-model-B","object":"model"}]}"#
                                .to_string(),
                        )
                    } else if method == "POST" && path.ends_with("/chat/completions") {
                        let has_model = serde_json::from_str::<serde_json::Value>(&body)
                            .ok()
                            .and_then(|v| v.get("model").cloned())
                            .is_some();
                        if has_model {
                            (
                                200,
                                r#"{"id":"chatcmpl-fake","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"SAFE"},"finish_reason":"stop"}]}"#
                                    .to_string(),
                            )
                        } else {
                            (400, r#"{"error":{"message":"model required by this backend"}}"#.to_string())
                        }
                    } else {
                        (404, r#"{"error":"not found"}"#.to_string())
                    }
                }
            };
            reqs.lock().unwrap().push(Recorded { method, path, body });
            write_http_response(&mut stream, status, &response);
        }
    });
    FakeServer {
        base: format!("http://127.0.0.1:{port}"),
        requests,
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Arranca el guild `llama_backend` REAL (Python, MCP stdio) apuntando al
/// backend falso por la vía de detección legacy (`OPENAI_BASE_URL`).
async fn spawn_llama_backend(fake: &FakeServer, tag: &str) -> GuildProcess {
    let repo = repo_root();
    let mut env = HashMap::new();
    env.insert("OPENAI_BASE_URL".to_string(), format!("{}/v1", fake.base));
    env.insert("PYTHONPATH".to_string(), repo.to_string_lossy().to_string());
    env.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());
    let name = format!("d33-llama-{tag}");
    let mut process = GuildProcess::new(
        &name,
        GuildLauncher::External {
            command: "python".to_string(),
            args: vec!["-m".to_string(), "guilds.core.llama_backend".to_string()],
            cwd: Some(repo.clone()),
            env: Some(env),
        },
        false,
        Some(120_000),
        1,
    );
    process
        .spawn(&repo, &TimeoutsConfig::default())
        .await
        .unwrap_or_else(|e| panic!("no se pudo arrancar el guild llama_backend: {e}"));
    process
}

/// Llama a `query_model` como lo hace el juez: sin `model`.
async fn call_query_model(process: &mut GuildProcess) -> (String, bool) {
    let mut args = serde_json::Map::new();
    args.insert("prompt".to_string(), serde_json::json!("responde SAFE"));
    args.insert("max_tokens".to_string(), serde_json::json!(32));
    let result = process
        .call_tool(CallToolRequestParam {
            name: "query_model".to_string().into(),
            arguments: Some(args),
        })
        .await;
    let value = serde_json::to_value(&result).expect("CallToolResult serializable");
    let text = value
        .pointer("/content/0/text")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let is_error = value
        .get("isError")
        .and_then(|v| v.as_bool())
        .or_else(|| value.get("is_error").and_then(|v| v.as_bool()))
        .unwrap_or(false);
    (text, is_error)
}

fn body_has_model(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model").cloned())
        .is_some()
}

// ---------------------------------------------------------------------------
// Comportamiento ACTUAL (vivo): sin `model` → 400 → el juez moriría en silencio
// ---------------------------------------------------------------------------

/// **D-33 cerrado.** El guild resuelve el modelo con `GET /v1/models` y lo manda
/// en `/v1/chat/completions`: el backend ya no responde 400 y la completion
/// llega. Antes de D-33 la peticion iba sin `model`, el backend devolvia 400 y la
/// capa 2 moria sin que nada lo mostrara.
#[tokio::test(flavor = "multi_thread")]
async fn d33_judge_request_carries_model_and_succeeds() {
    let fake = start_fake(Behavior::OpenAiCompatible);
    let mut process = spawn_llama_backend(&fake, "current").await;
    let (text, is_error) = call_query_model(&mut process).await;

    let bodies = fake.post_bodies_to("/chat/completions");
    assert!(
        !bodies.is_empty(),
        "D-33: el guild debe llamar a /v1/chat/completions; texto={text:?}"
    );
    let last = bodies.last().unwrap();
    assert!(
        body_has_model(last),
        "D-33: la peticion debe llevar 'model' (si no, el backend responde 400 y el juez \
         muere); cuerpo: {last}"
    );
    let model = serde_json::from_str::<serde_json::Value>(last)
        .ok()
        .and_then(|v| v.get("model").cloned())
        .and_then(|m| m.as_str().map(str::to_string))
        .unwrap_or_default();
    assert!(
        model == "fake-model-A" || model == "fake-model-B",
        "D-33: el 'model' debe salir de GET /v1/models; model={model:?}"
    );
    assert!(
        !is_error && text.contains("SAFE"),
        "D-33: con 'model' la completion debe llegar; texto={text:?} is_error={is_error}"
    );

    let _ = process.kill().await;
}

// ---------------------------------------------------------------------------
// D-33 (a) DESEADO: la petición lleva `model` y el veredicto llega
// ---------------------------------------------------------------------------

/// **DESEADO (a).** Con el backend OpenAI-compatible falso (dos modelos en
/// `/v1/models`), la petición del juez debe llevar `model` y la completion
/// (`SAFE`) debe llegar. Hoy falla: la petición va sin `model` y el backend
/// responde 400.
#[tokio::test(flavor = "multi_thread")]
async fn d33_desired_judge_request_carries_model_and_gets_verdict() {
    let fake = start_fake(Behavior::OpenAiCompatible);
    let mut process = spawn_llama_backend(&fake, "desired").await;
    let (text, is_error) = call_query_model(&mut process).await;

    let bodies = fake.post_bodies_to("/chat/completions");
    assert!(
        !bodies.is_empty(),
        "D-33 pendiente: el guild no llegó a /v1/chat/completions; texto={text:?}"
    );
    let last = bodies.last().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(last).unwrap_or_default();
    let model = parsed.get("model").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        !model.is_empty(),
        "D-33 pendiente: la petición sigue sin 'model' (el backend responde 400 y el juez muere); \
         cuerpo: {last}"
    );
    assert!(
        model == "fake-model-A" || model == "fake-model-B",
        "D-33: el 'model' debe salir de GET /v1/models (o de judge_model), no inventado; \
         model={model:?}"
    );
    assert!(
        text.contains("SAFE") && !is_error,
        "D-33: con 'model' la completion debe llegar; texto={text:?} is_error={is_error}"
    );

    let _ = process.kill().await;
}

// ---------------------------------------------------------------------------
// D-33 (b) DESEADO: el fallo del juez es VISIBLE en components.judge.by_kind
// ---------------------------------------------------------------------------

/// **DESEADO (b).** Un backend que devuelve 400 siempre: el fallo del juez debe
/// contar en `judge_telemetry().to_json()["by_kind"]["backend_error"]` (lo que
/// se expone en `GET /health?verbose=true` → `components.judge.by_kind`), no
/// desaparecer en un `debug!`. Hoy esa clave **no existe**: el test falla
/// diciéndolo.
#[tokio::test(flavor = "multi_thread")]
async fn d33_desired_judge_failure_is_visible() {
    let fake = start_fake(Behavior::Always400);
    // El juez llama a `{TYLLUAN_KERNEL_URL}/api/v1/guilds/llama_backend/tools/query_model`.
    // SAFETY: variable de proceso del binario de test.
    unsafe {
        std::env::set_var("TYLLUAN_KERNEL_URL", &fake.base);
    }
    let silva = Arc::new(SilvaDB::in_memory().await.expect("SilvaDB in memory"));

    let telemetry = judge_telemetry();
    let read_counter = |key: &str| -> u64 {
        telemetry.to_json()["by_kind"]
            .get(key)
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    };
    let before_backend = read_counter("backend_error");
    let before_any: u64 = ["http", "timeout", "transport", "parse", "backend_error"]
        .iter()
        .map(|k| read_counter(k))
        .sum();

    spawn_write_gate_judge(silva, "d33-node".to_string(), "texto de prueba".to_string());

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut backend_after = before_backend;
    let mut any_after = before_any;
    while Instant::now() < deadline {
        backend_after = read_counter("backend_error");
        any_after = ["http", "timeout", "transport", "parse", "backend_error"]
            .iter()
            .map(|k| read_counter(k))
            .sum();
        if backend_after > before_backend {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(
        any_after > before_any,
        "D-33: el juez no registró NINGÚN fallo observable (¿no llegó a llamar al backend?)"
    );
    let json = telemetry.to_json();
    assert!(
        json["by_kind"].get("backend_error").is_some(),
        "D-33 pendiente: components.judge.by_kind no tiene la clave 'backend_error'; \
         un 400 del backend del juez queda solo en debug!. by_kind actual: {}",
        json["by_kind"]
    );
    assert!(
        backend_after > before_backend,
        "D-33 pendiente: el 400 del backend no incrementó by_kind.backend_error \
         (antes={before_backend}, después={backend_after})"
    );
}
