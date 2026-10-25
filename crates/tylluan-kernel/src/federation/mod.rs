use serde::{Deserialize, Serialize};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;
use rusqlite::params;
use std::sync::Arc;
use tylluan_link::identity::NodeIdentity;

pub mod permissions;

pub use permissions::PeerPermissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationPeer {
    pub name: String,
    pub url: String,
    /// HTTP bearer token used to authenticate TO this peer (and FROM this peer on receive).
    #[serde(alias = "token")]
    pub auth_token: String,
    /// ChaCha20-Poly1305 encryption key. Defaults to auth_token when empty (backwards compat).
    #[serde(default)]
    pub shared_secret: String,
    pub last_sync: Option<i64>,
    #[serde(default)]
    pub approved: bool,
    #[serde(default)]
    pub added_at: u64,
    /// Ed25519 public key (hex) fetched from peer's /api/v1/federation/identity on approval.
    #[serde(default)]
    pub ed25519_pubkey: String,
    /// External IP:port discovered via STUN, used for hole-punching direct connections.
    /// Format: "ip:port" e.g. "203.0.113.5:3001". Empty string means unknown.
    #[serde(default)]
    pub external_address: String,
    /// ADR-019 step 2: per-peer and per-space permissions. Fail-closed:
    /// default is deny-all. Stored as JSON TEXT in `federation_peers.permissions`.
    #[serde(default)]
    pub permissions: PeerPermissions,
}

impl FederationPeer {
    /// Key used for ChaCha20-Poly1305 payload encryption.
    /// Uses shared_secret when set; falls back to auth_token for backwards compat.
    pub fn encryption_key(&self) -> &str {
        if self.shared_secret.is_empty() {
            &self.auth_token
        } else {
            &self.shared_secret
        }
    }
}

// --- SQL persistence layer --------------------------------------------------

