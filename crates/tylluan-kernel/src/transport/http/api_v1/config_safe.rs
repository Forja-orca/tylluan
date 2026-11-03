//! A0 «Ajustes completos» (SPEC 2026-11-02): contrato seguro de configuración.
//!
//! - `GET /api/v1/config` nunca devuelve valores de secretos (`env`/`headers` de
//!   `external_mcp`, `env` de `services`): claves visibles, valor `"***"`.
//! - `GET /api/v1/config/schema`: campos editables, tipo, límites, si exige
//!   reinicio y si es sensible. Es el contrato que consume el dashboard (A1/A2).
//! - `POST /api/v1/config/validate`: dry-run de ediciones dirigidas; devuelve
//!   diff y errores, no escribe nada.
//! - `POST /api/v1/config/edit`: escribe SOLO claves editables y dirigidas, con
//!   copia previa del fichero y auditoría sin valores sensibles.
//!
//! El POST antiguo (`save_config_handler`) se mantiene por compatibilidad, pero
//! también hace copia previa y auditoría; la vía nueva es `/config/edit`.

use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::transport::http::HttpState;

/// Valor con el que se sustituyen los secretos en las respuestas.
pub const REDACTED: &str = "***";

#[derive(Debug, Clone, Copy)]
pub struct SchemaField {
    pub key: &'static str,
    pub section: &'static str,
    pub kind: &'static str,
    pub editable: bool,
    pub requires_restart: bool,
    pub sensitive: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub options: &'static [&'static str],
    pub reason: &'static str,
}

pub const SCHEMA: &[SchemaField] = &[
    // General
    SchemaField { key: "low_memory_mode", section: "general", kind: "boolean", editable: true, requires_restart: false, sensitive: false, min: None, max: None, options: &[], reason: "" },
    SchemaField { key: "dev_mode", section: "general", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], reason: "solo el operador (tylluan.toml)" },
    SchemaField { key: "host", section: "general", kind: "string", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], reason: "solo el operador (tylluan.toml)" },
    SchemaField { key: "port", section: "general", kind: "integer", editable: false, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(65535.0), options: &[], reason: "solo el operador (tylluan.toml)" },
    // Inferencia (absorbe Model Hub)
    SchemaField { key: "memory.embedding_model", section: "inference", kind: "string", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &[], reason: "" },
    SchemaField { key: "memory.vector_dimensions", section: "inference", kind: "integer", editable: false, requires_restart: true, sensitive: false, min: Some(1024.0), max: Some(1024.0), options: &[], reason: "invariante 1024: migrar vectores antes de cambiarla" },
    SchemaField { key: "inference.device", section: "inference", kind: "enum", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &["cpu", "directml", "cuda", "coreml"], reason: "" },
    SchemaField { key: "inference.max_threads", section: "inference", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(256.0), options: &[], reason: "" },
    SchemaField { key: "inference.primary_model", section: "inference", kind: "string", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &[], reason: "" },
    SchemaField { key: "silva.reranker_enabled", section: "inference", kind: "boolean", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &[], reason: "" },
    // Mantenimiento
    SchemaField { key: "memory.background_concurrency", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(1.0), max: Some(32.0), options: &[], reason: "" },
    SchemaField { key: "memory.background_max_wait_secs", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(0.0), max: Some(3600.0), options: &[], reason: "" },
    SchemaField { key: "memory.reindex_node_delay_ms", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(0.0), max: Some(10000.0), options: &[], reason: "" },
    SchemaField { key: "memory.reindex_nodes_per_tick", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(1.0), max: Some(10000.0), options: &[], reason: "" },
    SchemaField { key: "memory.reindex_tick_secs", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(1.0), max: Some(86400.0), options: &[], reason: "" },
    SchemaField { key: "logging.max_file_size_mb", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(1.0), max: Some(10240.0), options: &[], reason: "" },
    SchemaField { key: "logging.max_files", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(1.0), max: Some(100.0), options: &[], reason: "" },
    // Seguridad (solo lectura en A0; A3 añade acciones con código de operador)
    SchemaField { key: "security.intent_filter", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], reason: "sensible: solo lectura en A0 (A3: código de operador)" },
    SchemaField { key: "security.coherence_gate_hybrid_enabled", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], reason: "sensible: solo lectura en A0" },
    SchemaField { key: "security.encrypt_at_rest", section: "security", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], reason: "sensible: solo lectura en A0" },
    SchemaField { key: "security.external_acl.enabled", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], reason: "B1/D-96: solo el operador en tylluan.toml" },
    // Conectores (B1: solo el operador)
    SchemaField { key: "external_mcp", section: "connectors", kind: "array", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], reason: "B1: [[external_mcp]] solo en tylluan.toml" },
    SchemaField { key: "services", section: "connectors", kind: "table", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], reason: "B1: [services] solo en tylluan.toml" },
    SchemaField { key: "wake.command", section: "connectors", kind: "string", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], reason: "B1: solo el operador" },
];

