use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    Json,
    extract::{State, Path},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use dashmap::DashMap;

use crate::transport::http::HttpState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractVote {
    pub agent_id: String,
    pub vote: String,
    pub cycles: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractDelivery {
    pub agent_id: String,
    pub summary: String,
    pub delivered_at: u64,
}

#[derive(Debug, Serialize)]
pub struct WorkContract {
    pub id: String,
    pub task: String,
    pub budget: i32,
    pub budget_remaining: AtomicI64,
    pub team: Vec<String>,
    pub consolidator: String,
    pub channel_id: String,
    pub status: String,
    pub created_at: u64,
    pub deliveries: Vec<ContractDelivery>,
    pub votes: Vec<ContractVote>,
    pub extensions: i32,
}

impl Clone for WorkContract {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            task: self.task.clone(),
            budget: self.budget,
            budget_remaining: AtomicI64::new(self.budget_remaining.load(Ordering::Acquire)),
            team: self.team.clone(),
            consolidator: self.consolidator.clone(),
            channel_id: self.channel_id.clone(),
            status: self.status.clone(),
            created_at: self.created_at,
            deliveries: self.deliveries.clone(),
            votes: self.votes.clone(),
            extensions: self.extensions,
        }
    }
}

#[derive(Deserialize)]
pub struct CreateContractRequest {
    pub task: String,
    pub budget: Option<i32>,
    pub team: Vec<String>,
    pub consolidator: String,
    pub channel_id: String,
}

#[derive(Deserialize)]
pub struct TickRequest {
    pub agent_id: String,
}

#[derive(Deserialize)]
pub struct DeliverRequest {
    pub agent_id: String,
    pub summary: String,
}

#[derive(Deserialize)]
pub struct VoteRequest {
    pub agent_id: String,
    pub vote: String,
    pub cycles: Option<i32>,
}

#[derive(Deserialize)]
pub struct CloseRequest {
    pub agent_id: String,
    pub summary: String,
}

#[derive(Clone)]
pub struct ContractRegistry {
    pub contracts: Arc<DashMap<String, WorkContract>>,
}

impl ContractRegistry {
    pub fn new() -> Self {
        Self {
            contracts: Arc::new(DashMap::new()),
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

pub async fn contract_create_handler(
    State(state): State<Arc<HttpState>>,
    Json(req): Json<CreateContractRequest>,
) -> impl IntoResponse {
    let id = format!("bwc-{}", uuid::Uuid::new_v4());
    let budget = req.budget.unwrap_or(15).max(1).min(100);
    let contract = WorkContract {
        id: id.clone(),
        task: req.task,
        budget,
        budget_remaining: AtomicI64::new(budget as i64),
        team: req.team,
        consolidator: req.consolidator,
        channel_id: req.channel_id,
        status: "open".to_string(),
        created_at: now_unix(),
        deliveries: vec![],
        votes: vec![],
        extensions: 0,
    };
    let remaining = contract.budget_remaining.load(Ordering::Acquire);
    let status = contract.status.clone();
    state.contract_registry.contracts.insert(id.clone(), contract);

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "contract_id": id,
            "status": status,
            "budget_remaining": remaining
        }))
    )
}

pub async fn contract_tick_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(_req): Json<TickRequest>,
) -> impl IntoResponse {
    let mut entry = match state.contract_registry.contracts.get_mut(&id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "contract not found"}))),
    };

    let remaining = entry.budget_remaining.fetch_sub(1, Ordering::AcqRel) - 1;
    if remaining < 0 {
        entry.budget_remaining.store(0, Ordering::Release);
        if entry.status != "blocked" && entry.status != "extended" {
            entry.status = "blocked".to_string();
            let _ = state.broadcast_tx.send(serde_json::json!({
                "type": "work-contract:blocked",
                "contract_id": id,
                "ts": now_unix()
            }));
        }
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "remaining": 0,
                "status": "blocked",
                "action": "request_extension"
            }))
        );
    }

    if entry.status == "open" {
        entry.status = "in_progress".to_string();
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "remaining": remaining,
            "status": &entry.status
        }))
    )
}

pub async fn contract_get_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let entry = match state.contract_registry.contracts.get(&id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "contract not found"}))),
    };

    let mut votes_json: Vec<serde_json::Value> = vec![];
    for v in &entry.votes {
        votes_json.push(serde_json::json!({
            "agent_id": &v.agent_id,
            "vote": &v.vote,
            "cycles": v.cycles
        }));
    }

    let remaining = entry.budget_remaining.load(Ordering::Acquire);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "id": &entry.id,
            "task": &entry.task,
            "budget": entry.budget,
            "budget_remaining": remaining,
            "team": &entry.team,
            "consolidator": &entry.consolidator,
            "channel_id": &entry.channel_id,
            "status": &entry.status,
            "created_at": entry.created_at,
            "deliveries": &entry.deliveries,
            "votes": votes_json,
            "extensions": entry.extensions
        }))
    )
}

