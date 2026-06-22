use axum::{
    extract::{State, ws::{Message, WebSocket, WebSocketUpgrade}, Path as AxumPath},
    response::IntoResponse,
    Json,
    http::StatusCode,
};
use std::sync::{Arc, LazyLock};
use tokio::sync::broadcast;
use tracing::warn;
use crate::transport::http::HttpState;
use futures_util::{SinkExt, StreamExt};

// Global broadcast channel message that supports text and binary.
#[derive(Clone, Debug)]
pub enum CanvasBroadcastMsg {
    Binary(Vec<u8>),
    Text(String),
}

// Global broadcast channel for Yjs and JSON synchronization messages.
// This allows all connected websocket clients to receive canvas updates.
static CANVAS_BROADCAST: LazyLock<(
    broadcast::Sender<CanvasBroadcastMsg>,
    broadcast::Receiver<CanvasBroadcastMsg>
)> = LazyLock::new(|| {
    let (tx, rx) = broadcast::channel(1000);
    (tx, rx)
});

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CanvasNode {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(rename = "type")]
    pub node_type: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CanvasEdge {
    pub id: String,
    pub source: String,
    pub target: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct CanvasState {
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum CanvasIncomingMsg {
    #[serde(rename = "request_sync")]
    RequestSync {
        #[serde(rename = "channelId")]
        channel_id: String,
    },
    #[serde(rename = "sync_response")]
    SyncResponse {
        #[serde(rename = "channelId")]
        channel_id: String,
        nodes: Vec<CanvasNode>,
        edges: Vec<CanvasEdge>,
    },
    #[serde(rename = "node_moved")]
    NodeMoved {
        #[serde(rename = "channelId")]
        channel_id: String,
        id: String,
        x: f64,
        y: f64,
    },
    #[serde(rename = "node_added")]
    NodeAdded {
        #[serde(rename = "channelId")]
        channel_id: String,
        node: CanvasNode,
    },
    #[serde(rename = "edge_added")]
    EdgeAdded {
        #[serde(rename = "channelId")]
        channel_id: String,
        edge: CanvasEdge,
    },
    #[serde(rename = "node_deleted")]
    NodeDeleted {
        #[serde(rename = "channelId")]
        channel_id: String,
        id: String,
    },
}

pub async fn canvas_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<HttpState>) {
    let (mut sender, mut receiver) = socket.split();
    let tx = CANVAS_BROADCAST.0.clone();
    let mut rx = tx.subscribe();

    // Task to receive messages from the global broadcast and send to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let ws_msg = match msg {
                CanvasBroadcastMsg::Binary(bytes) => Message::Binary(bytes.into()),
                CanvasBroadcastMsg::Text(text) => Message::Text(text.into()),
            };
            if let Err(e) = sender.send(ws_msg).await {
                warn!("Failed to send message to WS client: {:?}", e);
                break;
            }
        }
    });

    // Task to receive messages from this client and publish to global broadcast + persist
    let tx_clone = tx.clone();
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Binary(bytes) => {
                    // Broadcast the delta update to all other connected clients
                    let _ = tx_clone.send(CanvasBroadcastMsg::Binary(bytes.to_vec()));
                }
                Message::Text(text) => {
                    // Broadcast text updates to other connected clients
                    let _ = tx_clone.send(CanvasBroadcastMsg::Text(text.to_string()));

                    // Handle canvas persistence asynchronously
                    if let Ok(incoming) = serde_json::from_str::<CanvasIncomingMsg>(&text) {
                        let silva = state_clone.silva.clone();
                        let tx_internal = tx_clone.clone();
                        tokio::spawn(async move {
                            if let CanvasIncomingMsg::RequestSync { channel_id } = &incoming {
                                let node_id = format!("canvas_state:{}", channel_id);
                                if let Ok(Some(node)) = silva.get_node(&node_id).await
                                    && let Ok(state_data) = serde_json::from_str::<CanvasState>(&node.content) {
                                        let response_json = serde_json::json!({
                                            "type": "sync_response",
                                            "channelId": channel_id,
                                            "nodes": state_data.nodes,
                                            "edges": state_data.edges
                                        });
                                        if let Ok(response_str) = serde_json::to_string(&response_json) {
                                            let _ = tx_internal.send(CanvasBroadcastMsg::Text(response_str));
                                        }
                                    }
                            } else {
                                if let Err(e) = handle_canvas_persistence(incoming, silva).await {
                                    warn!("Failed to persist canvas state: {:?}", e);
                                }
                            }
                        });
                    }
                }
                Message::Close(_) => {
                    break;
                }
                _ => {}
            }
        }
    });

    // If any task completes, abort the other one
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    };
}

