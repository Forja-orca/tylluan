//! D-104 (P-3, ADR provisional): bóveda de secretos de servidores MCP externos.
//!
//! Decisión de José (T2624): opción B — referencia `vault:<nombre>` en la config y
//! tabla cifrada en la misma base que ya abre [`crate::config::open_db`], con el
//! mismo punto único de apertura. Rotación documentada, no automatizada.
//!
//! ## Qué garantiza y qué no
//!
//! - **No** guarda nada en claro: el valor se cifra con AEAD (ChaCha20-Poly1305,
//!   ya en el árbol por federación) y el `name` va como AAD, así que un ciphertext
//!   movido a otra fila no descifra.
//! - Con `encrypt_at_rest` sin soporte de cifrado en el binario **se niega a
//!   guardar** (fail-closed, criterio 5 del §3 de D-97): no hay modo "en claro
//!   porque funciona".
//! - **No** protege contra un proceso comprometido: el secreto vive en memoria del
//!   guild mientras corre. Eso queda escrito y no se puede arreglar sin broker.
//!
//! La clave se deriva del material de `TYLLUAN_DB_KEY` (o keychain/fichero) con
//! separación de dominio, para no reutilizar literalmente la clave de SQLCipher.

use anyhow::{bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

/// Separación de dominio: la clave de la bóveda nunca es la clave de SQLCipher.
const VAULT_DOMAIN: &[u8] = b"tylluan-external-secrets-v1";

/// Nombre de la tabla (una sola, sin esquema por guild: el nombre ya namespacea).
const TABLE: &str = "external_secrets";

/// Prefijo de referencia que aparece en `tylluan.toml` / config.
pub const VAULT_REF_PREFIX: &str = "vault:";

/// ¿Es `value` una referencia a la bóveda en vez de un valor en claro?
pub fn is_vault_ref(value: &str) -> bool {
    value.starts_with(VAULT_REF_PREFIX)
}

/// Nombre del secreto referenciado, si `value` es una referencia.
pub fn vault_ref_name(value: &str) -> Option<&str> {
    value
        .strip_prefix(VAULT_REF_PREFIX)
        .filter(|name| !name.is_empty())
}

/// Deriva la clave de contenido de la bóveda a partir de la clave de la base
/// (hex de 64 caracteres) con separación de dominio por SHA-256.
pub fn derive_key(db_key_hex: &str) -> Result<[u8; 32]> {
    let raw = hex_to_bytes(db_key_hex).context("la clave de la base no es hex válida")?;
    let mut hasher = Sha256::new();
    hasher.update(VAULT_DOMAIN);
    hasher.update(raw);
    Ok(hasher.finalize().into())
}

fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("hex inválido ({} caracteres)", hex.len());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(Into::into))
        .collect()
}

/// ¿Puede este binario/config guardar secretos? Fail-closed a propósito.
pub fn vault_available() -> Result<()> {
    if !cfg!(feature = "encryption") {
        bail!(
            "bóveda de secretos deshabilitada: el binario no se compiló con \
             --features encryption, así que no hay cifrado en reposo del que heredar \
             protección. Rebuild con: cargo build --features encryption"
        );
    }
    let cfg = crate::config::TylluanConfig::load_cached()
        .map_err(|e| anyhow::anyhow!("no se pudo cargar la config: {e}"))?;
    let guard = cfg
        .try_read()
        .map_err(|_| anyhow::anyhow!("config en escritura; reintenta"))?;
    if !guard.security.encrypt_at_rest {
        bail!(
            "bóveda de secretos deshabilitada: [security] encrypt_at_rest = false. \
             La bóveda no guarda secretos sin cifrado en reposo (fail-closed)."
        );
    }
    Ok(())
}

/// Crea la tabla si no existe. Idempotente.
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE TABLE IF NOT EXISTS {TABLE} (
            name       TEXT PRIMARY KEY,
            ciphertext BLOB NOT NULL,
            nonce      BLOB NOT NULL,
            updated_at INTEGER NOT NULL
        );"
    ))
    .context("creando external_secrets")?;
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("nombre de secreto vacío");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        bail!("nombre de secreto inválido: {name:?} (usa [a-zA-Z0-9._-])");
    }
    Ok(())
}

fn encrypt(key: &[u8; 32], name: &str, value: &str) -> Result<(Vec<u8>, [u8; 12])> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).context("clave de bóveda inválida")?;
    let mut nonce = [0u8; 12];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut nonce);
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: value.as_bytes(),
                aad: name.as_bytes(),
            },
        )
        .map_err(|_| anyhow::anyhow!("cifrado de secreto falló"))?;
    Ok((ct, nonce))
}

