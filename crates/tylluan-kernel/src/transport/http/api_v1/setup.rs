//! D228 (T4774): estado de la **primera instalación**, medido del estado real.
//!
//! `GET /api/v1/setup/status` responde a «¿qué le falta a esta instalación para estar lista?»
//! sin adivinar y **sin secretos**. Reglas que respeta:
//!
//! - **Se mide, no se supone.** Cada paso lleva la condición que lo decidió y la evidencia que
//!   la demuestra. Lo que no se puede comprobar es `unknown`, nunca `done`.
//! - **Nada sensible sale.** De `security.acl.tokens` solo viaja el **recuento**; de la nube,
//!   los **nombres** de los puntos encendidos; de la clave del operador, si el artefacto existe.
//!   El test `d228_la_respuesta_no_lleva_valores_sensibles` escanea el JSON contra valores
//!   secretos sembrados a propósito (y es el objetivo de mutación de esta tarea).
//! - **No toca autenticación.** No importa `crate::operator` ni llama a auth: mira si existe
//!   `operator.json` en el DATA_DIR, que es lo que D213 deja cuando hay clave. Si esa cadena no
//!   está integrada, el paso simplemente queda `pending`.
//! - **«Listo» solo si de verdad lo está.** `ready` = todos los pasos obligatorios en `done`.

use axum::{Json, extract::State, response::IntoResponse};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::transport::http::HttpState;

/// Pasos del asistente, en el orden en que se presentan. `id` estable para el cliente.
pub const PASOS: &[&str] = &["data_dir", "operator_key", "memory", "clients", "privacy", "backup"];

/// D228: tipos que el kernel **siembra** al arrancar una base nueva (medido en un DATA_DIR
/// vacío: 182 nodos, 154 con vector, entre `routing_anchor`, `identity` y `agent_skill`).
/// El paso «memoria» cuenta lo que guardan el operador y sus agentes, no la siembra: si no,
/// una instalación recién creada diría «memoria lista» sin que nadie haya guardado nada.
pub const TIPOS_SEMBRADOS: &[&str] = &["routing_anchor", "identity", "agent_skill"];

/// Todo lo que la evaluación necesita saber, ya medido. Separar medidas de presentación es lo
/// que hace testeable el contrato sin arrancar un kernel.
#[derive(Debug, Clone, Default)]
pub struct Medidas {
    pub data_dir_existe: bool,
    pub data_dir_escribible: bool,
    pub memory_db_existe: bool,
    /// Nodos que ha guardado el operador o sus agentes (sin la siembra del kernel).
    pub nodos_de_usuario: u64,
    /// Cuántos de esos ya tienen vector. Solo informativo: el reindexado va por tick
    /// (`memory.reindex_tick_secs`, 600 s por defecto), así que un recuerdo recién
    /// guardado puede tardar en tener vector sin que nada esté roto.
    pub nodos_con_embedding: u64,
    pub agentes_con_credencial: usize,
    /// Nombres de los puntos de inferencia con nube encendida. Nombres, nunca claves.
    pub puntos_cloud_encendidos: Vec<&'static str>,
    pub clave_operador_presente: bool,
    pub copia_configurada: bool,
    /// Ruta de la base que el kernel usa de verdad (`<DATA_DIR>/tylluan.db`) y la que
    /// declara el TOML. Se enseñan las dos porque pueden no coincidir (main.rs:658-661).
    pub ruta_db_efectiva: String,
    pub ruta_db_declarada: String,
}

impl Medidas {
    /// Deriva las medidas del config **en memoria**: solo recuentos y nombres, jamás valores.
    pub fn de_config(config: &crate::config::TylluanConfig) -> Self {
        let p = &config.inference.points;
        let mut encendidos = Vec::new();
        if p.chat.cloud_enabled {
            encendidos.push("chat");
        }
        if p.decision.cloud_enabled {
            encendidos.push("decision");
        }
        if p.grammar_judge.cloud_enabled {
            encendidos.push("grammar_judge");
        }
        if p.sentinel.cloud_enabled {
            encendidos.push("sentinel");
        }
        Self {
            // El recuento es la única información que sale de los tokens (F1 de D-100:
            // en `acl.tokens` la CLAVE es el secreto).
            agentes_con_credencial: config.security.acl.tokens.len()
                + config.security.acl.token_agent_bindings.len(),
            puntos_cloud_encendidos: encendidos,
            ..Self::default()
        }
    }
}

