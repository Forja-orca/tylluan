use std::path::Path;
use std::fs;
use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer};
use rand_core::OsRng;
use sha2::{Sha256, Digest};

const MAGIC: &[u8; 4] = b"TLID";

/// Serializes identity creation WITHIN one process. Cross-process races are
/// handled by `write_atomic` + the post-rename re-read in `generate`; without
/// this lock, several threads in the same process (tests, an in-process
/// multi-boot) could each pass the `path.exists()` check and then diverge on
/// which key they return. Poisoning is irrelevant: the guarded section is
/// pure filesystem work with no shared in-memory state, so we recover the
/// guard instead of propagating a panic.
static IDENTITY_LOCK: Mutex<()> = Mutex::new(());

/// Makes each writer's temp file name unique within the process (the PID
/// alone is not enough when several threads race, as the T1277-CI regression
/// test demonstrated).
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct NodeIdentity {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
    public_key_hex: String,
    node_id: String,
}

impl NodeIdentity {
    pub fn load_or_generate(path: &Path) -> anyhow::Result<Self> {
        Self::load_or_create(path)
    }

    pub fn load_or_create(path: &Path) -> anyhow::Result<Self> {
        let _guard = IDENTITY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if path.exists() {
            Self::read_existing(path)
        } else {
            Self::generate(path)
        }
    }

    /// Parse an existing `identity.key`. Deliberately NEVER regenerates on
    /// corruption: a corrupt key is a hard error (deleting it breaks
    /// federation peers). That policy is exactly why an interrupted write
    /// used to be permanent -- hence the atomic write in `generate`.
    fn read_existing(path: &Path) -> anyhow::Result<Self> {
        let data = fs::read(path)?;
        if data.len() != 68 || &data[0..4] != MAGIC {
            anyhow::bail!(
                "identity.key corrupted — restore from backup or delete to generate new identity \
                 (WARNING: deleting breaks federation peers)"
            );
        }
        let mut secret = [0u8; 32];
        let mut public = [0u8; 32];
        secret.copy_from_slice(&data[4..36]);
        public.copy_from_slice(&data[36..68]);

        let mut keypair_bytes = [0u8; 64];
        keypair_bytes[..32].copy_from_slice(&secret);
        keypair_bytes[32..].copy_from_slice(&public);

        let signing_key = SigningKey::from_keypair_bytes(&keypair_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse identity.key: {e}"))?;

        let verifying_key = signing_key.verifying_key();
        let derived_public = verifying_key.to_bytes();
        if derived_public != public {
            anyhow::bail!("identity.key corrupted — public key does not match secret key");
        }

        let public_key_hex = hex::encode(public);
        let node_id = Self::node_id_from_public(&public);

        tracing::info!("Loaded Ed25519 identity: {}", public_key_hex);

        Ok(Self { signing_key, verifying_key, public_key_hex, node_id })
    }

    fn generate(path: &Path) -> anyhow::Result<Self> {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let public = verifying_key.to_bytes();
        let keypair = signing_key.to_keypair_bytes();

        let mut secret_seed = [0u8; 32];
        secret_seed.copy_from_slice(&keypair[..32]);

        let mut buf = Vec::with_capacity(68);
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&secret_seed);
        buf.extend_from_slice(&public);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Self::write_atomic(path, &buf)?;

        tracing::info!("Generated new Ed25519 identity: {}", hex::encode(public));

        // Race convergence: two first-boot processes that both saw "no file"
        // both generate and rename; the rename order picks a winner on disk.
        // Re-reading guarantees every racer returns the SAME identity -- the
        // one actually stored -- instead of one process signing with a key
        // that no longer exists on disk. Single-writer path: this re-reads
        // exactly the bytes just written.
        Self::read_existing(path)
    }

