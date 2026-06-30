//! # Noise Protocol — Encrypted Transport Overlay (M14-C)
//!
//! Two modes:
//! - XK (3-message handshake): bidirectional session over TCP (start_p2p_listener/noise_accept)
//! - NK (1-message handshake): stateless encrypt/decrypt for HTTP payloads (noise_encrypt_payload/noise_decrypt_payload)
//!
//! Both use Ed25519→X25519 key conversion and ChaCha20-Poly1305 AEAD.

use crate::identity::NodeIdentity;
use sha2::{Sha256, Digest};
use snow::{Builder, HandshakeState, TransportState};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tracing::info;
use ed25519_dalek::{SigningKey, VerifyingKey};

const NOISE_PARAMS: &str = "Noise_XK_25519_ChaChaPoly_BLAKE2s";

pub fn ed25519_pub_to_x25519(pk: &VerifyingKey) -> [u8; 32] {
    pk.to_montgomery().to_bytes()
}

pub fn ed25519_secret_to_x25519(sk: &SigningKey) -> [u8; 32] {
    // to_scalar_bytes() returns SHA-512(seed)[0..32] without X25519 clamping.
    // Apply RFC 7748 §5 clamping explicitly before use as a Curve25519 scalar.
    let mut scalar = sk.to_scalar_bytes();
    scalar[0] &= 248;   // clear bits 0-2
    scalar[31] &= 127;  // clear bit 7
    scalar[31] |= 64;   // set bit 6
    scalar
}

/// Derive Tylluan node_id (SHA-256[:16] of Ed25519 pubkey) from raw pubkey bytes.
fn node_id_from_ed25519_bytes(pubkey: &[u8; 32]) -> String {
    hex::encode(&Sha256::digest(pubkey)[..16])
}

// ─── Stateless NK payload encryption (for HTTP federation) ──────────────

const NK_PARAMS: &str = "Noise_NK_25519_ChaChaPoly_BLAKE2s";

/// Encrypt a payload for a peer whose Ed25519 public key we know.
/// Uses Noise NK pattern: 1-message handshake with ephemeral key exchange + AEAD.
/// Output format: [ephemeral_x25519_pubkey (32 bytes) + AEAD ciphertext].
/// Provides forward secrecy: ephemeral key is generated fresh each call.
pub fn noise_encrypt_payload(
    data: &[u8],
    identity: &NodeIdentity,
    peer_pubkey_hex: &str,
) -> anyhow::Result<Vec<u8>> {
    let peer_ed = hex::decode(peer_pubkey_hex)
        .map_err(|e| anyhow::anyhow!("invalid peer pubkey hex: {}", e))?;
    if peer_ed.len() != 32 {
        anyhow::bail!("peer pubkey must be 32 bytes");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&peer_ed);
    let peer_pk = VerifyingKey::from_bytes(&arr)
        .map_err(|e| anyhow::anyhow!("invalid peer Ed25519 pubkey: {}", e))?;
    let peer_x_pub = ed25519_pub_to_x25519(&peer_pk);

    let my_sk = ed25519_secret_to_x25519(identity.signing_key());
    let params: snow::params::NoiseParams = NK_PARAMS.parse()
        .map_err(|e| anyhow::anyhow!("invalid Noise params: {}", e))?;

    let mut initiator: HandshakeState = Builder::new(params)
        .local_private_key(&my_sk)
        .map_err(|e| anyhow::anyhow!("Noise local key: {}", e))?
        .remote_public_key(&peer_x_pub)
        .map_err(|e| anyhow::anyhow!("Noise remote key: {}", e))?
        .build_initiator()
        .map_err(|e| anyhow::anyhow!("Noise NK initiator: {}", e))?;

    let mut buf = vec![0u8; data.len() + 100];
    let n = initiator.write_message(data, &mut buf)
        .map_err(|e| anyhow::anyhow!("Noise NK encrypt: {}", e))?;
    buf.truncate(n);
    Ok(buf)
}

