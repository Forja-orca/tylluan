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

use axum::{extract::State, response::IntoResponse, Json};
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
