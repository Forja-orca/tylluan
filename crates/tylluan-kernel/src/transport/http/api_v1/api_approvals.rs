//! `GET /api/v1/approvals` — mitad de lectura del contrato v2 «juicios pendientes».
//!
//! Une los **dos** almacenes que hoy existen por separado: grants
//! (`security::grants::list_pending`, peticiones vivas) y planes
//! (`security::grants::list_plans`, trabajo pendiente de aprobación). Sin esta
//! unión, la mitad de lo que espera criterio es invisible (un plan pendiente real
//! en producción no aparecía en ningún sitio).
//!
//! Solo lectura: no aprueba, no rechaza, no pone en duda. Las acciones llegan en
//! la segunda tanda del contrato.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;

use crate::transport::http::auth::{current_auth_kind, current_bound_agent_id, operator_gate_allows};
use crate::transport::http::HttpState;

/// Marca cada entrada con `kind`, `can_*` y el estado, **en el servidor**.
///
/// Puro: el cliente no decide permisos y el test no necesita HTTP.
pub(crate) fn unify_approvals(
    grants: Vec<serde_json::Value>,
    plans: Vec<serde_json::Value>,
    requester: &str,
    operator: bool,
) -> Vec<serde_json::Value> {
    let mut out: Vec<serde_json::Value> = Vec::with_capacity(grants.len() + plans.len());
    for mut item in grants {
        let autor = item.get("agent_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if let Some(obj) = item.as_object_mut() {
            obj.insert("kind".into(), serde_json::json!("grant"));
            obj.insert("state".into(), serde_json::json!("pending"));
            obj.insert("can_approve".into(), serde_json::json!(puede_aprobar(&autor, requester, operator)));
            // hold_plan solo conoce planes: poner en duda un grant devolveria
            // not_found, asi que no se anuncia como posible.
            obj.insert("can_hold".into(), serde_json::json!(false));
            obj.insert("can_withdraw".into(), serde_json::json!(false));
        }
        out.push(item);
    }
    for mut item in plans {
        let autor = item.get("agent_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let retirado_por_su_autor = !requester.is_empty() && autor == requester;
        if let Some(obj) = item.as_object_mut() {
            // El contrato exige `kind` y `state` también en planes: la función
            // no debe depender de que el origen los traiga ya puestos.
            obj.insert("kind".into(), serde_json::json!("plan"));
            obj.entry("state".to_string()).or_insert(serde_json::json!("pending"));
            obj.insert("can_approve".into(), serde_json::json!(puede_aprobar(&autor, requester, operator)));
            obj.insert("can_hold".into(), serde_json::json!(true));
            obj.insert("can_withdraw".into(), serde_json::json!(retirado_por_su_autor));
        }
        out.push(item);
    }
    out
}

/// D-13: nadie aprueba su propio plan salvo el operador.
fn puede_aprobar(autor: &str, requester: &str, operator: bool) -> bool {
    if operator {
        return true;
    }
    let anonimo = |quien: &str| quien.is_empty() || quien == "anonymous";
    !(autor == requester || (anonimo(autor) && anonimo(requester)))
}

#[derive(serde::Deserialize, Default)]
pub struct ApproveRequest {
    /// `this_time` (por defecto) | `this_session` | `always_for_guild`.
    #[serde(default)]
    pub grant_level: Option<String>,
    /// Motivo para la auditoría (opcional al aprobar).
    #[serde(default)]
    pub reason: String,
}

/// `POST /api/v1/approvals/{id}/approve`.
///
/// **Delega en `approve_action`** a propósito: ahí viven la guarda D-13
/// (autoaprobación), la ejecución del plan almacenado y la auditoría con
/// `human_intervention`. Duplicar ese camino aquí crearía dos formas de aprobar que
/// mañana discrepan.
pub async fn approve_handler(
    State(state): State<Arc<HttpState>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<ApproveRequest>,
) -> impl IntoResponse {
    let Some(server_arc) = state.server.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "kernel server not initialized" })),
        )
            .into_response();
    };
    let mut args = serde_json::Map::new();
    args.insert("requestId".to_string(), serde_json::json!(id));
    args.insert("approved".to_string(), serde_json::json!(true));
    args.insert(
        "grant_level".to_string(),
        serde_json::json!(req.grant_level.clone().unwrap_or_else(|| "this_time".to_string())),
    );
    if let Some(who) = current_bound_agent_id().filter(|w| !w.is_empty()) {
        args.insert("agent_id".to_string(), serde_json::json!(who));
    }
    if !req.reason.trim().is_empty() {
        args.insert("reason".to_string(), serde_json::json!(req.reason.trim()));
    }

    let server = server_arc.read().await;
    let result = server.handle_kernel_tool("approve_action", Some(args)).await;
    let text = match result {
        Ok(res) => {
            let cuerpo = res
                .content
                .iter()
                .filter_map(|c| c.as_text())
                .map(|t| t.text.clone())
                .collect::<Vec<_>>()
                .join("
");
            if res.is_error == Some(true) {
                // `self_approval` es 403, no 400: es una regla, no una petición mal formada.
                let status = if cuerpo.contains("self_approval") {
                    StatusCode::FORBIDDEN
                } else if cuerpo.contains("not found") || cuerpo.contains("not_found") {
                    StatusCode::NOT_FOUND
                } else {
                    StatusCode::BAD_REQUEST
                };
                return (status, Json(serde_json::json!({ "error": cuerpo }))).into_response();
            }
            cuerpo
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("{e}") })),
            )
                .into_response();
        }
    };
    let payload: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "result": text }));
    Json(serde_json::json!({ "status": "approved", "id": id, "result": payload })).into_response()
}

