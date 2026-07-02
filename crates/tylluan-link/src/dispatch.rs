use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use crate::gossip::HardwareCaps;
use crate::capability::CapabilityRegistry;
use crate::identity::NodeIdentity;
use crate::noise::{noise_encrypt_payload, noise_decrypt_payload};
use crate::transport::{MeshTransport, TransportError};

// ─── Phase 3: Guild Dispatch Request/Response ───────────────────────────

/// A request to execute a guild tool on a remote peer.
/// Serialized and encrypted via Noise NK before sending over MeshTransport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildDispatchRequest {
    /// The guild to execute (e.g. "bash", "vision")
    pub guild: String,
    /// The tool within the guild to call (e.g. "execute_command")
    pub tool: String,
    /// JSON arguments for the tool
    pub args: serde_json::Value,
    /// Request ID for correlating response
    pub request_id: String,
    /// Sender node ID
    pub sender_id: String,
    /// Optional timeout in seconds
    pub timeout_secs: Option<u64>,
}

/// Response from a remote guild dispatch execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildDispatchResponse {
    /// Correlates to the original request
    pub request_id: String,
    /// True if the tool executed successfully
    pub success: bool,
    /// JSON result payload
    pub result: serde_json::Value,
    /// Error message if success == false
    pub error: Option<String>,
    /// Node ID that executed the request
    pub executor_id: String,
    /// Duration of execution in milliseconds
    pub duration_ms: u64,
}

/// Encrypt a GuildDispatchRequest with Noise NK and send it over a transport.
pub async fn send_dispatch_request(
    transport: &mut dyn MeshTransport,
    identity: &NodeIdentity,
    peer_pubkey_hex: &str,
    request: &GuildDispatchRequest,
) -> Result<(), TransportError> {
    let json = serde_json::to_vec(request)
        .map_err(|e| TransportError::Serialize(format!("dispatch request: {}", e)))?;
    let encrypted = noise_encrypt_payload(&json, identity, peer_pubkey_hex)
        .map_err(|e| TransportError::Serialize(format!("noise encrypt dispatch: {}", e)))?;
    transport.send(&encrypted).await
}

/// Receive a GuildDispatchRequest from a transport and decrypt it with Noise NK.
pub async fn receive_dispatch_request(
    transport: &mut dyn MeshTransport,
    identity: &NodeIdentity,
    expected_peer_pubkey_hex: &str,
) -> Result<GuildDispatchRequest, TransportError> {
    let encrypted = transport.receive().await?;
    let decrypted = noise_decrypt_payload(&encrypted, identity, expected_peer_pubkey_hex)
        .map_err(|e| TransportError::Deserialize(format!("noise decrypt dispatch: {}", e)))?;
    serde_json::from_slice(&decrypted)
        .map_err(|e| TransportError::Deserialize(format!("dispatch request: {}", e)))
}

/// Encrypt a GuildDispatchResponse with Noise NK and send it over a transport.
pub async fn send_dispatch_response(
    transport: &mut dyn MeshTransport,
    identity: &NodeIdentity,
    peer_pubkey_hex: &str,
    response: &GuildDispatchResponse,
) -> Result<(), TransportError> {
    let json = serde_json::to_vec(response)
        .map_err(|e| TransportError::Serialize(format!("dispatch response: {}", e)))?;
    let encrypted = noise_encrypt_payload(&json, identity, peer_pubkey_hex)
        .map_err(|e| TransportError::Serialize(format!("noise encrypt dispatch response: {}", e)))?;
    transport.send(&encrypted).await
}

/// Receive a GuildDispatchResponse from a transport and decrypt it with Noise NK.
pub async fn receive_dispatch_response(
    transport: &mut dyn MeshTransport,
    identity: &NodeIdentity,
    expected_peer_pubkey_hex: &str,
) -> Result<GuildDispatchResponse, TransportError> {
    let encrypted = transport.receive().await?;
    let decrypted = noise_decrypt_payload(&encrypted, identity, expected_peer_pubkey_hex)
        .map_err(|e| TransportError::Deserialize(format!("noise decrypt dispatch response: {}", e)))?;
    serde_json::from_slice(&decrypted)
        .map_err(|e| TransportError::Deserialize(format!("dispatch response: {}", e)))
}

// ─── Phase 2: DispatchRouter ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum DispatchDecision {
    Local,
    Remote { node_id: String, addr: String },
    RemoteTcp { node_id: String, addr: String, tcp_port: u16 },
}

#[derive(Debug, Default, Clone)]
struct PeerStats {
    latency_ms: Option<f32>,
    consecutive_failures: usize,
    last_failure: Option<Instant>,
}

pub struct DispatchRouter {
    registry: Arc<Mutex<CapabilityRegistry>>,
    peer_stats: Arc<Mutex<HashMap<String, PeerStats>>>,
    cooldown: Duration,
}