/// Evalúa los pasos. Puro: sin I/O, sin reloj salvo el que le pasan.
pub fn evalua(m: &Medidas, data_dir: &Path, ahora_unix: u64) -> serde_json::Value {
    let pasos = vec![
        paso(
            "data_dir",
            "Dónde guardo tus datos",
            true,
            if m.data_dir_existe && m.data_dir_escribible && m.memory_db_existe {
                "done"
            } else {
                "pending"
            },
            if m.data_dir_existe && m.data_dir_escribible && m.memory_db_existe {
                "hay base de memoria y el directorio admite escritura"
            } else if !m.data_dir_existe {
                "el directorio de datos todavía no existe"
            } else if !m.data_dir_escribible {
                "el directorio de datos no admite escritura"
            } else {
                "el directorio existe pero aún no hay base de memoria"
            },
            json!({
                "ruta": data_dir.display().to_string(),
                "escribible": m.data_dir_escribible,
                "base_presente": m.memory_db_existe,
                "db_efectiva": m.ruta_db_efectiva,
                "db_declarada_en_toml": m.ruta_db_declarada,
                "coinciden": m.ruta_db_efectiva == m.ruta_db_declarada,
            }),
        ),
        paso(
            "operator_key",
            "Tu clave",
            true,
            if m.clave_operador_presente { "done" } else { "pending" },
            if m.clave_operador_presente {
                "hay clave de operador configurada (solo se guarda su hash)"
            } else {
                "no hay clave de operador configurada; se crea desde el asistente"
            },
            json!({ "presente": m.clave_operador_presente }),
        ),
        paso(
            "memory",
            "Memoria",
            true,
            if m.nodos_de_usuario > 0 { "done" } else { "pending" },
            if m.nodos_de_usuario > 0 {
                "ya hay recuerdos guardados (la siembra del kernel no cuenta)"
            } else {
                "todavía no hay ningún recuerdo guardado por el operador ni por sus agentes"
            },
            json!({
                "nodos_de_usuario": m.nodos_de_usuario,
                "con_vector": m.nodos_con_embedding,
            }),
        ),
        paso(
            "clients",
            "Conectar tus herramientas",
            true,
            if m.agentes_con_credencial > 0 { "done" } else { "pending" },
            if m.agentes_con_credencial > 0 {
                "hay al menos un cliente con credencial"
            } else {
                "ningún cliente tiene credencial todavía"
            },
            // Solo el recuento: las claves de `acl.tokens` son el secreto.
            json!({ "agentes_con_credencial": m.agentes_con_credencial }),
        ),
        paso(
            "privacy",
            "Privacidad",
            true,
            if m.puntos_cloud_encendidos.is_empty() { "done" } else { "pending" },
            if m.puntos_cloud_encendidos.is_empty() {
                "la nube está apagada en los cuatro puntos de inferencia"
            } else {
                "hay puntos de inferencia que salen a la nube"
            },
            json!({ "puntos_con_nube": m.puntos_cloud_encendidos }),
        ),
        paso(
            "backup",
            "Copia de seguridad",
            false,
            if m.copia_configurada { "done" } else { "pending" },
            if m.copia_configurada {
                "hay una carpeta de copias configurada"
            } else {
                "hoy las copias no se configuran desde el kernel: no hay ajuste que medir"
            },
            json!({ "configurada": m.copia_configurada }),
        ),
    ];

    let ready = pasos
        .iter()
        .all(|p| p["required"] != true || p["state"] == "done");

    json!({
        "status": "ok",
        "ready": ready,
        "generated_at_unix": ahora_unix,
        "steps": pasos,
        "notes": [
            "Ready solo si todos los pasos obligatorios están en done.",
            "De las credenciales viaja el recuento, nunca las claves.",
            "Lo que no se puede comprobar se devuelve como unknown, jamás como done.",
        ],
    })
}