#[derive(serde::Deserialize, Default)]
pub struct JudgementRequest {
    /// Motivo de la decisión. **Obligatorio en `hold`**: un «no me convence» sin
    /// motivo no sirve para volver a mirarlo.
    #[serde(default)]
    pub reason: String,
}

/// Auditoría de una decisión HTTP de juicio.
///
/// `approve` delega en `approve_action` (que ya audita); las otras tres no pasaban
/// por ahí y **no dejaban rastro** (T4361/T4391). `human_intervention` es el mismo
/// criterio que en la tool: solo credencial de clase operador cuenta como humano.
fn auditar_juicio(
    accion: &str,
    guild: &str,
    tool: &str,
    quien: &str,
    detalle: &str,
    operator: bool,
) {
    crate::transport::server::handler_do::audit::audit_or_warn(
        &format!("approvals:{accion}"),
        guild,
        tool,
        quien,
        true,
        detalle,
        0,
        operator,
    );
}

/// `POST /api/v1/approvals/{id}/reject` — descarta sin ejecutar.
pub async fn reject_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<JudgementRequest>,
) -> impl IntoResponse {
    let plan = crate::security::grants::get_plan(&id).await;
    let quitado = if plan.is_some() {
        crate::security::grants::remove_plan(&id).await
    } else {
        crate::security::grants::remove(&id).await
    };
    if !quitado {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found", "id": id })),
        )
            .into_response();
    }
    let quien = current_bound_agent_id().unwrap_or_default();
    let (guild, tool) = match &plan {
        Some(p) => (p.guild.as_str(), p.tool.as_str()),
        None => ("grant", "grant"),
    };
    auditar_juicio(
        "reject",
        guild,
        tool,
        &quien,
        &format!("rechazado: {}", req.reason.trim()),
        operator_gate_allows(current_auth_kind()),
    );
    tracing::info!("approvals: '{id}' rechazado por HTTP — {}", req.reason.trim());
    Json(serde_json::json!({
        "status": "rejected",
        "kind": if plan.is_some() { "plan" } else { "grant" },
        "reason": req.reason.trim(),
    }))
    .into_response()
}

/// `POST /api/v1/approvals/{id}/hold` — deja el plan **en duda**, sin consumirlo.
///
/// Jose 2026-11-15: dudar es una decisión, no un error. `reason` obligatorio.
pub async fn hold_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<JudgementRequest>,
) -> impl IntoResponse {
    let reason = req.reason.trim().to_string();
    if reason.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "hold_requires_reason",
                "detail": "escribe qué falta para poder juzgar esta acción",
                "id": id,
            })),
        )
            .into_response();
    }
    if crate::security::grants::hold_plan(&id, &reason).await {
        let quien = current_bound_agent_id().unwrap_or_default();
        let (guild, tool, autoria) = match crate::security::grants::get_plan(&id).await {
            Some(p) => (p.guild, p.tool, p.agent_id),
            None => ("plan".to_string(), "plan".to_string(), String::new()),
        };
        auditar_juicio(
            "hold",
            &guild,
            &tool,
            &quien,
            &format!("en duda (autor: {autoria}): {reason}"),
            operator_gate_allows(current_auth_kind()),
        );
        return Json(serde_json::json!({
            "status": "held",
            "id": id,
            "reason": reason,
            "note": "el plan sigue pendiente: no se ha ejecutado ni rechazado",
        }))
        .into_response();
    }
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": "not_found", "id": id })),
    )
        .into_response()
}

