//! T-DSH-116 paso 2: runtime de los puntos de inferencia configurables.
//!
//! Traduce `[inference.points.<punto>]` (config) a una llamada HTTP real segun
//! el dialecto del proveedor, con dos invariantes:
//! - **Nube opt-in por punto**: si el punto resuelve a nube y
//!   `cloud_enabled = false`, se devuelve error ANTES de abrir ninguna conexion.
//! - **Ultimo recurso local**: los llamantes (juez de gramatica, decision) caen
//!   al camino local (servidor efimero / reglas) cuando esto devuelve error.
//!
//! Dialectos de chat soportados aqui: OpenAI compatible
//! (`/v1/chat/completions`), Anthropic compatible (`/v1/messages`), Ollama
//! (`/api/chat`) y el llama-server del usuario (OpenAI compatible). El dialecto
//! de decision SystemOne se resuelve en `router::decision` (wire tipado), no
//! aqui.

use crate::config::{InferencePointConfig, TylluanConfig};

/// Locality efectiva de un punto (ADR-020 E-2/E-3): `systemone_http` es local
/// mientras el endpoint sea loopback; solo es nube si el endpoint no lo es.
pub fn effective_is_cloud(point: &InferencePointConfig) -> bool {
    point.is_cloud()
}

/// Lee la clave del proveedor desde su variable de entorno (nunca del TOML).
fn api_key(point: &InferencePointConfig) -> Option<String> {
    point
        .api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .and_then(|var| std::env::var(var).ok())
        .filter(|v| !v.trim().is_empty())
}

fn trim_slash(base: &str) -> &str {
    base.trim_end_matches('/')
}

