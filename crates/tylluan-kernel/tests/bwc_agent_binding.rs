//! B-1 / LEAD-01 — BWC no liga el `agent_id` del payload a la identidad del bearer.
//!
//! Los seis handlers de `api_contracts.rs` leen `agent_id` del JSON y nunca
//! consultan `current_bound_agent_id()`. La ligadura token→agente sí se aplica
//! en `a2a.rs`, `api_memory.rs`, `mcp.rs`, `api_v1/mod.rs` y
//! `server/handlers.rs`; los contratos son la excepción.
//!
//! Evidencia en vivo (kernel de prueba, 47093, 2026-10-28): con el bearer
//! ligado a `agentA`, un contrato desechable aceptó `deliver` y `vote` con
//! `agent_id: "agentB"` a 200 y el `GET` del contrato quedó con entregas y
//! voto a nombre de `agentB`.
//!
//! AUD-04 (`0441cda`) añadió el chequeo de roster, que **no** cubre esto: basta
//! con reclamar el nombre de otro miembro del equipo. Además, ahora que hay una
//! sola entrega por participante, suplantar a `bob` consume el turno de `bob` y
//! le devuelve 409 cuando él lo intente de verdad.
//!
//! Los dos casos de aquí abajo describen la conducta correcta y por eso
//! **fallan hoy**: van con `#[ignore]` para que el árbol siga verde sin borrar
//! la deuda. Quien arregle el handler borra el atributo y el test debe pasar
//! tal cual, sin tocarlo.
//!
//!     cargo test -p tylluan-kernel --test bwc_agent_binding -- --ignored
//!     cargo test -p tylluan-kernel --test bwc_agent_binding
//!
//! No cubierto aquí: `close` tiene el mismo hueco (el consolidador se elige en
//! el `create`, así que suplantarlo es escribir su nombre en el JSON) pero no
//! se reprodujo en vivo, y `tick` solo avanza la máquina de estados.

mod support;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_contracts::{
    CreateContractRequest, DeliverRequest, VoteRequest, contract_create_handler,
    contract_deliver_handler, contract_vote_handler,
};
use tylluan_kernel::transport::http::auth::{ACL_AGENT_ID, current_bound_agent_id};

/// El bearer pertenece a `ALICE`; `BOB` es el otro miembro del roster.
const ALICE: &str = "alice";
const BOB: &str = "bob";

async fn new_contract(state: &Arc<HttpState>) -> String {
    let (status, Json(body)) = contract_create_handler(
        State(state.clone()),
        Json(CreateContractRequest {
            task: "tarea de regresion B-1".to_string(),
            budget: Some(10),
            team: vec![ALICE.to_string(), BOB.to_string()],
            consolidator: ALICE.to_string(),
            channel_id: "general".to_string(),
            agent_id: None,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create: {body}");
    body["contract_id"].as_str().expect("contract_id").to_string()
}

/// Premisa del harness: el middleware instala `ACL_AGENT_ID` con el agente
/// resuelto del bearer, y fuera de ese scope no hay identidad (fail-closed en
/// los handlers que sí la consultan). Si esto se rompe, los tests de abajo
/// dejarían de probar nada.
#[tokio::test]
async fn bound_agent_is_visible_inside_scope_and_absent_outside() {
    assert_eq!(
        current_bound_agent_id(),
        None,
        "sin scope no hay identidad ligada: los handlers que la consultan fallan cerrado"
    );

    ACL_AGENT_ID
        .scope(ALICE.to_string(), async {
            assert_eq!(current_bound_agent_id().as_deref(), Some(ALICE));
            // Vacío = token sin ligadura (comportamiento retrocompatible).
            ACL_AGENT_ID
                .scope(String::new(), async {
                    assert_eq!(current_bound_agent_id(), None);
                })
                .await;
        })
        .await;

    // El scope no se filtra a la tarea de fuera.
    assert_eq!(current_bound_agent_id(), None);
}

/// El bearer de `ALICE` no puede entregar como `BOB`.
/// Hoy: 200 + entrega a nombre de `BOB`. Esperado: 403.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deliver_rejects_claim_of_another_bound_agent() {
    let state = support::test_state("bwc_agent_binding_deliver").await;
    let id = new_contract(&state).await;

    let (status, Json(body)) = ACL_AGENT_ID
        .scope(ALICE.to_string(), async {
            contract_deliver_handler(
                State(state.clone()),
                Path(id.clone()),
                Json(DeliverRequest {
                    // BOB está en el roster, así que el chequeo de AUD-04 lo
                    // deja pasar: lo único que falta es mirar quién lo pide.
                    agent_id: BOB.to_string(),
                    summary: "me hago pasar por bob".to_string(),
                }),
            )
            .await
        })
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "el bearer ligado a '{ALICE}' no puede entregar como '{BOB}'. Respuesta: {body}"
    );
}

/// El bearer de `ALICE` no puede votar como `BOB`.
/// Hoy: 200 + voto a nombre de `BOB`. Esperado: 403.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn vote_rejects_claim_of_another_bound_agent() {
    let state = support::test_state("bwc_agent_binding_vote").await;
    let id = new_contract(&state).await;

    let (status, Json(body)) = ACL_AGENT_ID
        .scope(ALICE.to_string(), async {
            contract_vote_handler(
                State(state.clone()),
                Path(id.clone()),
                Json(VoteRequest {
                    agent_id: BOB.to_string(),
                    vote: "approve".to_string(),
                    cycles: Some(0),
                }),
            )
            .await
        })
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "el bearer ligado a '{ALICE}' no puede votar como '{BOB}'. Respuesta: {body}"
    );
}