/// `POST /api/v1/approvals/{id}/withdraw` — el autor retira su propio plan.
pub async fn withdraw_handler(
    axum::extract::Path(id): axum::extract::Path<String>,
    Json(req): Json<JudgementRequest>,
) -> impl IntoResponse {
    let Some(plan) = crate::security::grants::get_plan(&id).await else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "not_found", "id": id })),
        )
            .into_response();
    };
    // T4361: sin identidad NO se retira nada. Antes `!quien.is_empty() && …` hacía
    // que una petición sin identidad se saltara la comprobación de autor.
    let Some(quien) = current_bound_agent_id().filter(|w| !w.trim().is_empty()) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthenticated",
                "detail": "retirar un plan exige identidad: no hay credencial ligada a esta petición",
                "id": id,
            })),
        )
            .into_response();
    };
    if plan.agent_id != quien {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "not_author",
                "id": id,
                "author": plan.agent_id,
                "requester": quien,
                "detail": "solo el autor retira su propio plan; para el resto existe hold o reject",
            })),
        )
            .into_response();
    }
    if crate::security::grants::withdraw_plan(&id).await {
        auditar_juicio(
            "withdraw",
            &plan.guild,
            &plan.tool,
            &quien,
            &format!("retirado por su autor: {}", req.reason.trim()),
            operator_gate_allows(current_auth_kind()),
        );
        return Json(serde_json::json!({
            "status": "withdrawn",
            "id": id,
            "reason": req.reason.trim(),
        }))
        .into_response();
    }
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({ "error": "already_resolved", "id": id })),
    )
        .into_response()
}

/// `GET /api/v1/approvals`
pub async fn list_approvals_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    let requester = current_bound_agent_id().unwrap_or_default();
    let operator = operator_gate_allows(current_auth_kind());
    let grants = crate::security::grants::list_pending().await;
    let plans = crate::security::grants::list_plans().await;
    let approvals = unify_approvals(grants, plans, &requester, operator);
    Json(serde_json::json!({
        "approvals": approvals,
        "count": approvals.len(),
        "judgement": crate::security::grants::judgement_counters(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(agent: &str) -> serde_json::Value {
        serde_json::json!({"id": "g1", "guild": "fs", "tool_name": "write", "agent_id": agent})
    }

    fn plan(agent: &str) -> serde_json::Value {
        serde_json::json!({"id": "p1", "guild": "bash", "tool_name": "run", "agent_id": agent, "state": "pending"})
    }

    /// Los dos almacenes salen en la misma lista, cada uno con su `kind`.
    #[test]
    fn une_grants_y_planes_con_su_kind() {
        let out = unify_approvals(vec![grant("a")], vec![plan("a")], "b", false);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["kind"], "grant");
        assert_eq!(out[1]["kind"], "plan");
        // hold solo existe para planes: un grant no anuncia una accion que fallaria.
        assert_eq!(out[0]["can_hold"], false);
        assert_eq!(out[1]["can_hold"], true);
    }

    /// D-13 en la respuesta: el autor de un plan no puede aprobarlo…
    #[test]
    fn el_autor_no_puede_aprobar_su_plan() {
        let out = unify_approvals(vec![], vec![plan("deep")], "deep", false);
        assert_eq!(out[0]["can_approve"], false);
        // …pero sí puede ponerlo en duda y retirarlo.
        assert_eq!(out[0]["can_hold"], true);
        assert_eq!(out[0]["can_withdraw"], true);
    }

    /// …ni un agente anónimo sobre otro anónimo (misma regla que aprobar).
    #[test]
    fn anonimo_sobre_anonimo_tampoco() {
        let out = unify_approvals(vec![], vec![plan("anonymous")], "anonymous", false);
        assert_eq!(out[0]["can_approve"], false);
    }

    /// El operador sí puede aprobar lo suyo.
    #[test]
    fn el_operador_puede_aprobar_lo_suyo() {
        let out = unify_approvals(vec![], vec![plan("deep")], "deep", true);
        assert_eq!(out[0]["can_approve"], true);
    }

    /// `withdraw` es solo para el autor (un tercero ve false).
    #[test]
    fn withdraw_solo_para_el_autor() {
        let out = unify_approvals(vec![], vec![plan("deep")], "buffy", false);
        assert_eq!(out[0]["can_withdraw"], false);
        assert_eq!(out[0]["can_approve"], true, "un plan ajeno sí se puede aprobar");
    }
}
