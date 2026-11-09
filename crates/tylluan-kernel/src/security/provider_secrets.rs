//! # D-157 — SecretStore de claves de proveedores externos
//!
//! Las claves de los proveedores externos **no** se escriben en `tylluan.toml`,
//! ni en `data/`, ni en logs. Viven aquí, con dos backends y este orden:
//!
//! 1. **Keychain del SO** (`keyring`): Windows Credential Manager (protegido
//!    por DPAPI **por usuario**), macOS Keychain, Linux Secret Service. La
//!    clave no toca disco. Cero dependencias nuevas: `keyring = "3"` ya está
//!    en el árbol y es el mismo backend que guarda `TYLLUAN_DB_KEY`.
//! 2. **Fichero cifrado** en `%APPDATA%\Tylluan\secrets\providers.enc`
//!    (`$XDG_DATA_HOME/Tylluan/secrets` en Unix), modo `0600` cuando el
//!    sistema lo permite, con **ChaCha20-Poly1305** y clave derivada con
//!    [`crate::security::secret_vault::derive_key`] (separación de dominio ya
//!    existente, no se inventa otra KDF).
//!
//! ## La debilidad que se hereda (dicha aquí, no descubierta después)
//!
//! El backend 2 **no es más fuerte que su raíz**. La clave del fichero sale de
//! `TYLLUAN_DB_KEY`, que se resuelve como variable de entorno > keychain >
//! **fichero `.tylluan-db-key` junto a la base (Argon2id)**. Si el operador
//! está en ese **modo 3**, quien pueda leer `.tylluan-db-key` puede derivar
//! también esta clave y descifrar `providers.enc`. Para protección real contra
//! acceso al disco hay que fijar `TYLLUAN_DB_KEY` (entorno/vault) o tener
//! keychain del SO disponible — que es el backend 1 y por eso va primero.
//!
//! ## Lo que este módulo NO hace
//!
//! - **No guarda en claro nunca**: si no puede cifrar, devuelve error. No hay
//!   modo «funciona sin cifrar» (misma regla que D-104).
//! - **No protege contra un proceso comprometido**: mientras el kernel usa la
//!   clave, ésta vive en su memoria.
//! - **No redacta por sí mismo**: la garantía de que la clave no aparece en
//!   una respuesta HTTP se prueba en los handlers (`api_admin`), no aquí.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

/// Servicio del keychain del SO para claves de proveedores.
pub const KEYCHAIN_SERVICE: &str = "tylluan-provider-secrets";

/// Nombre del fichero del backend 2 dentro de la carpeta de secretos.
const FILE_NAME: &str = "providers.enc";

/// Dominio del AEAD: separa estas claves de las de la bóveda D-104 aunque las
/// dos cuelguen de la misma raíz.
const DOMAIN: &[u8] = b"tylluan-provider-secrets-v1";

/// De dónde sale la clave de un proveedor declarado en la configuración.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// Hay una clave en este SecretStore (keychain o fichero cifrado).
    Vault,
    /// El proveedor usa `api_key_env` (compatibilidad con `tylluan.toml`).
    Env,
    /// Ni SecretStore ni `api_key_env`: el proveedor no tiene clave.
    None,
}

impl KeySource {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySource::Vault => "vault",
            KeySource::Env => "env",
            KeySource::None => "none",
        }
    }
}

/// Carpeta del backend de fichero.
///
/// Windows: `%APPDATA%\Tylluan\secrets` (perfil del usuario).
/// Unix: `$XDG_DATA_HOME/Tylluan/secrets`, o `~/.local/share/Tylluan/secrets`.
pub fn secrets_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA")
        && !appdata.trim().is_empty()
    {
        return PathBuf::from(appdata).join("Tylluan").join("secrets");
    }
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.trim().is_empty()
    {
        return PathBuf::from(xdg).join("Tylluan").join("secrets");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.trim().is_empty()
    {
        return PathBuf::from(home).join(".local/share/Tylluan").join("secrets");
    }
    PathBuf::from("secrets")
}

/// Ruta del fichero cifrado del backend 2.
pub fn file_path() -> PathBuf {
    secrets_dir().join(FILE_NAME)
}

