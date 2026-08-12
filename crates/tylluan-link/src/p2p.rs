//! M14-F: Real P2P Guild Dispatch over Noise XK.
//!
//! Provides `P2pSessionPool` for reusing encrypted TCP sessions and
//! `execute_remote_tcp()` for dispatching `GuildDispatchRequest` over Noise XK.

use crate::dispatch::{GuildDispatchRequest, GuildDispatchResponse};
use crate::identity::NodeIdentity;
use crate::noise::{noise_accept, noise_connect, NoiseSession};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

#[derive(Debug)]
pub enum DispatchError {
    Io(std::io::Error),
    Timeout,
    Protocol(String),
    Serialize(String),
}

impl fmt::Display for DispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DispatchError::Io(e) => write!(f, "IO error: {e}"),
            DispatchError::Timeout => write!(f, "dispatch timed out"),
            DispatchError::Protocol(s) => write!(f, "protocol error: {s}"),
            DispatchError::Serialize(s) => write!(f, "serialize error: {s}"),
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<std::io::Error> for DispatchError {
    fn from(e: std::io::Error) -> Self {
        DispatchError::Io(e)
    }
}

pub struct PooledSession {
    pub noise: NoiseSession,
    pub write: OwnedWriteHalf,
    pub read: OwnedReadHalf,
    pub last_used: Instant,
}

pub struct P2pSessionPool {
    pool: HashMap<String, PooledSession>,
    pub max_per_peer: usize,
    pub keepalive_secs: u64,
}

impl P2pSessionPool {
    pub fn new(max_per_peer: usize, keepalive_secs: u64) -> Self {
        Self {
            pool: HashMap::new(),
            max_per_peer,
            keepalive_secs,
        }
    }

    pub fn prune(&mut self) {
        let cutoff = Duration::from_secs(self.keepalive_secs);
        self.pool.retain(|_, s| s.last_used.elapsed() < cutoff);
    }

    pub fn len(&self) -> usize {
        self.pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }

    fn remove(&mut self, key: &str) -> Option<PooledSession> {
        self.pool.remove(key)
    }

    fn insert(&mut self, key: String, session: PooledSession) {
        if self.pool.len() >= self.max_per_peer
            && let Some(oldest) = self.pool.iter().min_by_key(|(_, s)| s.last_used)
        {
            let k = oldest.0.clone();
            self.pool.remove(&k);
        }
        self.pool.insert(key, session);
    }
}

pub async fn execute_remote_tcp(
    pool: &mut P2pSessionPool,
    request: GuildDispatchRequest,
    peer_addr: SocketAddr,
    peer_pubkey_hex: &str,
    identity: &NodeIdentity,
) -> Result<GuildDispatchResponse, DispatchError> {
    let timeout_secs = request.timeout_secs.unwrap_or(30);

    // Bug fix: extract session from pool before using; reinsert only on success
    let mut session_opt = pool.remove(peer_pubkey_hex);

    let mut session = if let Some(s) = session_opt.take() {
        // Reuse existing session
        s
    } else {
        // New connection
        let mut stream = TcpStream::connect(peer_addr).await?;
        let pipe = noise_connect(&mut stream, identity, peer_pubkey_hex)
            .await
            .map_err(|e| DispatchError::Protocol(e.to_string()))?;
        let (read, write) = stream.into_split();
        PooledSession {
            noise: pipe.session,
            write,
            read,
            last_used: Instant::now(),
        }
    };

    let data = serde_json::to_vec(&request).map_err(|e| DispatchError::Serialize(e.to_string()))?;
    timeout(Duration::from_secs(timeout_secs), session.noise.async_encrypt_write(&mut session.write, &data))
        .await
        .map_err(|_| DispatchError::Timeout)??;
    let resp_bytes = timeout(Duration::from_secs(timeout_secs), session.noise.async_decrypt_read(&mut session.read))
        .await
        .map_err(|_| DispatchError::Timeout)??;
    let response: GuildDispatchResponse = serde_json::from_slice(&resp_bytes)
        .map_err(|e| DispatchError::Serialize(e.to_string()))?;

    // Reinsert only on success (bug fix: broken session is dropped)
    session.last_used = Instant::now();
    pool.insert(peer_pubkey_hex.to_string(), session);
    Ok(response)
}

/// Async handler for inbound P2P dispatch requests.
pub type P2pHandlerFn = Arc<dyn Fn(GuildDispatchRequest) -> Pin<Box<dyn Future<Output = GuildDispatchResponse> + Send>> + Send + Sync + 'static>;

/// Provider of approved peers' X25519 static keys for the responder side.
/// Returned list may be empty (fail-closed: nothing executes). In production
/// the kernel derives these from `peers.db` Ed25519 pubkeys of approved peers
/// via `noise::x25519_from_ed25519_hex`; a closure allows runtime updates
/// (approving a peer does not require a listener restart).
pub type ApprovedPeersFn = Arc<dyn Fn() -> Vec<[u8; 32]> + Send + Sync + 'static>;