async fn post_json(
    client: &reqwest::Client,
    url: &str,
    headers: &[(&str, String)],
    body: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut req = client.post(url).json(body);
    for (k, v) in headers {
        req = req.header(*k, v);
    }
    let resp = req.send().await.map_err(|e| format!("punto: error HTTP: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("punto: HTTP {}", resp.status()));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| format!("punto: respuesta no JSON: {e}"))
}

/// Completion de texto para un punto. `Err` = el llamante debe caer al camino
/// local (E-5), nunca a la nube por su cuenta.
pub async fn complete_for_point(
    point_name: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let cfg = {
        let lock = TylluanConfig::load_cached().map_err(|e| format!("punto: config: {e}"))?;
        lock.read().await.clone()
    };
    let point = match point_name {
        "decision" => cfg.inference.points.decision.clone(),
        "grammar_judge" => cfg.inference.points.grammar_judge.clone(),
        "sentinel" => cfg.inference.points.sentinel.clone(),
        "chat" => cfg.inference.points.chat.clone(),
        other => return Err(format!("punto desconocido: {other}")),
    };
    complete_with_config(&cfg, &point, prompt, max_tokens).await
}

async fn complete_with_config(
    cfg: &TylluanConfig,
    point: &InferencePointConfig,
    prompt: &str,
    max_tokens: u32,
) -> Result<String, String> {
    let provider = point.provider();
    // Guard duro de nube: ni una conexion si el punto no la ha activado.
    if effective_is_cloud(point) && !point.cloud_enabled {
        return Err(format!(
            "punto: '{provider}' resuelve a nube y cloud_enabled=false (no se abre conexion)"
        ));
    }
    if provider == "local_ephemeral" {
        return Err("punto: local_ephemeral (usar el camino local)".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("punto: cliente HTTP: {e}"))?;

    let (endpoint, model) = if provider == "user_llama_server" {
        let endpoint = point
            .endpoint
            .clone()
            .unwrap_or_else(|| cfg.inference.llama.endpoint.clone());
        let model = point
            .model
            .clone()
            .unwrap_or_else(|| cfg.inference.llama.provider.clone());
        (endpoint, model)
    } else {
        let endpoint = point
            .endpoint
            .clone()
            .ok_or_else(|| format!("punto: '{provider}' sin endpoint"))?;
        let model = point.model.clone().unwrap_or_default();
        (endpoint, model)
    };
    let key = api_key(point);

    match provider {
        "user_llama_server" | "openai_compatible" => {
            let url = format!("{}/chat/completions", trim_slash(&endpoint));
            let mut headers: Vec<(&str, String)> = vec![("content-type", "application/json".into())];
            if let Some(k) = &key {
                headers.push(("authorization", format!("Bearer {k}")));
            }
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "max_tokens": max_tokens,
                "temperature": 0.0,
                "stream": false,
            });
            let json = post_json(&client, &url, &headers, &body).await?;
            json.pointer("/choices/0/message/content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| "punto: respuesta OpenAI sin choices[0].message.content".to_string())
        }
        "anthropic_compatible" => {
            let url = format!("{}/messages", trim_slash(&endpoint));
            let mut headers: Vec<(&str, String)> = vec![
                ("content-type", "application/json".into()),
                ("anthropic-version", "2023-06-01".into()),
            ];
            if let Some(k) = &key {
                headers.push(("x-api-key", k.clone()));
            }
            let body = serde_json::json!({
                "model": model,
                "max_tokens": max_tokens,
                "messages": [{"role": "user", "content": prompt}],
            });
            let json = post_json(&client, &url, &headers, &body).await?;
            json.pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| "punto: respuesta Anthropic sin content[0].text".to_string())
        }
        "ollama_compatible" => {
            let url = format!("{}/api/chat", trim_slash(&endpoint));
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": prompt}],
                "stream": false,
            });
            let json = post_json(&client, &url, &[], &body).await?;
            json.pointer("/message/content")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .ok_or_else(|| "punto: respuesta Ollama sin message.content".to_string())
        }
        "systemone_http" => Err(
            "punto: systemone_http es dialecto de decision (wire tipado), no de chat".to_string(),
        ),
        other => Err(format!("punto: proveedor no soportado en chat: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Servidor HTTP falso de loopback: responde `response` a cualquier POST y
    /// cuenta peticiones. Se apaga al soltar el handle.
    struct FakeServer {
        url: String,
        hits: Arc<AtomicUsize>,
        _handle: tokio::task::JoinHandle<()>,
    }

    async fn fake_server(response: serde_json::Value) -> FakeServer {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_task = hits.clone();
        let response = response.to_string();
        let handle = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                let hits = hits_task.clone();
                let response = response.clone();
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = vec![0u8; 8192];
                    let _ = socket.read(&mut buf).await;
                    hits.fetch_add(1, Ordering::SeqCst);
                    let body = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response.len(),
                        response
                    );
                    let _ = socket.write_all(body.as_bytes()).await;
                });
            }
        });
        FakeServer {
            url: format!("http://{addr}"),
            hits,
            _handle: handle,
        }
    }

    fn base_cfg() -> TylluanConfig {
        TylluanConfig::default()
    }

    fn point(provider: &str, endpoint: &str) -> InferencePointConfig {
        InferencePointConfig {
            provider: Some(provider.to_string()),
            endpoint: Some(endpoint.to_string()),
            model: Some("m".to_string()),
            api_key_env: None,
            cloud_enabled: false,
        }
    }

    #[tokio::test]
    async fn openai_dialect_parses_choices() {
        let srv = fake_server(serde_json::json!({
            "choices": [{"message": {"content": "SAFE"}}]
        }))
        .await;
        let mut p = point("openai_compatible", &srv.url);
        p.cloud_enabled = true;
        let out = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap();
        assert_eq!(out, "SAFE");
        assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn anthropic_dialect_parses_content() {
        let srv = fake_server(serde_json::json!({
            "content": [{"type": "text", "text": "SUSPICIOUS"}]
        }))
        .await;
        let mut p = point("anthropic_compatible", &srv.url);
        p.cloud_enabled = true;
        let out = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap();
        assert_eq!(out, "SUSPICIOUS");
        assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn ollama_dialect_parses_message() {
        let srv = fake_server(serde_json::json!({"message": {"content": "ok"}})).await;
        let mut p = point("ollama_compatible", &srv.url);
        p.cloud_enabled = true;
        let out = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap();
        assert_eq!(out, "ok");
        assert_eq!(srv.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cloud_disabled_never_opens_a_connection() {
        let srv = fake_server(serde_json::json!({"choices": [{"message": {"content": "x"}}]})).await;
        // openai_compatible es nube: sin cloud_enabled el guard corta ANTES de
        // tocar la red, aunque el endpoint sea loopback.
        let p = point("openai_compatible", &srv.url);
        let err = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap_err();
        assert!(err.contains("cloud_enabled=false"), "{err}");
        assert_eq!(srv.hits.load(Ordering::SeqCst), 0, "no debe abrir conexion");
    }

    #[tokio::test]
    async fn local_ephemeral_defers_to_local_path() {
        let p = point("local_ephemeral", "http://127.0.0.1:1");
        let err = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap_err();
        assert!(err.contains("local_ephemeral"), "{err}");
    }

    #[tokio::test]
    async fn systemone_is_decision_dialect_not_chat() {
        let mut p = point("systemone_http", "http://127.0.0.1:8787");
        p.cloud_enabled = false; // loopback => local, no requiere opt-in
        let err = complete_with_config(&base_cfg(), &p, "hola", 8).await.unwrap_err();
        assert!(err.contains("systemone_http"), "{err}");
    }
}