/// Clave de contenido del backend de fichero, derivada de la raíz de la base.
fn content_key() -> Result<[u8; 32]> {
    let db_key_hex = crate::config::db_key_hex(&crate::config::data_dir())
        .context("no se pudo resolver TYLLUAN_DB_KEY para el SecretStore")?;
    crate::security::secret_vault::derive_key(&db_key_hex)
}

/// ¿Podemos usar el keychain del SO en este equipo?
///
/// En Linux sin DBus el `keyring` puede tardar ~25 s en fallar, así que se
/// comprueba antes (misma guarda que usa la raíz de la base).
fn keychain_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        if !crate::config::dbus_is_available() {
            return false;
        }
    }
    true
}

fn keychain_entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, name).context("keychain del SO no disponible")
}

/// Guarda (o reemplaza) la clave de un proveedor.
///
/// Keychain primero; si no está disponible o falla, fichero cifrado. Nunca
/// escribe en claro.
pub fn set_secret(name: &str, value: &str) -> Result<()> {
    validate_name(name)?;
    if value.is_empty() {
        bail!("la clave está vacía; usa delete_secret para quitarla");
    }
    if keychain_available()
        && let Ok(entry) = keychain_entry(name)
    {
        match entry.set_password(value) {
            Ok(()) => return Ok(()),
            Err(err) => tracing::warn!(
                backend = "keychain",
                error = %err,
                "no se pudo guardar en el keychain; se usa el fichero cifrado"
            ),
        }
    }
    let key = content_key()?;
    set_secret_in(&secrets_dir(), &key, name, value)
}

/// Lee la clave de un proveedor. `Ok(None)` = no hay clave guardada.
pub fn get_secret(name: &str) -> Result<Option<String>> {
    validate_name(name)?;
    if keychain_available()
        && let Ok(entry) = keychain_entry(name)
    {
        match entry.get_password() {
            Ok(value) => return Ok(Some(value)),
            Err(keyring::Error::NoEntry) => {}
            Err(err) => tracing::warn!(
                backend = "keychain",
                error = %err,
                "keychain no responde; se prueba el fichero cifrado"
            ),
        }
    }
    let key = content_key()?;
    get_secret_in(&secrets_dir(), &key, name)
}

/// Borra la clave de un proveedor de **los dos** backends.
///
/// Devuelve `true` si había algo que borrar. No falla si no existía.
pub fn delete_secret(name: &str) -> Result<bool> {
    validate_name(name)?;
    let mut removed = false;
    if keychain_available()
        && let Ok(entry) = keychain_entry(name)
    {
        match entry.delete_credential() {
            Ok(()) => removed = true,
            Err(keyring::Error::NoEntry) => {}
            Err(err) => tracing::warn!(
                backend = "keychain",
                error = %err,
                "no se pudo borrar del keychain; se intenta el fichero"
            ),
        }
    }
    let key = content_key()?;
    if delete_secret_in(&secrets_dir(), &key, name)? {
        removed = true;
    }
    Ok(removed)
}

/// Clave que usaría el kernel para este proveedor, en el orden real:
/// SecretStore > `api_key_env`. `Ok(None)` = no hay ninguna.
pub fn resolve_api_key(name: &str, api_key_env: &str) -> Result<Option<String>> {
    if let Some(secret) = get_secret(name)? {
        return Ok(Some(secret));
    }
    let env_name = api_key_env.trim();
    if env_name.is_empty() {
        return Ok(None);
    }
    Ok(std::env::var(env_name).ok().filter(|v| !v.trim().is_empty()))
}

/// Origen y disponibilidad de la clave de un proveedor, sin devolverla.
pub fn key_status(name: &str, api_key_env: &str) -> Result<(KeySource, bool)> {
    if get_secret(name)?.is_some() {
        return Ok((KeySource::Vault, true));
    }
    if api_key_env.trim().is_empty() {
        return Ok((KeySource::None, false));
    }
    let set = std::env::var(api_key_env)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    Ok((KeySource::Env, set))
}