pub fn schema_field(key: &str) -> Option<&'static SchemaField> {
    SCHEMA.iter().find(|f| f.key == key)
}

pub fn schema_json() -> serde_json::Value {
    let fields: Vec<serde_json::Value> = SCHEMA
        .iter()
        .map(|f| {
            let mut obj = json!({
                "key": f.key,
                "section": f.section,
                "type": f.kind,
                "editable_via_api": f.editable,
                "requires_restart": f.requires_restart,
                "sensitive": f.sensitive,
                "reason": f.reason,
            });
            if !f.options.is_empty() {
                obj["options"] = json!(f.options);
            }
            if let Some(min) = f.min {
                obj["min"] = json!(min);
            }
            if let Some(max) = f.max {
                obj["max"] = json!(max);
            }
            obj
        })
        .collect();
    json!({
        "schema_version": 1,
        "source": "tylluan.toml",
        "fields": fields,
        "notes": [
            "editable_via_api=false exige edición manual del TOML (B1/operador).",
            "sensitive=true: A0 solo lectura; A3 añadirá acciones con código de operador.",
            "requires_restart=true: el cambio no surte efecto hasta reiniciar el kernel."
        ]
    })
}

/// Sustituye valores de secretos por `***` conservando las claves.
pub fn redact_secrets(value: &mut serde_json::Value) {
    if let Some(servers) = value
        .get_mut("external_mcp")
        .and_then(serde_json::Value::as_array_mut)
    {
        for server in servers {
            for field in ["env", "headers"] {
                if let Some(map) = server
                    .get_mut(field)
                    .and_then(serde_json::Value::as_object_mut)
                {
                    for (_k, v) in map.iter_mut() {
                        *v = json!(REDACTED);
                    }
                }
            }
        }
    }
    if let Some(services) = value
        .get_mut("services")
        .and_then(serde_json::Value::as_object_mut)
    {
        for (_name, service) in services.iter_mut() {
            if let Some(map) = service
                .get_mut("env")
                .and_then(serde_json::Value::as_object_mut)
            {
                for (_k, v) in map.iter_mut() {
                    *v = json!(REDACTED);
                }
            }
        }
    }
    // F1 (D-100): en `security.acl.tokens` y `token_agent_bindings` la CLAVE es
    // el secreto, no el valor. No se puede enmascarar el valor: hay que omitir
    // el mapa y exponer solo el numero de entradas.
    if let Some(acl) = value
        .get_mut("security")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|security| security.get_mut("acl"))
        .and_then(serde_json::Value::as_object_mut)
    {
        for field in ["tokens", "token_agent_bindings"] {
            if let Some(map) = acl.remove(field).and_then(|v| v.as_object().cloned()) {
                acl.insert(format!("{field}_count"), json!(map.len()));
            }
        }
    }
    // F2 (D-100): secretos de federacion por peer.
    if let Some(peers) = value
        .get_mut("federation_peers")
        .and_then(serde_json::Value::as_array_mut)
    {
        for peer in peers {
            for field in ["auth_token", "shared_secret"] {
                if peer.get(field).map(|v| v.is_string()).unwrap_or(false) {
                    peer[field] = json!(REDACTED);
                }
            }
        }
    }
}

