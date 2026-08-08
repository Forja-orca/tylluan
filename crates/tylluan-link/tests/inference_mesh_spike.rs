//! Spike: does the existing DispatchRouter/CapabilityRegistry route whole-model
//! inference requests to a trusted peer with zero new routing code?
//!
//! Context: docs/architecture/PROPOSAL_distributed_inference_credit_mesh.md
//! Same DST pattern as dispatch_dst.rs — no real processes spawned, deterministic.
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tylluan_link::capability::CapabilityRegistry;
use tylluan_link::dispatch::{DispatchDecision, DispatchRouter};
use tylluan_link::gossip::HardwareCaps;

fn make_registry() -> Arc<Mutex<CapabilityRegistry>> {
    Arc::new(Mutex::new(CapabilityRegistry::new(Duration::from_secs(300))))
}

fn inject_peer(
    registry: &Arc<Mutex<CapabilityRegistry>>,
    node_id: &str,
    addr: &str,
    ram_mb: u32,
    has_gpu: bool,
    load_avg: f32,
    capabilities: &[&str],
) {
    let hw = HardwareCaps { ram_mb, has_gpu, load_avg, supports_p2p: false, tcp_port: None };
    let caps: Vec<String> = capabilities.iter().map(|s| s.to_string()).collect();
    registry.lock().unwrap().ingest(node_id, addr, &hw, &caps, 1);
}

/// Node B has no GPU and no model loaded locally. Node A (a workstation with a GPU)
/// advertises "inference:llama-3-8b-q4" as a capability, exactly like it would advertise
/// "vision" or "bash" today. Confirms: zero new routing code needed for whole-model dispatch.
#[test]
fn spike_whole_model_capability_routes_to_gpu_peer() {
    let registry = make_registry();
    inject_peer(
        &registry,
        "workstation-A",
        "10.0.0.3:9000",
        32768,
        true,
        0.1,
        &["inference:llama-3-8b-q4", "vision"],
    );

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("workstation-A", 3.0);

    // Node B: no GPU, moderate load — cannot serve an 8B model locally.
    let local_no_gpu = HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.4, supports_p2p: false, tcp_port: None };

    let decision = router.route("inference:llama-3-8b-q4", &local_no_gpu, 15.0);
    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "workstation-A".into(), addr: "10.0.0.3:9000".into() },
        "whole-model inference capability must route to the GPU peer, same as any other guild capability"
    );
}

/// Two peers advertise different models. Router must pick the one matching the
/// exact capability string requested, not just "any inference peer".
#[test]
fn spike_router_distinguishes_between_models() {
    let registry = make_registry();
    inject_peer(&registry, "peer-8b", "10.0.0.4:9000", 32768, true, 0.1, &["inference:llama-3-8b-q4"]);
    inject_peer(&registry, "peer-2b", "10.0.0.5:9000", 8192, false, 0.1, &["inference:gemma-2b-q4"]);

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("peer-8b", 3.0);
    router.record_latency("peer-2b", 3.0);

    let local = HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.4, supports_p2p: false, tcp_port: None };

    let decision = router.route("inference:gemma-2b-q4", &local, 15.0);
    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "peer-2b".into(), addr: "10.0.0.5:9000".into() },
        "must route to the peer serving the specific model requested, not the strongest peer overall"
    );
}

/// REAL FINDING, not assumed: DispatchRouter has zero knowledge of FederationPeer.approved.
/// It only sees CapabilityRegistry entries. An unapproved peer that somehow got its
/// capabilities gossiped into the registry would be routed to exactly like an approved one.
/// This means the trust-mesh design in the proposal doc requires an explicit filter step
/// BEFORE calling router.route() -- capability discovery and trust are two separate layers
/// today, and nothing currently wires them together.
#[test]
fn spike_finding_router_is_trust_blind_by_design() {
    let registry = make_registry();
    // Simulates a peer whose capabilities got gossiped in but that was never approved
    // in PeerDb (no such check exists at this layer).
    inject_peer(&registry, "unapproved-stranger", "203.0.113.9:9000", 65536, true, 0.05, &["inference:llama-3-8b-q4"]);

    let router = DispatchRouter::new(registry, Duration::from_secs(60));
    router.record_latency("unapproved-stranger", 1.0);

    let local = HardwareCaps { ram_mb: 4096, has_gpu: false, load_avg: 0.4, supports_p2p: false, tcp_port: None };
    let decision = router.route("inference:llama-3-8b-q4", &local, 15.0);

    // This PASSES today, which is the finding itself: the router happily routes to a
    // peer with no trust relationship. Trust enforcement must happen at the capability
    // gossip ingestion boundary (only ingest from FederationPeer.approved==true), not
    // inside DispatchRouter -- confirming the proposal doc needs an explicit integration
    // step, not just "reuse what exists" as if it were already wired.
    assert_eq!(
        decision,
        DispatchDecision::Remote { node_id: "unapproved-stranger".into(), addr: "203.0.113.9:9000".into() },
        "documents current behavior: router does NOT check approval -- that gate must be added at gossip ingestion"
    );
}
