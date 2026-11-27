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
    /// D227 (T4771): nivel de la clasificacion del operador.
    /// `essential` = asistente de primera instalacion y Ajustes principal;
    /// `advanced` = afinado plegado; `internal` = solo diagnostico de lectura.
    pub level: &'static str,
    /// D227: valor por defecto como literal JSON. No se escribe a mano y se deja
    /// ahi: el test `defaults_del_esquema_coinciden_con_el_config_real` lo compara
    /// con `TylluanConfig::default()`, asi que un default inventado pone el test
    /// ROJO. Los dos campos de `DEFAULT_DINAMICO` se validan aparte porque su
    /// default depende de la plataforma o de las features de compilacion.
    pub default: &'static str,
    pub reason: &'static str,
}

/// T-DSH-178 (hallazgo H1 de Buffy, 2026-11-04): claves cuyo valor el kernel
/// captura SOLO al arrancar — appender rotativo de logging (main.rs:50-59),
/// supervisor de mantenimiento (main.rs:1455-1456) y bucle reindexador
/// (main.rs:1960-1961 copia + set_interval). Son la fuente única: el test
/// `t_dsh_178_boot_only_tabla_ata_el_esquema` las contrasta con SCHEMA en
/// ambos sentidos, de modo que quitar una entrada de aquí (o su marca en el
/// esquema) pone el test ROJO. `memory.reindex_node_delay_ms` NO está: se
/// lee fresco en cada pasada (api_memory.rs:134), es en caliente.
pub const BOOT_ONLY_KEYS: &[&str] = &[
    "logging.max_file_size_mb",
    "logging.max_files",
    "memory.background_concurrency",
    "memory.background_max_wait_secs",
    "memory.reindex_nodes_per_tick",
    "memory.reindex_tick_secs",
];