/// Decrypt a payload encrypted by `noise_encrypt_payload`.
/// Uses Noise NK pattern: reads ephemeral public key, derives shared key, decrypts.
pub fn noise_decrypt_payload(
    data: &[u8],
    identity: &NodeIdentity,
    peer_pubkey_hex: &str,
) -> anyhow::Result<Vec<u8>> {
    let peer_ed = hex::decode(peer_pubkey_hex)
        .map_err(|e| anyhow::anyhow!("invalid peer pubkey hex: {}", e))?;
    if peer_ed.len() != 32 {
        anyhow::bail!("peer pubkey must be 32 bytes");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&peer_ed);
    let _peer_pk = VerifyingKey::from_bytes(&arr)
        .map_err(|e| anyhow::anyhow!("invalid peer Ed25519 pubkey: {}", e))?;

    let my_sk = ed25519_secret_to_x25519(identity.signing_key());
    let params: snow::params::NoiseParams = NK_PARAMS.parse()
        .map_err(|e| anyhow::anyhow!("invalid Noise params: {}", e))?;

    let mut responder: HandshakeState = Builder::new(params)
        .local_private_key(&my_sk)
        .map_err(|e| anyhow::anyhow!("Noise local key: {}", e))?
        .build_responder()
        .map_err(|e| anyhow::anyhow!("Noise NK responder: {}", e))?;

    let mut buf = vec![0u8; data.len() + 100];
    let n = responder.read_message(data, &mut buf)
        .map_err(|e| anyhow::anyhow!("Noise NK decrypt: {}", e))?;
    buf.truncate(n);
    Ok(buf)
}

/// AEAD-encrypted session over Noise Protocol TransportState.
/// A single TransportState handles both encrypt (write_message) and decrypt (read_message)
/// with separate nonce counters per direction.
pub struct NoiseSession {
    state: TransportState,
}

impl NoiseSession {
    pub fn new(state: TransportState) -> Self {
        Self { state }
    }

    pub fn encrypt(&mut self, plaintext: &[u8], out: &mut [u8]) -> Result<usize, snow::Error> {
        self.state.write_message(plaintext, out)
    }

    pub fn decrypt(&mut self, ciphertext: &[u8], out: &mut [u8]) -> Result<usize, snow::Error> {
        self.state.read_message(ciphertext, out)
    }

    /// Encrypt and write with length-prefixed framing to an async TCP stream.
    pub async fn async_encrypt_write(
        &mut self,
        io: &mut OwnedWriteHalf,
        plaintext: &[u8],
    ) -> std::io::Result<()> {
        let mut buf = vec![0u8; plaintext.len() + 100];
        let n = self.state.write_message(plaintext, &mut buf)
            .map_err(std::io::Error::other)?;
        let len = (n as u16).to_be_bytes();
        io.write_all(&len).await?;
        io.write_all(&buf[..n]).await?;
        io.flush().await?;
        Ok(())
    }

    /// Read length-prefixed frame and decrypt from an async TCP stream.
    pub async fn async_decrypt_read(
        &mut self,
        io: &mut OwnedReadHalf,
    ) -> std::io::Result<Vec<u8>> {
        let mut len_buf = [0u8; 2];
        io.read_exact(&mut len_buf).await?;
        let frame_len = u16::from_be_bytes(len_buf) as usize;
        validate_frame_len(frame_len)?;
        let mut frame = vec![0u8; frame_len];
        io.read_exact(&mut frame).await?;
        let mut out = vec![0u8; frame_len + 16];
        let n = self.state.read_message(&frame, &mut out)
            .map_err(std::io::Error::other)?;
        out.truncate(n);
        Ok(out)
    }
}

fn validate_frame_len(frame_len: usize) -> std::io::Result<()> {
    if frame_len == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid Noise frame length"));
    }
    Ok(())
}

/// A fully established Noise-encrypted connection.
pub struct NoisedPipe {
    pub session: NoiseSession,
    pub peer_id: String,
    pub peer_addr: SocketAddr,
}