    /// Write `buf` to `path` atomically: a temp file in the SAME directory
    /// (so `fs::rename` is a same-filesystem atomic operation), 0600 where
    /// the platform supports it, fsync'd before the rename. A reader
    /// therefore never observes a partial file, and a process killed
    /// mid-write leaves only an ignorable `*.tmp.<pid>` behind instead of a
    /// truncated `identity.key` -- which, per `read_existing`, would be a
    /// permanent hard failure until an operator intervenes.
    fn write_atomic(path: &Path, buf: &[u8]) -> anyhow::Result<()> {
        let file_name = path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("identity path has no file name: {}", path.display()))?;
        let tmp_path = path.with_file_name(format!(
            "{}.tmp.{}.{}",
            file_name.to_string_lossy(),
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        // No pre-clean: the name is unique per writer, and a stale temp from a
        // crashed process is ignorable (it is never read back).

        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        {
            let mut f = opts.open(&tmp_path)?;
            f.write_all(buf)?;
            f.sync_all()?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(&tmp_path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&tmp_path, perms);
            }
        }

        fs::rename(&tmp_path, path)?;
        Ok(())
    }

    fn node_id_from_public(public_key: &[u8; 32]) -> String {
        let hash = Sha256::digest(public_key);
        hex::encode(&hash[..16])
    }

    pub fn sign(&self, data: &[u8]) -> Signature {
        self.signing_key.sign(data)
    }

    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }
}

// --- M12-B: Signed federation envelopes ---------------------------------------

use ed25519_dalek::Verifier;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignedEnvelope {
    pub node: serde_json::Value,
    pub signature: String,
    pub signer_pubkey: String,
    pub timestamp: i64,
}

/// Sign a node for federation sync.
/// Message: canonical_json(node) + "|" + timestamp + "|" + signer_pubkey
pub fn sign_node(
    identity: &NodeIdentity,
    node: &serde_json::Value,
) -> anyhow::Result<SignedEnvelope> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let pubkey = identity.public_key_hex().to_string();
    let canonical = serde_json::to_string(node)?;
    let message = format!("{canonical}|{timestamp}|{pubkey}");
    let sig = identity.sign(message.as_bytes());
    Ok(SignedEnvelope {
        node: node.clone(),
        signature: hex::encode(sig.to_bytes()),
        signer_pubkey: pubkey,
        timestamp,
    })
}