/// Enmascara los VALORES de un mapa de entorno/cabeceras (claves visibles).
pub fn redacted_map(
    map: &Option<std::collections::HashMap<String, String>>,
) -> Option<std::collections::HashMap<String, String>> {
    map.as_ref().map(|m| {
        m.keys()
            .map(|k| (k.clone(), REDACTED.to_string()))
            .collect()
    })
}

pub fn redacted_config_json(config: &crate::config::TylluanConfig) -> serde_json::Value {
    let mut value = serde_json::to_value(config).unwrap_or_else(|e| json!({"error": e.to_string()}));
    redact_secrets(&mut value);
    value
}

#[derive(Debug, Deserialize)]
pub struct ConfigEdit {
    pub key: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize, Default)]
pub struct ConfigEditsRequest {
    #[serde(default)]
    pub edits: Vec<ConfigEdit>,
    /// F4 (D-100): etag del fichero que vio el cliente (GET /config). Si no
    /// coincide con el actual, /config/edit responde 409 y no escribe.
    #[serde(default)]
    pub etag: Option<String>,
}

/// Etag del TOML actual (sha256 corto). Viaja en `x-config-etag`.
pub fn etag_of(raw: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(raw.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    hex[..16].to_string()
}

/// R2 (D-101): el etag es OBLIGATORIO en /config/edit (nada de «última gana»
/// silencioso): ausente -> 400; distinto -> 409.
pub fn etag_guard(
    expected: Option<&str>,
    current: &str,
) -> Result<(), (StatusCode, serde_json::Value)> {
    match expected {
        None => Err((
            StatusCode::BAD_REQUEST,
            json!({
                "ok": false,
                "error": "etag_required",
                "etag": current,
                "reason": "falta el etag de /config; vuelve a leer GET /config o /config/validate",
            }),
        )),
        Some(value) if value != current => Err((
            StatusCode::CONFLICT,
            json!({
                "ok": false,
                "error": "etag_mismatch",
                "etag": current,
                "reason": "la config cambio desde que la leiste; vuelve a leerla y repite la validacion",
            }),
        )),
        Some(_) => Ok(()),
    }
}

fn json_to_toml(value: &serde_json::Value) -> Result<toml::Value, String> {
    match value {
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(toml::Value::Integer(i))
            } else if let Some(f) = n.as_f64() {
                Ok(toml::Value::Float(f))
            } else {
                Err("numero no representable en TOML".into())
            }
        }
        _ => Err("tipo no soportado: solo boolean, string o integer".into()),
    }
}

fn validate_edit(edit: &ConfigEdit) -> Result<toml::Value, String> {
    let field = schema_field(&edit.key)
        .ok_or_else(|| format!("clave desconocida: {}", edit.key))?;
    if !field.editable {
        return Err(format!(
            "clave no editable por la API: {} ({})",
            edit.key, field.reason
        ));
    }
    let toml_value = json_to_toml(&edit.value)?;
    match field.kind {
        "boolean" => {
            if !matches!(toml_value, toml::Value::Boolean(_)) {
                return Err(format!("{} debe ser boolean", edit.key));
            }
        }
        "string" => {
            if !matches!(toml_value, toml::Value::String(_)) {
                return Err(format!("{} debe ser string", edit.key));
            }
        }
        "integer" => {
            let n = match toml_value {
                toml::Value::Integer(i) => i as f64,
                _ => return Err(format!("{} debe ser integer", edit.key)),
            };
            if let Some(min) = field.min
                && n < min
            {
                return Err(format!("{} por debajo del minimo {min}", edit.key));
            }
            if let Some(max) = field.max
                && n > max
            {
                return Err(format!("{} por encima del maximo {max}", edit.key));
            }
        }
        "enum" => {
            let s = match &toml_value {
                toml::Value::String(s) => s.clone(),
                _ => return Err(format!("{} debe ser string (enum)", edit.key)),
            };
            if !field.options.contains(&s.as_str()) {
                // Nunca repetir el valor recibido: puede ser un secreto pegado
                // en el campo equivocado (R1 de D-101).
                return Err(format!(
                    "{} valor invalido; opciones: {:?}",
                    edit.key, field.options
                ));
            }
        }
        other => return Err(format!("tipo de esquema no soportado: {other}")),
    }
    Ok(toml_value)
}