pub struct PeerDb {
    conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

/// SQLite reports an already-applied `ALTER TABLE ... ADD COLUMN` with
/// "duplicate column name". That is the ONLY expected error of the three safe
/// migrations; anything else must be warned (T-DSH-75).
fn is_duplicate_column_error(message: &str) -> bool {
    message.contains("duplicate column name")
}

impl PeerDb {
    pub fn open(db_path: &str) -> anyhow::Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))?;
         conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS federation_peers (
                 name          TEXT PRIMARY KEY,
                 url           TEXT NOT NULL,
                 auth_token    TEXT NOT NULL,
                 shared_secret TEXT NOT NULL DEFAULT '',
                 approved      INTEGER NOT NULL DEFAULT 0,
                 last_sync     INTEGER,
                 added_at      INTEGER NOT NULL,
                   ed25519_pubkey TEXT NOT NULL DEFAULT '',
                   external_address TEXT NOT NULL DEFAULT '',
                   permissions   TEXT NOT NULL DEFAULT '{}'
              );",
        )?;
        // Safe migrations: add columns if they don't exist yet (existing databases).
        // T-DSH-75: "duplicate column name" is the only EXPECTED error here; any
        // other failure (locked DB, column ceiling, corrupt schema...) is logged
        // non-fatally instead of being discarded.
        for alter_sql in [
            "ALTER TABLE federation_peers ADD COLUMN ed25519_pubkey TEXT NOT NULL DEFAULT '';",
            "ALTER TABLE federation_peers ADD COLUMN external_address TEXT NOT NULL DEFAULT '';",
            // ADR-019 step 2: existing rows default to deny-all (DEFAULT '{}').
            "ALTER TABLE federation_peers ADD COLUMN permissions TEXT NOT NULL DEFAULT '{}';",
        ] {
            if let Err(error) = conn.execute_batch(alter_sql)
                && !is_duplicate_column_error(&error.to_string())
            {
                tracing::warn!(
                    error = %error,
                    sql = alter_sql,
                    "federation: safe migration failed (non-fatal; continuing)"
                );
            }
        }
        Ok(Self { conn: Arc::new(std::sync::Mutex::new(conn)) })
    }

    pub fn insert(&self, peer: &FederationPeer) -> rusqlite::Result<()> {
        let added_at = if peer.added_at > 0 {
            peer.added_at as i64
        } else {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        };
        // NOTE: `permissions` is intentionally NOT in the ON CONFLICT update —
        // re-adding a peer (TOML seed, mDNS) must never clobber permissions set
        // via the dedicated endpoint.
        self.conn.lock().expect("peers db mutex").execute(
             "INSERT INTO federation_peers(name,url,auth_token,shared_secret,approved,last_sync,added_at,ed25519_pubkey,external_address,permissions)
              VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
              ON CONFLICT(name) DO UPDATE SET
                  url=excluded.url,
                  auth_token=excluded.auth_token,
                  shared_secret=excluded.shared_secret,
                  approved=excluded.approved,
                  ed25519_pubkey=excluded.ed25519_pubkey,
                  external_address=excluded.external_address",
            params![
                peer.name, peer.url, peer.auth_token, peer.shared_secret,
                peer.approved as i64, peer.last_sync, added_at, peer.ed25519_pubkey,
                peer.external_address, permissions::permissions_to_json(&peer.permissions)
            ],
        )?;
        Ok(())
    }

    /// ADR-019 step 2: persist the permissions JSON for a peer. Returns false
    /// when the peer does not exist.
    pub fn update_permissions(&self, name: &str, permissions_json: &str) -> rusqlite::Result<bool> {
        let n = self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET permissions=?2 WHERE name=?1",
            params![name, permissions_json],
        )?;
        Ok(n > 0)
    }

    pub fn remove(&self, name: &str) -> rusqlite::Result<bool> {
        let n = self.conn.lock().expect("peers db mutex").execute(
            "DELETE FROM federation_peers WHERE name=?1",
            params![name],
        )?;
        Ok(n > 0)
    }

    pub fn update_approved(
        &self,
        name: &str,
        auth_token: &str,
        shared_secret: Option<&str>,
        approved: bool,
    ) -> rusqlite::Result<bool> {
        let secret = shared_secret.unwrap_or("");
        let n = self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET auth_token=?2, shared_secret=?3, approved=?4 WHERE name=?1",
            params![name, auth_token, secret, approved as i64],
        )?;
        Ok(n > 0)
    }

    /// D-56: cambia solo `approved`, conservando token/secreto/pubkey. Se usa
    /// para revertir una aprobación si falla una escritura posterior.
    pub fn set_approved(&self, name: &str, approved: bool) -> rusqlite::Result<bool> {
        let n = self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET approved=?2 WHERE name=?1",
            params![name, approved as i64],
        )?;
        Ok(n > 0)
    }

    pub fn update_external_address(&self, name: &str, addr: &str) -> rusqlite::Result<bool> {
        let n = self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET external_address=?2 WHERE name=?1",
            params![name, addr],
        )?;
        Ok(n > 0)
    }

    pub fn update_ed25519_pubkey(&self, name: &str, pubkey: &str) -> rusqlite::Result<bool> {
        let n = self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET ed25519_pubkey=?2 WHERE name=?1",
            params![name, pubkey],
        )?;
        Ok(n > 0)
    }

    pub fn update_last_sync(&self, name: &str, ts: i64) -> rusqlite::Result<()> {
        self.conn.lock().expect("peers db mutex").execute(
            "UPDATE federation_peers SET last_sync=?2 WHERE name=?1",
            params![name, ts],
        )?;
        Ok(())
    }

    pub fn load_all(&self) -> rusqlite::Result<Vec<FederationPeer>> {
        let conn = self.conn.lock().expect("peers db mutex");
        let mut stmt = conn.prepare(
            "SELECT name,url,auth_token,shared_secret,approved,last_sync,added_at,ed25519_pubkey,external_address,permissions
             FROM federation_peers",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FederationPeer {
                name: row.get(0)?,
                url: row.get(1)?,
                auth_token: row.get(2)?,
                shared_secret: row.get(3)?,
                approved: row.get::<_, i64>(4)? != 0,
                last_sync: row.get(5)?,
                added_at: row.get::<_, i64>(6)? as u64,
                ed25519_pubkey: row.get::<_, String>(7).unwrap_or_default(),
                external_address: row.get::<_, String>(8).unwrap_or_default(),
                // Fail-closed: invalid stored JSON degrades to deny-all with a warn.
                permissions: permissions::parse_permissions(
                    &row.get::<_, String>(9).unwrap_or_default(),
                ),
            })
        })?;
        rows.collect()
    }
}

// --- Crypto -----------------------------------------------------------------