fn decrypt(key: &[u8; 32], name: &str, ct: &[u8], nonce: &[u8]) -> Result<String> {
    if nonce.len() != 12 {
        bail!("nonce con longitud inválida ({})", nonce.len());
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key).context("clave de bóveda inválida")?;
    let pt = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ct,
                aad: name.as_bytes(),
            },
        )
        .map_err(|_| {
            anyhow::anyhow!(
                "el secreto {name:?} no descifra: ciphertext alterado o movido de fila"
            )
        })?;
    String::from_utf8(pt).map_err(|_| anyhow::anyhow!("el secreto {name:?} no es UTF-8"))
}

/// Guarda (o rota) un secreto. **Fail-closed**: exige cifrado en reposo activo.
pub fn set_secret(conn: &Connection, key: &[u8; 32], name: &str, value: &str) -> Result<()> {
    vault_available()?;
    set_secret_unchecked(conn, key, name, value)
}

/// Igual que [`set_secret`] sin la puerta de disponibilidad. Solo para tests y
/// para el camino ya validado por el llamador.
pub(crate) fn set_secret_unchecked(
    conn: &Connection,
    key: &[u8; 32],
    name: &str,
    value: &str,
) -> Result<()> {
    validate_name(name)?;
    if value.is_empty() {
        bail!("valor de secreto vacío para {name:?}");
    }
    ensure_schema(conn)?;
    let (ct, nonce) = encrypt(key, name, value)?;
    conn.execute(
        &format!(
            "INSERT INTO {TABLE} (name, ciphertext, nonce, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
                ciphertext = excluded.ciphertext,
                nonce      = excluded.nonce,
                updated_at = excluded.updated_at"
        ),
        params![
            name,
            ct,
            nonce.as_slice(),
            chrono::Utc::now().timestamp_millis()
        ],
    )
    .context("escribiendo secreto")?;
    Ok(())
}

/// D-104 fase 2: resuelve un mapa (`headers`/`env`) usando la bóveda de la base
/// configurada. Pensado para las rutas HTTP que registran servidores externos en
/// caliente (`api_mcp.rs`), donde no hay contexto de arranque.
///
/// - Sin referencias `vault:` devuelve el mapa tal cual (no abre nada).
/// - Con referencias exige bóveda disponible y secreto existente (fail-closed).
/// - Registra los valores como secretos para la redacción de previews (D-107).
pub fn resolve_with_vault(
    map: &std::collections::HashMap<String, String>,
) -> Result<std::collections::HashMap<String, String>, VaultRefError> {
    // Los valores en claro también son credenciales: se registran para la redacción de
    // previews (D-107) ANTES de decidir si hace falta la bóveda.
    for value in map.values() {
        crate::security::redaction::register_secret_value(value);
    }
    if !map.values().any(|v| is_vault_ref(v)) {
        return Ok(map.clone());
    }
    let prepared = prepare_vault_ctx().map_err(|e| VaultRefError::Missing {
        name: format!("<bóveda no disponible: {e}>"),
    })?;
    let (conn, key) = prepared;
    let resolved = resolve_map(map, |name| get_secret(&conn, &key, name))?;
    for value in resolved.values().chain(map.values()) {
        crate::security::redaction::register_secret_value(value);
    }
    Ok(resolved)
}

/// Abre la bóveda siguiendo el mismo camino que el arranque: config cacheada,
/// `silva.db_path`, `config::open_db` y `db_key_hex` + `derive_key`.
fn prepare_vault_ctx() -> anyhow::Result<(Connection, [u8; 32])> {
    vault_available()?;
    let cfg = crate::config::TylluanConfig::load_cached()
        .map_err(|e| anyhow::anyhow!("config: {e}"))?;
    let silva_path = {
        let guard = cfg
            .try_read()
            .map_err(|_| anyhow::anyhow!("config en escritura"))?;
        std::path::PathBuf::from(&guard.silva.db_path)
    };
    let dir = silva_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let key = derive_key(&crate::config::db_key_hex(dir)?)?;
    let conn = crate::config::open_db(&silva_path)?;
    Ok((conn, key))
}