/// Perform server-side (responder) Noise XK handshake.
/// Returns the session (with single TransportState handling both tx/rx) and peer ID.
pub async fn noise_accept(
    stream: &mut tokio::net::TcpStream,
    identity: &NodeIdentity,
) -> Result<NoisedPipe, anyhow::Error> {
    let sk_bytes = ed25519_secret_to_x25519(identity.signing_key());
    let params: snow::params::NoiseParams = NOISE_PARAMS.parse()
        .map_err(|e| anyhow::anyhow!("invalid Noise params: {}", e))?;

    let mut responder: HandshakeState = Builder::new(params)
        .local_private_key(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Noise local key: {}", e))?
        .build_responder()
        .map_err(|e| anyhow::anyhow!("Noise responder: {}", e))?;

    let mut buf = [0u8; 4096];

    // Read →e, es — first need the length (Noise handshake messages are variable-length,
    // but for XK pattern msg1 is 80 bytes: e(32) + es(48) with ChaChaPoly)
    // Use a length-prefixed framing: initiator writes 2-byte length + message
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await?;
    let msg_len = u16::from_be_bytes(len_buf) as usize;

    let mut msg = vec![0u8; msg_len];
    stream.read_exact(&mut msg).await?;
    let mut _payload = [0u8; 4096];
    responder.read_message(&msg, &mut _payload)
        .map_err(|e| anyhow::anyhow!("Noise read msg1: {}", e))?;

    // Write ←e, ee — with length prefix
    let n = responder.write_message(&[], &mut buf)
        .map_err(|e| anyhow::anyhow!("Noise write msg2: {}", e))?;
    let resp_len = (n as u16).to_be_bytes();
    stream.write_all(&resp_len).await?;
    stream.write_all(&buf[..n]).await?;
    stream.flush().await?;

    // Read →s, se — length-prefixed
    stream.read_exact(&mut len_buf).await?;
    let msg_len = u16::from_be_bytes(len_buf) as usize;
    let mut msg = vec![0u8; msg_len];
    stream.read_exact(&mut msg).await?;
    responder.read_message(&msg, &mut _payload)
        .map_err(|e| anyhow::anyhow!("Noise read msg3: {} (len={})", e, msg_len))?;

    // Extract initiator's X25519 static key before consuming handshake state.
    // In XK the responder learns the initiator's static key during msg3.
    // We use its hex as a stable peer identifier (cannot reconstruct Ed25519 from X25519).
    let peer_id = responder
        .get_remote_static()
        .map(hex::encode)
        .unwrap_or_else(|| "noise-peer".to_string());

    let state = responder.into_transport_mode()
        .map_err(|e| anyhow::anyhow!("Noise transport mode: {}", e))?;

    let peer_addr = stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0)));
    info!("Noise handshake accepted from {} (peer_id: {})", peer_addr, peer_id);
    Ok(NoisedPipe {
        session: NoiseSession::new(state),
        peer_id,
        peer_addr,
    })
}

