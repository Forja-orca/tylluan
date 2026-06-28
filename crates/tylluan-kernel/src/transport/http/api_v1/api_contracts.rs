use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use axum::{
    Json,
    extract::{State, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
use rusqlite::params;
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

// --- SQL persistence layer --------------------------------------------------

pub struct ContractDb {
    conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ContractDb {
    pub fn open(db_path: &str) -> anyhow::Result<Self> {
        let conn = crate::config::open_db(std::path::Path::new(db_path))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS work_contracts (
                 id               TEXT PRIMARY KEY,
                 task             TEXT NOT NULL,
                 budget           INTEGER NOT NULL,
                 budget_remaining INTEGER NOT NULL,
                 team             TEXT NOT NULL,
                 consolidator     TEXT NOT NULL,
                 channel_id       TEXT NOT NULL,
                 status           TEXT NOT NULL,
                 created_at       INTEGER NOT NULL,
                 deliveries       TEXT NOT NULL DEFAULT '[]',
                 votes            TEXT NOT NULL DEFAULT '[]',
                 extensions       INTEGER NOT NULL DEFAULT 0
             );",
        )?;
        Ok(Self { conn: Arc::new(std::sync::Mutex::new(conn)) })
    }

    pub fn persist(&self, c: &WorkContract) -> rusqlite::Result<()> {
        let remaining = c.budget_remaining.load(Ordering::Acquire);
        let team_json = serde_json::to_string(&c.team).unwrap_or_else(|_| "[]".into());
        let deliveries_json = serde_json::to_string(&c.deliveries).unwrap_or_else(|_| "[]".into());
        let votes_json = serde_json::to_string(&c.votes).unwrap_or_else(|_| "[]".into());
        self.conn.lock().expect("contracts db mutex poisoned").execute(
            "INSERT INTO work_contracts(
                 id,task,budget,budget_remaining,team,consolidator,
                 channel_id,status,created_at,deliveries,votes,extensions)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(id) DO UPDATE SET
                 budget_remaining=excluded.budget_remaining,
                 status=excluded.status,
                 deliveries=excluded.deliveries,
                 votes=excluded.votes,
                 extensions=excluded.extensions",
            params![
                c.id, c.task, c.budget, remaining,
                team_json, c.consolidator, c.channel_id, c.status,
                c.created_at as i64, deliveries_json, votes_json, c.extensions
            ],
        )?;
        Ok(())
    }

    pub fn load_active(&self) -> rusqlite::Result<Vec<WorkContract>> {
        let conn = self.conn.lock().expect("contracts db mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT id,task,budget,budget_remaining,team,consolidator,
                    channel_id,status,created_at,deliveries,votes,extensions
             FROM work_contracts WHERE status NOT IN ('done')",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, i32>(11)?,
            ))
        })?;

        let mut out = vec![];
        for row in rows {
            let (id, task, budget, remaining, team_j, consolidator,
                 channel_id, status, created_at, del_j, votes_j, extensions) = row?;
            out.push(WorkContract {
                id,
                task,
                budget,
                budget_remaining: AtomicI64::new(remaining),
                team: serde_json::from_str(&team_j).unwrap_or_default(),
                consolidator,
                channel_id,
                status,
                created_at: created_at as u64,
                deliveries: serde_json::from_str(&del_j).unwrap_or_default(),
                votes: serde_json::from_str(&votes_j).unwrap_or_default(),
                extensions,
            });
        }
        Ok(out)
    }
}

// --- Request/response types -------------------------------------------------

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

// --- In-memory registry (DashMap cache over SQLite) -------------------------

#[derive(Clone)]
pub struct ContractRegistry {
    pub contracts: Arc<DashMap<String, WorkContract>>,
}

impl ContractRegistry {
    pub fn new() -> Self {
        Self { contracts: Arc::new(DashMap::new()) }
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

// --- Handlers ---------------------------------------------------------------

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

    if let Some(c) = state.contract_registry.contracts.get(&id) {
        let _ = state.contract_db.persist(&*c);
    }

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
        let _ = state.contract_db.persist(&*entry);
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

    let _ = state.contract_db.persist(&*entry);
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

    let _ = state.contract_db.persist(&*entry);
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
        let _ = state.contract_db.persist(&*entry);
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "extended",
                "additional_cycles": median,
                "extensions": entry.extensions
            }))
        )
    } else if majority {
        let _ = state.contract_db.persist(&*entry);
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": &entry.status,
                "note": "majority approved but max extensions reached, human approval required"
            }))
        )
    } else {
        let _ = state.contract_db.persist(&*entry);
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

    let _ = state.contract_db.persist(&*entry);
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

        let entry = registry.contracts.get_mut("bwc-002").unwrap();
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
            let entry = registry.contracts.get_mut("bwc-003").unwrap();
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

    #[test]
    fn test_contract_db_roundtrip() {
        let db = ContractDb::open(":memory:").expect("in-memory db");
        let c = WorkContract {
            id: "bwc-rt-01".into(),
            task: "roundtrip test".into(),
            budget: 5,
            budget_remaining: AtomicI64::new(3),
            team: vec!["alpha".into(), "beta".into()],
            consolidator: "alpha".into(),
            channel_id: "chan-rt".into(),
            status: "in_progress".into(),
            created_at: 2000,
            deliveries: vec![ContractDelivery {
                agent_id: "beta".into(),
                summary: "done".into(),
                delivered_at: 2001,
            }],
            votes: vec![],
            extensions: 0,
        };
        db.persist(&c).expect("persist");
        let loaded = db.load_active().expect("load_active");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "bwc-rt-01");
        assert_eq!(loaded[0].budget_remaining.load(Ordering::Acquire), 3);
        assert_eq!(loaded[0].team.len(), 2);
        assert_eq!(loaded[0].deliveries.len(), 1);
    }

    #[test]
    fn test_contract_db_done_excluded() {
        let db = ContractDb::open(":memory:").expect("in-memory db");
        let mut c = WorkContract {
            id: "bwc-done-01".into(),
            task: "finished".into(),
            budget: 5,
            budget_remaining: AtomicI64::new(0),
            team: vec!["alice".into()],
            consolidator: "alice".into(),
            channel_id: "chan-x".into(),
            status: "done".into(),
            created_at: 3000,
            deliveries: vec![],
            votes: vec![],
            extensions: 0,
        };
        db.persist(&c).expect("persist done");
        let loaded = db.load_active().expect("load_active");
        assert_eq!(loaded.len(), 0, "done contracts must not be loaded");

        c.status = "in_progress".into();
        c.id = "bwc-active-01".into();
        c.budget_remaining.store(3, Ordering::Release);
        db.persist(&c).expect("persist active");
        let loaded2 = db.load_active().expect("load_active 2");
        assert_eq!(loaded2.len(), 1);
    }
}