fn apply_key(root: &mut toml::Value, key: &str, value: toml::Value) -> Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        return Err(format!("clave invalida: {key}"));
    }
    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        if !current.is_table() {
            return Err(format!("{key}: la ruta padre no es una tabla"));
        }
        let table = current.as_table_mut().ok_or("tabla esperada")?;
        current = table
            .entry((*part).to_string())
            .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    }
    let last = parts[parts.len() - 1];
    let table = current.as_table_mut().ok_or("tabla esperada")?;
    table.insert(last.to_string(), value);
    Ok(())
}

fn flatten(value: &toml::Value, prefix: &str, out: &mut std::collections::BTreeMap<String, serde_json::Value>) {
    match value {
        toml::Value::Table(table) => {
            for (k, v) in table {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(v, &path, out);
            }
        }
        other => {
            out.insert(prefix.to_string(), json!(other.to_string()));
        }
    }
}

pub fn config_diff(old: &toml::Value, new: &toml::Value) -> Vec<serde_json::Value> {
    let mut a = std::collections::BTreeMap::new();
    let mut b = std::collections::BTreeMap::new();
    flatten(old, "", &mut a);
    flatten(new, "", &mut b);
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    let mut diff = Vec::new();
    for key in keys {
        let before = a.get(key);
        let after = b.get(key);
        if before != after {
            let sensitive = schema_field(key).map(|f| f.sensitive).unwrap_or(false)
                || key.starts_with("external_mcp.")
                || key.starts_with("services.");
            if sensitive {
                diff.push(json!({
                    "key": key,
                    "old": if before.is_some() { json!(REDACTED) } else { serde_json::Value::Null },
                    "new": if after.is_some() { json!(REDACTED) } else { serde_json::Value::Null },
                }));
            } else {
                diff.push(json!({
                    "key": key,
                    "old": before.cloned().unwrap_or(serde_json::Value::Null),
                    "new": after.cloned().unwrap_or(serde_json::Value::Null),
                }));
            }
        }
    }
    diff
}

fn candidate_config(raw: &str, edits: &[ConfigEdit]) -> Result<(toml::Value, toml::Value, Vec<serde_json::Value>), Vec<String>> {
    let mut value: toml::Value = toml::from_str(raw).map_err(|e| vec![format!("tylluan.toml no parsea: {e}")])?;
    let old = value.clone();
    let mut errors = Vec::new();
    for edit in edits {
        match validate_edit(edit) {
            Ok(v) => {
                if let Err(e) = apply_key(&mut value, &edit.key, v) {
                    errors.push(e);
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if errors.is_empty() {
        match toml::to_string(&value) {
            Ok(text) => {
                if let Err(e) = toml::from_str::<crate::config::TylluanConfig>(&text) {
                    errors.push(format!("config resultante invalida: {e}"));
                }
            }
            Err(e) => errors.push(format!("no se pudo serializar la config: {e}")),
        }
    }
    let diff = if errors.is_empty() {
        config_diff(&old, &value)
    } else {
        Vec::new()
    };
    if errors.is_empty() {
        Ok((old, value, diff))
    } else {
        Err(errors)
    }
}

fn config_path() -> PathBuf {
    crate::config::TylluanConfig::find_config_file().unwrap_or_else(|| PathBuf::from("tylluan.toml"))
}

fn requires_restart(diff: &[serde_json::Value]) -> bool {
    diff.iter().any(|d| {
        d.get("key")
            .and_then(|k| k.as_str())
            .and_then(schema_field)
            .map(|f| f.requires_restart)
            .unwrap_or(false)
    })
}

pub fn backup_path_for(path: &Path) -> Result<PathBuf, String> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tylluan.toml");
    Ok(path.with_file_name(format!("{file_name}.bak-{ts}")))
}

pub fn audit_change(diff: &[serde_json::Value], backup: &Path, source: &str, actor: &str) {
    let keys: Vec<&str> = diff
        .iter()
        .filter_map(|d| d.get("key").and_then(|k| k.as_str()))
        .collect();
    let record = json!({
        "ts": chrono::Utc::now().timestamp_millis(),
        "actor": actor,
        "source": source,
        "keys": keys,
        "diff": diff,
        "backup": backup.display().to_string(),
    });
    let path = crate::config::data_file("config_audit.jsonl");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut file) => {
            use std::io::Write;
            let _ = writeln!(file, "{}", record);
        }
        Err(e) => tracing::warn!("no se pudo escribir {}: {e}", path.display()),
    }
    // Los valores sensibles no van al log: solo claves y backup.
    tracing::info!(keys = ?keys, backup = %backup.display(), source, "config editada");
}

pub async fn config_schema_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(schema_json())).into_response()
}