/// Verify a signed envelope against a trusted public key.
/// Returns Ok(()) if signature is valid and timestamp is within 300s of now.
pub fn verify_envelope(
    envelope: &SignedEnvelope,
    trusted_pubkey_hex: &str,
) -> anyhow::Result<()> {
    // Anti-replay: ±300s window
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let diff = (envelope.timestamp - now).abs();
    if diff > 300 {
        anyhow::bail!(
            "envelope timestamp {} is outside ±300s window (diff={}s)",
            envelope.timestamp, diff
        );
    }

    // Verify signer matches the trusted peer
    if envelope.signer_pubkey != trusted_pubkey_hex {
        anyhow::bail!(
            "signer pubkey {} does not match trusted peer pubkey {}",
            envelope.signer_pubkey, trusted_pubkey_hex
        );
    }

    let canonical = serde_json::to_string(&envelope.node)?;
    let message = format!("{}|{}|{}", canonical, envelope.timestamp, envelope.signer_pubkey);

    let pubkey_bytes = hex::decode(&envelope.signer_pubkey)?;
    if pubkey_bytes.len() != 32 {
        anyhow::bail!("invalid pubkey length");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&pubkey_bytes);
    let verifying_key = VerifyingKey::from_bytes(&arr)?;

    let sig_bytes = hex::decode(&envelope.signature)?;
    if sig_bytes.len() != 64 {
        anyhow::bail!("invalid signature length");
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);

    verifying_key
        .verify(message.as_bytes(), &signature)
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Barrier};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn tmp_dir(label: &str) -> std::path::PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("tylluan_{label}_{id}"))
    }

    #[test]
    fn test_generate_and_load() {
        let dir = tmp_dir("id_test");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        let identity = NodeIdentity::load_or_create(&path).expect("should generate");
        let node_id = identity.node_id().to_string();
        let pubkey = identity.public_key_hex().to_string();
        assert_eq!(pubkey.len(), 64, "public key hex should be 64 chars");
        assert_eq!(node_id.len(), 32, "node id (SHA256 truncated) should be 32 hex chars");

        let loaded = NodeIdentity::load_or_create(&path).expect("should load");
        assert_eq!(loaded.public_key_hex(), pubkey, "loaded pubkey should match");
        assert_eq!(loaded.node_id(), node_id, "loaded node_id should match");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sign_and_verify() {
        let dir = tmp_dir("id_test_sign");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        let identity = NodeIdentity::load_or_create(&path).expect("should generate");
        let msg = b"hello tylluan mesh";
        let sig = identity.sign(msg);

        use ed25519_dalek::Verifier;
        let result = identity.verifying_key().verify(msg, &sig);
        assert!(result.is_ok(), "signature should verify");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sign_and_verify_envelope() {
        let dir = tmp_dir("id_test_env");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        let identity = NodeIdentity::load_or_create(&path).expect("should generate");
        let node = serde_json::json!({"id": "test-1", "content": "hello", "weight": 1.0});

        let envelope = sign_node(&identity, &node).expect("should sign");
        let result = verify_envelope(&envelope, identity.public_key_hex());
        assert!(result.is_ok(), "verify should pass: {:?}", result.err());

        // Wrong pubkey should fail
        let wrong = verify_envelope(&envelope, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert!(wrong.is_err(), "wrong pubkey should fail");

        // Tampered node should fail
        let mut tampered = envelope.clone();
        tampered.node = serde_json::json!({"id": "tampered"});
        let bad = verify_envelope(&tampered, identity.public_key_hex());
        assert!(bad.is_err(), "tampered node should fail");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_corrupted_file_fails() {
        let dir = tmp_dir("id_test_corrupt");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        fs::write(&path, b"TLIDxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").expect("write corrupted file");
        let result = NodeIdentity::load_or_create(&path);
        assert!(result.is_err(), "corrupted file should fail");

        let _ = fs::remove_dir_all(&dir);
    }

    /// Regression test for the 2026-10-18 CI flake: N threads racing the FIRST
    /// boot on the same path must all converge on ONE identity (the one on
    /// disk), never a mix of self-generated keys, and the atomic write must
    /// leave no temp file behind. Without `write_atomic` + convergence this
    /// fails: every thread sees "no file" and returns its own keypair.
    #[test]
    fn test_concurrent_load_or_create_converges_on_one_identity() {
        let dir = tmp_dir("id_test_concurrent");
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        const N: usize = 16;
        let barrier = Arc::new(Barrier::new(N));
        let pubkeys: Vec<String> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..N)
                .map(|_| {
                    let path = path.clone();
                    let barrier = Arc::clone(&barrier);
                    scope.spawn(move || {
                        barrier.wait();
                        NodeIdentity::load_or_create(&path)
                            .expect("concurrent load_or_create must succeed")
                            .public_key_hex()
                            .to_string()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("worker thread must not panic"))
                .collect()
        });

        let first = &pubkeys[0];
        assert!(
            pubkeys.iter().all(|k| k == first),
            "{N} concurrent callers must converge on ONE identity, got {pubkeys:?}"
        );

        let bytes = fs::read(&path).expect("identity file must exist after the race");
        assert_eq!(bytes.len(), 68, "final file must be a valid 68-byte identity");
        assert_eq!(&bytes[..4], b"TLID", "final file must keep the TLID magic");
        assert_eq!(
            hex::encode(&bytes[36..68]),
            *first,
            "the winner on disk must be the identity every caller returned"
        );

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .expect("dir must be readable")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        assert!(
            leftovers.is_empty(),
            "atomic write must not leave temp files behind: {leftovers:?}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