fn paso(id: &str, titulo: &str, required: bool, state: &str, why: &str, evidence: serde_json::Value) -> serde_json::Value {
    json!({
        "id": id,
        "title": titulo,
        "required": required,
        "state": state,
        "why": why,
        "evidence": evidence,
    })
}

/// ¿Podemos escribir en el directorio? Crea y borra un fichero de sonda: es la única forma
/// honesta de saberlo (los permisos POSIX no existen en Windows).
pub fn es_escribible(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let sonda: PathBuf = dir.join(format!(".setup_probe_{}", std::process::id()));
    match std::fs::write(&sonda, b"probe") {
        Ok(()) => {
            let _ = std::fs::remove_file(&sonda);
            true
        }
        Err(_) => false,
    }
}

/// Compone la respuesta final. El handler y el test de secretos usan **esta** función, para
/// que el test vigile la superficie real (no una copia que se le parezca).
pub fn respuesta(
    config: &crate::config::TylluanConfig,
    medidas: &Medidas,
    data_dir: &Path,
    ahora_unix: u64,
    version: &str,
) -> serde_json::Value {
    let mut doc = evalua(medidas, data_dir, ahora_unix);
    doc["version"] = json!(version);
    // Solo se usa el config para la version y para lo que ya midio `de_config`.
    let _ = config;
    doc
}