pub async fn validate_config_handler(Json(req): Json<ConfigEditsRequest>) -> impl IntoResponse {
    let raw = match std::fs::read_to_string(config_path()) {
        Ok(raw) => raw,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"errors": [format!("no se pudo leer tylluan.toml: {e}")]}))).into_response()
        }
    };
    if req.edits.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"errors": ["no hay ediciones"], "ok": false}))).into_response();
    }
    match candidate_config(&raw, &req.edits) {
        Ok((_old, _new, diff)) => (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "dry_run": true,
                "etag": etag_of(&raw),
                "diff": diff,
                "requires_restart": requires_restart(&diff),
                "errors": [],
            })),
        )
            .into_response(),
        Err(errors) => (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "dry_run": true, "errors": errors, "diff": []}))).into_response(),
    }
}

pub async fn edit_config_handler(
    State(_state): State<Arc<HttpState>>,
    Json(req): Json<ConfigEditsRequest>,
) -> impl IntoResponse {
    if req.edits.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "errors": ["no hay ediciones"]}))).into_response();
    }
    let path = config_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [format!("no se pudo leer {}: {e}", path.display())]}))).into_response()
        }
    };
    let current_etag = etag_of(&raw);
    if let Err((status, body)) = etag_guard(req.etag.as_deref(), &current_etag) {
        return (status, Json(body)).into_response();
    }
    let (_old, new_value, diff) = match candidate_config(&raw, &req.edits) {
        Ok(v) => v,
        Err(errors) => return (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "errors": errors}))).into_response(),
    };
    if diff.is_empty() {
        return (StatusCode::OK, Json(json!({"ok": true, "status": "no_changes", "applied": [], "diff": []}))).into_response();
    }
    let backup = match backup_path_for(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [e]}))).into_response(),
    };
    if let Err(e) = std::fs::copy(&path, &backup) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [format!("no se pudo hacer copia previa: {e}")]}))).into_response();
    }
    let tmp = path.with_extension("toml.tmp");
    let serialized = match toml::to_string(&new_value) {
        Ok(text) => text,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [format!("no se pudo serializar: {e}")], "backup": backup.display().to_string()}))).into_response()
        }
    };
    if let Err(e) = std::fs::write(&tmp, serialized) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [format!("no se pudo escribir temporal: {e}")], "backup": backup.display().to_string()}))).into_response();
    }
    if let Err(e) = std::fs::rename(&tmp, &path) {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [format!("no se pudo reemplazar tylluan.toml: {e}")], "backup": backup.display().to_string()}))).into_response();
    }
    audit_change(&diff, &backup, "POST /api/v1/config/edit", "config_api");
    let restart = requires_restart(&diff);
    if let Err(e) = crate::config::TylluanConfig::reload().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "ok": false,
            "status": "saved_reload_failed",
            "errors": [format!("config escrita pero reload fallo: {e}")],
            "backup": backup.display().to_string(),
            "diff": diff,
            "requires_restart": restart,
        }))).into_response();
    }
    let keys: Vec<&str> = diff.iter().filter_map(|d| d.get("key").and_then(|k| k.as_str())).collect();
    (StatusCode::OK, Json(json!({
        "ok": true,
        "status": "saved",
        "keys": keys,
        "diff": diff,
        "backup": backup.display().to_string(),
        "requires_restart": restart,
    }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_raw() -> String {
        r#"
[memory]
embedding_model = "mxbai-embed-large"
vector_dimensions = 1024

[inference]
device = "cpu"
max_threads = 4

[[external_mcp]]
name = "github"
url = "https://example.invalid/sse"
env = { GITHUB_TOKEN = "ghp_SECRETO", OTRA = "valor" }
headers = { Authorization = "Bearer SECRETO" }

[services.foo]
command = "foo"
env = { API_KEY = "secreto" }

[security.external_acl]
enabled = false

[wake]
command = "opencode"
"#
        .to_string()
    }

    #[test]
    fn redaction_hides_secret_values_but_keeps_key_names() {
        let config: crate::config::TylluanConfig = toml::from_str(&sample_raw()).expect("config");
        let json = redacted_config_json(&config);
        let text = json.to_string();
        assert!(!text.contains("ghp_SECRETO"), "{text}");
        assert!(!text.contains("Bearer SECRETO"), "{text}");
        assert!(!text.contains("secreto"), "{text}");
        assert!(text.contains("GITHUB_TOKEN"), "{text}");
        assert!(text.contains("Authorization"), "{text}");
        assert!(text.contains(REDACTED), "{text}");
        assert_eq!(json["external_mcp"][0]["env"]["GITHUB_TOKEN"], REDACTED);
        assert_eq!(json["services"]["foo"]["env"]["API_KEY"], REDACTED);
    }

    #[test]
    fn schema_marks_b1_keys_read_only_and_sensitive() {
        let schema = schema_json();
        let fields = schema["fields"].as_array().expect("fields");
        let ext_mcp = fields.iter().find(|f| f["key"] == "external_mcp").expect("external_mcp");
        assert_eq!(ext_mcp["editable_via_api"], false);
        assert_eq!(ext_mcp["sensitive"], true);
        let ext_acl = fields.iter().find(|f| f["key"] == "security.external_acl.enabled").expect("acl");
        assert_eq!(ext_acl["editable_via_api"], false);
        let model = fields.iter().find(|f| f["key"] == "memory.embedding_model").expect("model");
        assert_eq!(model["editable_via_api"], true);
        assert_eq!(model["requires_restart"], true);
        let dims = fields.iter().find(|f| f["key"] == "memory.vector_dimensions").expect("dims");
        assert_eq!(dims["editable_via_api"], false);
    }

    #[test]
    fn validate_reports_diff_and_rejects_unknown_protected_and_out_of_range() {
        let raw = sample_raw();
        let edits = vec![ConfigEdit { key: "memory.embedding_model".into(), value: json!("nuevo-modelo") }];
        let (_old, _new, diff) = candidate_config(&raw, &edits).expect("edits validas");
        assert!(diff.iter().any(|d| d["key"] == "memory.embedding_model"));
        assert!(requires_restart(&diff));

        for (key, value) in [
            ("no.existe", json!(1)),
            ("external_mcp", json!([])),
            ("security.external_acl.enabled", json!(true)),
            ("inference.max_threads", json!(9999)),
            ("low_memory_mode", json!("si")),
        ] {
            let bad = vec![ConfigEdit { key: key.into(), value }];
            let err = candidate_config(&raw, &bad).expect_err(key);
            assert!(!err.is_empty(), "{key}");
        }
    }

    #[test]
    fn sentinel_secret_never_leaks_on_any_surface() {
        // F5 (D-100): el MISMO centinela en env de external_mcp, env de
        // services, federation auth_token/shared_secret y como CLAVE de
        // security.acl.tokens/token_agent_bindings. No puede aparecer en
        // ninguna respuesta que A0 construye.
        let mut raw = sample_raw();
        raw.push_str(
            r#"
[[external_mcp]]
name = "centinela"
env = { CENTINELA = "TOKEN_SECRETO_123" }

[[federation_peers]]
name = "peer-centinela"
url = "https://peer.example.invalid"
auth_token = "TOKEN_SECRETO_123"
shared_secret = "TOKEN_SECRETO_123"

[security.acl]
tokens = { "TOKEN_SECRETO_123" = "admin" }
token_agent_bindings = { "TOKEN_SECRETO_123" = "a1" }
"#,
        );
        let config: crate::config::TylluanConfig = toml::from_str(&raw).expect("config con centinela");
        let redacted = redacted_config_json(&config);
        let diff = candidate_config(
            &raw,
            &[ConfigEdit {
                key: "memory.embedding_model".into(),
                value: json!("otro-modelo"),
            }],
        )
        .expect("validate")
        .2;
        let surfaces = [
            redacted.to_string(),
            schema_json().to_string(),
            serde_json::to_string(&diff).expect("json"),
        ];
        for text in &surfaces {
            assert!(!text.contains("TOKEN_SECRETO_123"), "fuga: {text}");
        }
        assert_eq!(redacted["security"]["acl"]["tokens_count"], 1);
        assert_eq!(redacted["security"]["acl"]["token_agent_bindings_count"], 1);
        assert!(redacted["security"]["acl"].get("tokens").is_none());
        assert_eq!(redacted["federation_peers"][0]["auth_token"], REDACTED);
        assert_eq!(redacted["federation_peers"][0]["shared_secret"], REDACTED);
        assert!(redacted["external_mcp"].as_array().unwrap().iter().any(|s| {
            s["env"]["CENTINELA"] == REDACTED
        }));
        // R1 (D-101): una edicion invalida con el centinela en el valor no puede
        // devolverlo en el JSON de errors (ni en el cuerpo 400).
        let bad = candidate_config(
            &raw,
            &[ConfigEdit {
                key: "inference.device".into(),
                value: json!("TOKEN_SECRETO_123"),
            }],
        )
        .expect_err("edicion invalida");
        let errors = serde_json::to_string(&bad).expect("json");
        assert!(!errors.contains("TOKEN_SECRETO_123"), "fuga en errors: {errors}");
    }

    #[test]
    fn etag_is_required_and_mismatch_is_409() {
        let current = etag_of("[memory]\nembedding_model = \"a\"\n");
        let (status, body) = etag_guard(None, &current).expect_err("sin etag");
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "etag_required");
        let (status, body) = etag_guard(Some("otro"), &current).expect_err("etag distinto");
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "etag_mismatch");
        assert_eq!(body["etag"], current);
        assert!(etag_guard(Some(&current), &current).is_ok());
    }

    #[test]
    fn etag_changes_with_content() {
        let a = etag_of("[memory]\nembedding_model = \"a\"\n");
        let b = etag_of("[memory]\nembedding_model = \"b\"\n");
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }

    #[test]
    fn diff_masks_sensitive_values() {
        let raw = sample_raw();
        // `services.foo.env` es sensible: el diff no puede llevar el valor.
        let mut value: toml::Value = toml::from_str(&raw).expect("toml");
        let old = value.clone();
        value["services"]["foo"]["env"]["API_KEY"] = toml::Value::String("OTRO_SECRETO".into());
        let diff = config_diff(&old, &value);
        let text = serde_json::to_string(&diff).expect("json");
        assert!(!text.contains("OTRO_SECRETO"), "{text}");
        assert!(!text.contains("secreto"), "{text}");
        assert!(text.contains("services.foo.env.API_KEY"), "{text}");
        assert!(text.contains(REDACTED), "{text}");
    }
}