/// Encrypts payload with ChaCha20-Poly1305. Key = first 32 bytes of SHA-256(shared_secret).
pub fn encrypt_payload(data: &[u8], shared_secret: &str) -> anyhow::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    let key_bytes = Sha256::digest(shared_secret.as_bytes());
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, data)
        .map_err(|e| anyhow::anyhow!("encrypt failed: {e}"))?;
    let mut result = nonce_bytes.to_vec();
    result.extend_from_slice(&ciphertext);
    Ok(result)
}

pub fn decrypt_payload(data: &[u8], shared_secret: &str) -> anyhow::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    if data.len() < 12 {
        anyhow::bail!("payload too short");
    }
    let key_bytes = Sha256::digest(shared_secret.as_bytes());
    let cipher = ChaCha20Poly1305::new_from_slice(&key_bytes)?;
    let nonce = Nonce::from_slice(&data[..12]);
    cipher
        .decrypt(nonce, &data[12..])
        .map_err(|e| anyhow::anyhow!("decrypt failed: {e}"))
}

// --- Crypto dispatch: Noise NK when pubkey available, ChaCha20 fallback ----------

/// Encrypt payload using best available crypto.
/// Uses Noise NK when this peer knows the target's Ed25519 pubkey (forward secrecy),
/// falls back to ChaCha20-Poly1305 with shared_secret for legacy peers.
pub fn encrypt_for_peer(
    data: &[u8],
    identity: &NodeIdentity,
    peer: &FederationPeer,
) -> anyhow::Result<Vec<u8>> {
    if !peer.ed25519_pubkey.is_empty() {
        tylluan_link::noise::noise_encrypt_payload(data, identity, &peer.ed25519_pubkey)
    } else {
        encrypt_payload(data, peer.encryption_key())
    }
}

