//! # TylluanLink P2P — Sovereign Peer-to-Peer Connection
//!
//! TCP-based P2P with HMAC-SHA256 challenge-response handshake.

pub mod capability;
pub mod dispatch;
pub mod dht;
pub mod gossip;
pub mod identity;
pub mod nat;
pub mod noise;
pub mod transport;

use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener as TcpListenerAsync;
use tokio::sync::RwLock;
use tokio::task;
use tracing::{info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    pub id: String,
    pub nonce: String,
    pub node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResponse {
    pub challenge_id: String,
    pub signature: String,
    pub node_id: String,
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub addr: SocketAddr,
    pub connected_at: Instant,
    pub verified: bool,
}

pub struct PeerManager {
    node_id: String,
    master_token: Vec<u8>,
    peers: Arc<RwLock<HashMap<String, Peer>>>,
    #[allow(dead_code)]
    pending: Arc<RwLock<HashMap<String, (String, Instant)>>>,
}

impl PeerManager {
    pub fn new(node_id: String, master_token: String) -> Self {
        Self {
            node_id,
            master_token: master_token.into_bytes(),
            peers: Arc::new(RwLock::new(HashMap::new())),
            pending: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create_challenge(&self, _peer_addr: SocketAddr) -> Challenge {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut nonce_bytes = [0u8; 16];
        rng.fill(&mut nonce_bytes);
        let nonce = STANDARD.encode(nonce_bytes);
        let challenge = Challenge {
            id: Uuid::new_v4().to_string(),
            nonce: nonce.clone(),
            node_id: self.node_id.clone(),
        };
        self.pending.write().await.insert(challenge.id.clone(), (nonce.clone(), Instant::now()));
        challenge
    }

    pub async fn verify_and_connect(&self, response: &ChallengeResponse, addr: SocketAddr) -> bool {
        let (nonce_stored, expired) = {
            let mut pending = self.pending.write().await;
            if let Some((nonce, created_at)) = pending.remove(&response.challenge_id) {
                if created_at.elapsed().as_secs() > 30 {
                    warn!("TylluanLink: Challenge '{}' expired", response.challenge_id);
                    (None, true)
                } else {
                    (Some(nonce), false)
                }
            } else {
                warn!("TylluanLink: Unknown challenge '{}'", response.challenge_id);
                (None, true)
            }
        };

        if expired {
            return false;
        }

        let nonce = nonce_stored.unwrap();
        let expected = self.sign_nonce(&nonce);
        
        if expected != response.signature {
            warn!("TylluanLink: Invalid signature from {}", addr);
            return false;
        }
        let peer = Peer {
            id: response.node_id.clone(),
            addr,
            connected_at: Instant::now(),
            verified: true,
        };
        self.peers.write().await.insert(response.node_id.clone(), peer);
        info!("TylluanLink: Peer '{}' connected from {}", response.node_id, addr);
        true
    }

    fn sign_nonce(&self, nonce: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(&self.master_token)
            .expect("HMAC can take key of any size");
        mac.update(nonce.as_bytes());
        let result = mac.finalize();
        STANDARD.encode(result.into_bytes())
    }

    pub async fn get_peers(&self) -> Vec<(String, SocketAddr)> {
        let peers = self.peers.read().await;
        peers.values().map(|p| (p.id.clone(), p.addr)).collect()
    }

    pub async fn disconnect_peer(&self, peer_id: &str) {
        if self.peers.write().await.remove(peer_id).is_some() {
            info!("TylluanLink: Peer '{}' disconnected", peer_id);
        }
    }

    #[allow(dead_code)]
    pub async fn is_peer_connected(&self, peer_id: &str) -> bool {
        self.peers.read().await.contains_key(peer_id)
    }
}

/// Start P2P listener on TCP port for inbound handshake connections.
#[allow(dead_code)]
pub async fn start_p2p_listener(
    peer_manager: Arc<PeerManager>,
    port: u16,
) -> task::JoinHandle<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = match TcpListenerAsync::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!("TylluanLink: Failed to bind P2P listener on {}: {}", addr, e);
            return task::spawn(async move {});
        }
    };

    info!("TylluanLink: 🔌 P2P listener started on port {}", port);

    task::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let pm = peer_manager.clone();
                    task::spawn(async move {
                        if let Err(e) = handle_inbound(stream, addr, pm).await {
                            warn!("TylluanLink: Error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    warn!("TylluanLink: Accept error: {}", e);
                }
            }
        }
    })
}