fn peer_is_approved(remote_x25519_hex: &str, approved: &[[u8; 32]]) -> bool {
    let Ok(remote) = hex::decode(remote_x25519_hex) else {
        return false;
    };
    remote.len() == 32
        && approved
            .iter()
            .any(|expected| expected.as_slice() == remote.as_slice())
}

/// Start a P2P listener that accepts Noise XK connections and handles GuildDispatchRequest.
/// Only calls `handler` for peers whose X25519 static key (learned during the
/// handshake) matches one of the approved keys from `approved_x25519`.
/// Unapproved peers get an encrypted error response and the connection is
/// closed BEFORE any request is read or executed (no guild callback).
/// Returns a JoinHandle and the bound SocketAddr (useful when port=0 for dynamic assignment).
pub async fn start_p2p_listener_noise(
    addr: SocketAddr,
    identity: Arc<NodeIdentity>,
    handler: P2pHandlerFn,
    approved_x25519: ApprovedPeersFn,
) -> tokio::io::Result<(tokio::task::JoinHandle<()>, SocketAddr)> {
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, peer_addr)) => {
                    let id = identity.clone();
                    let h = handler.clone();
                    let approved = approved_x25519.clone();
                    tokio::spawn(async move {
                        // Perform Noise XK handshake
                        let pipe = match noise_accept(&mut stream, &id).await {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::warn!("noise_accept failed from {}: {}", peer_addr, e);
                                return;
                            }
                        };

                        // Peer authorization: the initiator's X25519 static key
                        // (pipe.peer_id) must match an approved peer's derived key.
                        if !peer_is_approved(&pipe.peer_id, &approved()) {
                            tracing::warn!(
                                "P2P dispatch rejected: peer {} (x25519={}) is NOT approved",
                                peer_addr,
                                pipe.peer_id
                            );
                            let (mut _read, mut write) = stream.into_split();
                            let mut session = pipe.session;
                            let resp_bytes = match serde_json::to_vec(&GuildDispatchResponse {
                                request_id: "".to_string(),
                                success: false,
                                result: serde_json::Value::Null,
                                error: Some("peer not approved".to_string()),
                                executor_id: "local".to_string(),
                                duration_ms: 0,
                            }) {
                                Ok(b) => b,
                                Err(e) => {
                                    tracing::warn!("serialize rejection for {}: {}", peer_addr, e);
                                    return;
                                }
                            };
                            let _ = session.async_encrypt_write(&mut write, &resp_bytes).await;
                            return;
                        }

                        let (read, write) = stream.into_split();
                        let mut session = PooledSession {
                            noise: pipe.session,
                            write,
                            read,
                            last_used: Instant::now(),
                        };

                        // Read GuildDispatchRequest
                        let req_bytes = match session.noise.async_decrypt_read(&mut session.read).await {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!("decrypt read failed from {}: {}", peer_addr, e);
                                return;
                            }
                        };
                        let request: GuildDispatchRequest = match serde_json::from_slice(&req_bytes) {
                            Ok(r) => r,
                            Err(e) => {
                                tracing::warn!("deserialize failed from {}: {}", peer_addr, e);
                                return;
                            }
                        };

                        // Call handler (async via P2pHandlerFn)
                        let response = h(request).await;

                        // Write GuildDispatchResponse
                        let resp_bytes = match serde_json::to_vec(&response) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!("serialize failed for {}: {}", peer_addr, e);
                                return;
                            }
                        };
                        if let Err(e) = session.noise.async_encrypt_write(&mut session.write, &resp_bytes).await {
                            tracing::warn!("encrypt write failed to {}: {}", peer_addr, e);
                        }
                    });
                }
                Err(e) => {
                    tracing::warn!("accept error: {}", e);
                }
            }
        }
    });

    Ok((handle, bound_addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_new_is_empty() {
        let pool = P2pSessionPool::new(4, 300);
        assert!(pool.is_empty());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_pool_prune_noop_on_empty() {
        let mut pool = P2pSessionPool::new(4, 1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        pool.prune();
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_dispatch_error_display_io() {
        let err = DispatchError::Io(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "conn refused"));
        let msg = format!("{err}");
        assert!(msg.contains("conn refused"));
    }

    #[test]
    fn test_dispatch_error_timeout() {
        let err = DispatchError::Timeout;
        assert_eq!(format!("{err}"), "dispatch timed out");
    }

    #[test]
    fn test_peer_is_approved_matches_exact_key() {
        let approved = [[7u8; 32], [9u8; 32]];
        assert!(peer_is_approved(&hex::encode([7u8; 32]), &approved));
        assert!(peer_is_approved(&hex::encode([9u8; 32]), &approved));
        // Key not in the approved set
        assert!(!peer_is_approved(&hex::encode([8u8; 32]), &approved));
        // Invalid hex and wrong-length decodes never match
        assert!(!peer_is_approved("zzz", &approved));
        // Empty approved set is fail-closed
        assert!(!peer_is_approved(&hex::encode([7u8; 32]), &[]));
    }

    fn test_identity(label: &str, counter: &std::sync::atomic::AtomicU64) -> (Arc<NodeIdentity>, std::path::PathBuf) {
        use std::sync::atomic::Ordering;
        let id = counter.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("tylluan_p2p_{label}_{id}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("identity.key");
        let identity = Arc::new(NodeIdentity::load_or_create(&path).expect("identity"));
        (identity, dir)
    }

    fn approved_set(peers: &[Arc<NodeIdentity>]) -> ApprovedPeersFn {
        let keys: Vec<[u8; 32]> = peers
            .iter()
            .filter_map(|p| crate::noise::x25519_from_ed25519_hex(p.public_key_hex()))
            .collect();
        Arc::new(move || keys.clone())
    }

    #[tokio::test]
    async fn test_listener_runs_handler_only_for_approved_peer() {
        use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
        let counter = AtomicU64::new(0);
        let (server, server_dir) = test_identity("aprv_s", &counter);
        let (approved, approved_dir) = test_identity("aprv_a", &counter);
        let (intruder, intruder_dir) = test_identity("aprv_i", &counter);

        let server_pubkey_hex = server.public_key_hex().to_string();
        let executed = Arc::new(AtomicUsize::new(0));
        let handler: P2pHandlerFn = {
            let executed = executed.clone();
            Arc::new(move |req: GuildDispatchRequest| {
                let executed = executed.clone();
                Box::pin(async move {
                    executed.fetch_add(1, Ordering::Relaxed);
                    GuildDispatchResponse {
                        request_id: req.request_id,
                        success: true,
                        result: serde_json::json!("ok"),
                        error: None,
                        executor_id: "local".to_string(),
                        duration_ms: 0,
                    }
                })
            })
        };

        // Approve only `approved`. `intruder` is NOT in the set.
        let approved_set: ApprovedPeersFn = approved_set(&[approved.clone()]);

        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (handle, bound_addr) =
            start_p2p_listener_noise(addr, server.clone(), handler, approved_set)
                .await
                .unwrap();

        // Approved peer dispatch: handler must run.
        let approved_pubkey = server_pubkey_hex.clone();
        let approved_resp = {
            let approved = approved.clone();
            tokio::spawn(async move {
                let mut stream = tokio::net::TcpStream::connect(bound_addr).await.unwrap();
                let mut pipe = noise_connect(&mut stream, &approved, &approved_pubkey).await.unwrap();
                let (mut _read, mut write) = stream.into_split();
                let req = GuildDispatchRequest {
                    request_id: "r-approved".into(),
                    guild: "bash".into(),
                    tool: "run".into(),
                    args: serde_json::json!({}),
                    sender_id: "approved-peer".into(),
                    timeout_secs: Some(5),
                };
                pipe.session
                    .async_encrypt_write(&mut write, &serde_json::to_vec(&req).unwrap())
                    .await
                    .unwrap();
                let resp_bytes = pipe.session.async_decrypt_read(&mut _read).await.unwrap();
                let resp: GuildDispatchResponse = serde_json::from_slice(&resp_bytes).unwrap();
                resp
            })
        };

        // Unapproved (intruder) peer dispatch: handler must NOT run, gets a
        // rejection response with success=false before the loop reads a request.
        let intruder_pubkey = server_pubkey_hex.clone();
        let intruder_resp = {
            let intruder = intruder.clone();
            tokio::spawn(async move {
                let mut stream = tokio::net::TcpStream::connect(bound_addr).await.unwrap();
                let mut pipe = noise_connect(&mut stream, &intruder, &intruder_pubkey).await.unwrap();
                let (mut _read, _write) = stream.into_split();
                // The listener rejects BEFORE reading any request; emulate by
                // only reading the rejection frame (no request is sent).
                let resp_bytes =
                    tokio::time::timeout(Duration::from_secs(3), pipe.session.async_decrypt_read(&mut _read))
                        .await;
                match resp_bytes {
                    Ok(Ok(b)) => {
                        let resp: GuildDispatchResponse = serde_json::from_slice(&b).unwrap();
                        resp
                    }
                    _ => GuildDispatchResponse {
                        request_id: String::new(),
                        success: false,
                        result: serde_json::Value::Null,
                        error: Some("no response (timeout/err)".into()),
                        executor_id: String::new(),
                        duration_ms: 0,
                    },
                }
            })
        };

        let approved_resp =
            tokio::time::timeout(Duration::from_secs(5), approved_resp).await.unwrap().unwrap();
        assert!(approved_resp.success, "approved peer should execute and succeed");

        let intruder_resp =
            tokio::time::timeout(Duration::from_secs(5), intruder_resp).await.unwrap().unwrap();
        assert!(!intruder_resp.success, "intruder must be rejected");
        assert!(
            intruder_resp.error.as_deref().unwrap_or("").contains("not"),
            "rejection should carry a 'not approved' message, got: {:?}",
            intruder_resp.error
        );

        assert_eq!(
            executed.load(Ordering::Relaxed),
            1,
            "handler must run exactly once (only the approved peer)"
        );

        handle.abort();
        let _ = std::fs::remove_dir_all(&server_dir);
        let _ = std::fs::remove_dir_all(&approved_dir);
        let _ = std::fs::remove_dir_all(&intruder_dir);
    }
}
