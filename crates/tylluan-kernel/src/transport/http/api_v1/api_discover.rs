//! D-152: autodescubrimiento de proveedores de inferencia en loopback.
//!
//! Lo que pide Jose (T3277): la pestana Inferencia debe ensenar los proveedores que YA existen
//! en su maquina (LM Studio, llama.cpp, Ollama) sin que el los declare a mano. Este endpoint los
//! sondea y devuelve lo que encuentra; la UI pinta lo que responda esta ruta, no una lista fija.
//!
//! Reglas de diseno, y el motivo de cada una:
//! 1. **Solo loopback.** Las direcciones son constantes de este fichero (`127.0.0.1`): este
//!    endpoint no acepta URLs del llamante, asi que no puede convertirse en un SSRF hacia su red.
//!    Lo que el operador configure aparte (`[external_providers]`) se lista, pero **no se sondea
//!    aqui** si su host no es loopback.
//! 2. **Timeout corto (600 ms) y en paralelo.** Es una peticion de UI: si un proveedor no esta,
//!    tiene que decirlo rapido. Un servidor lento no puede retrasar la pestana.
//! 3. **Solo lectura.** Se piden dos listados publicos (`/v1/models` y `/api/tags`) y nada mas: no
//!    se envia ninguna credencial, no se crea ni se modifica nada.
//! 4. **No devuelve claves.** De los providers configurados solo se expone `api_key_env` (el
//!    NOMBRE de la variable) y `api_key_set` (si existe), nunca el valor. Misma regla que
//!    `list_external_providers_handler`.

use axum::{extract::State, response::IntoResponse, Json};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::transport::http::HttpState;

/// Timeout por sonda. Corto a proposito: es para pintar una pestana, no para esperar a un
/// servidor que este arrancando.
const PROBE_TIMEOUT: Duration = Duration::from_millis(600);

/// Un candidato a sondear: identificador estable, nombre para la UI y URL base.
struct Candidate {
    id: &'static str,
    label: &'static str,
    base_url: String,
    dialect: Dialect,
}

#[derive(Clone, Copy)]
enum Dialect {
    /// OpenAI compatible (`GET /v1/models`).
    OpenAiCompat,
    /// Ollama (`GET /api/tags`).
    Ollama,
}

/// Los tres que pidio Jose. `llama.cpp` usa el puerto que el TOML declare para el servidor del
/// usuario si esta puesto; si no, 9000, el default del lanzador (`api_inference.rs`).
fn candidates(configured_llama_port: Option<u16>) -> [Candidate; 3] {
    [
        Candidate {
            id: "lm-studio",
            label: "LM Studio",
            base_url: "http://127.0.0.1:1234".to_string(),
            dialect: Dialect::OpenAiCompat,
        },
        Candidate {
            id: "llama-cpp",
            label: "llama.cpp del usuario",
            base_url: format!("http://127.0.0.1:{}", configured_llama_port.unwrap_or(9000)),
            dialect: Dialect::OpenAiCompat,
        },
        Candidate {
            id: "ollama",
            label: "Ollama",
            base_url: "http://127.0.0.1:11434".to_string(),
            dialect: Dialect::Ollama,
        },
    ]
}

