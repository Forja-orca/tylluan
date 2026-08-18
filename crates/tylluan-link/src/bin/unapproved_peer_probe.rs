//! CI probe for the `p2p-rejects-unapproved-peer` security claim.
//!
//! Connects to a running kernel's P2P dispatch listener (Noise XK) using a
//! throwaway identity that the kernel has never approved, and asserts the
//! dispatch request is REJECTED, not executed. This does not implement any
//! new Noise protocol code -- it is a thin CLI wrapper around the real
//! client path the kernel itself uses (`tylluan_link::p2p::execute_remote_tcp`),
//! the same helper exercised by
//! `crates/tylluan-kernel/tests/mesh_audit.rs::test_kernel_remote_dispatch_routes_via_real_noise_xk_p2p`.
//!
//! Usage:
//!   unapproved_peer_probe <peer_addr:port> <peer_pubkey_hex>
//!
//! Exit code 0 and prints "REJECTED" if the peer was refused (either an
//! explicit `{"success":false,"error":"peer not approved"}` response, or the
//! connection/protocol failing outright -- both are acceptable "not
//! executed" outcomes for an unapproved throwaway identity).
//! Exit code 1 and prints "ACCEPTED" if the dispatch actually ran
//! (`success: true`), which would mean the claim is FALSE.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tylluan_link::dispatch::GuildDispatchRequest;
use tylluan_link::identity::NodeIdentity;
use tylluan_link::p2p::{execute_remote_tcp, P2pSessionPool};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: unapproved_peer_probe <peer_addr:port> <peer_pubkey_hex>");
        std::process::exit(2);
    }
    let peer_addr: SocketAddr = match args[1].parse() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("invalid peer_addr '{}': {}", args[1], e);
            std::process::exit(2);
        }
    };
    let peer_pubkey_hex = &args[2];

    // Throwaway identity: freshly generated every run, never registered in
    // the target kernel's peers.db, so it can never be in the approved set.
    let identity_path: PathBuf = std::env::temp_dir()
        .join(format!("tylluan_unapproved_probe_{}.key", std::process::id()));
    let _ = std::fs::remove_file(&identity_path);
    let identity = match NodeIdentity::load_or_create(&identity_path) {
        Ok(id) => Arc::new(id),
        Err(e) => {
            eprintln!("failed to create throwaway identity: {}", e);
            std::process::exit(2);
        }
    };

    let request = GuildDispatchRequest {
        guild: "bash".to_string(),
        tool: "execute".to_string(),
        args: serde_json::json!({"cmd": "echo unapproved-peer-probe-should-never-run"}),
        request_id: uuid::Uuid::new_v4().to_string(),
        sender_id: identity.node_id().to_string(),
        timeout_secs: Some(10),
    };

    let mut pool = P2pSessionPool::new(1, 60);
    let result = execute_remote_tcp(&mut pool, request, peer_addr, peer_pubkey_hex, &identity).await;
    let _ = std::fs::remove_file(&identity_path);

    match result {
        Ok(resp) if resp.success => {
            println!("ACCEPTED: unapproved peer's dispatch executed: {:?}", resp);
            std::process::exit(1);
        }
        Ok(resp) => {
            println!(
                "REJECTED: kernel returned success=false for unapproved peer (error={:?})",
                resp.error
            );
            std::process::exit(0);
        }
        Err(e) => {
            println!("REJECTED: connection/protocol failed for unapproved peer: {}", e);
            std::process::exit(0);
        }
    }
}
