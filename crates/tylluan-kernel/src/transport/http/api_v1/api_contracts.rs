use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    Json,
    extract::{State, Path, Query},
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

#[derive(Deserialize)]
pub struct ActiveContractQuery {
    pub channel_id: String,
}

pub async fn contract_active_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<ActiveContractQuery>,
) -> impl IntoResponse {
    let mut found = None;
    for entry in state.contract_registry.contracts.iter() {
        let contract = entry.value();
        if contract.channel_id == q.channel_id && contract.status != "done" {
            let remaining = contract.budget_remaining.load(Ordering::Acquire);
            found = Some(serde_json::json!({
                "contract_id": contract.id,
                "budget_remaining": remaining,
                "status": &contract.status
            }));
            break;
        }
    }

    match found {
        Some(json) => (StatusCode::OK, Json(json)).into_response(),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "no active contract found for this channel"}))).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI64;

    fn make_contract(id: &str) -> WorkContract {
        WorkContract {
            id: id.to_string(),
            task: "test task".into(),
            budget: 10,
            budget_remaining: AtomicI64::new(10),
            team: vec!["alice".into(), "bob".into()],
            consolidator: "alice".into(),
            channel_id: "chan-1".into(),
            status: "open".into(),
            created_at: 1000,
            deliveries: vec![],
            votes: vec![],
            extensions: 0,
        }
    }

    #[test]
    fn test_create_contract() {
        let registry = ContractRegistry::new();
        let c = make_contract("bwc-001");
        registry.contracts.insert("bwc-001".into(), c);

        let entry = registry.contracts.get("bwc-001").unwrap();
        assert_eq!(entry.task, "test task");
        assert_eq!(entry.budget, 10);
        assert_eq!(entry.budget_remaining.load(Ordering::Acquire), 10);
        assert_eq!(entry.status, "open");
        assert_eq!(entry.team.len(), 2);
    }

    #[test]
    fn test_tick_decrements_budget() {
        let registry = ContractRegistry::new();
        let c = make_contract("bwc-002");
        registry.contracts.insert("bwc-002".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-002").unwrap();
        let remaining = entry.budget_remaining.fetch_sub(1, Ordering::AcqRel) - 1;
        assert_eq!(remaining, 9);
        let remaining = entry.budget_remaining.fetch_sub(1, Ordering::AcqRel) - 1;
        assert_eq!(remaining, 8);
    }

    #[test]
    fn test_tick_to_zero_and_blocked() {
        let registry = ContractRegistry::new();
        let c = WorkContract {
            id: "bwc-003".into(),
            task: "test".into(),
            budget: 2,
            budget_remaining: AtomicI64::new(2),
            team: vec!["alice".into()],
            consolidator: "alice".into(),
            channel_id: "chan-1".into(),
            status: "open".into(),
            created_at: 1000,
            deliveries: vec![],
            votes: vec![],
            extensions: 0,
        };
        registry.contracts.insert("bwc-003".into(), c);

        {
            let mut entry = registry.contracts.get_mut("bwc-003").unwrap();
            entry.budget_remaining.fetch_sub(1, Ordering::AcqRel);
            let remaining = entry.budget_remaining.fetch_sub(1, Ordering::AcqRel) - 1;
            assert_eq!(remaining, 0);
        }

        {
            let mut entry = registry.contracts.get_mut("bwc-003").unwrap();
            let remaining = entry.budget_remaining.fetch_sub(1, Ordering::AcqRel) - 1;
            assert!(remaining < 0);
            entry.budget_remaining.store(0, Ordering::Release);
            if entry.status != "blocked" {
                entry.status = "blocked".to_string();
            }
            assert_eq!(entry.status, "blocked");
            assert_eq!(entry.budget_remaining.load(Ordering::Acquire), 0);
        }
    }

    #[test]
    fn test_delivery_tracking() {
        let registry = ContractRegistry::new();
        let c = make_contract("bwc-004");
        registry.contracts.insert("bwc-004".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-004").unwrap();
        entry.deliveries.push(ContractDelivery {
            agent_id: "alice".into(),
            summary: "did work".into(),
            delivered_at: 1001,
        });

        assert_eq!(entry.deliveries.len(), 1);
        assert_eq!(entry.deliveries[0].agent_id, "alice");
    }

    #[test]
    fn test_vote_majority_logic() {
        let registry = ContractRegistry::new();
        let c = make_contract("bwc-005");
        registry.contracts.insert("bwc-005".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-005").unwrap();
        entry.votes.push(ContractVote {
            agent_id: "alice".into(),
            vote: "approve".into(),
            cycles: Some(5),
        });

        let approves = entry.votes.iter().filter(|v| v.vote == "approve").count();
        let total = entry.votes.len();
        assert!(total == 1 && approves == 1);

        entry.votes.push(ContractVote {
            agent_id: "bob".into(),
            vote: "approve".into(),
            cycles: Some(5),
        });

        let approves = entry.votes.iter().filter(|v| v.vote == "approve").count();
        let total = entry.votes.len();
        assert!(total == 2 && approves == 2);
    }

    #[test]
    fn test_vote_retract_and_revote() {
        let registry = ContractRegistry::new();
        let c = make_contract("bwc-006");
        registry.contracts.insert("bwc-006".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-006").unwrap();
        entry.votes.push(ContractVote {
            agent_id: "alice".into(),
            vote: "approve".into(),
            cycles: Some(5),
        });
        assert_eq!(entry.votes.len(), 1);

        entry.votes.retain(|v| v.agent_id != "alice");
        entry.votes.push(ContractVote {
            agent_id: "alice".into(),
            vote: "reject".into(),
            cycles: None,
        });
        assert_eq!(entry.votes.len(), 1);
        assert_eq!(entry.votes[0].vote, "reject");
    }

    #[test]
    fn test_extension_median_cycles() {
        let registry = ContractRegistry::new();
        let c = WorkContract {
            id: "bwc-007".into(),
            task: "extend".into(),
            budget: 1,
            budget_remaining: AtomicI64::new(1),
            team: vec!["alice".into(), "bob".into(), "charlie".into()],
            consolidator: "alice".into(),
            channel_id: "chan-1".into(),
            status: "blocked".into(),
            created_at: 1000,
            deliveries: vec![],
            votes: vec![
                ContractVote { agent_id: "alice".into(), vote: "approve".into(), cycles: Some(10) },
                ContractVote { agent_id: "bob".into(), vote: "approve".into(), cycles: Some(5) },
                ContractVote { agent_id: "charlie".into(), vote: "approve".into(), cycles: Some(3) },
            ],
            extensions: 0,
        };
        registry.contracts.insert("bwc-007".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-007").unwrap();
        let approved_cycles: Vec<i32> = entry.votes.iter()
            .filter_map(|v| if v.vote == "approve" { v.cycles } else { None })
            .collect();
        let mut sorted = approved_cycles.clone();
        sorted.sort();
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(5).max(1).min(50);
        entry.budget_remaining.store(median as i64, Ordering::Release);
        entry.extensions += 1;
        entry.status = "extended".to_string();

        assert_eq!(median, 5);
        assert_eq!(entry.budget_remaining.load(Ordering::Acquire), 5);
        assert_eq!(entry.extensions, 1);
        assert_eq!(entry.status, "extended");
    }

    #[test]
    fn test_max_two_extensions() {
        let registry = ContractRegistry::new();
        let c = WorkContract {
            id: "bwc-008".into(),
            task: "max ext".into(),
            budget: 0,
            budget_remaining: AtomicI64::new(0),
            team: vec!["alice".into(), "bob".into()],
            consolidator: "alice".into(),
            channel_id: "chan-1".into(),
            status: "blocked".into(),
            created_at: 1000,
            deliveries: vec![],
            votes: vec![],
            extensions: 2,
        };
        registry.contracts.insert("bwc-008".into(), c);

        let mut entry = registry.contracts.get_mut("bwc-008").unwrap();
        if entry.extensions < 2 {
            entry.extensions += 1;
            entry.status = "extended".to_string();
        }

        assert_eq!(entry.extensions, 2);
        assert_eq!(entry.status, "blocked");
    }
}
