//! M14-F: Real P2P Guild Dispatch over Noise XK.
//!
//! Provides `P2pSessionPool` for reusing encrypted TCP sessions and
//! `execute_remote_tcp()` for dispatching `GuildDispatchRequest` over Noise XK.

use crate::dispatch::{GuildDispatchRequest, GuildDispatchResponse};
use crate::identity::NodeIdentity;
use crate::noise::{noise_connect, NoiseSession};
use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
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

    fn get_mut(&mut self, key: &str) -> Option<&mut PooledSession> {
        self.pool.get_mut(key)
    }

    fn insert(&mut self, key: String, session: PooledSession) {
        if self.pool.len() >= self.max_per_peer {
            if let Some(oldest) = self.pool.iter().min_by_key(|(_, s)| s.last_used) {
                let k = oldest.0.clone();
                self.pool.remove(&k);
            }
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

    if let Some(session) = pool.get_mut(peer_pubkey_hex) {
        session.last_used = Instant::now();
        let data = serde_json::to_vec(&request).map_err(|e| DispatchError::Serialize(e.to_string()))?;
        timeout(Duration::from_secs(timeout_secs), session.noise.async_encrypt_write(&mut session.write, &data))
            .await
            .map_err(|_| DispatchError::Timeout)??;
        let resp_bytes = timeout(Duration::from_secs(timeout_secs), session.noise.async_decrypt_read(&mut session.read))
            .await
            .map_err(|_| DispatchError::Timeout)??;
        let response: GuildDispatchResponse = serde_json::from_slice(&resp_bytes)
            .map_err(|e| DispatchError::Serialize(e.to_string()))?;
        return Ok(response);
    }

    let mut stream = TcpStream::connect(peer_addr).await?;
    let pipe = noise_connect(&mut stream, identity, peer_pubkey_hex)
        .await
        .map_err(|e| DispatchError::Protocol(e.to_string()))?;
    let (read, write) = stream.into_split();
    let mut session = PooledSession {
        noise: pipe.session,
        write,
        read,
        last_used: Instant::now(),
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

    session.last_used = Instant::now();
    pool.insert(peer_pubkey_hex.to_string(), session);
    Ok(response)
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