async fn handle_inbound(
    mut stream: tokio::net::TcpStream,
    addr: SocketAddr,
    pm: Arc<PeerManager>,
) -> std::io::Result<()> {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }

    let request: serde_json::Value = match serde_json::from_slice(&buf[..n]) {
        Ok(v) => v,
        Err(_) => {
            let r = serde_json::json!({"error": "invalid json"});
            stream.write_all(serde_json::to_string(&r).unwrap().as_bytes()).await?;
            stream.flush().await?;
            return Ok(());
        }
    };

    let rt = request.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if rt == "handshake_request" {
        let _nid = request.get("node_id").and_then(|v| v.as_str()).unwrap_or("unknown");

        // Generate nonce before spawn (thread_rng is not Send)
        let (cid, nonce_val) = {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let mut nb = [0u8; 16];
            rng.fill(&mut nb);
            (Uuid::new_v4().to_string(), STANDARD.encode(nb))
        };

        let challenge = Challenge {
            id: cid,
            nonce: nonce_val.clone(),
            node_id: "server-node".to_string(),
        };
        pm.pending.write().await.insert(challenge.id.clone(), (nonce_val.clone(), Instant::now()));

        let r = serde_json::json!({
            "type": "challenge",
            "challenge_id": challenge.id,
            "nonce": challenge.nonce,
            "node_id": "server-node"
        });
        stream.write_all(serde_json::to_string(&r).unwrap().as_bytes()).await?;
        stream.flush().await?;

        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Ok(());
        }

        let resp: serde_json::Value = match serde_json::from_slice(&buf[..n]) {
            Ok(v) => v,
            Err(_) => {
                let r = serde_json::json!({"error": "invalid json"});
                stream.write_all(serde_json::to_string(&r).unwrap().as_bytes()).await?;
                stream.flush().await?;
                return Ok(());
            }
        };

        let cr = ChallengeResponse {
            challenge_id: resp.get("challenge_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            signature: resp.get("signature").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            node_id: resp.get("node_id").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
        };

        let success = pm.verify_and_connect(&cr, addr).await;

        let result = if success {
            serde_json::json!({"type": "connected", "node_id": cr.node_id})
        } else {
            serde_json::json!({"error": "handshake_failed"})
        };
        stream.write_all(serde_json::to_string(&result).unwrap().as_bytes()).await?;
        stream.flush().await?;
        info!("TylluanLink: Handshake with {}: {}", addr, if success { "OK" } else { "FAIL" });
    } else {
        let r = serde_json::json!({"error": "unknown type"});
        stream.write_all(serde_json::to_string(&r).unwrap().as_bytes()).await?;
        stream.flush().await?;
    }

    Ok(())
}

/// Connect to a remote peer and perform handshake.
#[allow(dead_code)]
pub async fn connect_to_peer(
    peer_manager: Arc<PeerManager>,
    addr: String,
    port: u16,
) -> std::io::Result<()> {
    let full_addr = format!("{}:{}", addr, port);
    let mut stream = tokio::net::TcpStream::connect(&full_addr).await?;

    let req = serde_json::json!({"type": "handshake_request", "node_id": peer_manager.node_id.as_str()});
    stream.write_all(serde_json::to_string(&req).unwrap().as_bytes()).await?;
    stream.flush().await?;

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "peer closed"));
    }

    let resp: serde_json::Value = serde_json::from_slice(&buf[..n])
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if resp.get("error").is_some() {
        return Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "rejected"));
    }

    let cid = resp.get("challenge_id").and_then(|v| v.as_str()).unwrap_or("");
    let nonce = resp.get("nonce").and_then(|v| v.as_str()).unwrap_or("");

    let signature = {
        let mut mac = HmacSha256::new_from_slice(&peer_manager.master_token)
            .expect("HMAC can take key of any size");
        mac.update(nonce.as_bytes());
        let result = mac.finalize();
        STANDARD.encode(result.into_bytes())
    };

    let cr = ChallengeResponse {
        challenge_id: cid.to_string(),
        signature,
        node_id: peer_manager.node_id.clone(),
    };
    stream.write_all(serde_json::to_string(&cr).unwrap().as_bytes()).await?;
    stream.flush().await?;

    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "peer closed"));
    }

    let result: serde_json::Value = serde_json::from_slice(&buf[..n])
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    if result.get("error").is_some() {
        return Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "handshake failed"));
    }

    let pnid = result.get("node_id").and_then(|v| v.as_str()).unwrap_or("unknown");
    info!("TylluanLink: Connected to peer '{}' at {}:{}", pnid, addr, port);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handshake() {
        let pm = PeerManager::new("node-b".into(), "secret-token".into());
        let addr: SocketAddr = "127.0.0.1:9001".parse().unwrap();

        // 1. Create challenge (Server side)
        let challenge = pm.create_challenge(addr).await;

        // 2. Generate response (Client side simulation)
        let signature = pm.sign_nonce(&challenge.nonce);
        
        let cr = ChallengeResponse {
            challenge_id: challenge.id,
            signature,
            node_id: "node-a".into(),
        };

        // 3. Verify (Server side)
        let result = pm.verify_and_connect(&cr, addr).await;
        
        assert!(result, "Handshake verification failed with real PeerManager API");
    }
}