impl DispatchRouter {
    pub fn new(registry: Arc<Mutex<CapabilityRegistry>>, cooldown: Duration) -> Self {
        Self {
            registry,
            peer_stats: Arc::new(Mutex::new(HashMap::new())),
            cooldown,
        }
    }

    /// Record a latency measurement for a peer.
    pub fn record_latency(&self, node_id: &str, latency_ms: f32) {
        let mut stats = self.peer_stats.lock().unwrap();
        let entry = stats.entry(node_id.to_string()).or_default();
        entry.latency_ms = Some(latency_ms);
    }

    /// Record a failure for a peer to trigger the circuit breaker if consecutive failures >= threshold.
    pub fn record_failure(&self, node_id: &str) {
        let mut stats = self.peer_stats.lock().unwrap();
        let entry = stats.entry(node_id.to_string()).or_default();
        entry.consecutive_failures += 1;
        entry.last_failure = Some(Instant::now());
    }

    /// Record a success for a peer to reset consecutive failures.
    pub fn record_success(&self, node_id: &str) {
        let mut stats = self.peer_stats.lock().unwrap();
        let entry = stats.entry(node_id.to_string()).or_default();
        entry.consecutive_failures = 0;
        entry.last_failure = None;
    }

    /// Calculate routing score for a given peer or local hardware.
    fn calculate_score(hardware: &HardwareCaps, latency_ms: f32) -> f32 {
        let load_factor = 1.0 - hardware.load_avg.clamp(0.0, 1.0);
        let latency_factor = 1000.0 / latency_ms.max(1.0);
        let gpu_multiplier = if hardware.has_gpu { 2.0 } else { 1.0 };
        load_factor * latency_factor * gpu_multiplier
    }

    pub fn route(
        &self,
        guild: &str,
        local_caps: &HardwareCaps,
        local_latency_ms: f32, // Usually 0.0 or a low local processing overhead
    ) -> DispatchDecision {
        let local_score = Self::calculate_score(local_caps, local_latency_ms);

        let registry = self.registry.lock().unwrap();
        let stats = self.peer_stats.lock().unwrap();
        let now = Instant::now();

        let mut best_peer: Option<(String, String, f32, bool, Option<u16>)> = None;

        for (node_id, (record, _)) in registry.all_peers() {
            // Must support the requested guild
            if !record.capabilities.iter().any(|c| c == guild) {
                continue;
            }

            // Check circuit breaker
            if let Some(peer_stat) = stats.get(node_id)
                && peer_stat.consecutive_failures >= 3
                && let Some(last_fail) = peer_stat.last_failure
                && now.duration_since(last_fail) < self.cooldown
            {
                // Degraded / Circuit open — skip this peer
                continue;
            }

            // Obtain latency: default to 0.0 (favors exploration)
            let peer_latency = stats
                .get(node_id)
                .and_then(|s| s.latency_ms)
                .unwrap_or(0.0);

            let peer_score = Self::calculate_score(&record.hardware, peer_latency);

            let is_p2p = record.hardware.supports_p2p;
            let tcp_port = record.hardware.tcp_port;

            match &best_peer {
                Some((_, _, s, _, _)) if peer_score <= *s => {}
                _ => best_peer = Some((
                    node_id.clone(),
                    record.addr.clone(),
                    peer_score,
                    is_p2p,
                    tcp_port,
                )),
            }
        }

        if let Some((peer_id, addr, best_score, supports_p2p, tcp_port_opt)) = best_peer
            && best_score > local_score * 1.2
        {
            if supports_p2p && let Some(port) = tcp_port_opt {
                return DispatchDecision::RemoteTcp {
                    node_id: peer_id,
                    addr,
                    tcp_port: port,
                };
            }
            return DispatchDecision::Remote {
                node_id: peer_id,
                addr,
            };
        }

        DispatchDecision::Local
    }
}

// ─── Phase 3: DispatchQueue (moved from kernel http/mod.rs) ─────────────

/// A FIFO queue for dispatch items with TTL expiry and overflow protection.
#[derive(Debug, Clone)]
pub struct DispatchQueue {
    queue: VecDeque<(Instant, serde_json::Value)>,
    max_size: usize,
}