pub async fn contract_deliver_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<DeliverRequest>,
) -> impl IntoResponse {
    let mut entry = match state.contract_registry.contracts.get_mut(&id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "contract not found"}))),
    };

    entry.deliveries.push(ContractDelivery {
        agent_id: req.agent_id,
        summary: req.summary,
        delivered_at: now_unix(),
    });

    let delivered_count = entry.deliveries.len();
    let team_count = entry.team.len();
    let all_delivered = delivered_count >= team_count;

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "delivered",
            "deliveries": delivered_count,
            "team_size": team_count,
            "all_delivered": all_delivered
        }))
    )
}

pub async fn contract_vote_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<VoteRequest>,
) -> impl IntoResponse {
    let mut entry = match state.contract_registry.contracts.get_mut(&id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "contract not found"}))),
    };

    entry.votes.retain(|v| v.agent_id != req.agent_id);
    entry.votes.push(ContractVote {
        agent_id: req.agent_id,
        vote: req.vote.clone(),
        cycles: req.cycles,
    });

    let approves = entry.votes.iter().filter(|v| v.vote == "approve").count();
    let total = entry.votes.len();
    let majority = total > 0 && approves > total / 2;

    if majority && entry.status == "blocked" && entry.extensions < 2 {
        let approved_cycles: Vec<i32> = entry.votes.iter()
            .filter_map(|v| if v.vote == "approve" { v.cycles } else { None })
            .collect();
        let mut sorted = approved_cycles.clone();
        sorted.sort();
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(5).max(1).min(50);
        entry.budget_remaining.store(median as i64, Ordering::Release);
        entry.extensions += 1;
        entry.status = "extended".to_string();
        let _ = state.broadcast_tx.send(serde_json::json!({
            "type": "work-contract:extended",
            "contract_id": id,
            "additional_cycles": median,
            "extensions": entry.extensions,
            "ts": now_unix()
        }));
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "extended",
                "additional_cycles": median,
                "extensions": entry.extensions
            }))
        )
    } else if majority {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": &entry.status,
                "note": "majority approved but max extensions reached, human approval required"
            }))
        )
    } else {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": &entry.status,
                "votes": total,
                "approves": approves,
                "majority": false
            }))
        )
    }
}

