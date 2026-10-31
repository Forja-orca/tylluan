//! T-DSH-152 — persistencia de `AgentSession` en `silva.db`.
//!
//! Antes de este cambio, `AgentRegistry` vivía solo en RAM (`DashMap`): un
//! reinicio del kernel borraba todas las sesiones de watchers. Ahora cada
//! sesión se persiste en la tabla aditiva `agent_sessions` de SilvaDB y el
//! arranque la recarga (`AgentRegistry::rehydrate`).
//!
//! Los tests simulan el reinicio creando un `AgentRegistry` NUEVO (memoria
//! vacía) enlazado a la MISMA SilvaDB y llamando `rehydrate`: no se arranca
//! ningún proceso (regla de flota).

mod support;

use axum::Json;
use axum::extract::State;
use std::sync::atomic::Ordering;

use tylluan_kernel::transport::http::api_v1::api_agents::{
    self, AgentRegistry, HeartbeatRequest, StartSessionRequest, StopSessionRequest,
};

/// Crea una sesión por la API real, "reinicia" (registro nuevo, misma DB) y
/// comprueba que se recupera con sus campos intactos.
#[tokio::test(flavor = "multi_thread")]
async fn session_survives_restart_and_rehydrates_from_silva() {
    let state = support::test_state("t152_start").await;
    state.agent_registry.set_silva(state.silva.clone());

    let response = api_agents::agent_session_start_handler(
        State(state.clone()),
        Json(StartSessionRequest {
            agent_id: "watcher-1".to_string(),
            max_responses: 7,
        }),
    )
    .await;
    assert_eq!(response.status(), 200, "POST session/start debe aceptar");
    assert!(state.agent_registry.sessions.contains_key("watcher-1"));

    // Reinicio simulado: registro nuevo (RAM vacía), misma silva.db.
    let restarted = AgentRegistry::new(7200);
    restarted.set_silva(state.silva.clone());
    assert!(restarted.sessions.is_empty(), "el registro nuevo empieza vacío");

    let recovered = restarted.rehydrate().await.expect("rehydrate no debe fallar");
    assert_eq!(recovered, 1, "debe recuperar exactamente la sesión persistida");
    let session = restarted
        .sessions
        .get("watcher-1")
        .expect("la sesión debe estar en memoria tras rehydrate")
        .clone();
    assert_eq!(session.agent_id, "watcher-1");
    assert_eq!(session.max_responses, 7, "max_responses debe sobrevivir");
    assert_eq!(session.status, "active");
    assert_eq!(session.responses_sent, 0);
    assert!(
        restarted.is_session_valid("watcher-1"),
        "la sesión recuperada debe ser válida (active, dentro de TTL, con presupuesto)"
    );
    assert!(
        restarted.active.load(Ordering::Acquire),
        "el flag global active debe rearmarse al recuperar una sesión activa"
    );
}

/// El estado `stopped` también se persiste: tras el reinicio la sesión sigue
/// parada y no es válida.
#[tokio::test(flavor = "multi_thread")]
async fn stopped_session_stays_stopped_after_restart() {
    let state = support::test_state("t152_stop").await;
    state.agent_registry.set_silva(state.silva.clone());

    let start = api_agents::agent_session_start_handler(
        State(state.clone()),
        Json(StartSessionRequest {
            agent_id: "watcher-2".to_string(),
            max_responses: 3,
        }),
    )
    .await;
    assert_eq!(start.status(), 200);

    let stop = api_agents::agent_session_stop_handler(
        State(state.clone()),
        Json(StopSessionRequest {
            agent_id: "watcher-2".to_string(),
        }),
    )
    .await;
    assert_eq!(stop.status(), 200);

    let restarted = AgentRegistry::new(7200);
    restarted.set_silva(state.silva.clone());
    restarted.rehydrate().await.expect("rehydrate no debe fallar");
    let session = restarted
        .sessions
        .get("watcher-2")
        .expect("la sesión parada también debe recuperarse")
        .clone();
    assert_eq!(session.status, "stopped");
    assert!(
        !restarted.is_session_valid("watcher-2"),
        "una sesión stopped no debe considerarse válida tras el reinicio"
    );
}

/// El heartbeat actualiza `last_heartbeat_unix` y ese valor se persiste.
#[tokio::test(flavor = "multi_thread")]
async fn heartbeat_updates_persisted_timestamp() {
    let state = support::test_state("t152_heartbeat").await;
    state.agent_registry.set_silva(state.silva.clone());

    let start = api_agents::agent_session_start_handler(
        State(state.clone()),
        Json(StartSessionRequest {
            agent_id: "watcher-3".to_string(),
            max_responses: 5,
        }),
    )
    .await;
    assert_eq!(start.status(), 200);

    let beat = api_agents::agent_heartbeat_handler(
        State(state.clone()),
        Json(HeartbeatRequest {
            agent_id: "watcher-3".to_string(),
        }),
    )
    .await;
    assert_eq!(beat.status(), 200, "el heartbeat debe aceptarse");

    let restarted = AgentRegistry::new(7200);
    restarted.set_silva(state.silva.clone());
    restarted.rehydrate().await.expect("rehydrate no debe fallar");
    let session = restarted.sessions.get("watcher-3").expect("sesión recuperada").clone();
    assert!(
        session.last_heartbeat_unix >= session.started_at_unix,
        "el heartbeat persistido no puede ser anterior al arranque"
    );
}

/// Sin SilvaDB enlazada (estados de test que no la necesitan) el registro
/// sigue funcionando en memoria: `persist_session`/`rehydrate` son no-ops.
#[tokio::test(flavor = "multi_thread")]
async fn registry_without_silva_keeps_working_in_memory() {
    let registry = AgentRegistry::new(7200);
    assert_eq!(registry.rehydrate().await.expect("no-op"), 0);
    let session = tylluan_kernel::transport::http::api_v1::api_agents::AgentSession {
        agent_id: "mem-only".to_string(),
        started_at_unix: 1,
        last_heartbeat_unix: 1,
        responses_sent: 0,
        max_responses: 1,
        status: "active".to_string(),
    };
    registry.persist_session(&session).await;
    assert!(registry.sessions.is_empty(), "sin silva no hay rehidratación");
}