/// Perform client-side (initiator) Noise XK handshake.
/// `remote_pubkey` is the responder's Ed25519 public key as hex bytes.
pub async fn noise_connect(
    stream: &mut tokio::net::TcpStream,
    identity: &NodeIdentity,
    remote_pubkey_hex: &str,
) -> Result<NoisedPipe, anyhow::Error> {
    let remote_ed_pub = hex::decode(remote_pubkey_hex)
        .map_err(|e| anyhow::anyhow!("invalid hex pubkey: {}", e))?;
    if remote_ed_pub.len() != 32 {
        anyhow::bail!("pubkey must be 32 bytes");
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&remote_ed_pub);
    let remote_pk = VerifyingKey::from_bytes(&arr)
        .map_err(|e| anyhow::anyhow!("invalid Ed25519 pubkey: {}", e))?;
    let remote_x_pub = ed25519_pub_to_x25519(&remote_pk);

    let sk_bytes = ed25519_secret_to_x25519(identity.signing_key());
    let params: snow::params::NoiseParams = NOISE_PARAMS.parse()
        .map_err(|e| anyhow::anyhow!("invalid Noise params: {}", e))?;

    let mut initiator: HandshakeState = Builder::new(params)
        .local_private_key(&sk_bytes)
        .map_err(|e| anyhow::anyhow!("Noise local key: {}", e))?
        .remote_public_key(&remote_x_pub)
        .map_err(|e| anyhow::anyhow!("Noise remote key: {}", e))?
        .build_initiator()
        .map_err(|e| anyhow::anyhow!("Noise initiator: {}", e))?;

    let mut buf = [0u8; 4096];

    // Write →e, es — length-prefixed
    let n = initiator.write_message(&[], &mut buf)
        .map_err(|e| anyhow::anyhow!("Noise write msg1: {}", e))?;
    let msg_len = (n as u16).to_be_bytes();
    stream.write_all(&msg_len).await?;
    stream.write_all(&buf[..n]).await?;
    stream.flush().await?;

    // Read ←e, ee — length-prefixed
    let mut len_buf = [0u8; 2];
    stream.read_exact(&mut len_buf).await?;
    let msg_len = u16::from_be_bytes(len_buf) as usize;
    let mut msg = vec![0u8; msg_len];
    stream.read_exact(&mut msg).await?;
    let mut _payload = [0u8; 4096];
    initiator.read_message(&msg, &mut _payload)
        .map_err(|e| anyhow::anyhow!("Noise read msg2: {}", e))?;

    // Write →s, se — length-prefixed
    let n = initiator.write_message(&[], &mut buf)
        .map_err(|e| anyhow::anyhow!("Noise write msg3: {}", e))?;
    let msg_len = (n as u16).to_be_bytes();
    stream.write_all(&msg_len).await?;
    stream.write_all(&buf[..n]).await?;
    stream.flush().await?;

    let state = initiator.into_transport_mode()
        .map_err(|e| anyhow::anyhow!("Noise transport mode: {}", e))?;

    // Compute peer node_id the same way NodeIdentity does: SHA-256(ed25519_pubkey)[:16].
    let peer_id = node_id_from_ed25519_bytes(&arr);

    let peer_addr = stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0)));
    info!("Noise handshake completed with {} (peer_id: {})", peer_addr, peer_id);
    Ok(NoisedPipe {
        session: NoiseSession::new(state),
        peer_id,
        peer_addr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NOISE_TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn make_identity(label: &str) -> (NodeIdentity, std::path::PathBuf) {
        let id = NOISE_TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tylluan_noise_{}_{}", label, id));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.key");
        let identity = NodeIdentity::load_or_create(&path).expect("should create identity");
        (identity, dir)
    }

    #[test]
    fn test_key_conversion_clamping() {
        let sk = SigningKey::generate(&mut OsRng);
        let pk = sk.verifying_key();
        let x_pub = ed25519_pub_to_x25519(&pk);
        let x_sec = ed25519_secret_to_x25519(&sk);
        assert_eq!(x_pub.len(), 32);
        assert_eq!(x_sec.len(), 32);
        // Verify clamping invariants (RFC 7748 §5)
        assert_eq!(x_sec[0] & 0b111, 0, "low 3 bits of byte[0] must be 0");
        assert_eq!(x_sec[31] & 0b1000_0000, 0, "high bit of byte[31] must be 0");
        assert_ne!(x_sec[31] & 0b0100_0000, 0, "second-highest bit of byte[31] must be 1");
    }

    #[test]
    fn test_node_id_from_ed25519_bytes_is_32_hex_chars() {
        let sk = SigningKey::generate(&mut OsRng);
        let arr = sk.verifying_key().to_bytes();
        let node_id = node_id_from_ed25519_bytes(&arr);
        assert_eq!(node_id.len(), 32, "node_id must be 32 hex chars (16 bytes)");
        assert!(node_id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_noise_handshake_roundtrip() {
        let (server_id, server_dir) = make_identity("hs_s");
        let (client_id, client_dir) = make_identity("hs_c");
        let server_pubkey_hex = server_id.public_key_hex().to_string();
        let expected_server_node_id = server_id.node_id().to_string();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            noise_accept(&mut stream, &server_id).await.unwrap()
        });

        let client_task = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            noise_connect(&mut stream, &client_id, &server_pubkey_hex).await.unwrap()
        });

        let (server_pipe, client_pipe) = tokio::join!(server_task, client_task);
        let client_pipe = client_pipe.unwrap();
        let _ = server_pipe.unwrap();

        // noise_connect knows the remote Ed25519 pubkey → peer_id is the real node_id
        assert_eq!(client_pipe.peer_id, expected_server_node_id,
            "client should identify server by its real node_id");

        let _ = std::fs::remove_dir_all(&server_dir);
        let _ = std::fs::remove_dir_all(&client_dir);
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let (server_id, server_dir) = make_identity("enc_s");
        let (client_id, client_dir) = make_identity("enc_c");
        let server_pubkey_hex = server_id.public_key_hex().to_string();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let msg = b"hello noise world this is a secret message";

        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut pipe = noise_accept(&mut stream, &server_id).await.unwrap();
            let (mut read_half, mut write_half) = stream.into_split();
            let received = pipe.session.async_decrypt_read(&mut read_half).await.unwrap();
            assert_eq!(&received, msg, "server received correct plaintext");
            pipe.session.async_encrypt_write(&mut write_half, b"pong").await.unwrap();
            pipe
        });

        let client_task = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let mut pipe = noise_connect(&mut stream, &client_id, &server_pubkey_hex).await.unwrap();
            let (mut read_half, mut write_half) = stream.into_split();
            pipe.session.async_encrypt_write(&mut write_half, msg).await.unwrap();
            let response = pipe.session.async_decrypt_read(&mut read_half).await.unwrap();
            assert_eq!(response, b"pong", "client received correct response");
            pipe
        });

        let (server_result, client_result) = tokio::join!(server_task, client_task);
        let _ = (server_result.unwrap(), client_result.unwrap());

        let _ = std::fs::remove_dir_all(&server_dir);
        let _ = std::fs::remove_dir_all(&client_dir);
    }

    #[test]
    fn test_noise_nk_encrypt_decrypt_roundtrip() {
        let (alice, alice_dir) = make_identity("nk_a");
        let (bob, bob_dir) = make_identity("nk_b");
        let bob_pubkey = bob.public_key_hex().to_string();
        let msg = b"hello nk federation payload";

        // Alice encrypts for Bob
        let encrypted = noise_encrypt_payload(msg, &alice, &bob_pubkey)
            .expect("NK encrypt should succeed");
        assert!(encrypted.len() > 32, "output should include eph key (32) + AEAD ciphertext");

        // Bob decrypts from Alice
        let decrypted = noise_decrypt_payload(&encrypted, &bob, alice.public_key_hex())
            .expect("NK decrypt should succeed");
        assert_eq!(&decrypted, msg, "NK roundtrip should match");

        let _ = std::fs::remove_dir_all(&alice_dir);
        let _ = std::fs::remove_dir_all(&bob_dir);
    }

    #[test]
    fn test_noise_nk_wrong_key_fails() {
        let (alice, alice_dir) = make_identity("nk_w_a");
        let (bob, bob_dir) = make_identity("nk_w_b");
        let (eve, eve_dir) = make_identity("nk_w_e");
        let bob_pubkey = bob.public_key_hex().to_string();

        let encrypted = noise_encrypt_payload(b"secret", &alice, &bob_pubkey).unwrap();

        // Eve tries to decrypt (wrong identity)
        let result = noise_decrypt_payload(&encrypted, &eve, alice.public_key_hex());
        assert!(result.is_err(), "wrong receiver key should fail to decrypt");

        let _ = std::fs::remove_dir_all(&alice_dir);
        let _ = std::fs::remove_dir_all(&bob_dir);
        let _ = std::fs::remove_dir_all(&eve_dir);
    }
}
