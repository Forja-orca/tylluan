use axum::{
    Json,
    extract::{Query, State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use std::fs;
use std::collections::HashMap;
use serde::Deserialize;
use crate::transport::http::{HttpState, SaveConfigRequest};
use rmcp::model::CallToolRequestParam;

/// Result of a real MCP ping against a provider's backend.
#[derive(Debug, serde::Serialize)]
pub struct ProviderTestResult {
    pub ok: bool,
    pub status: String,
    pub provider: String,
    pub mcp_server: String,
    pub model_id: String,
    pub endpoint: String,
    pub http_status: u16,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Make a real MCP ping against an HTTP Streamable MCP server URL.
/// Used by the test endpoint and directly tested against a real TCP server.
pub async fn mcp_ping_server(endpoint: &str, timeout_secs: u64) -> ProviderTestResult {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ProviderTestResult {
            ok: false,
            status: "error".into(),
            provider: String::new(),
            mcp_server: String::new(),
            model_id: String::new(),
            endpoint: endpoint.to_string(),
            http_status: 0,
            latency_ms: 0,
            response_snippet: None,
            error: Some(format!("HTTP client build failed: {e}")),
        },
    };

    let ping_payload = serde_json::json!({"jsonrpc": "2.0", "id": "1", "method": "ping"});

    let start = std::time::Instant::now();
    let result = client.post(endpoint)
        .header("Content-Type", "application/json")
        .json(&ping_payload)
        .send()
        .await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) => {
            let http_status = resp.status().as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            let mcp_ok = serde_json::from_str::<serde_json::Value>(&body_text)
                .ok()
                .map(|v| v.get("result").is_some())
                .unwrap_or(false);

            ProviderTestResult {
                ok: mcp_ok,
                status: if mcp_ok { "online".into() } else { "unexpected_response".into() },
                provider: String::new(),
                mcp_server: String::new(),
                model_id: String::new(),
                endpoint: endpoint.to_string(),
                http_status,
                latency_ms,
                response_snippet: Some(body_text.chars().take(200).collect()),
                error: if mcp_ok { None } else {
                    Some("HTTP succeeded but MCP ping response lacks 'result' field".into())
                },
            }
        }
        Err(e) => ProviderTestResult {
            ok: false,
            status: "offline".into(),
            provider: String::new(),
            mcp_server: String::new(),
            model_id: String::new(),
            endpoint: endpoint.to_string(),
            http_status: 0,
            latency_ms,
            response_snippet: None,
            error: Some(format!("Connection failed: {e}")),
        },
    }
}

/// Result of testing an external LLM provider (OpenAI/Anthropic/Ollama compatible).
#[derive(Debug, serde::Serialize)]
pub struct ExternalProviderTestResult {
    pub ok: bool,
    pub status: String,
    pub provider: String,
    #[serde(rename = "type")]
    pub provider_type: String,
    pub base_url: String,
    pub model_tested: Option<String>,
    pub http_status: u16,
    pub latency_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Make a real API call against an external LLM provider.
/// Uses the appropriate endpoint format based on type:
/// - openai_compatible: POST /v1/chat/completions
/// - anthropic_compatible: POST /v1/messages
/// - ollama_compatible: POST /api/chat
pub async fn test_external_provider(
    provider_type: &str,
    base_url: &str,
    api_key: &str,
    model: &str,
    timeout_secs: u64,
) -> ExternalProviderTestResult {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ExternalProviderTestResult {
            ok: false, status: "error".into(),
            provider: String::new(), provider_type: provider_type.to_string(),
            base_url: base_url.to_string(), model_tested: Some(model.to_string()),
            http_status: 0, latency_ms: 0,
            response_snippet: None, error: Some(format!("HTTP client build failed: {e}")),
        },
    };

    let (endpoint_path, request_body, extra_headers) = match provider_type {
        "anthropic_compatible" => {
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "Respond with a single letter: A"}],
            });
            let headers = vec![
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("x-api-key".to_string(), api_key.to_string()),
            ];
            ("/v1/messages".to_string(), body, headers)
        }
        "ollama_compatible" => {
            let body = serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": "Respond with a single letter: A"}],
                "stream": false,
            });
            ("/api/chat".to_string(), body, vec![])
        }
        _ => { // openai_compatible
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "Respond with a single letter: A"}],
            });
            ("/v1/chat/completions".to_string(), body, vec![])
        }
    };

    let url = format!("{}{}", base_url.trim_end_matches('/'), endpoint_path);

    let mut req = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&request_body);

    if !api_key.is_empty() {
        // Anthropic's real API authenticates via x-api-key only (added below via
        // extra_headers) -- it doesn't use or expect a Bearer Authorization header.
        // Sending both isn't what "the real Anthropic format" should look like,
        // even if a lenient server tolerates the extra header.
        if provider_type != "anthropic_compatible" {
            req = req.header("Authorization", format!("Bearer {api_key}"));
        }
        for (hdr, val) in extra_headers {
            req = req.header(&hdr, val);
        }
    }

    let start = std::time::Instant::now();
    let result = req.send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(resp) => {
            let http_status = resp.status().as_u16();
            let body_text = resp.text().await.unwrap_or_default();
            let op_ok = (200..300).contains(&http_status);
            ExternalProviderTestResult {
                ok: op_ok,
                status: if op_ok { "online".into() } else { "error".into() },
                provider: String::new(),
                provider_type: provider_type.to_string(),
                base_url: base_url.to_string(),
                model_tested: Some(model.to_string()),
                http_status,
                latency_ms,
                response_snippet: Some(body_text.chars().take(300).collect()),
                error: if op_ok { None } else {
                    Some(format!("Provider returned HTTP {http_status}: {}", body_text.chars().take(200).collect::<String>()))
                },
            }
        }
        Err(e) => ExternalProviderTestResult {
            ok: false,
            status: "offline".into(),
            provider: String::new(),
            provider_type: provider_type.to_string(),
            base_url: base_url.to_string(),
            model_tested: Some(model.to_string()),
            http_status: 0,
            latency_ms,
            response_snippet: None,
            error: Some(format!("Connection failed: {e}")),
        },
    }
}

/// POST /api/v1/external-providers/{name}/test
pub async fn test_external_provider_handler(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let provider = {
        let config = state.config.read().await;
        config.external_providers.iter().find(|p| p.name == name).cloned()
    };

    let provider = match provider {
        Some(p) => p,
        None => {
            let names: Vec<String> = {
                let config = state.config.read().await;
                config.external_providers.iter().map(|p| p.name.clone()).collect()
            };
            return (StatusCode::NOT_FOUND, crate::transport::http::Utf8Json(serde_json::json!({
                "ok": false,
                "error": format!("External provider '{name}' not found. Available: {}", names.join(", "))
            }))).into_response();
        }
    };

    // Defense-in-depth: re-check SSRF + env-var safety before touching anything
    if let Err(msg) = provider.is_safe() {
        return (StatusCode::FORBIDDEN, crate::transport::http::Utf8Json(serde_json::json!({
            "ok": false,
            "status": "blocked",
            "error": msg,
        }))).into_response();
    }

    // Use override model from query param if provided
    let model = params.get("model").cloned()
        .or_else(|| provider.models.first().cloned())
        .unwrap_or_else(|| "gpt-4o-mini".to_string());

    // Read API key from environment (never from config)
    let api_key = std::env::var(&provider.api_key_env).unwrap_or_default();
    if api_key.is_empty() {
        return (StatusCode::OK, crate::transport::http::Utf8Json(serde_json::json!({
            "ok": false,
            "status": "no_key",
            "provider": provider.name,
            "type": provider.provider_type,
            "base_url": provider.base_url,
            "error": format!("Environment variable '{}' is not set or empty. Set it with the API key.", provider.api_key_env),
        }))).into_response();
    }

    let type_str = match provider.provider_type {
        crate::config::ExternalProviderType::OpenAICompatible => "openai_compatible",
        crate::config::ExternalProviderType::AnthropicCompatible => "anthropic_compatible",
        crate::config::ExternalProviderType::OllamaCompatible => "ollama_compatible",
    };

    let result = test_external_provider(type_str, &provider.base_url, &api_key, &model, 10).await;

    (StatusCode::OK, crate::transport::http::Utf8Json(serde_json::json!({
        "ok": result.ok,
        "status": result.status,
        "provider": provider.name,
        "type": result.provider_type,
        "base_url": result.base_url,
        "model_tested": result.model_tested,
        "http_status": result.http_status,
        "latency_ms": result.latency_ms,
        "response_snippet": result.response_snippet,
        "error": result.error,
    }))).into_response()
}

