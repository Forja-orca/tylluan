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
    // D5013 (AH01-R4, T5275): la carpeta se creaba con `create_dir_all`, que arrastra la ACL del
    // padre. `ensure_private_data_dir` es la misma maquinaria de D5012: crea y, **solo si la
    // carpeta no existia**, le corta la herencia con (OI)(CI). Un almacen que ya existe no se toca.
    crate::config::ensure_private_data_dir(dir)
        .with_context(|| format!("no se pudo crear {}", dir.display()))?;
    let mut stored = BTreeMap::new();
    for (name, value) in map {
        stored.insert(name.clone(), encrypt_value(key, name, value)?);
    }
    let raw = serde_json::to_string_pretty(&stored).context("no se pudo serializar el almacén")?;
    let path = data_path(dir);
    let tmp = path.with_extension("enc.tmp");
    write_private_file(&tmp, raw.as_bytes())?;
    // El `rename` se lleva el DACL del temporal, asi que cerrar el temporal cierra el destino.
    // Se corta tambien sobre el destino cuando es nuevo: cubre el caso de un `.enc.tmp` viejo
    // (de un proceso reventado) que ya existia y por tanto no se endurecio al escribirlo.
    let destino_nuevo = !path.exists();
    std::fs::rename(&tmp, &path)
        .with_context(|| format!("no se pudo reemplazar {}", path.display()))?;
    if destino_nuevo {
        crate::config::harden_new_secret(&path);
    }
    restrict_permissions(&path);
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    // D5013 (AH01-R4): se escribe y DESPUES se corta la herencia. La ventana crear->endurecer
    // existe (medida en ~40 ms, AH01-R2); esta cerrada por D5012 cuando la carpeta ya esta
    // cerrada, que es el caso de `secrets_dir()` bajo un DATA_DIR nuevo.
    //
    // D5013b-2 (turno 5287 del TL, punto 1): el cierre **ya no depende de que el fichero sea
    // nuevo**. Este fichero se renombra encima de `providers.enc` y el `rename` se lleva su DACL,
    // asi que la ACL del temporal es la que acaba teniendo el almacen. Con el `if nuevo` de antes,
    // un `.enc.tmp` huerfano de un proceso reventado (`nuevo = false`) no se cerraba y el
    // `rename` dejaba abierto hasta un `providers.enc` que ya estaba cerrado. Medido: los dos
    // tests `d5013b_2_*` de este modulo fallaban contra el codigo sin arreglar.
    // En Unix es el mismo defecto por otra via: `OpenOptions::mode` solo rige al CREAR, un huesped
    // conserva su 0644 — por eso ahi se fija el modo tambien despues de escribir.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("no se pudo crear {}", path.display()))?;
        file.write_all(bytes)
            .with_context(|| format!("no se pudo escribir {}", path.display()))?;
        drop(file);
        // Sobre un huesped esto es lo unico que cierra: `mode()` anterior no toco sus permisos.
        if let Err(e) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)) {
            tracing::warn!(
                "No pude fijar 0600 en {}: {e}. El temporal va a ser el almacen de claves.",
                path.display()
            );
        }
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).with_context(|| format!("no se pudo escribir {}", path.display()))?;
    }
    // No-op fuera de Windows; en Windows corta la herencia y auto-verifica (D5013/AH01-R3). Se
    // llama siempre: aqui no hay una ACL «del operador» que preservar (politica de Jose, T5266),
    // hay un temporal que dentro de dos lineas va a ser el almacen de claves de proveedores.
    crate::config::harden_new_secret(path);
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
        // D5013 (AH01-R4): el comentario de aqui decia que «endurecer la ACL requiere API nativa
        // y no se hace a mano». Falso desde AH-01: `config::harden_new_secret` la corta con
        // `icacls`, y ya se llama sobre el fichero NUEVO en `write_private_file` y sobre el
        // destino despues del `rename`. Aqui no se repite la llamada a proposito: sobre un
        // almacen que ya existia no se reescribe ninguna ACL (politica de Jose, T5266).
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

    // El test se compila en todos los SO (el recuento de tests debe ser igual en
    // Windows y Linux); solo la comprobación del modo es específica de Unix.
    #[test]
    fn file_is_written_0600_on_unix() {
        let tmp = tempfile::tempdir().unwrap();
        set_secret_in(tmp.path(), &KEY, "openai", "sk-1").unwrap();
        let metadata = std::fs::metadata(data_path(tmp.path())).unwrap();
        assert!(metadata.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "modo real: {mode:o}");
        }
    }

    /// D5013 (AH01-R4, T5275): el almacén de claves de proveedores —`providers.enc`, su
    /// `.enc.tmp` y la carpeta cuando es nueva— no pasaba por ninguna maquinaria de endurecido:
    /// `create_dir_all` + `fs::write` + un `restrict_permissions` que en Windows era un no-op
    /// con un comentario falso. Aquí se comprueba con SDDL real que **nace cerrado** aunque el
    /// padre esté abierto, y que el `rename` del temporal se lleva ese cierre.
    #[test]
    fn d5013_el_almacen_nace_cerrado_aunque_el_padre_esta_abierto() {
        // El test existe siempre; solo el cuerpo es de Windows (paridad de recuentos, gate CI).
        #[cfg(windows)]
        d5013_cuerpo_almacen_cerrado();
    }

    #[cfg(windows)]
    fn d5013_cuerpo_almacen_cerrado() {
        use crate::config::{sddl_is_too_open, sddl_of};

        let base = std::env::temp_dir().join(format!("tylluan_d5013_ps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("crear carpeta de prueba");
        let padre = base.join("padre");
        std::fs::create_dir_all(&padre).expect("crear el padre");
        // La ACE heredable de BUILTIN\Usuarios por SID (lo que hereda la raiz de `E:`), igual
        // que en `d5012_tests`: por SID para no depender del idioma del sistema.
        let salida = std::process::Command::new(r"C:\Windows\System32\icacls.exe")
            .arg(&padre)
            .args(["/grant", "*S-1-5-32-545:(OI)(CI)R"])
            .output()
            .expect("ejecutar icacls");
        assert!(salida.status.success(), "no pude abrir el padre de prueba");

        // CONTROL: otro directorio del MISMO padre, escrito a pelo, sí queda abierto. Sin esto
        // el escenario no representa el problema y el test pasaría sin probar nada (D5011c).
        let control = padre.join("a-pelo");
        std::fs::create_dir_all(&control).expect("crear el dir de control");
        std::fs::write(control.join("providers.enc"), "x").expect("crear el fichero de control");
        let sddl_ctl = sddl_of(&control.join("providers.enc")).expect("Get-Acl del control");
        assert!(
            sddl_is_too_open(&sddl_ctl),
            "control: escrito a pelo bajo este padre debe quedar abierto, SDDL: {sddl_ctl}"
        );

        // Lo que crea el código: carpeta NUEVA + almacén NUEVO, con la misma maquinaria del DATA_DIR.
        let dir = padre.join("secrets");
        let mut mapa = BTreeMap::new();
        mapa.insert("openai".to_string(), "sk-secreto-de-prueba".to_string());
        write_map(&dir, &KEY, &mapa).expect("escribir el almacen");

        let sddl_dir = sddl_of(&dir).expect("Get-Acl de la carpeta");
        assert!(
            !sddl_is_too_open(&sddl_dir),
            "R4: la carpeta de secretos nueva no puede heredar el padre abierto, SDDL: {sddl_dir}"
        );
        assert!(
            sddl_dir.contains("OICI"),
            "el cierre de la carpeta debe ser heredable para que `providers.enc` lo herede: {sddl_dir}"
        );

        let enc = data_path(&dir);
        let sddl_enc = sddl_of(&enc).expect("Get-Acl de providers.enc");
        assert!(
            !sddl_is_too_open(&sddl_enc),
            "R4: providers.enc no puede quedar con la ACL heredada, SDDL: {sddl_enc}"
        );
        assert!(
            !dir.join("providers.enc.tmp").exists(),
            "el temporal no puede quedar suelto despues del rename"
        );

        // El round-trip sigue funcionando: endurecer no rompió la escritura ni el rename.
        let leido = read_map(&dir, &KEY).expect("releer el almacen");
        assert_eq!(leido.get("openai").map(String::as_str), Some("sk-secreto-de-prueba"));

        let _ = std::fs::remove_dir_all(&base);
    }

    /// D5013b (T5278, mutacion J del TL): el test de arriba crea la carpeta con
    /// `ensure_private_data_dir`, asi que nace cerrada y `providers.enc` hereda ese cierre — la
    /// llamada a `harden_new_secret` **sobre el destino del `rename`** (`:309`) no se ejercita y
    /// la mutacion J sobrevive. Aqui la carpeta de secretos **ya existe y esta abierta** (ACE
    /// heredable `*S-1-5-32-545:(OI)(CI)R`, creada a pelo, sin pasar por `ensure_private_data_dir`)
    /// y hay un `providers.enc.tmp` **huerfano y abierto** de un proceso reventado: al existir,
    /// `write_private_file` no lo endurece (`nuevo = false`) y el `rename` se lleva esa ACL
    /// abierta. Lo unico que cierra el almacen es la llamada sobre el destino nuevo.
    #[test]
    fn d5013b_almacen_nuevo_sobre_carpeta_abierta_se_cierra_por_el_fichero() {
        // El test existe siempre; solo el cuerpo es de Windows (paridad de recuentos, gate CI).
        #[cfg(windows)]
        d5013b_cuerpo_carpeta_abierta();
    }

    #[cfg(windows)]
    fn d5013b_cuerpo_carpeta_abierta() {
        use crate::config::{sddl_is_too_open, sddl_of};

        let base = std::env::temp_dir().join(format!("tylluan_d5013b_ps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("crear carpeta de prueba");
        let padre = base.join("padre");
        std::fs::create_dir_all(&padre).expect("crear el padre");
        let salida = std::process::Command::new(r"C:\Windows\System32\icacls.exe")
            .arg(&padre)
            .args(["/grant", "*S-1-5-32-545:(OI)(CI)R"])
            .output()
            .expect("ejecutar icacls");
        assert!(salida.status.success(), "no pude abrir el padre de prueba");

        // La carpeta de secretos YA EXISTE y se crea A PELO: no pasa por `ensure_private_data_dir`,
        // y `write_map` no reescribe la ACL de un almacen que ya existe (politica de Jose, T5266).
        let dir = padre.join("secrets");
        std::fs::create_dir_all(&dir).expect("crear la carpeta de secretos a pelo");

        // CONTROL: un fichero normal en ESA carpeta hereda lo abierto. Sin esto el escenario no
        // representa el problema y el test pasaria sin probar nada (D5011c).
        let control = dir.join("a-pelo.txt");
        std::fs::write(&control, "x").expect("crear el fichero de control");
        let sddl_ctl = sddl_of(&control).expect("Get-Acl del control");
        assert!(
            sddl_is_too_open(&sddl_ctl),
            "control: escrito a pelo en esa carpeta debe quedar abierto, SDDL: {sddl_ctl}"
        );
        let sddl_dir = sddl_of(&dir).expect("Get-Acl de la carpeta");
        assert!(
            sddl_is_too_open(&sddl_dir),
            "precondicion: la carpeta de secretos debe estar abierta para que el test valga, SDDL: {sddl_dir}"
        );

        // Temporal huerfano: existe ANTES y esta abierto, asi que `write_private_file` no lo
        // endurece (`nuevo = false`) y el `rename` arrastra esa ACL al destino.
        let tmp = dir.join("providers.enc.tmp");
        std::fs::write(&tmp, "{\"huerfano\":1}").expect("crear el .enc.tmp huerfano");
        let sddl_tmp = sddl_of(&tmp).expect("Get-Acl del .enc.tmp");
        assert!(
            sddl_is_too_open(&sddl_tmp),
            "precondicion: el .enc.tmp huerfano debe nacer abierto, SDDL: {sddl_tmp}"
        );
        assert!(!data_path(&dir).exists(), "precondicion: providers.enc debe ser nuevo");

        // Y ahi se escribe un almacen NUEVO.
        let mut mapa = BTreeMap::new();
        mapa.insert("openai".to_string(), "sk-secreto-de-prueba".to_string());
        write_map(&dir, &KEY, &mapa).expect("escribir el almacen");

        let enc = data_path(&dir);
        assert!(enc.exists(), "el almacen nuevo debe quedar escrito");
        let sddl_enc = sddl_of(&enc).expect("Get-Acl de providers.enc");
        assert!(
            !sddl_is_too_open(&sddl_enc),
            "D5013b (mutacion J): providers.enc nuevo sobre una carpeta abierta y con un .enc.tmp \
             huerfano debe cerrarse por `harden_new_secret` sobre el destino del rename, SDDL: {sddl_enc}"
        );
        assert!(
            !tmp.exists(),
            "el temporal huerfano no puede quedar suelto despues del rename"
        );

        // Endurecer el destino no rompió el round-trip.
        let leido = read_map(&dir, &KEY).expect("releer el almacen");
        assert_eq!(leido.get("openai").map(String::as_str), Some("sk-secreto-de-prueba"));

        let _ = std::fs::remove_dir_all(&base);
    }
    // ── D5013b-2 (turno 5287 del TL, punto 1) ───────────────────────────────────────────
    // `write_private_file` endurece el temporal **solo si es nuevo** y en Unix `OpenOptions::mode`
    // solo rige al CREAR. Si queda un `providers.enc.tmp` huerfano de un proceso reventado, el
    // huesped NO se cierra y el `rename` se lleva esa ACL/modo al almacen: un `providers.enc` que
    // ya estaba cerrado pasa a quedar abierto, y el endurecido del destino tampoco lo cubre porque
    // `destino_nuevo` es `false`. Los dos tests de abajo fallan sin el arreglo de esta ronda.

    /// Windows: el padre abierto con la ACE heredable por SID (no depende del idioma), igual que
    /// en `d5013_tests` y `d5012_tests`.
    #[cfg(windows)]
    fn d5013b2_padre_abierto(dir: &std::path::Path) {
        let salida = std::process::Command::new(r"C:\Windows\System32\icacls.exe")
            .arg(dir)
            .args(["/grant", "*S-1-5-32-545:(OI)(CI)R"])
            .output()
            .expect("ejecutar icacls");
        assert!(salida.status.success(), "no pude abrir el padre de prueba");
    }

    #[cfg(unix)]
    fn d5013b2_modo(p: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777
    }

    /// Unidad: escribir sobre un huesped ABIERTO tiene que dejar el fichero CERRADO. Hoy no lo
    /// hace: `nuevo = false` salta el endurecido y el huesped conserva su ACL/modo abierto.
    #[test]
    fn d5013b_2_escribir_sobre_un_huesped_abierto_lo_deja_cerrado() {
        let base = std::env::temp_dir().join(format!("tylluan_d5013b2a_ps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("crear carpeta de prueba");
        let huesped = base.join("providers.enc.tmp");

        #[cfg(windows)]
        {
            use crate::config::{sddl_is_too_open, sddl_of};
            d5013b2_padre_abierto(&base);
            std::fs::write(&huesped, "{\"huerfano\":1}").expect("crear el huesped");
            let sddl = sddl_of(&huesped).expect("Get-Acl del huesped");
            assert!(
                sddl_is_too_open(&sddl),
                "precondicion: el huesped debe nacer abierto, SDDL: {sddl}"
            );

            write_private_file(&huesped, b"{\"openai\":\"sk-secreto-de-prueba\"}").expect("escribir");

            let sddl2 = sddl_of(&huesped).expect("Get-Acl tras escribir");
            assert!(
                !sddl_is_too_open(&sddl2),
                "D5013b-2: escribir sobre un .enc.tmp huerfano y abierto debe cerrarlo antes de \
                 que el rename se lleve esa ACL al almacen, SDDL: {sddl2}"
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::write(&huesped, "{\"huerfano\":1}").expect("crear el huesped");
            std::fs::set_permissions(&huesped, std::fs::Permissions::from_mode(0o644))
                .expect("abrir el huesped");
            assert_eq!(d5013b2_modo(&huesped), 0o644, "precondicion: el huesped esta 0644");

            write_private_file(&huesped, b"contenido").expect("escribir");

            let modo = d5013b2_modo(&huesped);
            assert_eq!(
                modo, 0o600,
                "D5013b-2: `mode(0o600)` solo rige al crear; hay que fijar el modo tambien sobre \
                 un huesped, modo real: {modo:o}"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Extremo: almacen **ya existente y cerrado** + `.enc.tmp` huerfano abierto. Es el caso que
    /// no cubria ninguno de los dos tests de D5013: el destino no es nuevo, asi que alli no se
    /// llama a `harden_new_secret`, y el `rename` sustituye la DACL cerrada por la del huesped.
    #[test]
    fn d5013b_2_almacen_existente_con_tmp_huerfanoabierto_sigue_cerrado() {
        let base = std::env::temp_dir().join(format!("tylluan_d5013b2b_ps_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("crear carpeta de prueba");

        #[cfg(windows)]
        {
            use crate::config::{sddl_is_too_open, sddl_of};
            d5013b2_padre_abierto(&base);
            // carpeta de secretos ya existente, creada a pelo (abierta): DATA_DIR anterior a D5012
            let dir = base.join("secrets");
            std::fs::create_dir_all(&dir).expect("crear la carpeta de secretos a pelo");

            // (1) el almacen se escribe con la maquinaria de D5013 y nace CERRADO
            let mut mapa = BTreeMap::new();
            mapa.insert("openai".to_string(), "sk-primera".to_string());
            write_map(&dir, &KEY, &mapa).expect("crear el almacen");
            let enc = data_path(&dir);
            let sddl_antes = sddl_of(&enc).expect("Get-Acl de providers.enc");
            assert!(
                !sddl_is_too_open(&sddl_antes),
                "precondicion: el almacen recien creado debe estar cerrado, SDDL: {sddl_antes}"
            );

            // (2) aparece el huesped abierto de un proceso reventado
            let tmp = dir.join("providers.enc.tmp");
            std::fs::write(&tmp, "{\"huerfano\":1}").expect("crear el .enc.tmp huerfano");
            let sddl_tmp = sddl_of(&tmp).expect("Get-Acl del huesped");
            assert!(
                sddl_is_too_open(&sddl_tmp),
                "precondicion: el .enc.tmp huerfano debe nacer abierto, SDDL: {sddl_tmp}"
            );
            assert!(enc.exists(), "precondicion: el almacen ya existe");

            // (3) sobrescritura normal (set_secret / delete_secret sobre almacen existente)
            mapa.insert("openai".to_string(), "sk-segunda".to_string());
            write_map(&dir, &KEY, &mapa).expect("sobrescribir el almacen");

            let sddl_ahora = sddl_of(&enc).expect("Get-Acl tras el rename");
            assert!(
                !sddl_is_too_open(&sddl_ahora),
                "D5013b-2: un .enc.tmp huerfano y abierto no puede dejar abierto un almacen que \
                 ya estaba cerrado. Antes: {sddl_antes} | ahora: {sddl_ahora}"
            );
            assert!(!tmp.exists(), "el temporal huerfano no puede quedar suelto");
            let leido = read_map(&dir, &KEY).expect("releer el almacen");
            assert_eq!(leido.get("openai").map(String::as_str), Some("sk-segunda"));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir = base.join("secrets");
            std::fs::create_dir_all(&dir).expect("crear la carpeta de secretos");
            let mut mapa = BTreeMap::new();
            mapa.insert("openai".to_string(), "sk-primera".to_string());
            write_map(&dir, &KEY, &mapa).expect("crear el almacen");
            let enc = data_path(&dir);
            assert_eq!(d5013b2_modo(&enc), 0o600, "precondicion: el almacen nace 0600");

            let tmp = dir.join("providers.enc.tmp");
            std::fs::write(&tmp, "{\"huerfano\":1}").expect("crear el .enc.tmp huerfano");
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o644))
                .expect("abrir el huesped");
            assert_eq!(d5013b2_modo(&tmp), 0o644, "precondicion: el huesped esta 0644");

            mapa.insert("openai".to_string(), "sk-segunda".to_string());
            write_map(&dir, &KEY, &mapa).expect("sobrescribir el almacen");
            // En Unix el `restrict_permissions` del destino cubre el descanso; lo que abre el
            // hueco es la ventana del huesped, que se cierra con el arreglo de esta ronda.
            assert_eq!(
                d5013b2_modo(&enc), 0o600,
                "el 0644 del huesped se colo por el rename hasta el almacen"
            );
            assert!(!tmp.exists(), "el temporal huerfano no puede quedar suelto");
        }
        let _ = std::fs::remove_dir_all(&base);
    }
}
