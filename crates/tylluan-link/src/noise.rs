//! # Noise Protocol — Encrypted Transport Overlay (M14-C)
//!
//! Replaces HMAC-SHA256 challenge-response with Noise XK pattern:
//! - XK: initiator knows responder's static public key
//! - Ephemeral X25519 keys for perfect forward secrecy
//! - ChaCha20-Poly1305 AEAD for transport encryption

use crate::identity::NodeIdentity;
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
    let mut scalar = sk.to_scalar_bytes();
    scalar[0] &= 248;
    scalar[31] &= 127;
    scalar[31] |= 64;
    scalar
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
        if frame_len > 65535 || frame_len == 0 {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid Noise frame length"));
        }
        let mut frame = vec![0u8; frame_len];
        io.read_exact(&mut frame).await?;
        let mut out = vec![0u8; frame_len + 16];
        let n = self.state.read_message(&frame, &mut out)
            .map_err(std::io::Error::other)?;
        out.truncate(n);
        Ok(out)
    }
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

    let state = responder.into_transport_mode()
        .map_err(|e| anyhow::anyhow!("Noise transport mode: {}", e))?;

    let peer_id = "noise-peer".to_string();

    info!("Noise handshake accepted from {}", stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0))));
    Ok(NoisedPipe {
        session: NoiseSession::new(state),
        peer_id,
        peer_addr: stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0))),
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

    info!("Noise handshake completed with peer at {}", stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0))));
    Ok(NoisedPipe {
        session: NoiseSession::new(state),
        peer_id: "noise-peer".to_string(),
        peer_addr: stream.peer_addr().unwrap_or(SocketAddr::from(([0,0,0,0], 0))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    fn make_identity(label: &str) -> NodeIdentity {
        let dir = std::env::temp_dir().join(format!("noise_test_{}_{}", label, std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("identity.key");
        let identity = NodeIdentity::load_or_create(&path).expect("should create identity");
        identity
    }

    fn cleanup(label: &str) {
        let dir = std::env::temp_dir().join(format!("noise_test_{}_{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_key_conversion() {
        let sk = SigningKey::generate(&mut OsRng);
        let pk = sk.verifying_key();
        let x_pub = ed25519_pub_to_x25519(&pk);
        let x_sec = ed25519_secret_to_x25519(&sk);
        assert_eq!(x_pub.len(), 32);
        assert_eq!(x_sec.len(), 32);
        assert_eq!(x_sec[0] & 248, x_sec[0], "bits 0-2 must be 0");
        assert_eq!(x_sec[31] & 128, 0, "bit 255 must be 0");
        assert_ne!(x_sec[31] & 64, 0, "bit 254 must be 1");
    }

    #[tokio::test]
    async fn test_noise_handshake_roundtrip() {
        let server_id = make_identity("hs_s");
        let client_id = make_identity("hs_c");
        let server_pubkey_hex = server_id.public_key_hex().to_string();

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
        let _ = (server_pipe.unwrap(), client_pipe.unwrap());

        cleanup("hs_s");
        cleanup("hs_c");
    }

    #[tokio::test]
    async fn test_encrypt_decrypt_roundtrip() {
        let server_id = make_identity("enc_s");
        let client_id = make_identity("enc_c");
        let server_pubkey_hex = server_id.public_key_hex().to_string();

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let msg = b"hello noise world this is a secret message";

        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut pipe = noise_accept(&mut stream, &server_id).await.unwrap();
            let (mut read_half, mut write_half) = stream.into_split();
            // Receive encrypted message from client
            let received = pipe.session.async_decrypt_read(&mut read_half).await.unwrap();
            assert_eq!(&received, msg, "server received correct plaintext");
            // Encrypt and send response back
            pipe.session.async_encrypt_write(&mut write_half, b"pong").await.unwrap();
            pipe
        });

        let client_task = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
            let mut pipe = noise_connect(&mut stream, &client_id, &server_pubkey_hex).await.unwrap();
            let (mut read_half, mut write_half) = stream.into_split();
            // Send encrypted message to server
            pipe.session.async_encrypt_write(&mut write_half, msg).await.unwrap();
            // Receive encrypted response
            let response = pipe.session.async_decrypt_read(&mut read_half).await.unwrap();
            assert_eq!(response, b"pong", "client received correct response");
            pipe
        });

        let (server_result, client_result) = tokio::join!(server_task, client_task);
        let _ = (server_result.unwrap(), client_result.unwrap());

        cleanup("enc_s");
        cleanup("enc_c");
    }
}