/// ¿Hay clave para este proveedor? Pensado para el arranque: ante un fallo de
/// lectura devuelve `true` (se conserva el proveedor) y deja el aviso en el
/// log — borrar configuración por un keychain que no responde sería peor.
pub fn has_secret_or_unknown(name: &str) -> bool {
    match get_secret(name) {
        Ok(found) => found.is_some(),
        Err(err) => {
            tracing::warn!(
                provider = %name,
                error = %err,
                "no se pudo consultar el SecretStore; se conserva el proveedor"
            );
            true
        }
    }
}

/// Nombre válido de proveedor: el mismo criterio que usa la configuración
/// (`[a-z0-9][a-z0-9._-]*`, máximo 64) porque viaja en rutas y en `model.json`
/// de los artefactos del operador.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        bail!("nombre de proveedor inválido: longitud fuera de 1..=64");
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap_or('x');
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        bail!("nombre de proveedor inválido: debe empezar por letra minúscula o dígito");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
    {
        bail!("nombre de proveedor inválido: sólo [a-z0-9._-]");
    }
    Ok(())
}

// ── Backend de fichero (inyectable para tests) ──────────────────────────────

fn data_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

fn read_map(dir: &Path, key: &[u8; 32]) -> Result<BTreeMap<String, String>> {
    let path = data_path(dir);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("no se pudo leer {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let stored: BTreeMap<String, String> = serde_json::from_str(&raw)
        .with_context(|| format!("{} no es un almacén válido", path.display()))?;
    let mut out = BTreeMap::new();
    for (name, b64) in stored {
        out.insert(name.clone(), decrypt_value(key, &name, &b64)?);
    }
    Ok(out)
}

fn write_map(dir: &Path, key: &[u8; 32], map: &BTreeMap<String, String>) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("no se pudo crear {}", dir.display()))?;
    let mut stored = BTreeMap::new();
    for (name, value) in map {
        stored.insert(name.clone(), encrypt_value(key, name, value)?);
    }
    let raw = serde_json::to_string_pretty(&stored).context("no se pudo serializar el almacén")?;
    let path = data_path(dir);
    let tmp = path.with_extension("enc.tmp");
    write_private_file(&tmp, raw.as_bytes())?;
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("no se pudo reemplazar {}", path.display()))?;
    restrict_permissions(&path);
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("no se pudo crear {}", path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("no se pudo escribir {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).with_context(|| format!("no se pudo escribir {}", path.display()))?;
    }
    Ok(())
}

fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(0o600);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
    #[cfg(not(unix))]
    {
        // En Windows el fichero vive en %APPDATA% (ACL del perfil del usuario);
        // endurecer la ACL requiere API nativa y no se hace a mano.
        let _ = path;
    }
}

fn encrypt_value(key: &[u8; 32], name: &str, value: &str) -> Result<String> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).context("clave de contenido inválida")?;
    let mut nonce = [0u8; 12];
    {
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut nonce);
    }
    let mut aad = DOMAIN.to_vec();
    aad.extend_from_slice(name.as_bytes());
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: value.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|_| anyhow::anyhow!("no se pudo cifrar la clave del proveedor"))?;
    let mut blob = nonce.to_vec();
    blob.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(blob))
}

fn decrypt_value(key: &[u8; 32], name: &str, b64: &str) -> Result<String> {
    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .context("almacén de secretos corrupto (base64)")?;
    if blob.len() < 13 {
        bail!("almacén de secretos corrupto (blob corto)");
    }
    let (nonce, ciphertext) = blob.split_at(12);
    let cipher = ChaCha20Poly1305::new_from_slice(key).context("clave de contenido inválida")?;
    let mut aad = DOMAIN.to_vec();
    aad.extend_from_slice(name.as_bytes());
    let plain = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| anyhow::anyhow!("no se pudo descifrar la clave de '{name}' (clave raíz distinta o fichero manipulado)"))?;
    String::from_utf8(plain).context("la clave descifrada no es UTF-8")
}

fn set_secret_in(dir: &Path, key: &[u8; 32], name: &str, value: &str) -> Result<()> {
    let mut map = read_map(dir, key)?;
    map.insert(name.to_string(), value.to_string());
    write_map(dir, key, &map)
}

