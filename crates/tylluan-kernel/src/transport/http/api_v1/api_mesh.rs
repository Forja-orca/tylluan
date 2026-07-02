use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;
use std::sync::Arc;

use crate::transport::http::HttpState;
use tylluan_link::dispatch::DispatchDecision;

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

pub async fn guild_peers_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let reg = state.capability_registry.lock().unwrap();
    let all: Vec<_> = reg.all_peers().collect();
    let peers: Vec<serde_json::Value> = all.into_iter().map(|(node_id, (rec, _))| {
        serde_json::json!({
            "node_id": node_id,
            "addr": rec.addr,
            "capabilities": rec.capabilities,
            "hardware": {
                "ram_mb": rec.hardware.ram_mb,
                "has_gpu": rec.hardware.has_gpu,
                "load_avg": rec.hardware.load_avg,
            },
        })
    }).collect();

    (StatusCode::OK, Json(serde_json::json!({
        "peers": peers,
        "count": peers.len(),
    })))
}

pub async fn guild_dispatch_remote_handler(
    State(state): State<Arc<HttpState>>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let guild = body.get("guild").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tool = body.get("tool").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let args = body.get("args").cloned().unwrap_or(serde_json::Value::Null);
    let peer_addr = body.get("peer_addr").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if guild.is_empty() || tool.is_empty() || peer_addr.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "success": false,
            "error": "Missing required fields: guild, tool, peer_addr"
        })));
    }

    let local_caps = {
        let reg = state.capability_registry.lock().unwrap();
        let all: Vec<_> = reg.all_peers().collect();
        all.into_iter()
            .find(|(n, _)| **n == state.node_identity.node_id().to_string())
            .map(|(_, (rec, _))| rec.hardware.clone())
            .unwrap_or_default()
    };

    let decision = {
        let router = state.dispatch_router.lock().unwrap();
        router.route(&guild, &local_caps, 5.0)
    };

    match decision {
        DispatchDecision::Local => {
            let tool_req = rmcp::model::CallToolRequestParam {
                name: tool.clone().into(),
                arguments: Some(args.as_object().cloned().unwrap_or_default()),
            };
            match state.registry.call_tool(&guild, tool_req).await {
                Ok(res) => (StatusCode::OK, Json(serde_json::json!({
                    "success": !res.is_error.unwrap_or(false),
                    "result": serde_json::json!(res.content),
                    "executor": "local",
                }))),
                Err(e) => (StatusCode::OK, Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string(),
                    "executor": "local",
                }))),
            }
        }
        DispatchDecision::Remote { ref addr, .. } => {
            let url = format!("http://{}/api/v1/guilds/dispatch/execute", peer_addr);
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default();

            let dispatch_body = serde_json::json!({
                "guild": guild,
                "tool": tool,
                "args": args,
                "request_id": uuid::Uuid::new_v4().to_string(),
                "sender_id": state.node_identity.node_id().to_string(),
                "timeout_secs": 60u64,
            });

            match client.post(&url).json(&dispatch_body).send().await {
                Ok(resp) => {
                    match resp.json::<serde_json::Value>().await {
                        Ok(result) => {
                            {
                                let mut q = state.dispatch_queue.lock().unwrap();
                                q.remove_timed_out(std::time::Duration::from_secs(300));
                            }
                            state.dispatch_router.lock().unwrap().record_success(addr);
                            (StatusCode::OK, Json(serde_json::json!({
                                "success": result.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                                "result": result,
                                "executor": &peer_addr,
                            })))
                        }
                        Err(e) => {
                            {
                                let mut q = state.dispatch_queue.lock().unwrap();
                                q.enqueue(dispatch_body);
                            }
                            state.dispatch_router.lock().unwrap().record_failure(addr);
                            (StatusCode::OK, Json(serde_json::json!({
                                "success": false,
                                "error": format!("peer response parse failed: {}", e),
                                "queued": true,
                                "executor": &peer_addr,
                            })))
                        }
                    }
                }
                Err(e) => {
                    {
                        let mut q = state.dispatch_queue.lock().unwrap();
                        q.enqueue(dispatch_body);
                    }
                    state.dispatch_router.lock().unwrap().record_failure(addr);
                    (StatusCode::OK, Json(serde_json::json!({
                        "success": false,
                        "error": format!("peer unreachable: {}", e),
                        "queued": true,
                        "executor": &peer_addr,
                    })))
                }
            }
        }
    }
}