/// Devuelve el valor en claro si existe.
pub fn get_secret(conn: &Connection, key: &[u8; 32], name: &str) -> Result<Option<String>> {
    ensure_schema(conn)?;
    let row: Option<(Vec<u8>, Vec<u8>)> = conn
        .query_row(
            &format!("SELECT ciphertext, nonce FROM {TABLE} WHERE name = ?1"),
            params![name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        None => Ok(None),
        Some((ct, nonce)) => Ok(Some(decrypt(key, name, &ct, &nonce)?)),
    }
}

/// Nombres guardados (nunca valores): lo único que puede salir por HTTP o logs.
pub fn list_names(conn: &Connection) -> Result<Vec<String>> {
    ensure_schema(conn)?;
    let mut stmt = conn.prepare(&format!("SELECT name FROM {TABLE} ORDER BY name"))?;
    let names = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(names)
}

/// Borra un secreto. Devuelve `true` si existía.
pub fn delete_secret(conn: &Connection, name: &str) -> Result<bool> {
    ensure_schema(conn)?;
    Ok(conn.execute(&format!("DELETE FROM {TABLE} WHERE name = ?1"), params![name])? > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_HEX: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn mem() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn roundtrip_cifra_y_descifra() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        set_secret_unchecked(&conn, &key, "github", "ghp_token_de_prueba_123").unwrap();
        assert_eq!(
            get_secret(&conn, &key, "github").unwrap().as_deref(),
            Some("ghp_token_de_prueba_123")
        );
        assert_eq!(get_secret(&conn, &key, "otro").unwrap(), None);
    }

    #[test]
    fn el_valor_no_aparece_en_claro_en_la_base() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        let marker = "MARCADOR_SECRETO_D104_XYZ";
        set_secret_unchecked(&conn, &key, "api", marker).unwrap();
        // D-104/TL: si no se puede leer la columna, el test debe FALLAR, no pasar
        // con un vector vacío (eso convertía el guardián en decoración).
        let blob: String = conn
            .query_row(
                "SELECT COALESCE(group_concat(hex(ciphertext)), '') FROM external_secrets",
                [],
                |r| r.get(0),
            )
            .expect("la columna ciphertext debe poder leerse");
        assert!(!blob.is_empty(), "no hay ciphertext que inspeccionar");
        let raw: Vec<u8> = blob.into_bytes();
        let marker_hex: String = marker.bytes().map(|b| format!("{b:02x}")).collect();
        let raw_hex = String::from_utf8_lossy(&raw).to_string();
        assert!(
            !raw_hex.contains(&marker_hex),
            "el texto en claro no puede estar en la columna cifrada (hex del marcador presente)"
        );
    }

    #[test]
    fn ciphertext_alterado_no_descifra() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        set_secret_unchecked(&conn, &key, "api", "valor").unwrap();
        conn.execute("UPDATE external_secrets SET ciphertext = x'00' || ciphertext", [])
            .unwrap();
        assert!(get_secret(&conn, &key, "api").is_err());
    }

    #[test]
    fn el_ciphertext_esta_ligado_al_nombre() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        set_secret_unchecked(&conn, &key, "a", "secreto_a").unwrap();
        let (ct, nonce): (Vec<u8>, Vec<u8>) = conn
            .query_row(
                "SELECT ciphertext, nonce FROM external_secrets WHERE name = 'a'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        set_secret_unchecked(&conn, &key, "b", "secreto_b").unwrap();
        conn.execute(
            "UPDATE external_secrets SET ciphertext = ?1, nonce = ?2 WHERE name = 'b'",
            params![ct, nonce],
        )
        .unwrap();
        assert!(
            get_secret(&conn, &key, "b").is_err(),
            "mover el ciphertext a otra fila debe fallar (AAD = nombre)"
        );
    }

    #[test]
    fn nombre_invalido_se_rechaza() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        assert!(set_secret_unchecked(&conn, &key, "", "x").is_err());
        assert!(set_secret_unchecked(&conn, &key, "con espacio", "x").is_err());
        assert!(set_secret_unchecked(&conn, &key, "ok-1.2_3", "x").is_ok());
    }

    /// D-107/TL (2026-11-04): sin ninguna referencia `vault:` los valores en claro de
    /// headers/env deben quedar igualmente registrados para la redacción de previews.
    #[test]
    fn resolve_with_vault_registra_los_valores_en_claro_aunque_no_haya_referencias() {
        let mut m = std::collections::HashMap::new();
        m.insert("X-Api-Key".to_string(), "ClaroSinReferenciaTL2026".to_string());
        let out = resolve_with_vault(&m).expect("sin referencias no abre la bóveda");
        assert_eq!(out, m, "sin referencias el mapa sale intacto");
        let masked = crate::security::redaction::scrub_registered("hola ClaroSinReferenciaTL2026 fin");
        assert!(!masked.contains("ClaroSinReferenciaTL2026"), "debe quedar registrado y enmascarado: {masked}");
    }

    #[test]
    fn referencias_vault_se_reconocen() {
        assert!(is_vault_ref("vault:github"));
        assert_eq!(vault_ref_name("vault:github"), Some("github"));
        assert_eq!(vault_ref_name("vault:"), None);
        assert!(!is_vault_ref("ghp_en_claro"));
    }

    #[test]
    fn lista_solo_nombres_y_borra() {
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        set_secret_unchecked(&conn, &key, "b", "1").unwrap();
        set_secret_unchecked(&conn, &key, "a", "2").unwrap();
        assert_eq!(list_names(&conn).unwrap(), vec!["a".to_string(), "b".to_string()]);
        assert!(delete_secret(&conn, "a").unwrap());
        assert!(!delete_secret(&conn, "a").unwrap());
        assert_eq!(list_names(&conn).unwrap(), vec!["b".to_string()]);
    }

    fn map_of(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn resolve_map_sustituye_solo_las_referencias() {
        let map = map_of(&[
            ("Authorization", "vault:github"),
            ("X-Trace", "en-claro"),
        ]);
        let out = resolve_map(&map, |name| {
            assert_eq!(name, "github");
            Ok(Some("ghp_real".to_string()))
        })
        .unwrap();
        assert_eq!(out["Authorization"], "ghp_real");
        assert_eq!(out["X-Trace"], "en-claro");
        assert!(
            !out.values().any(|v| v.starts_with(VAULT_REF_PREFIX)),
            "no debe quedar ninguna referencia sin resolver"
        );
    }

    #[test]
    fn resolve_map_falla_si_falta_el_secreto() {
        let map = map_of(&[("Authorization", "vault:ausente")]);
        let err = resolve_map(&map, |_| Ok(None)).unwrap_err();
        assert_eq!(
            err,
            VaultRefError::Missing {
                name: "ausente".to_string()
            }
        );
        assert!(err.to_string().contains("ausente"));
    }

    #[test]
    fn resolve_map_rechaza_referencia_vacia() {
        let map = map_of(&[("Authorization", "vault:")]);
        assert_eq!(resolve_map(&map, |_| Ok(None)).unwrap_err(), VaultRefError::InvalidRef);
    }

    #[test]
    fn resolve_map_no_deja_pasar_un_error_de_la_boveda() {
        let map = map_of(&[("Authorization", "vault:roto")]);
        let err = resolve_map(&map, |_| Err(anyhow::anyhow!("ciphertext alterado"))).unwrap_err();
        match err {
            VaultRefError::Missing { name } => assert!(name.contains("roto")),
            other => panic!("se esperaba Missing, fue {other:?}"),
        }
    }

    #[test]
    fn resolve_map_sin_referencias_no_toca_nada() {
        let map = map_of(&[("A", "1"), ("B", "2")]);
        let out = resolve_map(&map, |_| panic!("no debe consultar la bóveda")).unwrap();
        assert_eq!(out, map);
    }

    /// Criterio 5: sin `--features encryption` la bóveda se NIEGA a guardar.
    #[cfg(not(feature = "encryption"))]
    #[test]
    fn sin_cifrado_la_boveda_se_niega() {
        let err = vault_available().unwrap_err().to_string();
        assert!(
            err.contains("--features encryption"),
            "el error debe decir cómo arreglarlo, fue: {err}"
        );
        let conn = mem();
        let key = derive_key(KEY_HEX).unwrap();
        let e = set_secret(&conn, &key, "api", "x").unwrap_err().to_string();
        assert!(e.contains("bóveda"), "mensaje inesperado: {e}");
    }
}