pub async fn contract_close_handler(
    State(state): State<Arc<HttpState>>,
    Path(id): Path<String>,
    Json(req): Json<CloseRequest>,
) -> impl IntoResponse {
    let mut entry = match state.contract_registry.contracts.get_mut(&id) {
        Some(e) => e,
        None => return (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "contract not found"}))),
    };

    if req.agent_id != entry.consolidator {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "only consolidator can close"}))
        );
    }

    entry.status = "done".to_string();
    let _ = state.broadcast_tx.send(serde_json::json!({
        "type": "work-contract:closed",
        "contract_id": id,
        "ts": now_unix()
    }));

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "done",
            "contract_id": id
        }))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::collections::HashMap;
    use axum::{Router, body::Body, http::Request};
    use tower::ServiceExt;
    use tokio::sync::RwLock;
    use dashmap::DashMap;

    fn test_state() -> Arc<HttpState> {
        use tokio::sync::broadcast;
        let (broadcast_tx, _) = broadcast::channel(100);
        Arc::new(HttpState {
            version: "test".into(),
            auth_token: None,
            dev_mode: Some(true),
            server: None,
            registry: crate::registry::actor::RegistryHandle::new(),
            doctor: Arc::new(crate::doctor::Doctor::new_mock()),
            memory: Arc::new(crate::memory::hybrid::HybridMemory::new_mock()),
            silva: Arc::new(crate::memory::silva::SilvaDB::open_mock()),
            mailbox: Arc::new(crate::memory::mailbox::Mailbox::new()),
            coloquio: Arc::new(crate::memory::coloquio::ColoquioDb::new_mock()),
            matcher: Arc::new(crate::router::matcher::GuildMatcher::new()),
            start_time: std::time::Instant::now(),
            broadcast_tx,
            download_progress_tx: tokio::sync::broadcast::channel(10).0,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            guild_status_cache: Arc::new(std::sync::Mutex::new(None)),
            agent_rate_limiter: Arc::new(DashMap::new()),
            config: Arc::new(RwLock::new(crate::config::TylluanConfig::default())),
            tunnel_wsl_url: None,
            oauth: std::sync::Arc::new(crate::transport::http::oauth::OAuthState::new("http://localhost:9999".into())),
            metrics_ring: Arc::new(RwLock::new(crate::metrics_ring::MetricsRingBuffer::new())),
            jobs: Arc::new(crate::memory::jobs::JobQueue::new()),
            cancel_token: tokio_util::sync::CancellationToken::new(),
            node_router: Arc::new(crate::memory::agent_nodes::AgentNodeRouter::new(tokio::sync::broadcast::channel(10).0)),
            journal: Arc::new(crate::transport::http::api_v1::api_journal::JournalDb::open_mock()),
            agent_registry: crate::transport::http::api_v1::api_agents::AgentRegistry::new(7200),
            contract_registry: ContractRegistry::new(),
            health_ready: Arc::new(AtomicBool::new(true)),
        })
    }

    fn contract_routes() -> Router<Arc<HttpState>> {
        Router::new()
            .route("/api/v1/work-contracts", post(contract_create_handler))
            .route("/api/v1/work-contracts/{id}", get(contract_get_handler))
            .route("/api/v1/work-contracts/{id}/tick", post(contract_tick_handler))
            .route("/api/v1/work-contracts/{id}/deliver", post(contract_deliver_handler))
            .route("/api/v1/work-contracts/{id}/vote", post(contract_vote_handler))
            .route("/api/v1/work-contracts/{id}/close", post(contract_close_handler))
    }

    #[tokio::test]
    async fn test_create_and_get_contract() {
        let state = test_state();
        let app = contract_routes().with_state(state);

        let body = serde_json::json!({
            "task": "refactor auth module",
            "budget": 10,
            "team": ["alpha", "beta"],
            "consolidator": "alpha",
            "channel_id": "chan-1"
        });

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/work-contracts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        let contract_id = json["contract_id"].as_str().unwrap().to_string();
        assert_eq!(json["status"], "open");
        assert_eq!(json["budget_remaining"], 10);

        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&format!("/api/v1/work-contracts/{}", contract_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["task"], "refactor auth module");
        assert_eq!(json["status"], "open");
        assert_eq!(json["budget_remaining"], 10);
        assert_eq!(json["deliveries"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_tick_consumes_budget() {
        let state = test_state();
        let app = contract_routes().with_state(state);

        let body = serde_json::json!({
            "task": "tick test",
            "budget": 3,
            "team": ["agent-x"],
            "consolidator": "agent-x",
            "channel_id": "chan-1"
        });

        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/work-contracts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        let cid = json["contract_id"].as_str().unwrap().to_string();

        for expected_remaining in [2i64, 1, 0] {
            let tick = serde_json::json!({ "agent_id": "agent-x" });
            let resp = app.clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&format!("/api/v1/work-contracts/{}/tick", cid))
                        .header("content-type", "application/json")
                        .body(Body::from(serde_json::to_string(&tick).unwrap()))
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(resp.status(), StatusCode::OK);
            let json: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
            ).unwrap();
            assert_eq!(json["remaining"], expected_remaining);
        }

        let tick = serde_json::json!({ "agent_id": "agent-x" });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/tick", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&tick).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["action"], "request_extension");
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let state = test_state();
        let app = contract_routes().with_state(state);

        let body = serde_json::json!({
            "task": "lifecycle test",
            "budget": 10,
            "team": ["alice", "bob"],
            "consolidator": "alice",
            "channel_id": "chan-lc"
        });

        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/work-contracts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        let cid = json["contract_id"].as_str().unwrap().to_string();

        let tick = serde_json::json!({ "agent_id": "alice" });
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/tick", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&tick).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let deliver = serde_json::json!({
            "agent_id": "alice",
            "summary": "refactored auth service"
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/deliver", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&deliver).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["deliveries"], 1);
        assert_eq!(json["all_delivered"], false);

        let deliver2 = serde_json::json!({
            "agent_id": "bob",
            "summary": "wrote tests"
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/deliver", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&deliver2).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["deliveries"], 2);
        assert_eq!(json["all_delivered"], true);

        let close = serde_json::json!({
            "agent_id": "alice",
            "summary": "done"
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/close", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&close).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["status"], "done");

        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&format!("/api/v1/work-contracts/{}", cid))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["status"], "done");
    }

    #[tokio::test]
    async fn test_close_by_non_consolidator_forbidden() {
        let state = test_state();
        let app = contract_routes().with_state(state);

        let body = serde_json::json!({
            "task": "close guard test",
            "budget": 5,
            "team": ["alice", "bob"],
            "consolidator": "alice",
            "channel_id": "chan-close"
        });

        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/work-contracts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        let cid = json["contract_id"].as_str().unwrap().to_string();

        let close = serde_json::json!({
            "agent_id": "bob",
            "summary": "nope"
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/close", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&close).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_extension_vote_cycle() {
        let state = test_state();
        let app = contract_routes().with_state(state);

        let body = serde_json::json!({
            "task": "extend test",
            "budget": 1,
            "team": ["alice", "bob"],
            "consolidator": "alice",
            "channel_id": "chan-ext"
        });

        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/work-contracts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        let cid = json["contract_id"].as_str().unwrap().to_string();

        let tick = serde_json::json!({ "agent_id": "alice" });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/tick", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&tick).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["action"], "request_extension");

        let vote = serde_json::json!({
            "agent_id": "alice",
            "vote": "approve",
            "cycles": 5
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/vote", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&vote).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["majority"], false);

        let vote2 = serde_json::json!({
            "agent_id": "bob",
            "vote": "approve",
            "cycles": 5
        });
        let resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/v1/work-contracts/{}/vote", cid))
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&vote2).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), 1024).await.unwrap()
        ).unwrap();
        assert_eq!(json["status"], "extended");
        assert_eq!(json["additional_cycles"], 5);
        assert_eq!(json["extensions"], 1);
    }
}