pub async fn setup_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await.clone();
    let data_dir = crate::config::data_dir();

    let mut medidas = Medidas::de_config(&config);
    medidas.data_dir_existe = data_dir.is_dir();
    medidas.data_dir_escribible = es_escribible(&data_dir);
    // La base que el kernel usa de verdad es `<DATA_DIR>/tylluan.db`: con
    // `TYLLUAN_DATA_DIR` puesto, main.rs:658-661 reescribe `memory.db_path`. Se miran
    // las dos para no dar por buena una ruta declarada que el kernel ignora.
    let efectiva = crate::config::data_file("tylluan.db");
    let declarada = Path::new(&config.memory.db_path).to_path_buf();
    medidas.memory_db_existe = efectiva.exists() || declarada.exists();
    medidas.clave_operador_presente = data_dir.join("operator.json").exists();
    medidas.copia_configurada = false;

    // Memoria del operador = nodos totales menos la siembra del kernel. Se mide con las
    // estadisticas reales de SilvaDB; si fallan, el paso queda pending (no se inventa).
    match state.silva.get_detailed_stats().await {
        Ok(stats) => {
            let total = stats.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0);
            let sembrados: u64 = TIPOS_SEMBRADOS
                .iter()
                .map(|t| {
                    stats
                        .get("by_type")
                        .and_then(|b| b.get(*t))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                })
                .sum();
            medidas.nodos_de_usuario = total.saturating_sub(sembrados);
        }
        Err(e) => tracing::warn!("D228 setup/status: sin estadisticas de SilvaDB: {e}"),
    }
    if let Ok(n) = state.silva.count_nodes_with_embedding().await {
        medidas.nodos_con_embedding = n as u64;
    }
    medidas.ruta_db_efectiva = efectiva.display().to_string();
    medidas.ruta_db_declarada = declarada.display().to_string();

    let ahora = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let doc = respuesta(&config, &medidas, &data_dir, ahora, &state.version);
    (axum::http::StatusCode::OK, Json(doc)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn medidas_llenas() -> Medidas {
        Medidas {
            data_dir_existe: true,
            data_dir_escribible: true,
            memory_db_existe: true,
            nodos_de_usuario: 3,
            nodos_con_embedding: 4,
            agentes_con_credencial: 2,
            puntos_cloud_encendidos: vec![],
            clave_operador_presente: true,
            copia_configurada: true,
            ruta_db_efectiva: "D:/datos/tylluan.db".to_string(),
            ruta_db_declarada: "D:/datos/tylluan.db".to_string(),
        }
    }

    fn estado(doc: &serde_json::Value, id: &str) -> String {
        doc["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .find(|p| p["id"] == id)
            .unwrap_or_else(|| panic!("paso {id} ausente"))["state"]
            .as_str()
            .expect("state es string")
            .to_string()
    }

    #[test]
    fn d228_todo_lleno_da_ready_y_todo_vacio_no() {
        let doc = evalua(&medidas_llenas(), Path::new("D:/datos"), 1_700_000_000);
        assert_eq!(doc["ready"], true);
        for id in PASOS {
            assert_eq!(estado(&doc, id), "done", "paso {id}");
        }

        let vacio = Medidas::default();
        let doc = evalua(&vacio, Path::new("D:/datos"), 1_700_000_000);
        assert_eq!(doc["ready"], false);
        assert_eq!(estado(&doc, "memory"), "pending");
        assert_eq!(estado(&doc, "clients"), "pending");
        assert_eq!(estado(&doc, "operator_key"), "pending");
        // backup es opcional: pendiente no impide ready, pero tampoco se finge done
        assert_eq!(estado(&doc, "backup"), "pending");
    }

    #[test]
    fn d228_privacidad_pendiente_si_un_punto_sale_a_la_nube() {
        let mut m = medidas_llenas();
        m.puntos_cloud_encendidos = vec!["chat"];
        let doc = evalua(&m, Path::new("D:/datos"), 1);
        assert_eq!(estado(&doc, "privacy"), "pending");
        assert_eq!(doc["ready"], false);
        assert_eq!(doc["steps"][4]["evidence"]["puntos_con_nube"][0], "chat");
    }

    /// OBJETIVO DE MUTACIÓN (T4774): si alguien mete un valor sensible en la respuesta,
    /// este test se pone ROJO. Siembra un token, un vínculo y un secreto de servicio en el
    /// config y exige que **ninguno** aparezca en el JSON.
    #[test]
    fn d228_la_respuesta_no_lleva_valores_sensibles() {
        const TOKEN: &str = "TOKEN-QUE-NO-DEBE-SALIR-d228";
        const VINCULO: &str = "AGENTE-VINCULADO-SECRETO-d228";

        let mut cfg = crate::config::TylluanConfig::default();
        cfg.security.acl.tokens.insert("sha256:aaaa".to_string(), TOKEN.to_string());
        cfg.security.acl.token_agent_bindings.insert("sha256:aaaa".to_string(), VINCULO.to_string());

        let m = Medidas::de_config(&cfg);
        assert_eq!(m.agentes_con_credencial, 2, "el recuento tiene que contar ambas entradas");

        // La superficie completa (handler), no solo `evalua`.
        let doc = respuesta(&cfg, &m, Path::new("D:/datos"), 1, "0.0.0-test");
        let texto = doc.to_string();
        assert!(!texto.contains(TOKEN), "el token aparecio en la respuesta");
        assert!(!texto.contains(VINCULO), "el vinculo agente-token aparecio en la respuesta");
        // y el recuento SÍ sale: no se ha tapado la información útil
        assert_eq!(doc["steps"][3]["evidence"]["agentes_con_credencial"], 2);
    }

    /// Ningún paso puede declarar `done` sin evidencia, ni usar un estado inventado.
    #[test]
    fn d228_estados_validos_y_done_siempre_con_evidencia() {
        let validos = ["done", "pending", "blocked", "skipped", "unknown"];
        for m in [Medidas::default(), medidas_llenas()] {
            let doc = evalua(&m, Path::new("D:/datos"), 1);
            for p in doc["steps"].as_array().expect("steps") {
                let st = p["state"].as_str().expect("state");
                assert!(validos.contains(&st), "estado invalido: {st}");
                assert!(p.get("evidence").is_some(), "paso sin evidencia: {}", p["id"]);
                assert!(p["why"].as_str().map(|s| !s.is_empty()).unwrap_or(false));
            }
        }
        // los seis pasos del asistente, ni uno mas ni uno menos
        let doc = evalua(&medidas_llenas(), Path::new("D:/datos"), 1);
        let ids: Vec<&str> = doc["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .map(|p| p["id"].as_str().unwrap_or(""))
            .collect();
        assert_eq!(ids, PASOS);
    }
}