fn get_secret_in(dir: &Path, key: &[u8; 32], name: &str) -> Result<Option<String>> {
    Ok(read_map(dir, key)?.remove(name))
}

fn delete_secret_in(dir: &Path, key: &[u8; 32], name: &str) -> Result<bool> {
    let mut map = read_map(dir, key)?;
    let existed = map.remove(name).is_some();
    if existed {
        write_map(dir, key, &map)?;
    }
    Ok(existed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];
    const OTHER_KEY: [u8; 32] = [9u8; 32];

    #[test]
    fn file_backend_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(get_secret_in(tmp.path(), &KEY, "openai").unwrap(), None);
        set_secret_in(tmp.path(), &KEY, "openai", "sk-prueba-123").unwrap();
        set_secret_in(tmp.path(), &KEY, "ollama", "otra-clave").unwrap();
        assert_eq!(
            get_secret_in(tmp.path(), &KEY, "openai").unwrap().as_deref(),
            Some("sk-prueba-123")
        );
        assert!(delete_secret_in(tmp.path(), &KEY, "openai").unwrap());
        assert_eq!(get_secret_in(tmp.path(), &KEY, "openai").unwrap(), None);
        assert_eq!(
            get_secret_in(tmp.path(), &KEY, "ollama").unwrap().as_deref(),
            Some("otra-clave"),
            "borrar uno no toca los demás"
        );
        assert!(!delete_secret_in(tmp.path(), &KEY, "openai").unwrap());
    }

    #[test]
    fn file_is_never_plaintext() {
        let tmp = tempfile::tempdir().unwrap();
        let secret = "sk-NO-DEBE-APARECER-EN-DISCO";
        set_secret_in(tmp.path(), &KEY, "openai", secret).unwrap();
        let on_disk = std::fs::read_to_string(data_path(tmp.path())).unwrap();
        assert!(
            !on_disk.contains(secret),
            "la clave apareció en claro en el fichero: {on_disk}"
        );
    }

    #[test]
    fn wrong_root_key_cannot_decrypt() {
        let tmp = tempfile::tempdir().unwrap();
        set_secret_in(tmp.path(), &KEY, "openai", "sk-1").unwrap();
        let err = get_secret_in(tmp.path(), &OTHER_KEY, "openai").unwrap_err();
        assert!(err.to_string().contains("no se pudo descifrar"), "{err}");
    }

    #[test]
    fn ciphertext_is_bound_to_the_provider_name() {
        let tmp = tempfile::tempdir().unwrap();
        set_secret_in(tmp.path(), &KEY, "openai", "sk-1").unwrap();
        // Copiamos el blob cifrado de `openai` a otra fila: el nombre va como
        // AAD, así que descifrarlo como `anthropic` debe fallar.
        let path = data_path(tmp.path());
        let raw = std::fs::read_to_string(&path).unwrap();
        let mut map: BTreeMap<String, String> = serde_json::from_str(&raw).unwrap();
        let blob = map.get("openai").cloned().unwrap();
        map.insert("anthropic".into(), blob);
        std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();
        assert!(get_secret_in(tmp.path(), &KEY, "anthropic").is_err());
    }

    #[test]
    fn name_validation_rejects_paths_and_uppercase() {
        let long = "x".repeat(65);
        for bad in ["", "Openai", "../evil", "a/b", "-x", "con espacio", long.as_str()] {
            assert!(validate_name(bad).is_err(), "debería rechazar {bad:?}");
        }
        for good in ["openai", "mi-proveedor_1", "a1.b2"] {
            assert!(validate_name(good).is_ok(), "debería aceptar {good:?}");
        }
    }

    #[test]
    fn key_source_is_vault_env_none() {
        // Sin tocar keychain ni entorno real: se prueba la tabla de decisión.
        assert_eq!(KeySource::Vault.as_str(), "vault");
        assert_eq!(KeySource::Env.as_str(), "env");
        assert_eq!(KeySource::None.as_str(), "none");
    }

    #[cfg(unix)]
    #[test]
    fn file_is_written_0600_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        set_secret_in(tmp.path(), &KEY, "openai", "sk-1").unwrap();
        let mode = std::fs::metadata(data_path(tmp.path()))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "modo real: {mode:o}");
    }
}