#[derive(Deserialize)]
pub struct SetDeviceRequest { pub device: String }

#[derive(Deserialize)]
pub struct SetSandboxProfileRequest { pub profile: String }

#[derive(Deserialize)]
pub struct SetGuildSandboxOverrideRequest {
    pub guild: String,
    pub profile: String,
}

#[derive(Deserialize)]
pub struct SetSessionSandboxOverrideRequest {
    pub agent_id: String,
    pub profile: String,
}

pub async fn get_config_handler() -> impl IntoResponse {
    match crate::config::TylluanConfig::load_cached() {
        Ok(c) => match c.try_read() {
            Ok(config) => Json(config.clone()).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Config lock poisoned: {e}")).into_response(),
        },
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response()
    }
}

/// POST /api/v1/config/device — targeted, corruption-proof edit of the
/// `device = "..."` line in tylluan.toml. Server-side; the browser never
/// round-trips the whole config (that's what bricked it once).
pub async fn set_inference_device_handler(Json(req): Json<SetDeviceRequest>) -> impl IntoResponse {
    let device = req.device.trim().to_lowercase();
    if !["cpu", "directml", "cuda"].contains(&device.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "device must be one of: cpu, directml, cuda"
        }))).into_response();
    }
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };
    let mut replaced = false;
    let new_raw: String = raw.lines().map(|l| {
        if !replaced && l.trim_start().starts_with("device") && l.contains('=') {
            replaced = true;
            format!("device = \"{device}\"")
        } else {
            l.to_string()
        }
    }).collect::<Vec<_>>().join("\n");
    let new_raw = if replaced { new_raw } else {
        format!("{}\n\n[inference]\ndevice = \"{}\"\n", new_raw.trim_end(), device)
    };
    // Never write something that doesn't parse back.
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("refusing to write invalid TOML: {}", e)
        }))).into_response();
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "device": device, "restart_required": true
    }))).into_response()
}

/// GET /api/v1/config/device/status — reports the REAL ONNX execution
/// provider state, never a hardcoded/assumed value. Backlog item: no status
/// widget ships without a real endpoint behind it (3 prior incidents of
/// hardcoded/ghost dashboard data). Queries each inference guild directly
/// via the in-process guild registry (same path `guild_tool_call_handler`
/// uses) and reports per-guild real vs configured provider. If a guild is
/// unreachable, that guild's entry is honestly `null` — never faked as CPU.
pub async fn device_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let configured_device = match crate::config::TylluanConfig::load_cached() {
        Ok(cfg_lock) => match cfg_lock.try_read() {
            Ok(cfg) => Some(format!("{:?}", cfg.inference.device).to_lowercase()),
            Err(_) => None,
        },
        Err(_) => None,
    };

    // Inference guilds that load ONNX models locally. Extend this list if
    // more guilds gain their own onnxruntime sessions.
    const INFERENCE_GUILDS: &[(&str, &str)] = &[("vision", "vision_device_status")];

    let mut guilds = serde_json::Map::new();
    for (guild, tool) in INFERENCE_GUILDS {
        let params = CallToolRequestParam { name: (*tool).into(), arguments: None };
        let entry = match state.registry.call_tool(guild, params).await {
            Ok(res) => {
                let mut parsed_any: Option<serde_json::Value> = None;
                for c in res.content {
                    if let Some(text) = c.as_text()
                        && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text.text) {
                            parsed_any = Some(parsed);
                            break;
                        }
                }
                parsed_any.unwrap_or_else(|| serde_json::json!({
                    "status": "error",
                    "error": "guild returned no parseable status"
                }))
            }
            Err(e) => serde_json::json!({
                "status": "error",
                "error": format!("guild '{guild}' unreachable: {e}")
            }),
        };
        guilds.insert((*guild).to_string(), entry);
    }

    (StatusCode::OK, Json(serde_json::json!({
        "ok": true,
        "configured_device": configured_device,
        "guilds": guilds,
    }))).into_response()
}

/// POST /api/v1/config/sandbox-profile — targeted, corruption-proof edit of
/// `profile = "..."` under `[security.sandbox]` in tylluan.toml. Same pattern
/// as set_inference_device_handler: never round-trip the whole config through
/// the browser (that's what bricked it once).
pub async fn set_sandbox_profile_handler(Json(req): Json<SetSandboxProfileRequest>) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    let mut in_sandbox_section = false;
    let mut replaced = false;
    let mut saw_sandbox_section = false;
    let new_raw: String = raw.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_sandbox_section = trimmed.starts_with("[security.sandbox]");
            if in_sandbox_section { saw_sandbox_section = true; }
        } else if in_sandbox_section && !replaced && trimmed.starts_with("profile") && trimmed.contains('=') {
            replaced = true;
            return format!("profile = \"{profile}\"");
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let new_raw = if replaced {
        new_raw
    } else if saw_sandbox_section {
        // Section exists but no `profile` key yet — insert right after the header.
        let mut out = String::new();
        let mut inserted = false;
        for l in new_raw.lines() {
            out.push_str(l);
            out.push('\n');
            if !inserted && l.trim_start().starts_with("[security.sandbox]") {
                out.push_str(&format!("profile = \"{profile}\"\n"));
                inserted = true;
            }
        }
        out
    } else {
        format!("{}\n\n[security.sandbox]\nprofile = \"{}\"\n", new_raw.trim_end(), profile)
    };

    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("refusing to write invalid TOML: {}", e)
        }))).into_response();
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "profile": profile, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "profile": profile, "restart_required": false
    }))).into_response()
}

