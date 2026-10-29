//! T-DSH-124 — AUD-04/05 a nivel handler sobre `api_contracts.rs`.
//!
//! Cubre: no miembro del roster, entrega duplicada, voto parcial, empate y
//! fallo de persistencia (500). Usa el harness compartido `support::test_state`.

mod support;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_contracts::{
    CloseRequest, CreateContractRequest, DeliverRequest, VoteRequest, contract_close_handler,
    contract_create_handler, contract_deliver_handler, contract_vote_handler,
};

const TEAM: [&str; 2] = ["alice", "bob"];

async fn new_contract(state: &Arc<HttpState>) -> String {
    let (status, Json(body)) = contract_create_handler(
        State(state.clone()),
        Json(CreateContractRequest {
            task: "tarea de prueba".to_string(),
            budget: Some(10),
            team: TEAM.iter().map(|s| s.to_string()).collect(),
            consolidator: "alice".to_string(),
            channel_id: "general".to_string(),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "create: {body}");
    body["contract_id"].as_str().expect("contract_id").to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deliver_rejects_agent_not_in_roster() {
    let state = support::test_state("contracts_audit_roster").await;
    let id = new_contract(&state).await;

    let (status, Json(body)) = contract_deliver_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(DeliverRequest {
            agent_id: "mallory".to_string(),
            summary: "no soy del equipo".to_string(),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "deliver: {body}");
    assert_eq!(body["error"], "agent not in contract roster");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deliver_rejects_duplicate_participant() {
    let state = support::test_state("contracts_audit_dup").await;
    let id = new_contract(&state).await;

    let (status, _) = contract_deliver_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(DeliverRequest {
            agent_id: "alice".to_string(),
            summary: "primera".to_string(),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let (status, Json(body)) = contract_deliver_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(DeliverRequest {
            agent_id: "alice".to_string(),
            summary: "segunda".to_string(),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT, "deliver: {body}");
    assert_eq!(body["error"], "duplicate delivery");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_vote_is_not_majority_over_roster() {
    let state = support::test_state("contracts_audit_partial").await;
    let id = new_contract(&state).await;

    // 1 voto de 1 emitido ya no es mayoria: el quorum se mide sobre el roster (2).
    let (status, Json(body)) = contract_vote_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(VoteRequest {
            agent_id: "alice".to_string(),
            vote: "approve".to_string(),
            cycles: Some(5),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "vote: {body}");
    assert_eq!(body["majority"], false, "1 de 2 no es mayoria: {body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tie_is_not_majority() {
    let state = support::test_state("contracts_audit_tie").await;
    let id = new_contract(&state).await;

    let (status, _) = contract_vote_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(VoteRequest {
            agent_id: "alice".to_string(),
            vote: "approve".to_string(),
            cycles: Some(5),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let (status, Json(body)) = contract_vote_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(VoteRequest {
            agent_id: "bob".to_string(),
            vote: "reject".to_string(),
            cycles: None,
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "vote: {body}");
    assert_eq!(body["majority"], false, "empate 1-1 no es mayoria: {body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vote_rejects_agent_not_in_roster() {
    let state = support::test_state("contracts_audit_vote_roster").await;
    let id = new_contract(&state).await;

    let (status, Json(body)) = contract_vote_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(VoteRequest {
            agent_id: "mallory".to_string(),
            vote: "approve".to_string(),
            cycles: Some(5),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "vote: {body}");
    assert_eq!(body["error"], "agent not in contract roster");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persist_failure_returns_500() {
    let dir = std::env::temp_dir().join(format!(
        "tylluan_contracts_audit_{}",
        support::test_counter_next()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("contracts.db");
    let db_path_str = db_path.to_string_lossy().to_string();

    let state = support::test_state_with_contract_db("contracts_audit_persist", &db_path_str).await;
    let id = new_contract(&state).await;

    // Fuerza el fallo: otra conexion a la misma DB dropea la tabla que usa persist().
    {
        let conn = tylluan_kernel::config::open_db(&db_path).expect("open contract db");
        conn.execute_batch("DROP TABLE work_contracts").expect("drop table");
    }

    let (status, Json(body)) = contract_deliver_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(DeliverRequest {
            agent_id: "alice".to_string(),
            summary: "esto no debe persistir".to_string(),
        }),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "persist: {body}"
    );
    assert_eq!(body["error"], "contract_persist_failed");
    assert_eq!(body["contract_id"], id);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_by_non_consolidator_is_forbidden() {
    let state = support::test_state("contracts_audit_close").await;
    let id = new_contract(&state).await;

    let (status, Json(body)) = contract_close_handler(
        State(state.clone()),
        Path(id.clone()),
        Json(CloseRequest {
            agent_id: "bob".to_string(),
            summary: "no soy consolidador".to_string(),
        }),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::FORBIDDEN, "close: {body}");
}
