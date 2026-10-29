//! B-9 — ¿puede un bearer cerrar el contrato de otro agente?
//!
//! `contract_close_handler` compara `req.agent_id` con el **consolidador del
//! contrato** (`only consolidator can close`), pero no con la identidad del
//! token (`current_bound_agent_id()`). Es decir: el rol está comprobado, la
//! identidad no. El bearer de `alice` puede escribir `agent_id: "bob"` en el
//! JSON y cerrar el contrato de `bob` como si fuera él.
//!
//! Por eso hace falta crear el contrato **con el bearer de `bob`**: el
//! consolidador y la única identidad legítima para cerrarlo son el mismo.
//!
//! Estado a 2026-10-29, base `7ed537e`: D-69 (que introduce
//! `bound_agent_mismatch` → 403 `agent_identity_mismatch` en `close`) **está
//! aceptado pero todavía no en `main`**, así que el caso de suplantación va con
//! `#[ignore]`. Cuando aterrice: se borra el atributo y el test debe pasar tal
//! cual, sin tocar aserciones. El control de arriba pasa hoy y debe seguir
//! pasando después del arreglo.
//!
//!     cargo test -p tylluan-kernel --test bwc_close_binding
//!     cargo test -p tylluan-kernel --test bwc_close_binding -- --ignored
//!
//! Fichero aparte de `bwc_agent_binding.rs` a propósito: ese fichero está siendo
//! editado en PUSH202 (D-69 añade `agent_id` al `create` y retira dos
//! `#[ignore]`), y tocar el mismo sitio desde dos árboles garantiza conflicto.
//! Aquí el `create` se construye desde JSON (`serde_json::from_value`), que
//! sobrevive a que se añadan campos opcionales al struct.

mod support;

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use tylluan_kernel::transport::http::HttpState;
use tylluan_kernel::transport::http::api_v1::api_contracts::{
    CloseRequest, CreateContractRequest, contract_close_handler, contract_create_handler,
};
use tylluan_kernel::transport::http::auth::ACL_AGENT_ID;

const ALICE: &str = "alice";
const BOB: &str = "bob";

/// Construido desde JSON a propósito: `CreateContractRequest` gana campos
/// opcionales con D-69 (`agent_id`, `#[serde(default)]`), y un literal de struct
/// dejaría de compilar cada vez que se añada uno. Aquí solo se exige lo
/// obligatorio, y omitir `agent_id` es justo el camino documentado: «si no
/// viene, el consolidador es la identidad que actúa».
fn create_request(consolidator: &str) -> CreateContractRequest {
    serde_json::from_value(serde_json::json!({
        "task": "tarea de regresion B-9",
        "budget": 10,
        "team": [ALICE, BOB],
        "consolidator": consolidator,
        "channel_id": "general",
    }))
    .expect("CreateContractRequest deberia deserializar desde JSON")
}

/// Crea un contrato **actuando como su consolidador**, que es la única forma
/// legítima: tras D-69 el `create` también liga la identidad declarada al
/// bearer.
async fn new_contract_as_consolidator(state: &Arc<HttpState>, consolidator: &str) -> String {
    let (status, Json(body)) = ACL_AGENT_ID
        .scope(consolidator.to_string(), async {
            contract_create_handler(
                State(state.clone()),
                Json(create_request(consolidator)),
            )
            .await
        })
        .await;
    assert_eq!(status, StatusCode::CREATED, "create: {body}");
    body["contract_id"]
        .as_str()
        .expect("contract_id")
        .to_string()
}

/// Control: el consolidador de verdad cierra su propio contrato. Pasa hoy y
/// tiene que seguir pasando después de cualquier arreglo de binding — si este
/// se rompe, el 403 del caso de abajo no significaría nada.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_true_consolidator_can_close_its_own_contract() {
    let state = support::test_state("bwc_close_control").await;
    let id = new_contract_as_consolidator(&state, BOB).await;

    let (status, Json(body)) = ACL_AGENT_ID
        .scope(BOB.to_string(), async {
            contract_close_handler(
                State(state.clone()),
                Path(id.clone()),
                Json(CloseRequest {
                    agent_id: BOB.to_string(),
                    summary: "cierro mi propio contrato".to_string(),
                }),
            )
            .await
        })
        .await;

    assert_eq!(
        status,
        StatusCode::OK,
        "el consolidador '{BOB}' debe poder cerrar su contrato. Respuesta: {body}"
    );
    assert_eq!(body["status"], "done", "el contrato debe quedar cerrado: {body}");
}

/// El bearer de `ALICE` no puede cerrar el contrato de `BOB` declarándose `bob`.
/// Hoy: 200 + contrato `done` (cerrado a nombre de `bob`). Esperado: 403.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn close_rejects_claim_of_another_bound_agent() {
    let state = support::test_state("bwc_close_spoof").await;
    // El contrato es legítimamente de BOB: se crea con su bearer.
    let id = new_contract_as_consolidator(&state, BOB).await;

    let (status, Json(body)) = ACL_AGENT_ID
        .scope(ALICE.to_string(), async {
            contract_close_handler(
                State(state.clone()),
                Path(id.clone()),
                Json(CloseRequest {
                    // El chequeo de rol (`agent_id == consolidator`) sí pasa:
                    // lo único que falta es mirar quién lo está pidiendo.
                    agent_id: BOB.to_string(),
                    summary: "cierro el contrato de bob haciendome pasar por el".to_string(),
                }),
            )
            .await
        })
        .await;

    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "el bearer ligado a '{ALICE}' no puede cerrar como '{BOB}'. Respuesta: {body}"
    );
}