/// POST /api/v1/config/sandbox-profile/guild — targeted edit of a single
/// `guild_overrides.<key>` under `[security.sandbox.guild_overrides]`.
/// Never round-trips the full config through the browser.
pub async fn set_guild_sandbox_override_handler(
    Json(req): Json<SetGuildSandboxOverrideRequest>,
) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    if req.guild.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "guild name must not be empty"
        }))).into_response();
    }

    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    // Bare key form, always used INSIDE the [security.sandbox.guild_overrides]
    // bracket table -- never the dotted `guild_overrides."x"` form, which is
    // only valid directly under [security.sandbox] and was previously mixed
    // with the bracket-table form by mistake, producing invalid TOML anytime
    // the table didn't already exist (verified via curl: "unknown variant
    // `bash`, expected one of `strict`, `balanced`, `permissive`").
    let quoted_guild = format!("\"{}\"", req.guild.trim());
    let target_line = format!("{quoted_guild} = \"{profile}\"");

    let mut in_guild_overrides = false;
    let mut replaced = false;
    let mut saw_guild_overrides = false;

    let new_raw: String = raw.lines().map(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_guild_overrides = trimmed.starts_with("[security.sandbox.guild_overrides]");
            if in_guild_overrides { saw_guild_overrides = true; }
        } else if in_guild_overrides && !replaced && trimmed.starts_with(&quoted_guild) && trimmed.contains('=') {
            replaced = true;
            return target_line.clone();
        }
        l.to_string()
    }).collect::<Vec<_>>().join("\n");

    let new_raw = if replaced {
        new_raw
    } else if saw_guild_overrides {
        // Section exists but no entry for this guild — append inside the section.
        // Find the last line of the section and insert before the next section header.
        let mut out = String::new();
        let mut inserted = false;
        let mut in_override_section = false;
        for l in new_raw.lines() {
            let trimmed = l.trim_start();
            if trimmed.starts_with("[security.sandbox.guild_overrides]") {
                in_override_section = true;
            } else if in_override_section && trimmed.starts_with('[') {
                // Next section — insert right before
                if !inserted {
                    out.push_str(&format!("{target_line}\n"));
                    inserted = true;
                }
                in_override_section = false;
            }
            out.push_str(l);
            out.push('\n');
        }
        // If section was the last thing in the file, append at the end
        if !inserted {
            out.push_str(&format!("{target_line}\n"));
        }
        out
    } else {
        // No [security.sandbox.guild_overrides] table yet -- TOML doesn't
        // care about section ordering, so it's always valid to just append
        // a fresh table at the end of the file rather than hunting for the
        // right sibling-section insertion point under [security.sandbox].
        format!("{}\n\n[security.sandbox.guild_overrides]\n{}\n", new_raw.trim_end(), target_line)
    };

    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("refusing to write invalid TOML: {}", e)
        }))).into_response();
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "guild": req.guild.trim(), "profile": profile, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({
        "guild": req.guild.trim(), "profile": profile, "restart_required": false
    }))).into_response()
}

/// POST /api/v1/config/sandbox-profile/session — set in-memory per-agent_id
/// override. NOT persisted to TOML — lives only while the kernel runs.
pub async fn set_session_sandbox_override_handler(
    Json(req): Json<SetSessionSandboxOverrideRequest>,
) -> impl IntoResponse {
    let profile = req.profile.trim().to_lowercase();
    if !["strict", "balanced", "permissive"].contains(&profile.as_str()) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "profile must be one of: strict, balanced, permissive"
        }))).into_response();
    }
    let parsed = match profile.as_str() {
        "strict" => crate::config::SandboxProfile::Strict,
        "balanced" => crate::config::SandboxProfile::Balanced,
        "permissive" => crate::config::SandboxProfile::Permissive,
        _ => unreachable!(),
    };

    crate::config::set_session_override(&req.agent_id, parsed).await;

    (StatusCode::OK, Json(serde_json::json!({
        "agent_id": req.agent_id, "profile": profile,
        "scope": "session", "persisted": false
    }))).into_response()
}

/// DELETE /api/v1/config/sandbox-profile/guild/{guild} — removes that guild's
/// entry from [security.sandbox.guild_overrides] via targeted line removal.
/// Same never-round-trip-the-whole-file discipline as the POST handler above.
pub async fn delete_guild_sandbox_override_handler(
    Path(guild): Path<String>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    let quoted_guild = format!("\"{}\"", guild.trim());
    let mut in_guild_overrides = false;
    let mut removed = false;
    let new_raw: String = raw.lines().filter(|l| {
        let trimmed = l.trim_start();
        if trimmed.starts_with('[') {
            in_guild_overrides = trimmed.starts_with("[security.sandbox.guild_overrides]");
            return true;
        }
        if in_guild_overrides && trimmed.starts_with(&quoted_guild) && trimmed.contains('=') {
            removed = true;
            return false;
        }
        true
    }).collect::<Vec<_>>().join("\n");

    if !removed {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "guild": guild, "error": "no override found for this guild"
        }))).into_response();
    }

    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("refusing to write invalid TOML: {}", e)
        }))).into_response();
    }
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::OK, Json(serde_json::json!({
            "guild": guild, "restart_required": true,
            "warning": format!("written to disk but in-memory reload failed: {e} — restart to apply")
        }))).into_response();
    }
    (StatusCode::OK, Json(serde_json::json!({ "guild": guild, "restart_required": false }))).into_response()
}

/// DELETE /api/v1/config/sandbox-profile/session/{agent_id} — clears the
/// in-memory session override. Never touches the TOML (it was never
/// persisted there to begin with).
pub async fn delete_session_sandbox_override_handler(
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    crate::config::clear_session_override(&agent_id).await;
    (StatusCode::OK, Json(serde_json::json!({ "agent_id": agent_id }))).into_response()
}

pub async fn save_config_handler(State(state): State<Arc<HttpState>>, Json(req): Json<SaveConfigRequest>) -> impl IntoResponse {
    // Guard: never write content that doesn't parse as our config TOML.
    // (A dashboard bug once wrote JSON here and bricked the kernel config on restart.)
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&req.content) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": format!("content is not valid tylluan.toml — refusing to write: {}", e)
        }))).into_response();
    }

    let old_config = state.config.read().await.clone();
    let old_embedding = old_config.memory.embedding_model.clone();

    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &req.content) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }

    if let Ok(new_config) = toml::from_str::<crate::config::TylluanConfig>(&req.content) {
        let new_embedding = new_config.memory.embedding_model.clone();
        if new_embedding != old_embedding {
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "config_changed",
                "field": "embedding_model",
                "old_value": old_embedding,
                "new_value": new_embedding,
                "requires_restart": true,
                "message": "Cambio de modelo de embedding detectado. Se requiere reiniciar el kernel y reindexar todos los nodos.",
                "ts": chrono::Utc::now().timestamp_millis()
            }));
        }
    }
    (StatusCode::OK, Json(serde_json::json!({ "status": "saved" }))).into_response()
}

// --- SYSTEM ---

pub async fn audit_logs_handler() -> impl IntoResponse {
    let log_path = "logs/kernel.log";
    let content = fs::read_to_string(log_path).unwrap_or_default();
    let events: Vec<serde_json::Value> = content.lines().rev().take(100).map(|line| {
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        let ts = parts.first().cloned().unwrap_or("");
        let level = parts.get(1).cloned().unwrap_or("INFO");
        let source = parts.get(2).cloned().unwrap_or("kernel");
        let msg = parts.get(3).cloned().unwrap_or(line);

        serde_json::json!({
            "type": level.to_lowercase(),
            "source": source,
            "data": { "message": msg },
            "ts": ts
        })
    }).collect();

    Json(serde_json::json!({ "logs": events, "count": events.len() }))
}

pub async fn system_status_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let report = state.doctor.diagnose().await;
    let curr_count = {
        let curr_learner = state.doctor.curriculum();
        let curr = curr_learner.lock().unwrap_or_else(|e| e.into_inner());
        curr.get_stats()["total_entries"].as_u64().unwrap_or(0)
    };

    let status_json = serde_json::json!({
        "silva_healthy": report.storage.silva_db_ok,
        "mailbox_healthy": report.storage.memory_db_ok,
        "curriculum_entries": curr_count,
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "embeddings_loaded": true,
        "score": if report.status == "healthy" { 100 } else if report.status == "degraded" { 65 } else { 30 },
        "system": {
            "cpu_usage": report.system.cpu_usage_percent,
            "memory_percent": report.system.memory_percent,
            "used_memory_mb": report.system.used_memory_mb,
            "total_memory_mb": report.system.total_memory_mb,
            "process_count": report.system.process_count,
        }
    });

    crate::transport::http::Utf8Json(status_json)
}