/// Fallo al resolver una referencia `vault:`.
#[derive(Debug, PartialEq, Eq)]
pub enum VaultRefError {
    /// La referencia apunta a un secreto que no existe en la bóveda.
    /// Fail-closed: el llamador NO debe arrancar el servidor con una cabecera vacía.
    Missing { name: String },
    /// `vault:` sin nombre detrás.
    InvalidRef,
}

impl std::fmt::Display for VaultRefError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VaultRefError::Missing { name } => {
                write!(f, "el secreto {name:?} no existe en la bóveda")
            }
            VaultRefError::InvalidRef => write!(f, "referencia vault: sin nombre"),
        }
    }
}

impl std::error::Error for VaultRefError {}

/// Resuelve un mapa de cabeceras/variables: los valores `vault:<nombre>` se
/// sustituyen por el secreto en claro (solo en memoria) y el resto se deja tal
/// cual (los `headers` en claro siguen funcionando). Función pura respecto de la
/// base: el `lookup` lo pone el llamador, así el fail-closed se prueba sin base.
pub fn resolve_map<F>(
    map: &std::collections::HashMap<String, String>,
    mut lookup: F,
) -> Result<std::collections::HashMap<String, String>, VaultRefError>
where
    F: FnMut(&str) -> anyhow::Result<Option<String>>,
{
    let mut out = std::collections::HashMap::with_capacity(map.len());
    for (field, value) in map {
        if is_vault_ref(value) {
            let name = vault_ref_name(value).ok_or(VaultRefError::InvalidRef)?;
            let secret = match lookup(name) {
                Ok(Some(secret)) => secret,
                Ok(None) => {
                    return Err(VaultRefError::Missing {
                        name: name.to_string(),
                    })
                }
                Err(e) => {
                    return Err(VaultRefError::Missing {
                        name: format!("{name} ({e})"),
                    })
                }
            };
            out.insert(field.clone(), secret);
        } else {
            out.insert(field.clone(), value.clone());
        }
    }
    Ok(out)
}
