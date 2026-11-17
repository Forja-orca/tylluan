//! Instancias con nombre (T4391): puerto y `DATA_DIR` por instancia.
//!
//! El CLI tenía `DEFAULT_PORT` fijo en todas partes, así que dos kernels en la
//! misma máquina (producción + laboratorio) solo se distinguían por flags sueltos.
//! Aquí el nombre manda: `--instance lab` resuelve puerto y `DATA_DIR` del registro.
//!
//! Registro: `~/.tylluan/instances.toml` (o `TYLLUAN_INSTANCES_FILE`).
//!
//! ```toml
//! [instances.default]
//! port = 47004
//!
//! [instances.lab]
//! port = 47009
//! data_dir = "D:/tylluan-lab/data"
//! ```
//!
//! Fail-closed: un nombre no declarado es **error**, no un fallback silencioso al
//! puerto de producción. Sin fichero, solo existe `default` (puerto histórico).

use std::path::PathBuf;

/// Puerto histórico: el de `default` cuando no hay registro.
pub const DEFAULT_PORT: u16 = 47004;

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct InstanceEntry {
    pub port: u16,
    #[serde(default)]
    pub data_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstanceConfig {
    pub name: String,
    pub port: u16,
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, serde::Deserialize)]
struct Registry {
    #[serde(default)]
    instances: std::collections::BTreeMap<String, InstanceEntry>,
}

/// Ruta del registro: `TYLLUAN_INSTANCES_FILE` o `~/.tylluan/instances.toml`.
pub fn registry_path() -> PathBuf {
    if let Some(p) = std::env::var_os("TYLLUAN_INSTANCES_FILE") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".tylluan").join("instances.toml")
}

/// Resuelve una instancia a partir del **texto** del registro.
///
/// Puro a propósito: el test no toca el sistema de ficheros ni el entorno.
/// `raw = None` = no hay registro.
pub fn resolve_from_registry(
    raw: Option<&str>,
    name: &str,
) -> Result<InstanceConfig, String> {
    let name = name.trim();
    let name = if name.is_empty() { "default" } else { name };

    let Some(raw) = raw else {
        // Sin registro: solo el puerto histórico, y solo para `default`.
        if name == "default" {
            return Ok(InstanceConfig { name: name.to_string(), port: DEFAULT_PORT, data_dir: None });
        }
        return Err(format!(
            "instancia '{name}' no declarada y no hay registro de instancias ({}).              Crea [instances.{name}] con su puerto.",
            registry_path().display()
        ));
    };

    let reg: Registry = toml::from_str(raw)
        .map_err(|e| format!("registro de instancias ilegible: {e}"))?;

    if reg.instances.is_empty() {
        // Fichero presente pero vacío: mismo criterio que sin fichero.
        if name == "default" {
            return Ok(InstanceConfig { name: name.to_string(), port: DEFAULT_PORT, data_dir: None });
        }
        return Err(format!("instancia '{name}' no declarada en el registro de instancias"));
    }

    match reg.instances.get(name) {
        Some(e) => Ok(InstanceConfig {
            name: name.to_string(),
            port: e.port,
            data_dir: e.data_dir.as_ref().map(PathBuf::from),
        }),
        None => Err(format!(
            "instancia '{name}' no declarada. Declaradas: {}",
            reg.instances.keys().cloned().collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Igual que [`resolve_from_registry`] leyendo el fichero real.
pub fn load(name: &str) -> Result<InstanceConfig, String> {
    let path = registry_path();
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => Some(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("no se pudo leer {}: {e}", path.display())),
    };
    resolve_from_registry(raw.as_deref(), name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REG: &str = r#"
[instances.default]
port = 47004

[instances.lab]
port = 47009
data_dir = "D:/tylluan-lab/data"
"#;

    /// Sin registro, `default` sigue funcionando con el puerto histórico.
    #[test]
    fn sin_registro_default_usa_el_puerto_historico() {
        let i = resolve_from_registry(None, "default").unwrap();
        assert_eq!(i.port, DEFAULT_PORT);
        assert!(i.data_dir.is_none());
    }

    /// Sin registro, cualquier otro nombre **falla** (no se cae al 47004).
    #[test]
    fn sin_registro_un_nombre_desconocido_falla() {
        let e = resolve_from_registry(None, "lab").unwrap_err();
        assert!(e.contains("lab"), "{e}");
        assert!(e.contains("no declarada"), "{e}");
    }

    /// Una instancia declarada trae puerto y `DATA_DIR` propios.
    #[test]
    fn instancia_declarada_resuelve_puerto_y_data_dir() {
        let i = resolve_from_registry(Some(REG), "lab").unwrap();
        assert_eq!(i.port, 47009);
        assert_eq!(i.data_dir, Some(PathBuf::from("D:/tylluan-lab/data")));
    }

    /// Nombre no declarado en un registro con instancias: error que lista las que hay.
    #[test]
    fn nombre_no_declarado_lista_las_que_existen() {
        let e = resolve_from_registry(Some(REG), "prod2").unwrap_err();
        assert!(e.contains("prod2"), "{e}");
        assert!(e.contains("default") && e.contains("lab"), "{e}");
    }

    /// Un registro roto no se interpreta como «usa el puerto de producción».
    #[test]
    fn registro_ilegible_es_error() {
        let e = resolve_from_registry(Some("esto no es toml = = ="), "default").unwrap_err();
        assert!(e.contains("ilegible"), "{e}");
    }
}