#[derive(serde::Deserialize)]
pub struct TestConnectionPayload {
    pub endpoint: Option<String>,
    pub url: Option<String>,
    pub provider: Option<String>,
}

pub async fn test_connection_handler(
    payload: Option<axum::Json<TestConnectionPayload>>,
) -> impl IntoResponse {
    let p = payload.map(|b| b.0);
    let target_url = p.as_ref()
        .and_then(|p| p.endpoint.clone().or_else(|| p.url.clone()))
        .unwrap_or_else(|| "http://127.0.0.1:8080".to_string());

    let provider = p.as_ref()
        .and_then(|p| p.provider.clone())
        .unwrap_or_else(|| "llama-server".to_string());

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": false,
                    "status": "error",
                    "error": format!("Failed to build HTTP client: {}", e),
                    "endpoint": target_url
                })),
            ).into_response();
        }
    };

    let start = std::time::Instant::now();
    let res = client.get(&target_url).send().await;
    let latency_ms = start.elapsed().as_millis() as u64;

    match res {
        Ok(response) => {
            let status_code = response.status().as_u16();
            (
                StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": true,
                    "status": "online",
                    "http_status": status_code,
                    "latency_ms": latency_ms,
                    "provider": provider,
                    "endpoint": target_url
                })),
            ).into_response()
        }
        Err(e) => {
            (
                StatusCode::OK,
                crate::transport::http::Utf8Json(serde_json::json!({
                    "ok": false,
                    "status": "offline",
                    "error": format!("Servidor en {} no responde: {}", target_url, e),
                    "latency_ms": latency_ms,
                    "provider": provider,
                    "endpoint": target_url
                })),
            ).into_response()
        }
    }
}

pub async fn update_wsl_config_handler() -> impl IntoResponse { StatusCode::OK }

pub async fn list_inference_providers_handler() -> impl IntoResponse { StatusCode::OK }
pub async fn add_inference_provider_handler() -> impl IntoResponse { StatusCode::OK }

/// GET /api/v1/external-providers
pub async fn list_external_providers_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let providers: Vec<serde_json::Value> = {
        let config = state.config.read().await;
        config.external_providers.iter().map(|p| {
            serde_json::json!({
                "name": p.name,
                "type": p.provider_type,
                "base_url": p.base_url,
                "api_key_env": p.api_key_env,
                "models": p.models,
                "api_key_set": std::env::var(&p.api_key_env).is_ok(),
            })
        }).collect()
    };
    (StatusCode::OK, crate::transport::http::Utf8Json(serde_json::json!(providers)))
}

/// POST /api/v1/inference/providers/{name}/test
/// Makes a real MCP ping against the provider's backend server and returns
/// success/failure with measured latency. Resolves provider → MCP server →
/// server URL, then POSTs a minimal `{"jsonrpc":"2.0","id":"1","method":"ping"}`.
///
/// Only works for HTTP Streamable MCP servers (url field). SSE and stdio
/// servers return a clear error explaining the limitation rather than a false positive.
pub async fn test_inference_provider_handler(
    State(state): State<Arc<HttpState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    // 1. Find the provider in config
    let (mcp_server_name, model_id) = {
        let config = state.config.read().await;
        match config.inference.providers.iter().find(|p| p.name == name) {
            Some(p) => (p.mcp_server.clone(), p.model_id.clone()),
            None => return (StatusCode::NOT_FOUND, crate::transport::http::Utf8Json(serde_json::json!({
                "ok": false,
                "error": format!("Provider '{name}' not found. Available: {}",
                    config.inference.providers.iter().map(|p| p.name.as_str()).collect::<Vec<&str>>().join(", "))
            }))).into_response(),
        }
    };

    // 2. Resolve MCP server URL from external_mcp
    let server_url = {
        let config = state.config.read().await;
        config.external_mcp.iter()
            .find(|s| s.name == mcp_server_name)
            .and_then(|s| s.url.clone())
    };

    let target_url = match server_url {
        Some(url) => {
            // Normalise: strip trailing slash, append /messages for Streamable HTTP
            let base = url.trim_end_matches('/').to_string();
            if base.contains("/messages") { base } else { format!("{base}/messages") }
        }
        None => return (StatusCode::OK, crate::transport::http::Utf8Json(serde_json::json!({
            "ok": false,
            "status": "unsupported",
            "provider": name,
            "mcp_server": mcp_server_name,
            "error": format!("MCP server '{mcp_server_name}' has no HTTP URL (SSE or stdio). Only HTTP Streamable MCP servers can be tested remotely."),
            "model_id": model_id,
        }))).into_response(),
    };

    // 3. Real MCP ping via shared logic
    let result = mcp_ping_server(&target_url, 10).await;

    // Stamp provider metadata onto the result
    (StatusCode::OK, crate::transport::http::Utf8Json(serde_json::json!({
        "ok": result.ok,
        "status": result.status,
        "provider": name,
        "mcp_server": mcp_server_name,
        "model_id": model_id,
        "endpoint": result.endpoint,
        "http_status": result.http_status,
        "latency_ms": result.latency_ms,
        "response_snippet": result.response_snippet,
        "error": result.error,
    }))).into_response()
}

pub async fn health_detailed_handler(
    State(state): State<Arc<HttpState>>
) -> impl IntoResponse {
    let node_count = state.silva.node_count().await.unwrap_or(0);
    let edge_count = state.silva.edge_count().await.unwrap_or(0);

    // Guild health
    let (total_guilds, active_guilds) = state.registry.guild_stats().await.unwrap_or((0, 0));

    // Server capabilities
    let (embeddings_loaded, reranker_loaded) = if let Some(ref srv_arc) = state.server {
        if let Ok(s) = srv_arc.try_read() {
            let emb = s.matcher.engine().is_some();
            let rer = s.reranker.is_some();
            (emb, rer)
        } else { (false, false) }
    } else { (false, false) };

    // Overall health score (0-100)
    let mut score = 100u8;
    if !embeddings_loaded { score = score.saturating_sub(20); }
    if !reranker_loaded   { score = score.saturating_sub(10); }
    if active_guilds == 0 { score = score.saturating_sub(30); }
    if node_count == 0    { score = score.saturating_sub(10); }

    let status = if score >= 80 { "healthy" }
                 else if score >= 50 { "degraded" }
                 else { "critical" };

    Json(serde_json::json!({
        "status": status,
        "score": score,
        "version": &state.version,
        "components": {
            "embeddings": { "ok": embeddings_loaded, "model": "bge-m3" },
            "reranker":   { "ok": reranker_loaded,   "model": "jina-reranker-v1-turbo-en" },
            "guilds":     { "ok": active_guilds > 0,
                            "active": active_guilds, "total": total_guilds },
            "silva":      { "ok": node_count > 0,
                            "nodes": node_count, "edges": edge_count },
            "tunnel":     { "ok": state.tunnel_wsl_url.is_some(),
                            "wsl_url": state.tunnel_wsl_url }
        }
    }))
}

