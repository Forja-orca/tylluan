use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;
use std::sync::Arc;

use crate::transport::http::HttpState;

#[derive(Serialize)]
pub struct MeshStatusResponse {
    pub enabled: bool,
    pub peer_count: usize,
    pub buckets: usize,
    pub peers: Vec<MeshPeerEntry>,
}

#[derive(Serialize)]
pub struct MeshPeerEntry {
    pub node_id: String,
    pub addr: String,
    pub capabilities: Vec<String>,
    pub last_seen: i64,
}

pub async fn mesh_peers_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let rt = state.dht_routing_table.read().await;
    let peers: Vec<MeshPeerEntry> = rt.all_peers().into_iter().map(|e| MeshPeerEntry {
        node_id: e.node_id,
        addr: e.addr.to_string(),
        capabilities: e.capabilities,
        last_seen: e.last_seen_unix,
    }).collect();

    let total = peers.len();
    let buckets = rt.peer_count();

    (
        StatusCode::OK,
        Json(MeshStatusResponse {
            enabled: true,
            peer_count: total,
            buckets,
            peers,
        }),
    )
}

pub async fn mesh_refresh_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let bootstrap_config = {
        let config = state.config.read().await;
        tylluan_link::dht::BootstrapConfig {
            local_node_id: state.node_identity.node_id().to_string(),
            local_addr: "0.0.0.0:0".parse().unwrap(),
            use_mdns: config.mdns.advertise || config.mdns.discover,
            use_mainline: config.mesh.mainline_dht_enabled,
            seed_nodes: config.mesh.seed_nodes.clone(),
            dht_peers_path: std::path::PathBuf::from("data/dht_peers.json"),
            listen_port: config.nexus.port,
        }
    };

    let mut rt = state.dht_routing_table.write().await;
    match bootstrap_config.bootstrap(&mut rt).await {
        Ok(discovered) => {
            let count = discovered.len();
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "status": "ok",
                    "discovered": count,
                    "message": format!("DHT refresh discovered {} peers", count)
                })),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "status": "error",
                "error": e.to_string()
            })),
        ),
    }
}