pub const SCHEMA: &[SchemaField] = &[
    // general
    SchemaField { key: "low_memory_mode", section: "general", kind: "boolean", editable: true, requires_restart: false, sensitive: false, min: None, max: None, options: &[], level: "advanced", default: r#"false"#, reason: "" },
    SchemaField { key: "dev_mode", section: "general", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "solo el operador (tylluan.toml)" },
    SchemaField { key: "nexus.host", section: "general", kind: "string", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#""127.0.0.1""#, reason: "solo el operador (tylluan.toml)" },
    SchemaField { key: "nexus.port", section: "general", kind: "integer", editable: false, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(65535.0), options: &[], level: "essential", default: r#"47004"#, reason: "solo el operador (tylluan.toml)" },
    // memory
    SchemaField { key: "memory.db_path", section: "memory", kind: "string", editable: false, requires_restart: true, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#""./data/tylluan.db""#, reason: "mover la base exige migracion: lo hace el asistente, no un campo de texto" },
    SchemaField { key: "memory.embedding_model", section: "memory", kind: "string", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#""mxbai-embed-large""#, reason: "" },
    SchemaField { key: "memory.vector_dimensions", section: "memory", kind: "integer", editable: false, requires_restart: true, sensitive: false, min: Some(1024.0), max: Some(1024.0), options: &[], level: "essential", default: r#"1024"#, reason: "invariante 1024: migrar vectores antes de cambiarla" },
    SchemaField { key: "memory.embedding_idle_ttl_secs", section: "memory", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(0.0), max: Some(86400.0), options: &[], level: "advanced", default: r#"0"#, reason: "libera RAM de ONNX tras inactividad; 0 = apagado" },
    SchemaField { key: "silva.db_path", section: "memory", kind: "string", editable: false, requires_restart: true, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#""./data/silva.db""#, reason: "mover la base exige migracion" },
    SchemaField { key: "silva.decay_enabled", section: "memory", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"true"#, reason: "solo el operador: apaga la decadencia de toda la memoria" },
    SchemaField { key: "silva.reranker_enabled", section: "memory", kind: "boolean", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &[], level: "advanced", default: r#"true"#, reason: "" },
    // inference
    SchemaField { key: "inference.device", section: "inference", kind: "enum", editable: true, requires_restart: true, sensitive: false, min: None, max: None, options: &["cpu", "directml", "cuda", "coreml"], level: "essential", default: r#"auto"#, reason: "por defecto lo elige la plataforma (auto_select_device)" },
    SchemaField { key: "inference.max_threads", section: "inference", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(256.0), options: &[], level: "advanced", default: r#"null"#, reason: "" },
    SchemaField { key: "inference.primary_model", section: "inference", kind: "string", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#""""#, reason: "el chat lo elige el usuario; Tylluan no impone modelo (directiva de Jose)" },
    SchemaField { key: "inference.llama.models_dir", section: "inference", kind: "string", editable: false, requires_restart: true, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#"null"#, reason: "carpeta del usuario; se toca desde el asistente" },
    SchemaField { key: "inference.llama.ctx_size", section: "inference", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(512.0), max: Some(1048576.0), options: &[], level: "essential", default: r#"4096"#, reason: "" },
    SchemaField { key: "inference.llama.n_gpu_layers", section: "inference", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(-1.0), max: Some(999.0), options: &[], level: "essential", default: r#"99"#, reason: "" },
    SchemaField { key: "inference.llama.threads", section: "inference", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(256.0), options: &[], level: "essential", default: r#"4"#, reason: "" },
    SchemaField { key: "inference.llama.temperature", section: "inference", kind: "float", editable: true, requires_restart: false, sensitive: false, min: Some(0.0), max: Some(2.0), options: &[], level: "advanced", default: r#"0.7"#, reason: "" },
    // cloud
    SchemaField { key: "inference.points.chat.cloud_enabled", section: "cloud", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: solo el operador (D213); la nube no se enciende por API" },
    SchemaField { key: "inference.points.chat.model", section: "cloud", kind: "string", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"null"#, reason: "solo el operador: el modelo de chat lo elige el usuario" },
    SchemaField { key: "inference.points.chat.api_key_env", section: "cloud", kind: "string", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"null"#, reason: "solo el operador: apunta a una credencial de nube" },
    SchemaField { key: "inference.points.decision.cloud_enabled", section: "cloud", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: solo el operador (D213); la nube no se enciende por API" },
    SchemaField { key: "inference.points.grammar_judge.cloud_enabled", section: "cloud", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: solo el operador (D213); la nube no se enciende por API" },
    SchemaField { key: "inference.points.sentinel.cloud_enabled", section: "cloud", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: solo el operador (D213); la nube no se enciende por API" },
    SchemaField { key: "nexus.allow_hf_download", section: "cloud", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "descarga de modelos: solo el operador (sale a internet)" },
    SchemaField { key: "nexus.allowed_model_hosts", section: "cloud", kind: "array", editable: false, requires_restart: false, sensitive: false, min: None, max: None, options: &[], level: "advanced", default: r#"["huggingface.co","cdn-lfs.huggingface.co"]"#, reason: "lista: se edita en tylluan.toml" },
    // maintenance
    SchemaField { key: "memory.background_concurrency", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(32.0), options: &[], level: "advanced", default: r#"2"#, reason: "el supervisor se crea al arrancar (H1)" },
    SchemaField { key: "memory.background_max_wait_secs", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(0.0), max: Some(3600.0), options: &[], level: "advanced", default: r#"60"#, reason: "el supervisor se crea al arrancar (H1)" },
    SchemaField { key: "memory.reindex_node_delay_ms", section: "maintenance", kind: "integer", editable: true, requires_restart: false, sensitive: false, min: Some(0.0), max: Some(10000.0), options: &[], level: "advanced", default: r#"200"#, reason: "en caliente: se lee en cada pasada" },
    SchemaField { key: "memory.reindex_nodes_per_tick", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(10000.0), options: &[], level: "advanced", default: r#"100"#, reason: "el bucle lo captura al arrancar (H1)" },
    SchemaField { key: "memory.reindex_tick_secs", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(86400.0), options: &[], level: "advanced", default: r#"600"#, reason: "el intervalo se fija al arrancar (H1)" },
    SchemaField { key: "logging.max_file_size_mb", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(10240.0), options: &[], level: "advanced", default: r#"50"#, reason: "el appender rotativo se abre al arrancar (H1)" },
    SchemaField { key: "logging.max_files", section: "maintenance", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(100.0), options: &[], level: "advanced", default: r#"5"#, reason: "el appender rotativo se abre al arrancar (H1)" },
    // security
    SchemaField { key: "security.intent_filter", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"true"#, reason: "sensible: solo lectura (A3: codigo de operador)" },
    SchemaField { key: "security.coherence_gate_hybrid_enabled", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"false"#, reason: "sensible: solo lectura" },
    SchemaField { key: "security.encrypt_at_rest", section: "security", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"false"#, reason: "sensible: solo lectura; depende de la feature encryption" },
    SchemaField { key: "security.external_acl.enabled", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "B1/D-96: solo el operador en tylluan.toml" },
    SchemaField { key: "security.capabilities_enforce", section: "security", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"false"#, reason: "sensible: solo lectura" },
    SchemaField { key: "nexus.expose_guild_tools", section: "security", kind: "boolean", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"false"#, reason: "excepcion opt-in de AUD-07: amplia las tools del contrato" },
    // connectors
    SchemaField { key: "guilds.core.always_on", section: "connectors", kind: "array", editable: false, requires_restart: false, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#"["bash","memory","filesystem"]"#, reason: "se activan por guild desde /api/v1/guilds, no reescribiendo la lista" },
    SchemaField { key: "guilds.core.warm_pool", section: "connectors", kind: "array", editable: false, requires_restart: false, sensitive: false, min: None, max: None, options: &[], level: "essential", default: r#"[]"#, reason: "se activan por guild desde /api/v1/guilds" },
    SchemaField { key: "guilds.guild_max_concurrent_calls", section: "connectors", kind: "integer", editable: true, requires_restart: true, sensitive: false, min: Some(1.0), max: Some(32.0), options: &[], level: "advanced", default: r#"3"#, reason: "" },
    SchemaField { key: "external_mcp", section: "connectors", kind: "array", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"[]"#, reason: "B1: [[external_mcp]] solo en tylluan.toml" },
    SchemaField { key: "services", section: "connectors", kind: "table", editable: false, requires_restart: true, sensitive: true, min: None, max: None, options: &[], level: "advanced", default: r#"{}"#, reason: "B1: [services] solo en tylluan.toml" },
    // network
    SchemaField { key: "federation.auto_sync_mode", section: "network", kind: "string", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#""push""#, reason: "define que se comparte con otros equipos: solo el operador" },
    SchemaField { key: "p2p.enabled", section: "network", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "abre red hacia fuera: solo el operador" },
    SchemaField { key: "mesh.bootstrap_enabled", section: "network", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"true"#, reason: "sensible: mezcla malla LAN/DHT; se decide en tylluan.toml" },
    SchemaField { key: "mdns.advertise", section: "network", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: anunciar el nodo en la LAN" },
    SchemaField { key: "mdns.discover", section: "network", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "privacidad: descubrir otros nodos en la LAN" },
    SchemaField { key: "tunnel.enabled", section: "network", kind: "boolean", editable: false, requires_restart: false, sensitive: true, min: None, max: None, options: &[], level: "essential", default: r#"false"#, reason: "abre red hacia fuera: solo el operador" },
];

/// D227 (T4771): campos cuyo default NO es una constante del esquema.
/// `inference.device` lo elige la plataforma (`auto_select_device`) y
/// `security.encrypt_at_rest` depende de la feature `encryption`; el test los
/// valida contra el default REAL del binario en vez de contra un literal fijo.
pub const DEFAULT_DINAMICO: &[&str] = &["inference.device", "security.encrypt_at_rest"];

/// D227 (T4771): claves del esquema que NO resuelven contra `TylluanConfig`.
///
/// `dev_mode` se queda como clave plana por **exclusion explicita del TL** (T4771:
/// "no tocar auth ni dev_mode"), aunque la clave real del TOML sea `nexus.dev_mode`.
/// Se declara aqui para que el test de cruce no la tape en silencio: si algun dia se
/// renombra, hay que sacarla de esta lista y el test lo pedira.
pub const CLAVES_HEREDADAS: &[&str] = &["dev_mode"];

/// D227 v2: los grupos que el asistente pinta, con las claves que los respaldan. Un grupo sin
/// ningun campo `essential` dejaria la pantalla coja aunque el recuento total cuadre.
pub const GRUPOS_ASISTENTE: &[(&str, &[&str])] = &[
    ("datos", &["memory.db_path", "silva.db_path"]),
    ("clave", &["dev_mode", "nexus.host", "nexus.port"]),
    ("memoria", &["memory.embedding_model", "memory.vector_dimensions", "silva.decay_enabled"]),
    ("herramientas", &["security.external_acl.enabled", "guilds.core.always_on"]),
    ("privacidad", &["inference.points.chat.cloud_enabled", "nexus.allow_hf_download"]),
    ("red", &["p2p.enabled", "tunnel.enabled", "mesh.bootstrap_enabled", "mdns.advertise"]),
];

/// D227 v2 (T4775): campos editables que **ya lo eran en main** (schema v1). Todo lo que
/// sea editable y no esté aquí es afinado nuevo, y solo se admite el de rendimiento.
pub const EDITABLES_HEREDADOS: &[&str] = &[
    "low_memory_mode",
    "memory.embedding_model",
    "inference.device",
    "inference.max_threads",
    "silva.reranker_enabled",
    "memory.embedding_idle_ttl_secs",
    "memory.background_concurrency",
    "memory.background_max_wait_secs",
    "memory.reindex_node_delay_ms",
    "memory.reindex_nodes_per_tick",
    "memory.reindex_tick_secs",
    "logging.max_file_size_mb",
    "logging.max_files",
];

/// D227 v2: los **únicos** editables nuevos permitidos — afinado de rendimiento del chat del
/// usuario. Si alguien añade otro (privacidad, red, seguridad), el test lo dice.
pub const EDITABLES_NUEVOS_PERMITIDOS: &[&str] = &[
    "inference.llama.ctx_size",
    "inference.llama.n_gpu_layers",
    "inference.llama.threads",
    "inference.llama.temperature",
    "guilds.guild_max_concurrent_calls",
];

/// D227 v2: privacidad y red que la API **no** puede tocar (orden del TL en T4775).
pub const FUERA_DE_LA_API: &[&str] = &[
    "inference.points.chat.cloud_enabled",
    "inference.points.decision.cloud_enabled",
    "inference.points.grammar_judge.cloud_enabled",
    "inference.points.sentinel.cloud_enabled",
    "inference.points.chat.api_key_env",
    "nexus.allow_hf_download",
    "tunnel.enabled",
    "p2p.enabled",
    "federation.auto_sync_mode",
    "inference.primary_model",
];

pub fn schema_field(key: &str) -> Option<&'static SchemaField> {
    SCHEMA.iter().find(|f| f.key == key)
}

pub fn schema_json() -> serde_json::Value {
    let fields: Vec<serde_json::Value> = SCHEMA
        .iter()
        .map(|f| {
            // D227: `default` viaja como JSON ya tipado (no como string), para que
            // el dashboard lo pueda pintar sin adivinar el tipo del campo.
            let default = serde_json::from_str::<serde_json::Value>(f.default)
                .unwrap_or_else(|_| json!(f.default));
            let mut obj = json!({
                "key": f.key,
                "section": f.section,
                "type": f.kind,
                "level": f.level,
                "editable_via_api": f.editable,
                "requires_restart": f.requires_restart,
                "sensitive": f.sensitive,
                "default": default,
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
        "schema_version": 2,
        "source": "tylluan.toml",
        "fields": fields,
        "notes": [
            "editable_via_api=false exige edición manual del TOML (B1/operador).",
            "sensitive=true: A0 solo lectura; A3 añadirá acciones con código de operador.",
            "requires_restart=true: el cambio no surte efecto hasta reiniciar el kernel.",
            "level=essential|advanced|internal: clasificacion del operador (D227/T4771).",
            "default: valor de fabrica, contrastado por test contra TylluanConfig::default().",
            "sensitive=true: NUNCA editable por la API; solo lectura y accion de operador."
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
    // D227 (T4771): un campo sensible no se escribe por API, aunque alguien lo
    // marque `editable: true` por error en SCHEMA. Es el veto que pide el
    // contrato, y va ANTES del chequeo de `editable` para que una mutacion de
    // `editable` no abra la puerta.
    if field.sensitive {
        return Err(format!(
            "campo sensible: {} no se edita por la API ({})",
            edit.key, field.reason
        ));
    }
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
        "float" => {
            let n = match toml_value {
                toml::Value::Float(f) => f,
                toml::Value::Integer(i) => i as f64,
                _ => return Err(format!("{} debe ser numero", edit.key)),
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

/// D5022: claves que cambian **entre el `tylluan.toml` que había en disco y el texto que se va a
/// escribir**, calculadas sobre el TOML crudo, no sobre el config ya parseado a `TylluanConfig`.
///
/// `config_diff` compara el config **en memoria** contra el contenido **parseado a `TylluanConfig`**,
/// así que todo lo que `TylluanConfig` no modela es invisible: secciones desconocidas, erratas y
/// claves escritas en la tabla equivocada. Medido 2026-11-25: un `POST /api/v1/config` con
/// `embedding_model` dentro de `[nexus]` (ese campo vive en `[memory]`, `config.rs:851`) escribía el
/// fichero y dejaba en `config_audit.jsonl` `"keys": []` y `"diff": []`, o sea la auditoría decía
/// "no cambió nada" mientras el fichero acababa de nacer.
///
/// Devuelve **solo nombres de clave**, nunca valores: por esta vía pasan claves que el kernel no
/// modela y que pueden contener secretos, y la lista va al registro de auditoría y a la respuesta.
/// `Err` cuando algún lado no parsea como TOML (un fichero previo corrupto también es un hecho que
/// la auditoría debe poder nombrar). `old_raw = None` significa que no había fichero: entonces todas
/// las claves del contenido nuevo son cambios.
pub fn file_change_keys(old_raw: Option<&str>, new_raw: &str) -> Result<Vec<String>, String> {
    let old: toml::Value = match old_raw {
        Some(raw) => toml::from_str(raw).map_err(|e| format!("el tylluan.toml anterior no parsea: {e}"))?,
        None => toml::Value::Table(toml::map::Map::new()),
    };
    let new: toml::Value =
        toml::from_str(new_raw).map_err(|e| format!("el contenido nuevo no parsea: {e}"))?;

    let mut a = std::collections::BTreeMap::new();
    let mut b = std::collections::BTreeMap::new();
    flatten(&old, "", &mut a);
    flatten(&new, "", &mut b);

    let mut union: Vec<&String> = a.keys().chain(b.keys()).collect();
    union.sort();
    union.dedup();
    let mut changed = Vec::new();
    for key in union {
        if a.get(key) != b.get(key) {
            changed.push(key.clone());
        }
    }
    Ok(changed)
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

/// Ruta del `tylluan.toml` vivo (la misma que usa `/config/edit`).
pub fn config_path() -> PathBuf {
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

/// Cuantas copias `<config>.bak-<millis>` se conservan en el directorio de la config (caso 5 de
/// la politica de backups, 21-nov): las 3 mas recientes; la siguiente edicion poda el resto.
pub const KEEP_CONFIG_BACKUPS: usize = 3;

/// Marca numerica de una copia de `file_name` si el nombre encaja EXACTAMENTE con
/// `<file_name>.bak-<digitos>`. Cualquier otro nombre devuelve `None` y no se toca:
/// `tylluan.toml.bak` (sin marca), `tylluan.toml.bak-abc`, `tylluan.toml.bak-123.tmp`,
/// `tylluan.toml`, `providers.json.bak`, `otro.toml.bak-1`.
fn backup_mark(name: &str, file_name: &str) -> Option<u128> {
    let rest = name.strip_prefix(file_name)?.strip_prefix(".bak-")?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse::<u128>().ok()
}

/// Conserva las `keep` copias mas recientes de `config` en SU MISMO directorio y borra el resto.
/// Solo mira entradas de ese directorio (sin recursion), solo ficheros regulares y solo nombres
/// con el patron exacto: no borra la config viva, ni `.json.bak`, ni nada con otro nombre, ni
/// sigue enlaces. La antiguedad sale de la marca del nombre (millis de [`backup_path_for`]), no
/// del mtime, para que el orden sea estable aunque dos copias caigan en el mismo segundo.
/// Devuelve cuantas borro. Best-effort: un fallo no interrumpe la edicion.
pub fn prune_backups(config: &Path, keep: usize) -> usize {
    let (Some(dir), Some(file_name)) = (config.parent(), config.file_name().and_then(|n| n.to_str()))
    else {
        return 0;
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    let mut copies: Vec<(u128, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(mark) = backup_mark(name, file_name) else {
            continue;
        };
        // Solo ficheros regulares: un directorio con ese nombre exacto no se toca.
        match entry.file_type() {
            Ok(t) if t.is_file() => copies.push((mark, entry.path())),
            _ => continue,
        }
    }
    if copies.len() <= keep {
        return 0;
    }
    copies.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut removed = 0;
    for (mark, path) in copies.into_iter().skip(keep) {
        match std::fs::remove_file(&path) {
            Ok(()) => removed += 1,
            Err(e) => {
                tracing::warn!(backup = %path.display(), mark, "no se pudo podar la copia de config: {e}")
            }
        }
    }
    removed
}

/// Copia previa de `path` + poda de las copias viejas: es el unico punto por el que deben pasar
/// las tres vias que editan la config (`/config/edit`, persistencia interna y POST de
/// compatibilidad), para que la poda no se pueda olvidar en una de ellas.
pub fn backup_and_prune(path: &Path) -> Result<PathBuf, String> {
    let backup = backup_path_for(path)?;
    std::fs::copy(path, &backup).map_err(|e| format!("no se pudo hacer copia previa: {e}"))?;
    cerrar_acl_de_copia(&backup);
    prune_backups(path, KEEP_CONFIG_BACKUPS);
    Ok(backup)
}

/// D5032d (H-HER-AI-01, P2, hallazgo de @hermes T5335): la copia previa nacía con la **ACL
/// heredada del directorio**. Medido en la máquina del operador: `tylluan.toml` lleva
/// `BUILTIN\Usuarios:(I)(RX)` y `NT AUTHORITY\Usuarios autentificados:(I)(M)`, así que las hasta
/// 3 copias `tylluan.toml.bak-<millis>` que deja `backup_and_prune` quedaban legibles (y la del
/// `Usuarios autentificados`, escribibles) para cualquier usuario local. Desde D5032 esas copias
/// pueden contener `[security.acl.token_agent_bindings]` y los hashes de los tokens, y la vía ya
/// existía antes de D5032 (`api_admin.rs:967`, `config_safe.rs:1816` y `:1830`).
///
/// El remedio es el que ya usa el resto de secretos: [`crate::config::harden_new_secret`] corta la
/// herencia en Windows (`icacls /inheritance:r /grant:r <cuenta>:F SYSTEM:F`) y se auto-verifica
/// con `verificar_dacl_cerrada`; **no-op fuera de Windows**, por eso el modo 0600 explícito de
/// Unix (mismo valor que `PRIVATE_CONFIG_MODE`, y `std::fs::copy` solo conserva el modo del padre,
/// que no siempre es 0600). Como `harden_new_secret`, nunca convierte un 200 en un 500: si el
/// cierre falla, avisa y la copia queda como estaba.
fn cerrar_acl_de_copia(copia: &Path) {
    crate::config::harden_new_secret(copia);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(copia, std::fs::Permissions::from_mode(PRIVATE_CONFIG_MODE))
        {
            tracing::warn!(
                "no se pudo poner {:#o} en la copia {}: {e}",
                PRIVATE_CONFIG_MODE,
                copia.display()
            );
        }
    }
    #[cfg(not(any(unix, windows)))]
    let _ = copia;
}

/// D5018: copia previa **solo si hay fichero que copiar**.
///
/// `save_config_handler` (el POST de compatibilidad de `/api/v1/config`) llamaba siempre a
/// `backup_and_prune`, que falla cuando el fichero no existe. En un kernel arrancado **sin config
/// descubierta** (medido 2026-11-25, kernel propio en puerto propio) eso devolvía
/// `500 {"error":"no se pudo hacer copia previa: … (os error 2)"}` sin escribir nada y sin decir
/// que lo que faltaba era el propio `tylluan.toml`. Este POST no edita: recibe el TOML completo,
/// así que **crear** el config es un caso válido, y el fichero creado es el que el kernel
/// descubrirá al reiniciar (carril `<cwd>/tylluan.toml` de `find_config_file`, `config.rs:3284`).
///
/// Devuelve `None` cuando no había nada que respaldar; un fallo real al copiar un fichero que sí
/// existe sigue siendo `Err`.
pub fn backup_and_prune_if_exists(path: &Path) -> Result<Option<PathBuf>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    backup_and_prune(path).map(Some)
}

/// D5018 (a): modo que debe quedar en un `tylluan.toml` **recien creado** en Unix. `harden_new_secret`
/// endurece la ACL de Windows y es no-op fuera, asi que sin esto el fichero nace con el umask del
/// proceso (tipico 0644) y por aqui pueden escribirse `token_agent_bindings`.
pub const PRIVATE_CONFIG_MODE: u32 = 0o600;

/// D5018 (a): aplica [`PRIVATE_CONFIG_MODE`] a un fichero recien escrito. Se llama **antes** del
/// rename, asi no hay ninguna ventana en la que el `tylluan.toml` definitivo este abierto y el
/// rename conserva el modo. Best-effort: si falla avisa y devuelve `false`; no se descarta un
/// cambio que ya esta en disco. En Windows devuelve `false` a proposito: alli endurece
/// `harden_new_secret` (ACL), no el modo POSIX.
pub fn set_private_mode(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::set_permissions(path, std::fs::Permissions::from_mode(PRIVATE_CONFIG_MODE)) {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!("no se pudo poner 0600 en {}: {e}", path.display());
                false
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

/// Forma del asiento de auditoría, separada de su escritura en disco para poder **afirmar**
/// contenido en los tests: `backup` debe ser `null` (no `""`, no una ruta inventada) cuando el
/// `tylluan.toml` se acababa de crear, y `file_keys` debe estar aunque `keys` esté vacío.
pub fn audit_record(
    diff: &[serde_json::Value],
    backup: Option<&Path>,
    file_keys: &[String],
    source: &str,
    actor: &str,
) -> serde_json::Value {
    let keys: Vec<&str> = diff
        .iter()
        .filter_map(|d| d.get("key").and_then(|k| k.as_str()))
        .collect();
    json!({
        "ts": chrono::Utc::now().timestamp_millis(),
        "actor": actor,
        "source": source,
        "keys": keys,
        "diff": diff,
        "file_keys": file_keys,
        "backup": backup.map(|p| p.display().to_string()),
    })
}

pub fn audit_change(diff: &[serde_json::Value], backup: &Path, source: &str, actor: &str) {
    audit_change_with_backup(diff, Some(backup), &[], source, actor);
}

/// D5018: la misma auditoría con copia previa **opcional**. Cuando el `tylluan.toml` se acaba de
/// crear no existe copia que registrar: el asiento deja `"backup": null` en vez de una ruta que no
/// existe.
///
/// `file_keys` (D5022) son las claves literales que cambiaron en el fichero, calculadas con
/// `file_change_keys`: muestran lo que `config_diff` no puede ver. Vacío en las vías que ya difieran
/// clave por clave (`/config/edit`), donde el diff semántico es completo.
pub fn audit_change_with_backup(
    diff: &[serde_json::Value],
    backup: Option<&Path>,
    file_keys: &[String],
    source: &str,
    actor: &str,
) {
    let keys: Vec<&str> = diff
        .iter()
        .filter_map(|d| d.get("key").and_then(|k| k.as_str()))
        .collect();
    let record = audit_record(diff, backup, file_keys, source, actor);
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
    match backup {
        Some(p) => tracing::info!(keys = ?keys, backup = %p.display(), source, "config editada"),
        None => tracing::info!(
            keys = ?keys,
            backup = "ninguna (config creada)",
            source,
            "config editada"
        ),
    }
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
    let backup = match backup_and_prune(&path) {
        Ok(p) => p,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false, "errors": [e]}))).into_response(),
    };
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

// ── D-157: proveedores externos por la misma vía validada que /config/edit ──
//
// José/TL (T3700): los campos NO secretos (name, base_url, models) persisten en
// la configuración, no en el SecretStore, y lo hacen con ETag obligatorio,
// copia previa, auditoría y escritura atómica — exactamente lo que ya hace
// `/config/edit`. Estas funciones son esa vía para el array
// `[[external_providers]]`, que no es editable clave a clave porque no es
// escalar.

const PROVIDERS_KEY: &str = "external_providers";

type ApiError = (StatusCode, serde_json::Value);

fn api_error(status: StatusCode, code: &str, message: String) -> ApiError {
    (
        status,
        json!({"ok": false, "error": code, "errors": [message]}),
    )
}

fn parse_toml(raw: &str) -> Result<toml::Value, ApiError> {
    toml::from_str::<toml::Value>(raw).map_err(|e| {
        api_error(
            StatusCode::BAD_REQUEST,
            "config_unparsable",
            format!("tylluan.toml no se pudo parsear: {e}"),
        )
    })
}

fn serialize_toml(value: &toml::Value) -> Result<String, ApiError> {
    toml::to_string(value).map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "serialize_failed",
            format!("no se pudo serializar la configuración: {e}"),
        )
    })
}

fn provider_name_of(entry: &toml::Value) -> Option<&str> {
    entry.get("name").and_then(|n| n.as_str())
}

/// Añade o reemplaza (por `name`) un proveedor externo y devuelve el TOML
/// resultante más el diff auditado. **La clave no viaja aquí**: sólo
/// `name`, `type`, `base_url`, `api_key_env` y `models`.
///
/// ETag ausente → 400; ETag que no coincide → 409 (nada de «última gana»).
pub fn upsert_external_provider_toml(
    raw: &str,
    etag: Option<&str>,
    provider: &crate::config::ExternalProvider,
) -> Result<(String, Vec<serde_json::Value>), ApiError> {
    let current = etag_of(raw);
    etag_guard(etag, &current)?;
    let old = parse_toml(raw)?;
    let mut new = old.clone();
    let table = toml::Value::try_from(provider).map_err(|e| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "serialize_failed",
            format!("no se pudo serializar el proveedor: {e}"),
        )
    })?;
    {
        let root = new.as_table_mut().ok_or_else(|| {
            api_error(StatusCode::BAD_REQUEST, "config_unparsable", "tylluan.toml no es una tabla".into())
        })?;
        if !root.contains_key(PROVIDERS_KEY) {
            root.insert(PROVIDERS_KEY.to_string(), toml::Value::Array(Vec::new()));
        }
        let list = root
            .get_mut(PROVIDERS_KEY)
            .and_then(|v| v.as_array_mut())
            .ok_or_else(|| {
                api_error(
                    StatusCode::BAD_REQUEST,
                    "config_unparsable",
                    format!("{PROVIDERS_KEY} existe pero no es una lista"),
                )
            })?;
        match list
            .iter()
            .position(|entry| provider_name_of(entry) == Some(provider.name.as_str()))
        {
            Some(idx) => list[idx] = table,
            None => list.push(table),
        }
    }
    let serialized = serialize_toml(&new)?;
    let diff = config_diff(&old, &new);
    Ok((serialized, diff))
}

/// Quita el proveedor `name` de `[[external_providers]]`. Si no existe: 404.
pub fn remove_external_provider_toml(
    raw: &str,
    etag: Option<&str>,
    name: &str,
) -> Result<(String, Vec<serde_json::Value>), ApiError> {
    let current = etag_of(raw);
    etag_guard(etag, &current)?;
    let old = parse_toml(raw)?;
    let mut new = old.clone();
    let removed = {
        let root = new.as_table_mut().ok_or_else(|| {
            api_error(StatusCode::BAD_REQUEST, "config_unparsable", "tylluan.toml no es una tabla".into())
        })?;
        let Some(list) = root.get_mut(PROVIDERS_KEY).and_then(|v| v.as_array_mut()) else {
            return Err(api_error(
                StatusCode::NOT_FOUND,
                "provider_not_found",
                format!("no hay {PROVIDERS_KEY} en la configuración"),
            ));
        };
        match list
            .iter()
            .position(|entry| provider_name_of(entry) == Some(name))
        {
            Some(idx) => {
                list.remove(idx);
                true
            }
            None => false,
        }
    };
    if !removed {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "provider_not_found",
            format!("el proveedor '{name}' no existe"),
        ));
    }
    let serialized = serialize_toml(&new)?;
    let diff = config_diff(&old, &new);
    Ok((serialized, diff))
}

/// Escribe el TOML nuevo con copia previa, `rename` atómico y auditoría.
/// Devuelve la ruta de la copia de seguridad.
pub fn persist_config_raw(
    path: &Path,
    raw: &str,
    diff: &[serde_json::Value],
    source: &str,
    actor: &str,
) -> Result<PathBuf, ApiError> {
    let backup = backup_and_prune(path)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, "backup_failed", e))?;
    let tmp = path.with_extension("toml.tmp");
    if let Err(e) = std::fs::write(&tmp, raw) {
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "write_failed",
            format!("no se pudo escribir temporal: {e}"),
        ));
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        return Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "write_failed",
            format!("no se pudo reemplazar tylluan.toml: {e}"),
        ));
    }
    audit_change(diff, &backup, source, actor);
    Ok(backup)
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

    // ── D-157: alta/baja de proveedores externos por la vía validada ────────

    fn d157_provider(name: &str, url: &str) -> crate::config::ExternalProvider {
        crate::config::ExternalProvider {
            name: name.into(),
            provider_type: crate::config::ExternalProviderType::OpenAICompatible,
            base_url: url.into(),
            api_key_env: String::new(),
            models: vec!["modelo-a".into()],
        }
    }

    fn d157_raw_with_two_providers() -> String {
        r#"
[security.external_acl]
enabled = false

[[external_providers]]
name = "ollama"
type = "ollama_compatible"
base_url = "http://127.0.0.1:11434"
models = []

[[external_providers]]
name = "openai"
type = "openai_compatible"
base_url = "https://api.openai.com"
api_key_env = "OPENAI_API_KEY"
models = ["gpt-4o-mini"]
"#
        .to_string()
    }

    #[test]
    fn d157_upsert_requires_etag_and_adds_without_the_secret() {
        let raw = sample_raw();
        let provider = d157_provider("local-ui", "https://api.example.com/v1");

        let (status, body) = upsert_external_provider_toml(&raw, None, &provider).unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "etag_required");

        let etag = etag_of(&raw);
        let (new_raw, diff) = upsert_external_provider_toml(&raw, Some(&etag), &provider).unwrap();
        assert!(!diff.is_empty(), "el cambio debe quedar auditado");
        let parsed: toml::Value = toml::from_str(&new_raw).unwrap();
        let list = parsed
            .get(PROVIDERS_KEY)
            .and_then(|v| v.as_array())
            .expect("external_providers creado");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get("name").and_then(|v| v.as_str()), Some("local-ui"));
        assert_eq!(
            list[0].get("base_url").and_then(|v| v.as_str()),
            Some("https://api.example.com/v1")
        );
        // Lo importante: la clave no viaja a la configuración por esta vía.
        assert!(!new_raw.contains("api_key ="), "la config no lleva la clave");
        // Y no se pisa el resto del fichero.
        assert!(new_raw.contains("[security.external_acl]"));
        assert!(new_raw.contains("[[external_mcp]]"));
    }

    #[test]
    fn d157_upsert_replaces_by_name_and_keeps_the_others() {
        let raw = d157_raw_with_two_providers();
        let etag = etag_of(&raw);
        let updated = d157_provider("ollama", "http://127.0.0.1:11435");
        let (new_raw, _) = upsert_external_provider_toml(&raw, Some(&etag), &updated).unwrap();
        let parsed: toml::Value = toml::from_str(&new_raw).unwrap();
        let list = parsed.get(PROVIDERS_KEY).and_then(|v| v.as_array()).unwrap();
        assert_eq!(list.len(), 2, "no debe duplicar el proveedor existente");
        let ollama = list
            .iter()
            .find(|e| e.get("name").and_then(|v| v.as_str()) == Some("ollama"))
            .expect("ollama sigue ahí");
        assert_eq!(
            ollama.get("base_url").and_then(|v| v.as_str()),
            Some("http://127.0.0.1:11435")
        );
        let openai = list
            .iter()
            .find(|e| e.get("name").and_then(|v| v.as_str()) == Some("openai"))
            .expect("openai no se toca");
        assert_eq!(
            openai.get("api_key_env").and_then(|v| v.as_str()),
            Some("OPENAI_API_KEY"),
            "las entradas por variable de entorno siguen intactas"
        );
    }

    #[test]
    fn d157_remove_deletes_only_the_named_provider() {
        let raw = d157_raw_with_two_providers();
        let etag = etag_of(&raw);
        let (new_raw, diff) = remove_external_provider_toml(&raw, Some(&etag), "ollama").unwrap();
        assert!(!diff.is_empty());
        let parsed: toml::Value = toml::from_str(&new_raw).unwrap();
        let list = parsed.get(PROVIDERS_KEY).and_then(|v| v.as_array()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].get("name").and_then(|v| v.as_str()), Some("openai"));
    }

    #[test]
    fn d157_remove_unknown_provider_is_404() {
        let raw = d157_raw_with_two_providers();
        let etag = etag_of(&raw);
        let (status, body) = remove_external_provider_toml(&raw, Some(&etag), "no-existe").unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"], "provider_not_found");
    }

    #[test]
    fn d157_stale_etag_is_409_and_nothing_changes() {
        let raw = d157_raw_with_two_providers();
        let provider = d157_provider("nuevo", "https://api.example.com");
        let (status, body) = upsert_external_provider_toml(&raw, Some("0000000000000000"), &provider).unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"], "etag_mismatch");
        let (status, _) = remove_external_provider_toml(&raw, Some("0000000000000000"), "ollama").unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
    }

    /// D-104 criterio 2: una referencia a la bóveda (`vault:nombre`) y un valor
    /// en claro de `[[external_mcp]]` no pueden salir NUNCA por `GET /api/v1/config`.
    #[test]
    fn d104_vault_ref_and_plain_secret_never_reach_the_safe_config() {
        let raw = r#"
[[external_mcp]]
name = "github"
url = "https://example.invalid/sse"
headers = { Authorization = "vault:github", X-Plain = "ghp_EN_CLARO_D104" }
env = { API_TOKEN = "vault:github-env" }
"#;
        let config: crate::config::TylluanConfig = toml::from_str(raw).expect("config");
        let text = redacted_config_json(&config).to_string();

        assert!(
            !text.contains("ghp_EN_CLARO_D104"),
            "el valor en claro no puede aparecer: {text}"
        );
        // D-97 §3.2 decía que las referencias se muestran "tal cual" (`vault:github`).
        // La implementación real enmascara TODO valor. Se fija aquí lo que hace hoy
        // para que cualquier cambio de criterio sea deliberado y visible.
        assert!(
            !text.contains("vault:github"),
            "hoy la referencia también se enmascara: {text}"
        );
        assert!(text.contains("Authorization"), "las claves sí se ven: {text}");
        assert!(text.contains("API_TOKEN"), "las claves sí se ven: {text}");
    }

    /// La misma garantía en el endpoint de conectores (`GET /api/v1/mcp/external`).
    #[test]
    fn d104_redacted_map_masks_vault_refs_too() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("Authorization".to_string(), "vault:github".to_string());
        headers.insert("X-Plain".to_string(), "ghp_EN_CLARO_D104".to_string());

        let out = redacted_map(&Some(headers)).expect("mapa");
        assert_eq!(out["Authorization"], REDACTED);
        assert_eq!(out["X-Plain"], REDACTED);
        assert_eq!(out.len(), 2, "las claves se conservan");
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
    fn t_dsh_178_boot_only_tabla_ata_el_esquema_en_ambos_sentidos() {
        // Sentido 1: toda clave de la tabla existe en el esquema, es editable y
        // está marcada requires_restart=true. Quitar la marca en SCHEMA -> ROJO.
        for key in BOOT_ONLY_KEYS {
            let f = schema_field(key).unwrap_or_else(|| panic!("BOOT_ONLY_KEYS registra {key}, ausente del esquema"));
            assert!(f.requires_restart, "{key} debe marcarse requires_restart=true (H1)");
            assert!(f.editable, "{key} debe seguir editable via A0");
        }
        // Sentido 2: el conjunto de claves de la seccion maintenance marcadas
        // true en SCHEMA es EXACTAMENTE la tabla. Quitar una entrada de
        // BOOT_ONLY_KEYS (mutacion) o anadir una clave true sin registrarla
        // aqui -> ROJO. Es el lazo que pide el TL: la proteccion no puede
        // retirarse sin romper el test.
        let mut marcadas: Vec<&str> = SCHEMA
            .iter()
            .filter(|f| f.section == "maintenance" && f.requires_restart)
            .map(|f| f.key)
            .collect();
        marcadas.sort_unstable();
        let mut tabla: Vec<&str> = BOOT_ONLY_KEYS.to_vec();
        tabla.sort_unstable();
        assert_eq!(marcadas, tabla, "esquema(maintenance, requires_restart) != BOOT_ONLY_KEYS");
        // Y la clave caliente sigue caliente: si alguien la marca true sin
        // justification, este assert lo pilla.
        let delay = schema_field("memory.reindex_node_delay_ms").expect("delay_ms");
        assert!(!delay.requires_restart, "reindex_node_delay_ms se lee en cada pasada (api_memory.rs:134), es en caliente");
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
    /// D227 (T4771) — OBJETIVO DE MUTACION: marcar un campo sensible como
    /// editable en SCHEMA tiene que poner este test ROJO. Y no vale que el
    /// esquema salga vacio de sensibles: se exige un minimo real.
    #[test]
    fn d227_ningun_campo_sensible_es_editable_por_api() {
        let sensibles: Vec<&str> = SCHEMA.iter().filter(|f| f.sensitive).map(|f| f.key).collect();
        assert!(
            sensibles.len() >= 8,
            "se esperaban al menos 8 campos sensibles, hay {}: {:?}",
            sensibles.len(),
            sensibles
        );
        let editables: Vec<&str> = SCHEMA
            .iter()
            .filter(|f| f.sensitive && f.editable)
            .map(|f| f.key)
            .collect();
        assert!(
            editables.is_empty(),
            "sensibles editables por API (viola T4771): {:?}",
            editables
        );
    }

    /// El veto no depende solo de la bandera: `validate_edit` rechaza el
    /// campo por `sensitive` aunque `editable` fuese true.
    #[test]
    fn d227_validate_edit_rechaza_campo_sensible() {
        let edit = ConfigEdit {
            key: "security.intent_filter".to_string(),
            value: json!(false),
        };
        let err = validate_edit(&edit).expect_err("un sensible no se escribe por API");
        assert!(err.contains("sensible"), "error poco claro: {err}");
        assert!(err.contains("security.intent_filter"), "error sin la clave: {err}");
    }

    /// Cruce real con config.rs: cada `default` del esquema tiene que ser
    /// EXACTAMENTE el de `TylluanConfig::default()`. Un default inventado
    /// rompe este test (es el fallo que quiero que no vuelva a pasar).
    #[test]
    fn d227_defaults_del_esquema_coinciden_con_el_config_real() {
        let real = serde_json::to_value(crate::config::TylluanConfig::default())
            .expect("TylluanConfig serializa");
        let mut comprobados = 0usize;
        for f in SCHEMA {
            if CLAVES_HEREDADAS.contains(&f.key) {
                comprobados += 1;
                continue;
            }
            let valor_real = valor_en_config(&real, f.key)
                .unwrap_or_else(|| panic!("{}: la clave no existe en TylluanConfig", f.key));
            if DEFAULT_DINAMICO.contains(&f.key) {
                match f.key {
                    "inference.device" => {
                        let s = valor_real.as_str().unwrap_or_default();
                        assert!(
                            f.options.contains(&s),
                            "inference.device={s} no esta entre las opciones {:?}",
                            f.options
                        );
                    }
                    "security.encrypt_at_rest" => {
                        assert_eq!(
                            valor_real.as_bool(),
                            Some(cfg!(feature = "encryption")),
                            "security.encrypt_at_rest debe seguir a la feature encryption"
                        );
                    }
                    otro => panic!("DEFAULT_DINAMICO declara {otro} sin rama de validacion"),
                }
                comprobados += 1;
                continue;
            }
            let declarado: serde_json::Value = serde_json::from_str(f.default)
                .unwrap_or_else(|e| panic!("{}: default no es JSON ({}): {}", f.key, f.default, e));
            // Los `f32` de config.rs no viajan exactos a JSON (0.7f32 ->
            // 0.699999988079071): en floats se exige igualdad con tolerancia,
            // no el artefacto de precision escrito a mano en el esquema.
            if f.kind == "float" {
                let esperado = declarado.as_f64().expect("default float");
                let real_f = valor_real.as_f64().expect("campo float en config.rs");
                assert!(
                    (esperado - real_f).abs() < 1e-5,
                    "{}: default del esquema {esperado} != config.rs {real_f}",
                    f.key
                );
                comprobados += 1;
                continue;
            }
            assert_eq!(
                declarado, valor_real,
                "{}: el default del esquema no es el de config.rs",
                f.key
            );
            comprobados += 1;
        }
        assert_eq!(comprobados, SCHEMA.len(), "el test tiene que cubrir todos los campos");
        assert!(comprobados >= 40, "se esperaban ~40-50 campos, hay {comprobados}");
    }

    /// Ninguna clave del esquema puede ser un fantasma (el caso `wake.command`,
    /// que el esquema declaraba y config.rs no tiene).
    #[test]
    fn d227_toda_clave_del_esquema_existe_en_el_config_real() {
        let real = serde_json::to_value(crate::config::TylluanConfig::default())
            .expect("TylluanConfig serializa");
        let fantasmas: Vec<&str> = SCHEMA
            .iter()
            .filter(|f| !CLAVES_HEREDADAS.contains(&f.key))
            .filter(|f| valor_en_config(&real, f.key).is_none())
            .map(|f| f.key)
            .collect();
        assert!(fantasmas.is_empty(), "claves que config.rs no tiene: {:?}", fantasmas);
    }

    /// Clasificacion del operador: niveles conocidos y suficientes esenciales.
    #[test]
    fn d227_niveles_conocidos_y_esenciales_suficientes() {
        let validos = ["essential", "advanced", "internal"];
        let malos: Vec<&str> = SCHEMA
            .iter()
            .filter(|f| !validos.contains(&f.level))
            .map(|f| f.key)
            .collect();
        assert!(malos.is_empty(), "nivel desconocido en: {:?}", malos);
        // D227 v2: al sacar `inference.primary_model` de esenciales (directiva de Jose) el
        // suelo baja a 28. El recuento solo no basta: se exige que **cada grupo del asistente**
        // tenga al menos un campo esencial, que es lo que de verdad necesita la pantalla.
        let esenciales = SCHEMA.iter().filter(|f| f.level == "essential").count();
        assert!(
            esenciales >= 28,
            "se esperaban >=28 esenciales para el asistente, hay {esenciales}"
        );
        for (grupo, claves) in GRUPOS_ASISTENTE {
            let cubierto = claves.iter().any(|k| {
                SCHEMA
                    .iter()
                    .any(|f| f.key == *k && f.level == "essential")
            });
            assert!(cubierto, "el grupo '{grupo}' no tiene ningun campo esencial");
        }
    }

    /// Tamano y unicidad: 40-52 campos (T4771 pide ~40-50) y sin repetidas.
    #[test]
    fn d227_esquema_en_rango_y_sin_claves_repetidas() {
        assert!(
            (40..=52).contains(&SCHEMA.len()),
            "SCHEMA tiene {} campos; el contrato pide ~40-50",
            SCHEMA.len()
        );
        let mut vistas = std::collections::HashSet::new();
        for f in SCHEMA {
            assert!(vistas.insert(f.key), "clave repetida en SCHEMA: {}", f.key);
        }
    }

    /// El JSON que consume el dashboard lleva level y default ya tipados.
    #[test]
    fn d227_schema_json_expone_level_y_default_tipados() {
        let doc = schema_json();
        assert_eq!(doc["schema_version"], 2);
        let campos = doc["fields"].as_array().expect("fields es array");
        assert_eq!(campos.len(), SCHEMA.len());
        let puerto = campos
            .iter()
            .find(|f| f["key"] == "nexus.port")
            .expect("nexus.port en el esquema");
        assert_eq!(puerto["level"], "essential");
        assert_eq!(puerto["default"], 47004);
        let decay = campos
            .iter()
            .find(|f| f["key"] == "silva.decay_enabled")
            .expect("silva.decay_enabled en el esquema");
        assert_eq!(decay["default"], true);
        // y ningun sensible sale editable en el documento
        let malos: Vec<&str> = campos
            .iter()
            .filter(|f| f["sensitive"] == true && f["editable_via_api"] == true)
            .filter_map(|f| f["key"].as_str())
            .collect();
        assert!(malos.is_empty(), "sensibles editables en el JSON: {:?}", malos);
    }

    /// Un campo float se valida por rango y se escribe como float.
    #[test]
    fn d227_validate_edit_acepta_float_y_respeta_rango() {
        let ok = validate_edit(&ConfigEdit {
            key: "inference.llama.temperature".to_string(),
            value: json!(0.3),
        });
        assert!(ok.is_ok(), "0.3 deberia aceptarse: {:?}", ok.err());
        let alto = validate_edit(&ConfigEdit {
            key: "inference.llama.temperature".to_string(),
            value: json!(9.0),
        });
        assert!(alto.is_err(), "9.0 deberia rechazarse por maximo");
        let texto = validate_edit(&ConfigEdit {
            key: "inference.llama.temperature".to_string(),
            value: json!("caliente"),
        });
        assert!(texto.is_err(), "un string no es un float");
    }

    /// Resuelve `a.b.c` sobre el JSON del config.
    fn valor_en_config(config: &serde_json::Value, key: &str) -> Option<serde_json::Value> {
        let mut actual = config;
        for parte in key.split('.') {
            actual = actual.get(parte)?;
        }
        Some(actual.clone())
    }
    /// La lista de heredadas no puede crecer sin justificarlo: hoy solo es
    /// `dev_mode`, por exclusion del TL. Si aparece otra, este test lo dice.
    #[test]
    fn d227_claves_heredadas_son_solo_las_declaradas() {
        assert_eq!(CLAVES_HEREDADAS, &["dev_mode"]);
        // y ninguna heredada puede estar ademas en DEFAULT_DINAMICO (doble excusa)
        for k in CLAVES_HEREDADAS {
            assert!(!DEFAULT_DINAMICO.contains(k), "{k} tiene dos excepciones a la vez");
            assert!(SCHEMA.iter().any(|f| f.key == *k), "{k} ya no esta en SCHEMA");
        }
    }
    /// D227 v2 (T4775) — OBJETIVO DE MUTACION: privacidad y red no se tocan por API.
    /// Marcar cualquiera de estos como editable pone el test ROJO.
    #[test]
    fn d227v2_privacidad_y_red_fuera_de_la_api() {
        for clave in FUERA_DE_LA_API {
            let f = schema_field(clave).unwrap_or_else(|| panic!("{clave} no esta en el esquema"));
            assert!(!f.editable, "{clave} NO puede ser editable por API (T4775)");
            assert!(f.sensitive, "{clave} tiene que declararse sensible (T4775)");
        }
        // y ninguna puede escribirse de verdad: el veto responde 400
        for clave in ["inference.points.chat.cloud_enabled", "p2p.enabled", "tunnel.enabled"] {
            let edit = ConfigEdit { key: (*clave).to_string(), value: json!(true) };
            let err = validate_edit(&edit).expect_err("un campo de FUERA_DE_LA_API no se escribe");
            assert!(err.contains("sensible"), "{clave}: error sin el veto de sensible -> {err}");
        }
    }

    /// D227 v2: el unico aflojamiento nuevo frente a main es afinado de rendimiento.
    #[test]
    fn d227v2_los_editables_nuevos_son_solo_afinado_de_rendimiento() {
        let mut nuevos: Vec<&str> = SCHEMA
            .iter()
            .filter(|f| f.editable && !EDITABLES_HEREDADOS.contains(&f.key))
            .map(|f| f.key)
            .collect();
        nuevos.sort_unstable();
        let mut permitidos = EDITABLES_NUEVOS_PERMITIDOS.to_vec();
        permitidos.sort_unstable();
        assert_eq!(nuevos, permitidos, "hay editables nuevos que no son afinado de rendimiento");
        // y no se ha perdido ningun heredado: los 13 siguen editables
        for clave in EDITABLES_HEREDADOS {
            let f = schema_field(clave).unwrap_or_else(|| panic!("{clave} desaparecio"));
            assert!(f.editable, "{clave} era editable en main y dejo de serlo");
        }
    }

    // ── Caso 5 de la politica de backups: solo quedan las 3 copias mas recientes ─────────
    // Estos tests entran por `backup_and_prune`, no por el helper suelto: si alguien quita la
    // llamada a la poda, el primero se pone rojo (mutacion comprobada a mano, ver informe).

    fn marca_de(nombre: &str) -> Option<u128> {
        nombre
            .rsplit_once(".bak-")
            .and_then(|(_, m)| m.parse::<u128>().ok())
    }

    fn copias_en(dir: &std::path::Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .filter(|n| n.starts_with("tylluan.toml.bak-"))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn backup_and_prune_deja_solo_las_tres_mas_recientes() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(&cfg, "[memory]\nvector_dimensions = 1024\n").unwrap();
        for mark in [1000u128, 2000, 3000, 4000, 5000] {
            std::fs::write(dir.path().join(format!("tylluan.toml.bak-{mark}")), b"vieja").unwrap();
        }

        let backup = backup_and_prune(&cfg).expect("copia previa");

        assert!(backup.exists(), "la copia nueva debe existir");
        let copias = copias_en(dir.path());
        assert_eq!(
            copias.len(),
            KEEP_CONFIG_BACKUPS,
            "de 5 copias + la nueva deben quedar 3, quedan {copias:?}"
        );
        let mut marcas: Vec<u128> = copias.iter().filter_map(|n| marca_de(n)).collect();
        marcas.sort_unstable();
        assert!(
            marcas.iter().all(|m| *m >= 3000),
            "se conservan las 3 mas recientes, no {marcas:?}"
        );
        assert!(cfg.exists(), "la config viva no se toca");
    }

    // ── D5018: POST /api/v1/config sobre un kernel sin config descubierta ──────────────
    //
    // Medido 2026-11-25: `save_config_handler` llamaba a `backup_and_prune` sin comprobar si el
    // fichero existia, y `std::fs::copy` devolvia `os error 2` → HTTP 500 `no se pudo hacer copia
    // previa: … (os error 2)` sin escribir nada y sin decir que faltaba el propio tylluan.toml.
    // El POST recibe el TOML completo, asi que CREAR el config es el caso valido.

    #[test]
    fn d5018_sin_fichero_no_hay_nada_que_respaldar_y_no_es_error() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        assert!(!cfg.exists());

        assert_eq!(
            backup_and_prune_if_exists(&cfg),
            Ok(None),
            "sin config no hay copia que hacer: None, no un 500"
        );
        assert!(!cfg.exists(), "preparar la escritura no debe crear el config por su cuenta");
        assert!(copias_en(dir.path()).is_empty(), "ni copias fantasma");
    }

    #[test]
    fn d5018_con_fichero_si_hace_copia() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(&cfg, "[memory]\nvector_dimensions = 1024\n").unwrap();

        let backup = backup_and_prune_if_exists(&cfg)
            .expect("sin error")
            .expect("con fichero si hay copia");

        assert!(backup.exists(), "la copia debe existir");
        assert_eq!(copias_en(dir.path()).len(), 1, "una sola copia");
        assert!(cfg.exists(), "la config viva no se toca");
    }

    // ── D5032d (H-HER-AI-01, P2, @hermes T5335): la copia previa no puede heredar una ACL abierta ──
    //
    // Medido 2026-11-27: `E:\tylluan\tylluan.toml` tiene `BUILTIN\Usuarios:(I)(RX)` y
    // `NT AUTHORITY\Usuarios autentificados:(I)(M)`; `std::fs::copy` nace con la ACL heredada del
    // directorio, asi que las copias `tylluan.toml.bak-<millis>` quedaban al alcance de cualquier
    // usuario local. Hoy no hay `.bak` en disco (0 ficheros), pero el primer `POST /api/v1/config`
    // o `/api/v1/acl/tokens` los crea.

    #[cfg(windows)]
    #[test]
    fn d5032d_copia_previa_nace_con_dacl_cerrada() {
        use crate::config::{sddl_is_too_open, sddl_of};

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(
            &cfg,
            "[security.acl]\ndefault_role = \"reader\"\n\n[security.acl.token_agent_bindings]\n\"sha256:abc\" = \"deep\"\n",
        )
        .unwrap();

        // Precondicion: un fichero normal en este mismo directorio SI esta abierto. Sin esto el
        // test no prueba nada (una carpeta ya cerrada daria verde sin el arreglo).
        let normal = dir.path().join("normal.txt");
        std::fs::write(&normal, b"abierto").unwrap();
        let sddl_normal = sddl_of(&normal).expect("Get-Acl funciona en esta maquina");
        assert!(
            sddl_is_too_open(&sddl_normal),
            "precondicion: el directorio de prueba deberia herir una DACL abierta, y no: {sddl_normal}"
        );

        let backup = backup_and_prune(&cfg).expect("copia previa");
        let sddl = sddl_of(&backup).expect("Get-Acl de la copia");
        assert!(
            !sddl_is_too_open(&sddl),
            "la copia de la config sigue abierta a grupos amplios: {sddl}"
        );

        // Y el endurecimiento no rompe lo que ya hacia `backup_and_prune`.
        assert!(backup.is_file(), "la copia sigue existiendo");
        assert!(cfg.is_file(), "la config viva no se toca");
        assert_eq!(
            std::fs::read_to_string(&backup).expect("la copia se lee"),
            std::fs::read_to_string(&cfg).expect("la config se lee"),
            "la copia conserva el contenido"
        );
    }

    #[cfg(unix)]
    #[test]
    fn d5032d_copia_previa_nace_con_modo_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(&cfg, "[security.acl]\ndefault_role = \"reader\"\n").unwrap();
        // El padre adrede en 0644: `std::fs::copy` conservaria ese modo si no se cierra la copia.
        std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o644)).unwrap();

        let backup = backup_and_prune(&cfg).expect("copia previa");
        let modo = std::fs::metadata(&backup).expect("metadata").permissions().mode();
        assert_eq!(
            modo & 0o777,
            PRIVATE_CONFIG_MODE,
            "la copia debe nacer en {:o}, no {:o}",
            PRIVATE_CONFIG_MODE,
            modo & 0o777
        );
    }

    #[test]
    fn d5018_la_variante_estricta_sigue_fallando_sin_fichero() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");

        let err = backup_and_prune(&cfg).expect_err("la variante estricta no se relaja");

        assert!(err.contains("copia previa"), "mensaje esperado, salio: {err}");
    }

    // ── D5022: la auditoría del POST no puede quedarse muda cuando el fichero sí cambió ──────
    //
    // `config_diff` compara el config EN MEMORIA contra el contenido parseado a `TylluanConfig`, asi
    // que las claves que el esquema no modela son invisibles. Medido 2026-11-25: un POST con
    // `embedding_model` escrito en `[nexus]` (ese campo vive en `[memory]`, config.rs:851) dejo en
    // `config_audit.jsonl` `"keys": []` y `"diff": []` mientras el fichero acababa de crearse.

    #[test]
    fn d5022_sin_fichero_previo_toda_clave_cuenta_aunque_el_esquema_no_la_conozca() {
        let nuevo = "[nexus]\nhost = \"127.0.0.1\"\nport = 47228\nembedding_model = \"none\"\n";

        let claves = file_change_keys(None, nuevo).expect("sin fichero previo: todo es nuevo");

        assert!(
            claves.contains(&"nexus.embedding_model".to_string()),
            "la clave fuera del esquema debe aparecer, salio: {claves:?}"
        );
        assert!(claves.contains(&"nexus.port".to_string()), "salio: {claves:?}");
    }

    #[test]
    fn d5022_una_clave_movida_de_tabla_deja_de_ser_invisible() {
        let viejo = "[memory]\nembedding_model = \"none\"\n";
        let nuevo = "[nexus]\nembedding_model = \"none\"\n";

        let claves = file_change_keys(Some(viejo), nuevo).expect("los dos parsean");

        assert!(claves.contains(&"memory.embedding_model".to_string()), "salio: {claves:?}");
        assert!(claves.contains(&"nexus.embedding_model".to_string()), "salio: {claves:?}");
    }

    #[test]
    fn d5022_la_lista_de_claves_nunca_lleva_valores() {
        let nuevo = "[security]\ntoken_secreto = \"SUPERSECRETO_9f3a\"\n";

        let claves = file_change_keys(None, nuevo).expect("sin fichero previo");
        let junta = claves.join(",");

        assert!(claves.contains(&"security.token_secreto".to_string()), "salio: {claves:?}");
        assert!(
            !junta.contains("SUPERSECRETO_9f3a"),
            "el valor sensible se colo en la lista: {junta}"
        );
    }

    #[test]
    fn d5022_fichero_previo_corrupto_se_nombra_en_lugar_de_fingir_que_no_cambio_nada() {
        let err = file_change_keys(Some("esto [ no es toml"), "[nexus]\nport = 1\n")
            .expect_err("el TOML previo corrupto debe ser Err");

        assert!(err.contains("anterior no parsea"), "mensaje inesperado: {err}");
    }

    #[test]
    fn d5022_contenido_identico_no_inventa_cambios() {
        let crudo = "[nexus]\nport = 47004\n";

        assert!(file_change_keys(Some(crudo), crudo).expect("identico").is_empty());
    }

    #[test]
    fn backup_and_prune_no_toca_otros_nombres() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(&cfg, "a = 1\n").unwrap();
        for mark in [1u128, 2, 3, 4, 5] {
            std::fs::write(dir.path().join(format!("tylluan.toml.bak-{mark}")), b"x").unwrap();
        }
        let otros = [
            "tylluan.toml.bak",             // sin marca
            "tylluan.toml.bak-abc",         // marca no numerica
            "tylluan.toml.bak-6.tmp",       // otro sufijo
            "tylluan.toml.old",             // otro nombre
            "providers.json.bak",           // patron de mod.rs, no este
            "otro.toml.bak-1",              // otra config
        ];
        for nombre in otros {
            std::fs::write(dir.path().join(nombre), b"no tocar").unwrap();
        }

        backup_and_prune(&cfg).expect("copia previa");

        for nombre in otros {
            assert!(
                dir.path().join(nombre).exists(),
                "{nombre} no debia borrarse: solo se poda el patron exacto"
            );
        }
        assert!(!dir.path().join("tylluan.toml.bak-1").exists(), "la mas vieja se poda");
    }

    #[test]
    fn backup_and_prune_no_borra_un_directorio_con_ese_nombre() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("tylluan.toml");
        std::fs::write(&cfg, "a = 1\n").unwrap();
        let carpeta = dir.path().join("tylluan.toml.bak-9999999999999");
        std::fs::create_dir(&carpeta).unwrap();
        for mark in [1u128, 2, 3, 4] {
            std::fs::write(dir.path().join(format!("tylluan.toml.bak-{mark}")), b"x").unwrap();
        }

        backup_and_prune(&cfg).expect("copia previa");

        assert!(carpeta.is_dir(), "solo se borran ficheros regulares");
    }

    #[test]
    fn backup_mark_exige_el_patron_exacto() {
        assert_eq!(backup_mark("tylluan.toml.bak-12", "tylluan.toml"), Some(12));
        for malo in [
            "tylluan.toml.bak",
            "tylluan.toml.bak-",
            "tylluan.toml.bak-12.tmp",
            "tylluan.toml.bak-x",
            "tylluan.toml",
            "otro.toml.bak-12",
            "providers.json.bak",
        ] {
            assert_eq!(backup_mark(malo, "tylluan.toml"), None, "{malo} no es copia podable");
        }
    }
    /// D5018 (a): el 0600 del config nuevo. El test se compila en **ambas** plataformas para no
    /// romper el recuento que valida `check_test_count.sh` (paridad Windows/Linux, convencion de
    /// D206 en config.rs:3654). En Unix se comprueba el modo real DEL FICHERO DEFINITIVO tras el
    /// rename; en Windows se comprueba que la via del modo no se aplica, que es donde manda la ACL.
    #[test]
    fn d5018_el_config_nuevo_toma_modo_privado_antes_del_rename() {
        let dir = tempfile::tempdir().unwrap();
        let tmp = dir.path().join("tylluan.toml.tmp");
        let destino = dir.path().join("tylluan.toml");
        std::fs::write(&tmp, "[nexus]\nlog_level = \"warn\"\n").unwrap();
        let aplicado = set_private_mode(&tmp);
        std::fs::rename(&tmp, &destino).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = std::fs::metadata(&destino).unwrap().permissions().mode() & 0o777;
            assert!(aplicado, "en Unix el 0600 tenia que aplicarse");
            assert_eq!(modo, PRIVATE_CONFIG_MODE, "el rename perdio el modo del config");
            assert!(!crate::config::mode_is_too_open(modo));
        }
        #[cfg(not(unix))]
        {
            assert!(!aplicado, "en Windows endurece la ACL, no el modo POSIX");
        }
        assert!(!crate::config::mode_is_too_open(PRIVATE_CONFIG_MODE));
    }
}