pub async fn admin_reload_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    // Check dev_mode
    if !state.dev_mode.unwrap_or(false) {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "admin/reload only available in dev_mode"
        }))).into_response();
    }

    let start = std::time::Instant::now();

    // Get all active guilds
    let guild_statuses = match state.registry.status_all().await {
        Ok(statuses) => statuses,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": format!("Failed to get guild status: {}", e)
            }))).into_response();
        }
    };

    let guild_names: Vec<String> = guild_statuses
        .iter()
        .map(|s| s.name.clone())
        .collect();

    // Kill and restart each guild
    let mut reloaded = 0;
    for name in &guild_names {
        if let Err(e) = state.registry.kill_guild(name).await {
            tracing::warn!("Failed to kill guild {}: {}", name, e);
            continue;
        }

        // Small delay to ensure process fully exits
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        if let Err(e) = state.registry.ensure_running(name).await {
            tracing::warn!("Failed to restart guild {}: {}", name, e);
            continue;
        }

        reloaded += 1;
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    (StatusCode::OK, Json(serde_json::json!({
        "reloaded": true,
        "guilds": reloaded,
        "attempted": guild_names.len(),
        "elapsed_ms": elapsed_ms,
        "guild_names": guild_names
    }))).into_response()
}

pub async fn meta_prune_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.meta_cognitive_prune(0.15, 168, 48) {
        Ok(count) => Json(serde_json::json!({"archived": count, "status": "ok"})).into_response(),
        Err(e) => Json(serde_json::json!({"error": e.to_string(), "status": "error"})).into_response(),
    }
}

pub async fn admin_shutdown_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Admin actions allowed only from localhost"})),
        ).into_response();
    }

    tracing::info!("🛑 Admin shutdown requested via POST /api/v1/admin/shutdown. Cancelling tokio token...");
    state.cancel_token.cancel();

    (StatusCode::OK, Json(serde_json::json!({"status": "shutdown_initiated"}))).into_response()
}

/// Emergency kill: stop all guilds immediately without restart, then shutdown kernel.
pub async fn admin_emergency_kill_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Emergency kill allowed only from localhost"})),
        ).into_response();
    }

    tracing::warn!("🚨 EMERGENCY KILL requested. Killing all guilds and shutting down.");

    let mut killed = 0;
    if let Ok(statuses) = state.registry.status_all().await {
        for gs in &statuses {
            if let Err(e) = state.registry.kill_guild(&gs.name).await {
                tracing::error!("Failed to kill guild {}: {}", gs.name, e);
            } else {
                killed += 1;
            }
        }
    }

    state.cancel_token.cancel();

    (StatusCode::OK, Json(serde_json::json!({
        "status": "emergency_kill_complete",
        "guilds_killed": killed
    }))).into_response()
}

/// Kill a specific guild by name (for rogue agent mitigation).
pub async fn admin_kill_guild_handler(
    State(state): State<Arc<HttpState>>,
    headers: axum::http::HeaderMap,
    Path(guild_name): Path<String>,
) -> impl IntoResponse {
    let host = headers.get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let is_local = host.starts_with("127.0.0.1") || host.starts_with("localhost") || host.starts_with("[::1]");
    if !is_local {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Admin actions allowed only from localhost"})),
        ).into_response();
    }

    tracing::warn!("🛑 Kill requested for guild '{}'", guild_name);

    match state.registry.kill_guild(&guild_name).await {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({
            "status": "killed",
            "guild": guild_name
        }))).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(serde_json::json!({
            "error": format!("Failed to kill guild '{}': {}", guild_name, e)
        }))).into_response(),
    }
}

// --- SESSIONS ---

pub async fn list_sessions_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let sessions = state.sessions.read().await;
    let list: Vec<serde_json::Value> = sessions.values().map(|s| {
        serde_json::json!({
            "id": s.id,
            "client_name": s.client_name,
            "agent_id": s.agent_id,
            "tool_count": s.tool_count,
            "last_intent": s.last_intent,
            "last_guild": s.last_guild,
            "last_active_unix": s.last_active_unix,
            "created_unix": s.created_unix,
        })
    }).collect();
    Json(serde_json::json!({ "sessions": list }))
}

pub async fn session_detail_handler(
    State(state): State<Arc<HttpState>>,
    Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let sessions = state.sessions.read().await;
    match sessions.get(&session_id) {
        Some(s) => Json(serde_json::json!({
            "id": s.id,
            "client_name": s.client_name,
            "agent_id": s.agent_id,
            "tool_count": s.tool_count,
            "last_intent": s.last_intent,
            "last_guild": s.last_guild,
            "last_active_unix": s.last_active_unix,
            "created_unix": s.created_unix,
        })).into_response(),
        None => (StatusCode::NOT_FOUND,
                 Json(serde_json::json!({"error":"session not found"}))).into_response(),
    }
}

pub async fn revoke_session_handler(
    State(state): State<Arc<HttpState>>,
    Path(session_id): axum::extract::Path<String>,
) -> impl IntoResponse {
    let mut sessions = state.sessions.write().await;
    if sessions.remove(&session_id).is_some() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

/// M31-P3: GET /api/v1/sessions/resume?agent_id=<id>
/// M40-P5: returns the full cross-client resume package (identity + last task +
/// summary + recent memories + pending actions) via the single assembler
/// `build_resume_context`. The flat compat fields (found/summary/node_id/...)
/// are preserved so the M31-P3 CLI consumer keeps working unchanged.
pub async fn sessions_resume_handler(
    State(state): State<Arc<HttpState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let agent_id = match params.get("agent_id") {
        Some(id) if !id.trim().is_empty() => id.trim().to_string(),
        _ => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "Missing or empty 'agent_id' query parameter"
        }))).into_response(),
    };

    let ctx = crate::transport::server::bootstrap::build_resume_context(
        state.silva.clone(), &state.journal, &agent_id,
    ).await;
    (StatusCode::OK, Json(ctx)).into_response()
}

// --- GRANTS (HITL via grants.rs, not pending_approvals) ---

// --- MAINTENANCE ---

pub async fn maintenance_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut total_bytes = 0u64;
    for path in &["./data/silva.db", "./data/silva.db-wal", "./data/silva.db-shm", "./data/tylluan.db"] {
        if let Ok(meta) = fs::metadata(path) { total_bytes += meta.len(); }
    }

    let last_export = fs::read_dir("./data/exports").ok()
        .and_then(|dir| {
            dir.filter_map(|e| e.ok())
               .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
               .filter_map(|e| e.metadata().ok())
               .filter_map(|m| m.modified().ok())
               .max()
        })
        .map(|t| { let dt: chrono::DateTime<chrono::Utc> = t.into(); dt.format("%Y-%m-%d %H:%M").to_string() })
        .unwrap_or_else(|| "Never".to_string());

    let brain_size_human = if total_bytes > 1_073_741_824 {
        format!("{:.2} GB", total_bytes as f64 / 1_073_741_824.0)
    } else {
        format!("{:.2} MB", total_bytes as f64 / 1_048_576.0)
    };

    let node_count = state.silva.node_count().await.unwrap_or(0);
    let edge_count = state.silva.edge_count().await.unwrap_or(0);
    let orphan_count = state.silva.orphan_node_count().await.unwrap_or(0);

    Json(serde_json::json!({
        "status": "ok",
        "brain_size_bytes": total_bytes,
        "brain_size_human": brain_size_human,
        "last_export": last_export,
        "storage_mode": "SQLite WAL",
        "node_count": node_count,
        "edge_count": edge_count,
        "orphan_node_count": orphan_count,
    }))
}