impl DispatchQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            max_size,
        }
    }

    /// Enqueue an item. Returns false if the queue is full.
    pub fn enqueue(&mut self, item: serde_json::Value) -> bool {
        if self.queue.len() >= self.max_size {
            return false;
        }
        self.queue.push_back((Instant::now(), item));
        true
    }

    /// Dequeue the oldest item (FIFO).
    pub fn dequeue(&mut self) -> Option<serde_json::Value> {
        self.queue.pop_front().map(|(_, v)| v)
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Return items that have exceeded the given TTL, without removing them.
    pub fn peek_timed_out(&self, timeout: Duration) -> Vec<serde_json::Value> {
        let now = Instant::now();
        self.queue
            .iter()
            .filter(|(enqueued, _)| now.duration_since(*enqueued) >= timeout)
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// Remove items that have exceeded the given TTL.
    pub fn remove_timed_out(&mut self, timeout: Duration) {
        let now = Instant::now();
        self.queue.retain(|(enqueued, _)| now.duration_since(*enqueued) < timeout);
    }
}

impl Default for DispatchQueue {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::capability::CapabilityRegistry;

    fn make_registry() -> Arc<Mutex<CapabilityRegistry>> {
        Arc::new(Mutex::new(CapabilityRegistry::new(Duration::from_secs(300))))
    }

    #[test]
    fn test_route_local_first_if_no_peers() {
        let registry = make_registry();
        let router = DispatchRouter::new(registry, Duration::from_secs(60));
        let local_caps = HardwareCaps {
            ram_mb: 8192,
            has_gpu: false,
            load_avg: 0.2,
            supports_p2p: false,
            tcp_port: None,
        };

        let decision = router.route("vision", &local_caps, 5.0);
        assert_eq!(decision, DispatchDecision::Local);
    }

    #[test]
    fn test_route_remote_if_peer_has_better_score() {
        let registry = make_registry();
        let peer_hw = HardwareCaps {
            ram_mb: 16384,
            has_gpu: true,
            load_avg: 0.1,
            supports_p2p: false,
            tcp_port: None,
        };
        {
            let mut reg = registry.lock().unwrap();
            reg.ingest(
                "peer-gpu",
                "10.0.0.5:9000",
                &peer_hw,
                &["vision".to_string()],
                1,
            );
        }

        let router = DispatchRouter::new(registry, Duration::from_secs(60));
        router.record_latency("peer-gpu", 10.0);

        let local_caps = HardwareCaps {
            ram_mb: 4096,
            has_gpu: false,
            load_avg: 0.8,
            supports_p2p: false,
            tcp_port: None,
        };

        // Peer score will be way higher because of GPU and low load
        let decision = router.route("vision", &local_caps, 20.0);
        assert_eq!(
            decision,
            DispatchDecision::Remote {
                node_id: "peer-gpu".to_string(),
                addr: "10.0.0.5:9000".to_string(),
            }
        );
    }

    #[test]
    fn test_route_favors_unknown_peer_latency_zero() {
        let registry = make_registry();
        let peer_hw = HardwareCaps {
            ram_mb: 8192,
            has_gpu: false,
            load_avg: 0.1,
            supports_p2p: false,
            tcp_port: None,
        };
        {
            let mut reg = registry.lock().unwrap();
            reg.ingest(
                "peer-unknown",
                "10.0.0.6:9000",
                &peer_hw,
                &["bash".to_string()],
                1,
            );
        }

        let router = DispatchRouter::new(registry, Duration::from_secs(60));
        // Latency is not recorded -> defaults to 0.0 (favors exploration)

        let local_caps = HardwareCaps {
            ram_mb: 8192,
            has_gpu: false,
            load_avg: 0.5,
            supports_p2p: false,
            tcp_port: None,
        };

        let decision = router.route("bash", &local_caps, 10.0);
        assert_eq!(
            decision,
            DispatchDecision::Remote {
                node_id: "peer-unknown".to_string(),
                addr: "10.0.0.6:9000".to_string(),
            }
        );
    }

    #[test]
    fn test_circuit_breaker_trips_and_recovers() {
        let registry = make_registry();
        let peer_hw = HardwareCaps {
            ram_mb: 8192,
            has_gpu: false,
            load_avg: 0.1,
            supports_p2p: false,
            tcp_port: None,
        };
        {
            let mut reg = registry.lock().unwrap();
            reg.ingest(
                "peer-fail",
                "10.0.0.7:9000",
                &peer_hw,
                &["git".to_string()],
                1,
            );
        }

        let router = DispatchRouter::new(registry, Duration::from_millis(50));
        router.record_latency("peer-fail", 5.0);

        let local_caps = HardwareCaps {
            ram_mb: 4096,
            has_gpu: false,
            load_avg: 0.9,
            supports_p2p: false,
            tcp_port: None,
        };

        // Initially routed remote
        assert_eq!(
            router.route("git", &local_caps, 50.0),
            DispatchDecision::Remote {
                node_id: "peer-fail".to_string(),
                addr: "10.0.0.7:9000".to_string()
            }
        );

        // Record 3 failures to trigger circuit breaker
        router.record_failure("peer-fail");
        router.record_failure("peer-fail");
        router.record_failure("peer-fail");

        // Now routes local because peer is degraded
        assert_eq!(router.route("git", &local_caps, 50.0), DispatchDecision::Local);

        // Sleep to exceed cooldown of 50ms
        std::thread::sleep(Duration::from_millis(60));

        // Router recovers and routes remote again
        assert_eq!(
            router.route("git", &local_caps, 50.0),
            DispatchDecision::Remote {
                node_id: "peer-fail".to_string(),
                addr: "10.0.0.7:9000".to_string()
            }
        );

        // Success resets the circuit breaker
        router.record_success("peer-fail");
    }
}
