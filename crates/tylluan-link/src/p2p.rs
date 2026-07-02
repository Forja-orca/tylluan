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
            DispatchError::Io(e) => write!(f, "IO error: {}", e),
            DispatchError::Timeout => write!(f, "dispatch timed out"),
            DispatchError::Protocol(s) => write!(f, "protocol error: {}", s),
            DispatchError::Serialize(s) => write!(f, "serialize error: {}", s),
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

/// Start a P2P listener that accepts Noise XK connections and handles GuildDispatchRequest.
/// Returns a JoinHandle and the bound SocketAddr (useful when port=0 for dynamic assignment).
pub async fn start_p2p_listener_noise(
    addr: SocketAddr,
    identity: Arc<NodeIdentity>,
    handler: P2pHandlerFn,
) -> tokio::io::Result<(tokio::task::JoinHandle<()>, SocketAddr)> {
    let listener = TcpListener::bind(addr).await?;
    let bound_addr = listener.local_addr()?;

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((mut stream, peer_addr)) => {
                    let id = identity.clone();
                    let h = handler.clone();
                    tokio::spawn(async move {
                        // Perform Noise XK handshake
                        let pipe = match noise_accept(&mut stream, &id).await {
                            Ok(p) => p,
                            Err(e) => {
                                tracing::warn!("noise_accept failed from {}: {}", peer_addr, e);
                                return;
                            }
                        };
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
        let msg = format!("{}", err);
        assert!(msg.contains("conn refused"));
    }

    #[test]
    fn test_dispatch_error_timeout() {
        let err = DispatchError::Timeout;
        assert_eq!(format!("{}", err), "dispatch timed out");
    }
}