/// Extrae la lista de modelos de una respuesta `/v1/models` o `/api/tags`.
///
/// Se hace a mano y tolerante: si el proveedor contesta otra cosa, se devuelve vacio en vez de
/// error. Un listado vacio es informacion util; un 500 no.
pub fn parse_models(dialect_is_ollama: bool, body: &serde_json::Value) -> Vec<String> {
    let listas = if dialect_is_ollama {
        body.get("models")
    } else {
        body.get("data")
    };
    let Some(items) = listas.and_then(|l| l.as_array()) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|m| {
            // OpenAI: {"id": "..."} ; Ollama: {"name": "..."} o {"model": "..."}
            m.get("id")
                .or_else(|| m.get("name"))
                .or_else(|| m.get("model"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// Puerto del llama.cpp del usuario, sacado de `[inference.points.*]`.
///
/// **Solo se aceptan endpoints de loopback**: un punto que apunte a `https://api.openai.com:443`
/// no puede hacer que la sonda acabe pidiendo `127.0.0.1:443`. Es el punto 3 de la revision del TL
/// (T3286) y por eso vive en una funcion pura con test propio.
pub fn llama_port_from_points(endpoints: &[&str]) -> Option<u16> {
    endpoints
        .iter()
        .filter_map(|e| {
            let sin_esquema = e
                .trim()
                .trim_start_matches("http://")
                .trim_start_matches("https://");
            let autoridad = sin_esquema.split('/').next().unwrap_or("");
            let (host, puerto) = autoridad.rsplit_once(':')?;
            let host = host.trim_matches(['[', ']']);
            let es_loopback = host.eq_ignore_ascii_case("localhost")
                || host == "127.0.0.1"
                || host == "::1";
            if !es_loopback {
                return None;
            }
            puerto.parse::<u16>().ok()
        })
        .next()
}

/// Tope de cuerpo por sonda (punto 2 del TL, T3286): 256 KiB. Un listado de modelos real son
/// unos pocos KB; por encima de esto, algo va mal y no se lee mas.
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Lee el cuerpo con tope. Devuelve (JSON si se pudo parsear, si se trunco).
///
/// Se lee por trozos porque `resp.json()` no tiene limite: un servidor local (o uno que se haga
/// pasar por local) podria mandar un cuerpo enorme y comerse la memoria del kernel.
async fn read_body_capped(
    resp: reqwest::Response,
    max_bytes: usize,
) -> (Option<serde_json::Value>, bool) {
    let mut acumulado: Vec<u8> = Vec::new();
    let mut resp = resp;
    let mut truncado = false;
    while let Ok(Some(trozo)) = resp.chunk().await {
        if acumulado.len() + trozo.len() > max_bytes {
            let hueco = max_bytes.saturating_sub(acumulado.len());
            acumulado.extend_from_slice(&trozo[..hueco]);
            truncado = true;
            break;
        }
        acumulado.extend_from_slice(&trozo);
    }
    let valor = serde_json::from_slice::<serde_json::Value>(&acumulado).ok();
    (valor, truncado)
}

async fn probe(client: &reqwest::Client, cand: &Candidate) -> serde_json::Value {
    let (path, is_ollama) = match cand.dialect {
        Dialect::OpenAiCompat => ("/v1/models", false),
        Dialect::Ollama => ("/api/tags", true),
    };
    let url = format!("{}{}", cand.base_url, path);
    let inicio = Instant::now();
    let respuesta = client.get(&url).send().await;
    let latencia_ms = inicio.elapsed().as_millis() as u64;

    match respuesta {
        Ok(resp) if resp.status().is_success() => {
            // Punto 2 del TL: tope de cuerpo ANTES de parsear. Un servidor local comprometido
            // podria contestar un JSON de cientos de MB; se lee por trozos y se corta.
            let (cuerpo, truncado) = read_body_capped(resp, MAX_BODY_BYTES).await;
            let modelos = cuerpo
                .as_ref()
                .map(|v| parse_models(is_ollama, v))
                .unwrap_or_default();
            serde_json::json!({
                "id": cand.id,
                "label": cand.label,
                "base_url": cand.base_url,
                "detected": true,
                "latency_ms": latencia_ms,
                "models": modelos,
                "models_count": modelos.len(),
                "body_truncated": truncado,
            })
        }
        Ok(resp) => serde_json::json!({
            "id": cand.id,
            "label": cand.label,
            "base_url": cand.base_url,
            "detected": false,
            "latency_ms": latencia_ms,
            "models": [],
            "models_count": 0,
            "detail": format!("HTTP {}", resp.status().as_u16()),
        }),
        Err(e) => serde_json::json!({
            "id": cand.id,
            "label": cand.label,
            "base_url": cand.base_url,
            "detected": false,
            "latency_ms": latencia_ms,
            "models": [],
            "models_count": 0,
            // "no contesta" es el caso normal de un proveedor que no esta arrancado.
            "detail": if e.is_timeout() { "timeout".to_string() } else { "sin respuesta".to_string() },
        }),
    }
}

/// `GET /api/v1/inference/providers/discover` — proveedores de loopback detectados.
pub async fn discover_providers_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    // Puerto del llama.cpp del usuario: si el TOML lo declara en algun punto de inferencia, se usa.
    let (llama_port, configurados) = {
        let config = state.config.read().await;
        // Punto 3 del TL: el puerto sale de los puntos de inferencia SOLO si son loopback, y el
        // default pegado al del lanzador (9000, `api_inference.rs:139`), no 8080 inventado.
        let endpoints: Vec<&str> = [
            config.inference.points.decision.endpoint.as_deref(),
            config.inference.points.grammar_judge.endpoint.as_deref(),
            config.inference.points.sentinel.endpoint.as_deref(),
            config.inference.points.chat.endpoint.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        let puerto = llama_port_from_points(&endpoints)
            .or_else(|| {
                let puerto_cfg = config.inference.llama.port;
                if puerto_cfg == 0 { None } else { Some(puerto_cfg) }
            });
        let externos: Vec<serde_json::Value> = config
            .external_providers
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "type": p.provider_type,
                    "base_url": p.base_url,
                    // NOMBRE de la variable, nunca el valor. Misma regla que /external-providers.
                    "api_key_env": p.api_key_env,
                    "api_key_set": std::env::var(&p.api_key_env).is_ok(),
                    "models": p.models,
                })
            })
            .collect();
        (puerto, externos)
    };

    // Punto 1 del TL (T3286): sin proxy y sin seguir redirecciones.
    // - `no_proxy()`: si el entorno tiene HTTP_PROXY, una sonda "a loopback" saldria por el proxy.
    // - `Policy::none()`: un servidor local podria redirigir a fuera y convertir esto en un SSRF.
    let client = match reqwest::Client::builder()
        .timeout(PROBE_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("no se pudo crear el cliente HTTP: {e}") })),
            );
        }
    };

    // En paralelo: tres sondas de 600 ms en serie serian 1,8 s de pestana. Son exactamente tres,
    // asi que `tokio::join!` basta y evita mover futuros que piden prestado `client`/`candidatos`
    // (que es lo que rompia al meterlos en un JoinSet).
    let [c0, c1, c2] = candidates(llama_port);
    let (d0, d1, d2) = tokio::join!(probe(&client, &c0), probe(&client, &c1), probe(&client, &c2));
    let detectados: Vec<serde_json::Value> = vec![d0, d1, d2];

    let alguno = detectados
        .iter()
        .any(|p| p.get("detected").and_then(|d| d.as_bool()).unwrap_or(false));

    (
        axum::http::StatusCode::OK,
        Json(serde_json::json!({
        "scanned": detectados.len(),
        "any_detected": alguno,
        "providers": detectados,
        // Lo que el operador ya declaro. Se expone aparte para que la UI pueda separar
        // "detectado por sonda" de "configurado a mano" sin mezclarlos.
        "configured_external": configurados,
        "how_to_read": "providers son los sondeados en loopback (solo lectura, timeout 600 ms). \
                        configured_external es lo declarado en [external_providers]; de ahi solo \
                        viaja el NOMBRE de la variable de entorno de la clave y si esta puesta, \
                        nunca el valor.",
        })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_modelos_de_openai_compatible() {
        let cuerpo = serde_json::json!({
            "object": "list",
            "data": [
                {"id": "qwen2.5-coder-7b", "object": "model"},
                {"id": "llama-3.1-8b", "object": "model"}
            ]
        });
        assert_eq!(
            parse_models(false, &cuerpo),
            vec!["qwen2.5-coder-7b".to_string(), "llama-3.1-8b".to_string()]
        );
    }

    #[test]
    fn parsea_modelos_de_ollama() {
        let cuerpo = serde_json::json!({
            "models": [
                {"name": "gemma3:4b", "model": "gemma3:4b"},
                {"name": "qwen3:8b", "model": "qwen3:8b"}
            ]
        });
        assert_eq!(
            parse_models(true, &cuerpo),
            vec!["gemma3:4b".to_string(), "qwen3:8b".to_string()]
        );
    }

    #[test]
    fn respuesta_inesperada_da_lista_vacia_y_no_error() {
        // Un proveedor que contesta algo que no es un listado (p. ej. su raiz HTML) no puede
        // romper la pestana: lista vacia, y la UI mostrara "detectado sin modelos".
        assert!(parse_models(false, &serde_json::json!({"status": "ok"})).is_empty());
        assert!(parse_models(true, &serde_json::json!("texto")).is_empty());
        assert!(parse_models(false, &serde_json::json!(null)).is_empty());
    }

    #[test]
    fn candidatos_incluyen_los_tres_de_la_peticion_y_respetan_el_puerto_de_llamacpp() {
        let por_defecto = candidates(None);
        assert_eq!(por_defecto.len(), 3);
        assert_eq!(por_defecto[0].base_url, "http://127.0.0.1:1234"); // LM Studio
        assert_eq!(por_defecto[1].base_url, "http://127.0.0.1:9000"); // llama.cpp: mismo default que el lanzador
        assert_eq!(por_defecto[2].base_url, "http://127.0.0.1:11434"); // Ollama

        let con_puerto = candidates(Some(9000));
        assert_eq!(con_puerto[1].base_url, "http://127.0.0.1:9000");
    }

    /// Punto 3 del TL: un endpoint REMOTO no puede decidir el puerto que sondeamos en loopback.
    #[test]
    fn puerto_de_llamacpp_ignora_endpoints_que_no_son_loopback() {
        // El caso que preocupaba al TL: un punto en la nube con :443 haria sondear 127.0.0.1:443.
        assert_eq!(llama_port_from_points(&["https://api.openai.com:443"]), None);
        assert_eq!(llama_port_from_points(&["http://10.0.0.5:8080"]), None);
        // Loopback en sus tres formas: si se acepta.
        assert_eq!(llama_port_from_points(&["http://127.0.0.1:8080/v1"]), Some(8080));
        assert_eq!(llama_port_from_points(&["http://localhost:9000"]), Some(9000));
        assert_eq!(llama_port_from_points(&["http://[::1]:9100"]), Some(9100));
        // Mezcla: el primero valido manda, y uno remoto antes no rompe nada.
        assert_eq!(
            llama_port_from_points(&["https://api.anthropic.com:443", "http://127.0.0.1:9100"]),
            Some(9100)
        );
        // Sin puerto explicito no hay nada que sondear (no se inventa un default aqui).
        assert_eq!(llama_port_from_points(&["http://127.0.0.1"]), None);
    }
}