async fn handle_canvas_persistence(
    incoming: CanvasIncomingMsg,
    silva: Arc<crate::memory::silva::SilvaDB>,
) -> anyhow::Result<()> {
    let channel_id = match &incoming {
        CanvasIncomingMsg::RequestSync { .. } => return Ok(()),
        CanvasIncomingMsg::SyncResponse { channel_id, .. } => channel_id,
        CanvasIncomingMsg::NodeMoved { channel_id, .. } => channel_id,
        CanvasIncomingMsg::NodeAdded { channel_id, .. } => channel_id,
        CanvasIncomingMsg::EdgeAdded { channel_id, .. } => channel_id,
        CanvasIncomingMsg::NodeDeleted { channel_id, .. } => channel_id,
    };

    let node_id = format!("canvas_state:{}", channel_id);
    let mut state = if let Ok(Some(node)) = silva.get_node(&node_id).await {
        serde_json::from_str::<CanvasState>(&node.content).unwrap_or_else(|_| CanvasState { nodes: vec![], edges: vec![] })
    } else {
        CanvasState { nodes: vec![], edges: vec![] }
    };

    match incoming {
        CanvasIncomingMsg::SyncResponse { nodes, edges, .. } => {
            state.nodes = nodes;
            state.edges = edges;
        }
        CanvasIncomingMsg::NodeMoved { id, x, y, .. } => {
            if let Some(n) = state.nodes.iter_mut().find(|n| n.id == id) {
                n.x = x;
                n.y = y;
            }
        }
        CanvasIncomingMsg::NodeAdded { node, .. } => {
            state.nodes.retain(|n| n.id != node.id);
            state.nodes.push(node);
        }
        CanvasIncomingMsg::EdgeAdded { edge, .. } => {
            state.edges.retain(|e| e.id != edge.id);
            state.edges.push(edge);
        }
        CanvasIncomingMsg::NodeDeleted { id, .. } => {
            state.nodes.retain(|n| n.id != id);
            state.edges.retain(|e| e.source != id && e.target != id);
        }
        _ => {}
    }

    let serialized = serde_json::to_string(&state)?;
    silva.upsert_node(&node_id, "canvas_state", &serialized, "{}").await?;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct CreateCanvasNodePayload {
    pub id: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    #[serde(rename = "type")]
    pub node_type: String,
}

pub async fn canvas_create_node_handler(
    AxumPath(channel): AxumPath<String>,
    State(state): State<Arc<HttpState>>,
    Json(payload): Json<CreateCanvasNodePayload>,
) -> impl IntoResponse {
    let silva = &state.silva;
    let node_id = format!("canvas_state:{}", channel);
    
    // Load existing state
    let mut canvas_state = if let Ok(Some(node)) = silva.get_node(&node_id).await {
        serde_json::from_str::<CanvasState>(&node.content).unwrap_or_else(|_| CanvasState { nodes: vec![], edges: vec![] })
    } else {
        CanvasState { nodes: vec![], edges: vec![] }
    };
    
    // Add/Update node
    let new_node = CanvasNode {
        id: payload.id.clone(),
        label: payload.label.clone(),
        x: payload.x,
        y: payload.y,
        node_type: payload.node_type.clone(),
    };
    canvas_state.nodes.retain(|n| n.id != new_node.id);
    canvas_state.nodes.push(new_node.clone());
    
    let serialized = match serde_json::to_string(&canvas_state) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    
    if let Err(e) = silva.upsert_node(&node_id, "canvas_state", &serialized, "{}").await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response();
    }
    
    // Broadcast the update to connected WebSockets
    let broadcast_msg = CanvasIncomingMsg::NodeAdded {
        channel_id: channel.clone(),
        node: new_node,
    };
    if let Ok(msg_str) = serde_json::to_string(&broadcast_msg) {
        let _ = CANVAS_BROADCAST.0.send(CanvasBroadcastMsg::Text(msg_str));
    }
    
    (StatusCode::OK, Json(serde_json::json!({"status": "success"}))).into_response()
}