// --- SECURITY SCOPES PERSISTENCE (Point 7) ---

#[derive(serde::Deserialize, serde::Serialize)]
pub struct ScopeEntry {
    pub role: String,
    pub scopes: Vec<String>,
}

pub async fn get_security_scopes_handler() -> impl IntoResponse {
    let cfg = crate::config::TylluanConfig::load().unwrap_or_default();
    let acl = &cfg.security.acl;
    
    let mut roles = Vec::new();
    for (role_name, scopes) in &acl.roles {
        roles.push(serde_json::json!({
            "role": role_name,
            "scopes": scopes
        }));
    }

    if roles.is_empty() {
        roles.push(serde_json::json!({
            "role": "admin",
            "scopes": ["read", "write", "admin"]
        }));
        roles.push(serde_json::json!({
            "role": "agent",
            "scopes": ["read", "write"]
        }));
        roles.push(serde_json::json!({
            "role": "viewer",
            "scopes": ["read"]
        }));
    }

    Json(serde_json::json!({ "roles": roles }))
}

pub async fn save_security_scopes_handler(
    Json(payload): Json<Vec<ScopeEntry>>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    
    let mut cfg = crate::config::TylluanConfig::load().unwrap_or_default();
    for entry in payload {
        cfg.security.acl.roles.insert(entry.role, entry.scopes);
    }

    if let Ok(toml_str) = toml::to_string_pretty(&cfg) {
        let _ = fs::write(&config_path, toml_str);
        let _ = crate::config::TylluanConfig::reload().await;
        (StatusCode::OK, Json(serde_json::json!({ "success": true, "message": "Scopes de seguridad actualizados en tylluan.toml" }))).into_response()
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": "Error serializando TOML" }))).into_response()
    }
}

// --- SESSION RESUME ACTION (Point 8) ---

#[derive(serde::Deserialize)]
pub struct SessionResumeRequest {
    pub session_id: String,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub client_name: Option<String>,
}

/// M40-P5: real resume action, never a fabricated success.
///
/// Effects (all observable, none invented):
/// - If the session exists in the live map, its `last_active` is bumped and
///   `client_name` is re-bound when the caller says it's now continuing from a
///   different client (cross-client handoff).
/// - The full resume package (`build_resume_context`) is returned so the
///   caller picks up exactly where the agent left off.
///
/// The journal is deliberately NOT written here: the last in-progress task is
/// preserved -- "resumed" is not the task, it would clobber real continuity.
pub async fn sessions_resume_action_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<SessionResumeRequest>,
) -> impl IntoResponse {
    let session_id = req.session_id.trim();
    if session_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "success": false, "message": "session_id no puede estar vacío" }))).into_response();
    }

    // Real side effect: refresh the session's last_active + optional client rebind.
    let mut session_snapshot = None;
    {
        let mut sessions = state.sessions.write().await;
        if let Some(s) = sessions.get_mut(session_id) {
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
            s.last_active_unix = now_unix;
            if let Some(cn) = req.client_name.as_deref() {
                s.client_name = cn.to_string();
            }
            session_snapshot = Some(serde_json::json!({
                "client_name": s.client_name,
                "agent_id": s.agent_id,
                "tool_count": s.tool_count,
                "last_intent": s.last_intent,
                "last_guild": s.last_guild,
                "last_active_unix": s.last_active_unix,
            }));
        }
    }

    // Resume target agent: explicit arg wins, then the session's bound agent,
    // then "anonymous" (never guessed, never fabricated).
    let agent_id = req.agent_id.clone()
        .filter(|id| !id.trim().is_empty())
        .or_else(|| session_snapshot.as_ref().and_then(|s| s["agent_id"].as_str().map(|v| v.to_string())))
        .unwrap_or_else(|| "anonymous".to_string());

    let context = crate::transport::server::bootstrap::build_resume_context(
        state.silva.clone(), &state.journal, &agent_id,
    ).await;

    Json(serde_json::json!({
        "success": true,
        "session_id": session_id,
        "session": session_snapshot,
        "resume_context": context,
        "resumed_at": chrono::Utc::now().to_rfc3339()
    })).into_response()
}

// --- MAINTENANCE ACTIONS (Point 10) ---

pub async fn maintenance_onnx_clean_handler() -> impl IntoResponse {
    let mut count = 0;
    let mut bytes = 0u64;
    let cache_dir = std::path::Path::new("./data/cache/onnx");
    if cache_dir.exists()
        && let Ok(entries) = std::fs::read_dir(cache_dir)
    {
        for e in entries.flatten() {
            if let Ok(m) = e.metadata() {
                bytes += m.len();
                count += 1;
            }
            let _ = std::fs::remove_file(e.path());
        }
    }
    Json(serde_json::json!({
        "success": true,
        "message": format!("Caché ONNX limpiada: {} archivos removidos ({:.2} MB)", count, bytes as f64 / 1_048_576.0)
    }))
}

pub async fn maintenance_logs_compact_handler() -> impl IntoResponse {
    let mut count = 0;
    let log_dir = std::path::Path::new("./logs");
    if log_dir.exists()
        && let Ok(entries) = std::fs::read_dir(log_dir)
    {
        for e in entries.flatten() {
            if e.path().extension().and_then(|ext| ext.to_str()) == Some("log")
                && let Ok(meta) = e.metadata()
                && meta.len() > 5 * 1024 * 1024
            {
                let _ = std::fs::write(e.path(), "");
                count += 1;
            }
        }
    }
    Json(serde_json::json!({
        "success": true,
        "message": format!("Compactación de logs completada: {} archivos truncados", count)
    }))
}

// --- PROJECT SKILLS (Point 5) ---

pub async fn project_skills_list_handler() -> impl IntoResponse {
    let mut skills = Vec::new();
    let skill_dirs = &["./.agents/skills", "./.tylluan/skills", "./skills"];

    for dir_path in skill_dirs {
        if let Ok(entries) = std::fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name_opt = if path.is_file() {
                    path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else if path.is_dir() {
                    path.file_name().and_then(|s| s.to_str()).map(|s| s.to_string())
                } else {
                    None
                };
                if let Some(name) = name_opt
                    && !name.is_empty()
                {
                    skills.push(serde_json::json!({ "name": name }));
                }
            }
        }
    }

    // No mock fallback: if no skills directory exists, return empty list.
    // The caller (ProjectSkillsPanel) handles the empty state.
    Json(skills)
}

#[derive(serde::Deserialize)]
pub struct SaveSkillPayload {
    pub name: String,
    pub content: String,
}

pub async fn project_skills_save_handler(
    Json(payload): Json<SaveSkillPayload>,
) -> impl IntoResponse {
    let name = payload.name.trim();
    if name.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "success": false, "message": "Nombre de habilidad requerido" }))).into_response();
    }
    
    let skills_dir = std::path::Path::new("./.agents/skills");
    let _ = std::fs::create_dir_all(skills_dir);
    let skill_file = skills_dir.join(format!("{name}.md"));
    
    match std::fs::write(&skill_file, &payload.content) {
        Ok(_) => Json(serde_json::json!({ "success": true, "message": format!("Habilidad '{}' guardada exitosamente", name) })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "success": false, "message": e.to_string() }))).into_response()
    }
}

// --- BACKGROUND JOBS (Point 6) ---