/// Decrypt payload using best available crypto.
/// Matches the encryption method used by the sender.
pub fn decrypt_from_peer(
    data: &[u8],
    identity: &NodeIdentity,
    peer: &FederationPeer,
) -> anyhow::Result<Vec<u8>> {
    if !peer.ed25519_pubkey.is_empty() {
        tylluan_link::noise::noise_decrypt_payload(data, identity, &peer.ed25519_pubkey)
    } else {
        decrypt_payload(data, peer.encryption_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let data = b"hello federation";
        let secret = "test-secret-42";
        let enc = encrypt_payload(data, secret).expect("encrypt failed");
        let dec = decrypt_payload(&enc, secret).expect("decrypt failed");
        assert_eq!(dec, data);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let data = b"sensitive data";
        let enc = encrypt_payload(data, "correct-key").unwrap();
        assert!(decrypt_payload(&enc, "wrong-key").is_err());
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let data = b"same input";
        let e1 = encrypt_payload(data, "key").unwrap();
        let e2 = encrypt_payload(data, "key").unwrap();
        assert_ne!(e1, e2);
    }

    #[test]
    fn test_encryption_key_fallback() {
        let peer = FederationPeer {
            name: "test".into(),
            url: "http://x".into(),
            auth_token: "auth-tok".into(),
            shared_secret: "".into(),
            last_sync: None,
            approved: true,
            added_at: 0,
            ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        assert_eq!(peer.encryption_key(), "auth-tok", "empty shared_secret falls back to auth_token");
    }

    #[test]
    fn test_encryption_key_uses_shared_secret() {
        let peer = FederationPeer {
            name: "test".into(),
            url: "http://x".into(),
            auth_token: "auth-tok".into(),
            shared_secret: "my-secret".into(),
            last_sync: None,
            approved: true,
            added_at: 0,
            ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        assert_eq!(peer.encryption_key(), "my-secret");
    }

    #[test]
    fn test_peer_db_roundtrip() {
        let db = PeerDb::open(":memory:").expect("in-memory db");
        let peer = FederationPeer {
            name: "alice".into(),
            url: "http://alice:3000".into(),
            auth_token: "tok-alice".into(),
            shared_secret: "secret-alice".into(),
            last_sync: None,
            approved: true,
            added_at: 1000,
            ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        db.insert(&peer).expect("insert");
        let loaded = db.load_all().expect("load_all");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "alice");
        assert_eq!(loaded[0].auth_token, "tok-alice");
        assert_eq!(loaded[0].shared_secret, "secret-alice");
        assert!(loaded[0].approved);
    }

    #[test]
    fn test_peer_db_remove() {
        let db = PeerDb::open(":memory:").expect("in-memory db");
        let peer = FederationPeer {
            name: "bob".into(), url: "http://bob:3000".into(),
            auth_token: "tok-bob".into(), shared_secret: "".into(),
            last_sync: None, approved: false, added_at: 0, ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        db.insert(&peer).expect("insert");
        assert!(db.remove("bob").expect("remove"));
        assert!(!db.remove("bob").expect("remove again"));
        assert_eq!(db.load_all().expect("load").len(), 0);
    }

    #[test]
    fn test_peer_db_update_approved() {
        let db = PeerDb::open(":memory:").expect("in-memory db");
        let peer = FederationPeer {
            name: "carol".into(), url: "http://carol:3000".into(),
            auth_token: "old-tok".into(), shared_secret: "".into(),
            last_sync: None, approved: false, added_at: 0, ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        db.insert(&peer).expect("insert");
        let found = db.update_approved("carol", "new-tok", Some("new-secret"), true)
            .expect("update");
        assert!(found);
        let loaded = db.load_all().expect("load");
        assert_eq!(loaded[0].auth_token, "new-tok");
        assert_eq!(loaded[0].shared_secret, "new-secret");
        assert!(loaded[0].approved);
    }

    #[test]
    fn test_peer_db_last_sync() {
        let db = PeerDb::open(":memory:").expect("in-memory db");
        let peer = FederationPeer {
            name: "dave".into(), url: "http://dave:3000".into(),
            auth_token: "tok-dave".into(), shared_secret: "".into(),
            last_sync: None, approved: true, added_at: 0, ed25519_pubkey: "".into(),
            external_address: "".into(), permissions: PeerPermissions::default(),
        };
        db.insert(&peer).expect("insert");
        db.update_last_sync("dave", 9999).expect("update_last_sync");
        let loaded = db.load_all().expect("load");
        assert_eq!(loaded[0].last_sync, Some(9999));
    }

    #[test]
    fn migration_error_classification() {
        assert!(is_duplicate_column_error("duplicate column name: ed25519_pubkey"));
        assert!(!is_duplicate_column_error("too many columns on federation_peers"));
        assert!(!is_duplicate_column_error("database is locked"));
        assert!(!is_duplicate_column_error("no such table: federation_peers"));
    }

    /// T-DSH-75: re-running the safe migrations on an already-migrated DB is
    /// the normal path (all three ALTERs are "duplicate column") and must NOT
    /// emit a warning.
    #[test]
    fn migration_duplicate_column_is_ignored_without_warn() {
        let path = std::env::temp_dir().join(format!("federation_mig_dup_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let path_str = path.to_str().expect("utf8 temp path");

        let captured = crate::test_support::capture_warns(|| {
            let first = PeerDb::open(path_str).expect("first open creates the DB");
            drop(first);
            // Second open re-runs the three safe migrations: all duplicates.
            let second = PeerDb::open(path_str).expect("second open re-runs migrations");
            drop(second);
        });

        let _ = std::fs::remove_file(&path);
        assert!(
            !captured.contains("safe migration failed"),
            "duplicate-column migrations must not warn, got: {captured}"
        );
    }

    /// T-DSH-75: a non-duplicate migration failure (here: SQLite's column
    /// ceiling) is logged non-fatally instead of discarded; `open` still works.
    #[test]
    fn migration_non_duplicate_error_warns() {
        let path = std::env::temp_dir().join(format!("federation_mig_err_{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            // Pre-create `federation_peers` at SQLite's default column ceiling
            // (2000) so ALTER ADD COLUMN fails with "too many columns".
            let raw = rusqlite::Connection::open(&path).expect("raw sqlite open");
            let columns: Vec<String> = (0..2000).map(|i| format!("c{i} INTEGER")).collect();
            raw.execute_batch(&format!("CREATE TABLE federation_peers ({});", columns.join(", ")))
                .expect("pre-create table at column ceiling");
        }
        let path_str = path.to_str().expect("utf8 temp path");

        let captured = crate::test_support::capture_warns(|| {
            let db = PeerDb::open(path_str).expect("migration errors are non-fatal");
            drop(db);
        });

        let _ = std::fs::remove_file(&path);
        assert!(
            captured.contains("safe migration failed"),
            "a non-duplicate ALTER failure must warn, got: {captured}"
        );
    }
}
