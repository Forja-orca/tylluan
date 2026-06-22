use serde::{Deserialize, Serialize};
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationPeer {
    pub name: String,
    pub url: String,
    pub token: String,
    pub last_sync: Option<i64>,
    /// Must be explicitly set to true by a human before sync is allowed.
    /// mDNS auto-discovered peers start as false.
    #[serde(default)]
    pub approved: bool,
}

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
}