pub async fn background_jobs_list_handler() -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();
    let jobs = vec![
        serde_json::json!({
            "id": "bg_night_consolidation",
            "name": "NightConsolidation Cron",
            "status": "active",
            "created_at": now,
            "description": "FSRS biological memory consolidation & graph autolink"
        }),
        serde_json::json!({
            "id": "bg_gossip_anti_entropy",
            "name": "Gossip Anti-Entropy",
            "status": "active",
            "created_at": now,
            "description": "P2P Mesh state push-pull synchronization"
        }),
    ];
    let total = jobs.len();
    Json(serde_json::json!({ "jobs": jobs, "total": total }))
}

// --- AUDIT TRAIL (Point 1) ---

#[derive(serde::Deserialize)]
pub struct AuditTrailParams {
    pub agent_id: Option<String>,
    pub limit: Option<usize>,
}

pub async fn audit_trail_handler(
    State(state): State<Arc<HttpState>>,
    axum::extract::Query(params): axum::extract::Query<AuditTrailParams>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(25);
    let silva_nodes = state.silva.get_recent_nodes(limit).await.unwrap_or_default();
    
    let mut entries = Vec::new();
    for node in silva_nodes {
        let agent = if !node.provenance.is_empty() { &node.provenance } else { "system" };
        if let Some(ref filter) = params.agent_id
            && !agent.to_lowercase().contains(&filter.to_lowercase())
        {
            continue;
        }
        let intent = if !node.content.is_empty() {
            node.content.chars().take(80).collect::<String>()
        } else {
            "cognitive_intent".to_string()
        };
        let created = node.created_at.unwrap_or_default();
        
        entries.push(serde_json::json!({
            "agent_id": agent,
            "guild": node.node_type,
            "intent_preview": intent,
            "allowed": true,
            "timestamp": created,
        }));
    }
    
    // No synthetic filler entry when the trail is genuinely empty: the dashboard
    // (AuditTrailPanel.tsx) already renders an honest "no records" empty state.
    // A prior version injected a fake 'antigravity/dashboard/audit_trail_remediation'
    // entry here, presenting fabricated data as a real audit record.
    let total = entries.len();
    Json(serde_json::json!({
        "entries": entries,
        "total": total,
    }))
}

pub async fn maintenance_export_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use axum::http::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
    let nodes = state.silva.get_nodes_paginated(10_000, 0).await.unwrap_or_default();
    let edges = state.silva.get_all_edges().await.unwrap_or_default();
    let ts = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let body = serde_json::json!({
        "version": "1.0",
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "graph": { "nodes": nodes, "edges": edges }
    });
    (
        [
            (CONTENT_DISPOSITION, format!("attachment; filename=\"tylluan-backup-{ts}.json\"")),
            (CONTENT_TYPE, "application/json".to_string()),
        ],
        Json(body),
    ).into_response()
}

pub async fn maintenance_purge_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    tracing::warn!("âš ï¸ MANUAL PURGE REQUESTED via dashboard.");
    let conn = Arc::clone(&state.silva.conn_lock());
    let result = tokio::task::spawn_blocking(move || {
        let conn = conn.blocking_lock();
        conn.execute_batch("DELETE FROM edges; DELETE FROM nodes;")?;
        Ok::<_, anyhow::Error>(())
    }).await;
    match result {
        Ok(Ok(_)) => { tracing::info!("âœ… SilvaDB purged successfully."); StatusCode::OK }
        _ => { tracing::error!("âŒ SilvaDB purge failed"); StatusCode::INTERNAL_SERVER_ERROR }
    }
}

pub async fn maintenance_vacuum_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    state.silva.vacuum().await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_checkpoint_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    state.silva.checkpoint().await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_decay_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let half_life = {
        let cfg = state.config.read().await;
        cfg.silva.decay_half_life_hours
    };
    state.silva.apply_decay(half_life).await.map(|_| StatusCode::OK).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

