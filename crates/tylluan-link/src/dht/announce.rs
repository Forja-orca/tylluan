use crate::identity::NodeIdentity;
use ed25519_dalek::Verifier;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAnnouncement {
    pub node_id: String,
    pub public_key: String,
    pub addr: String,
    pub capabilities: Vec<String>,
    pub tylluan_version: String,
    pub timestamp: i64,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedCapabilities {
    pub capabilities: Vec<String>,
    pub node_id: String,
    pub timestamp: i64,
    pub signature: String,
}

impl PeerAnnouncement {
    pub fn new(
        identity: &NodeIdentity,
        addr: &str,
        capabilities: Vec<String>,
        tylluan_version: &str,
    ) -> Self {
        let timestamp = now_unix();
        let payload = announce_payload(
            identity.node_id(),
            identity.public_key_hex(),
            addr,
            &capabilities,
            timestamp,
        );
        let sig = identity.sign(payload.as_bytes());

        Self {
            node_id: identity.node_id().to_string(),
            public_key: identity.public_key_hex().to_string(),
            addr: addr.to_string(),
            capabilities,
            tylluan_version: tylluan_version.to_string(),
            timestamp,
            signature: hex::encode(sig.to_bytes()),
        }
    }

    pub fn verify(&self) -> anyhow::Result<()> {
        let now = now_unix();
        let diff = (self.timestamp - now).abs();
        if diff > 300 {
            anyhow::bail!(
                "announcement timestamp {} outside ±300s window (diff={}s)",
                self.timestamp,
                diff
            );
        }

        let payload = announce_payload(
            &self.node_id,
            &self.public_key,
            &self.addr,
            &self.capabilities,
            self.timestamp,
        );

        let pubkey_bytes = hex::decode(&self.public_key)?;
        if pubkey_bytes.len() != 32 {
            anyhow::bail!("invalid pubkey length");
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&pubkey_bytes);
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&arr)?;

        let sig_bytes = hex::decode(&self.signature)?;
        if sig_bytes.len() != 64 {
            anyhow::bail!("invalid signature length");
        }
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);

        verifying_key
            .verify(payload.as_bytes(), &signature)
            .map_err(|e| anyhow::anyhow!("announcement signature verification failed: {}", e))
    }

    pub fn encode(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn decode(data: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(data)?)
    }
}

fn announce_payload(
    node_id: &str,
    public_key: &str,
    addr: &str,
    capabilities: &[String],
    timestamp: i64,
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        node_id,
        public_key,
        addr,
        serde_json::to_string(capabilities).unwrap_or_default(),
        timestamp
    )
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity() -> NodeIdentity {
        let dir = std::env::temp_dir().join(format!("tylluan_announce_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("identity.key");
        NodeIdentity::load_or_create(&path).expect("should generate")
    }

    #[test]
    fn test_sign_and_verify_announcement() {
        let identity = test_identity();
        let ann = PeerAnnouncement::new(
            &identity,
            "192.168.1.1:3000",
            vec!["mesh".into(), "federation".into()],
            "0.4.0",
        );
        assert!(ann.verify().is_ok(), "self-signed announcement should verify");
    }

    #[test]
    fn test_tampered_fails() {
        let identity = test_identity();
        let mut ann = PeerAnnouncement::new(
            &identity,
            "192.168.1.1:3000",
            vec!["mesh".into()],
            "0.4.0",
        );
        ann.capabilities = vec!["evil".into()];
        assert!(ann.verify().is_err(), "tampered announcement should fail");
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let identity = test_identity();
        let ann = PeerAnnouncement::new(
            &identity,
            "10.0.0.1:3000",
            vec!["mesh".into()],
            "0.4.0",
        );
        let encoded = ann.encode().expect("should encode");
        let decoded = PeerAnnouncement::decode(&encoded).expect("should decode");
        assert_eq!(decoded.node_id, ann.node_id);
        assert_eq!(decoded.addr, ann.addr);
    }
}
