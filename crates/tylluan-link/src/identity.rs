use std::path::Path;
use std::fs;
use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer};
use rand_core::OsRng;
use sha2::{Sha256, Digest};

const MAGIC: &[u8; 4] = b"TLID";

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
        if path.exists() {
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
                .map_err(|e| anyhow::anyhow!("Failed to parse identity.key: {}", e))?;

            let verifying_key = signing_key.verifying_key();
            let derived_public = verifying_key.to_bytes();
            if derived_public != public {
                anyhow::bail!("identity.key corrupted — public key does not match secret key");
            }

            let public_key_hex = hex::encode(public);
            let node_id = Self::node_id_from_public(&public);

            tracing::info!("Loaded Ed25519 identity: {}", public_key_hex);

            Ok(Self { signing_key, verifying_key, public_key_hex, node_id })
        } else {
            Self::generate(path)
        }
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
        fs::write(path, &buf)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(path, perms);
            }
        }

        let public_key_hex = hex::encode(public);
        let node_id = Self::node_id_from_public(&public);

        tracing::info!("Generated new Ed25519 identity: {}", public_key_hex);

        Ok(Self { signing_key, verifying_key, public_key_hex, node_id })
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_and_load() {
        let dir = std::env::temp_dir().join(format!("tylluan_id_test_{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("tylluan_id_test_sign_{}", std::process::id()));
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
    fn test_corrupted_file_fails() {
        let dir = std::env::temp_dir().join(format!("tylluan_id_test_corrupt_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("identity.key");

        fs::write(&path, b"TLIDxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx").expect("write corrupted file");
        let result = NodeIdentity::load_or_create(&path);
        assert!(result.is_err(), "corrupted file should fail");

        let _ = fs::remove_dir_all(&dir);
    }
}