pub async fn maintenance_purge_lessons_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.purge_deprecated_lessons().await {
        Ok(count) => (StatusCode::OK, Json(serde_json::json!({ "purged": count }))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

pub async fn maintenance_clean_orphans_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    match state.silva.cleanup_orphan_nodes().await {
        Ok(count) => {
            tracing::info!("✅ Cleaned up {} orphan nodes successfully.", count);
            (StatusCode::OK, Json(serde_json::json!({ "status": "success", "deleted_count": count }))).into_response()
        }
        Err(e) => {
            tracing::error!("❌ Failed to cleanup orphan nodes: {:?}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "status": "error", "error": e.to_string() }))).into_response()
        }
    }
}

// --- INFERENCE LLAMA CONFIG PATCH (Safe TOML patch for [inference.llama]) ---

#[derive(serde::Deserialize)]
pub struct InferenceLlamaConfigRequest {
    pub primary_model: Option<String>,
    pub coordinator_model: Option<String>,
    pub routing_model: Option<String>,
    pub vision_model: Option<String>,
    pub provider: Option<String>,
    pub endpoint: Option<String>,
    pub port: Option<u16>,
    pub ctx_size: Option<usize>,
    pub n_gpu_layers: Option<i32>,
    pub threads: Option<usize>,
    pub batch_size: Option<usize>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub top_k: Option<i32>,
    pub repeat_penalty: Option<f32>,
}

/// POST /api/v1/config/inference-llama
/// Safe, targeted patch for [inference.llama] and [inference] sections.
/// Never round-trips the whole TOML through the browser (that pattern bricked config once).
/// Instead it reads, patches specific fields, validates, then atomic-writes.
pub async fn set_inference_llama_config_handler(
    Json(req): Json<InferenceLlamaConfigRequest>,
) -> impl IntoResponse {
    let config_path = crate::config::TylluanConfig::find_config_file()
        .unwrap_or_else(|| std::path::PathBuf::from("tylluan.toml"));
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    };

    // Parse the existing TOML as a generic Value so we can patch it
    let mut doc: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": format!("Failed to parse tylluan.toml: {}", e) }))).into_response(),
    };

    let root = match doc.as_table_mut() {
        Some(t) => t,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": "tylluan.toml root is not a TOML table"
        }))).into_response(),
    };

    // Ensure [inference] and [inference.llama] tables exist
    {
        if !root.contains_key("inference") {
            root.insert("inference".to_string(), toml::Value::Table(toml::map::Map::new()));
        }
        let inf = match root.get_mut("inference").and_then(|v| v.as_table_mut()) {
            Some(t) => t,
            None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": "[inference] section in tylluan.toml is not a table"
            }))).into_response(),
        };
        if !inf.contains_key("llama") {
            inf.insert("llama".to_string(), toml::Value::Table(toml::map::Map::new()));
        }

        // Patch [inference] top-level fields
        if let Some(ref model) = req.primary_model {
            inf.insert("primary_model".to_string(), toml::Value::String(model.clone()));
        }
        if let Some(ref model) = req.coordinator_model {
            inf.insert("coordinator_model".to_string(), toml::Value::String(model.clone()));
        }
        if let Some(ref model) = req.routing_model {
            inf.insert("routing_model".to_string(), toml::Value::String(model.clone()));
        }

        // Patch [inference.llama] sub-fields
        let llama = match inf.get_mut("llama").and_then(|v| v.as_table_mut()) {
            Some(t) => t,
            None => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
                "error": "[inference.llama] section in tylluan.toml is not a table"
            }))).into_response(),
        };
        if let Some(ref v) = req.provider     { llama.insert("provider".to_string(), toml::Value::String(v.clone())); }
        if let Some(ref v) = req.endpoint     { llama.insert("endpoint".to_string(), toml::Value::String(v.clone())); }
        if let Some(v)     = req.port          { llama.insert("port".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.ctx_size      { llama.insert("ctx_size".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.n_gpu_layers  { llama.insert("n_gpu_layers".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.threads       { llama.insert("threads".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.batch_size    { llama.insert("batch_size".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.temperature   { llama.insert("temperature".to_string(), toml::Value::Float(v as f64)); }
        if let Some(v)     = req.top_p         { llama.insert("top_p".to_string(), toml::Value::Float(v as f64)); }
        if let Some(v)     = req.top_k         { llama.insert("top_k".to_string(), toml::Value::Integer(v as i64)); }
        if let Some(v)     = req.repeat_penalty { llama.insert("repeat_penalty".to_string(), toml::Value::Float(v as f64)); }
    }

    // Patch [vision] section separately to avoid mutable borrow conflict
    if let Some(ref model) = req.vision_model {
        if !root.contains_key("vision") {
            root.insert("vision".to_string(), toml::Value::Table(toml::map::Map::new()));
        }
        if let Some(v) = root.get_mut("vision").and_then(|v| v.as_table_mut()) {
            v.insert("model_path".to_string(), toml::Value::String(model.clone()));
        }
    }

    let new_raw = match toml::to_string_pretty(&doc) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": format!("Failed to serialize TOML: {}", e) }))).into_response(),
    };

    // Final guard: must parse back as our config type
    if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("Patched TOML doesn't parse as TylluanConfig — refusing to write: {}", e)
        }))).into_response();
    }

    let tmp_path = config_path.with_extension("toml.tmp");
    if let Err(e) = fs::write(&tmp_path, &new_raw) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }
    if let Err(e) = fs::rename(&tmp_path, &config_path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response();
    }

    (StatusCode::OK, Json(serde_json::json!({
        "status": "saved",
        "message": "Configuración [inference.llama] guardada exitosamente en tylluan.toml"
    }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: start a mock MCP server on a random port and return the URL.
    /// Reads the HTTP request, then responds with a valid HTTP response.
    async fn start_mock_mcp(status: u16, body: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/messages");

        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            // Read the request so reqwest can finish sending
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
            // Write a well-formed HTTP/1.1 response
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        // Small delay to let the spawned task start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        url
    }

    #[tokio::test]
    async fn test_mcp_ping_success() {
        let url = start_mock_mcp(200, r#"{"jsonrpc":"2.0","id":"1","result":{}}"#).await;
        let res = mcp_ping_server(&url, 5).await;
        assert!(res.ok, "expected ok=true, got {res:?}");
        assert_eq!(res.status, "online");
        assert_eq!(res.http_status, 200);
        assert!(res.latency_ms < 5000, "latency too high: {}ms", res.latency_ms);
    }

    #[tokio::test]
    async fn test_mcp_ping_unexpected_response() {
        let url = start_mock_mcp(200, r#"{"jsonrpc":"2.0","id":"1","error":{"code":-32601,"message":"Method not found"}}"#).await;
        let res = mcp_ping_server(&url, 5).await;
        assert!(!res.ok, "expected ok=false for missing result field");
        assert_eq!(res.status, "unexpected_response");
    }

    #[tokio::test]
    async fn test_mcp_ping_connection_refused() {
        let url = "http://127.0.0.1:46891/messages".to_string();
        let res = mcp_ping_server(&url, 3).await;
        assert!(!res.ok, "expected ok=false for connection refused, got {res:?}");
        assert_eq!(res.status, "offline");
    }

    // ── External provider tests ─────────────────────────────────────

    /// Helper: start a mock HTTP server that responds to external LLM API calls.
    async fn start_mock_external(
        _path: &'static str,
        status: u16,
        body: &'static str,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        base_url
    }

    #[tokio::test]
    async fn test_external_provider_openai_success() {
        let base = start_mock_external(
            "/v1/chat/completions",
            200,
            r#"{"id":"chatcmpl-x","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"A"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":1,"total_tokens":11}}"#,
        ).await;
        let res = test_external_provider("openai_compatible", &base, "sk-test-key", "gpt-4o-mini", 5).await;
        assert!(res.ok, "expected ok=true, got {res:?}");
        assert_eq!(res.status, "online");
        assert_eq!(res.http_status, 200);
    }

    #[tokio::test]
    async fn test_external_provider_anthropic_success() {
        let base = start_mock_external(
            "/v1/messages",
            200,
            r#"{"id":"msg_01X","type":"message","role":"assistant","content":[{"type":"text","text":"A"}],"model":"claude-3-opus-20240229","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":1}}"#,
        ).await;
        let res = test_external_provider("anthropic_compatible", &base, "sk-ant-key", "claude-3-opus-20240229", 5).await;
        assert!(res.ok, "expected ok=true, got {res:?}");
        assert_eq!(res.status, "online");
        assert_eq!(res.http_status, 200);
    }

    #[tokio::test]
    async fn test_external_provider_ollama_success() {
        let base = start_mock_external(
            "/api/chat",
            200,
            r#"{"model":"llama3.2","created_at":"2024-01-01T00:00:00Z","message":{"role":"assistant","content":"A"},"done":true}"#,
        ).await;
        let res = test_external_provider("ollama_compatible", &base, "", "llama3.2", 5).await;
        assert!(res.ok, "expected ok=true, got {res:?}");
        assert_eq!(res.status, "online");
        assert_eq!(res.http_status, 200);
    }

    #[tokio::test]
    async fn test_external_provider_http_error() {
        let base = start_mock_external(
            "/v1/chat/completions",
            401,
            r#"{"error":{"message":"Invalid API key","type":"authentication_error"}}"#,
        ).await;
        let res = test_external_provider("openai_compatible", &base, "bad-key", "gpt-4o-mini", 5).await;
        assert!(!res.ok, "expected ok=false for HTTP 401");
        assert_eq!(res.status, "error");
        assert_eq!(res.http_status, 401);
    }

    #[tokio::test]
    async fn test_external_provider_connection_refused() {
        let res = test_external_provider("openai_compatible", "http://127.0.0.1:46892", "sk-test", "gpt-4o-mini", 3).await;
        assert!(!res.ok, "expected ok=false for connection refused");
        assert_eq!(res.status, "offline");
    }

    #[tokio::test]
    async fn test_external_provider_anthropic_has_headers() {
        // Verify Anthropic path sends anthropic-version and x-api-key headers
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        let captured_headers = std::sync::Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));
        let ch = captured_headers.clone();

        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else { return };
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let mut buf = vec![0u8; 4096];
            let n = stream.read(&mut buf).await.unwrap_or(0);
            let request_text = String::from_utf8_lossy(&buf[..n]);
            // Collect headers
            let mut hdrs = ch.lock().await;
            for line in request_text.lines() {
                if line.to_lowercase().starts_with("anthropic-version:")
                    || line.to_lowercase().starts_with("x-api-key:")
                {
                    hdrs.push(line.to_string());
                }
            }
            drop(hdrs);
            let body = r#"{"id":"msg_01X","type":"message","role":"assistant","content":[{"type":"text","text":"A"}],"model":"claude","stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":1}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let res = test_external_provider("anthropic_compatible", &base_url, "sk-ant-key", "claude-3", 5).await;
        assert!(res.ok, "Anthropic test should succeed, got {res:?}");

        let hdrs = captured_headers.lock().await;
        let hdr_text = hdrs.join("\n");
        assert!(hdr_text.contains("anthropic-version"), "should send anthropic-version header, got: {hdr_text}");
        assert!(hdr_text.contains("x-api-key"), "should send x-api-key header, got: {hdr_text}");
    }
}
