//! # MCP handler for `tylluan_decide` (ADR-021, 6th sovereign tool)
//!
//! Thin surface over [`crate::router::decision::DecisionService`]: parse the
//! arguments into the canonical [`DecideRequest`], call the service, and return
//! the canonical [`DecideResponse`] as JSON. The REST twin
//! (`POST /api/v1/decide`) shares the same types and the same service call, so
//! both doors answer identically. All validation, the global switch and the
//! audit row live in the service, not here.

use rmcp::{Error as McpError, model::*};

use super::TylluanServer;
use crate::registry::proxy::error_result;
use crate::router::decision::{DecideRequest, DecideStatus};

/// T-DSH-122: parseo puro de los argumentos MCP (testeable sin servidor).
pub(crate) fn parse_decide_arguments(args: serde_json::Value) -> Result<DecideRequest, String> {
    serde_json::from_value(args).map_err(|e| e.to_string())
}

pub async fn handle_tylluan_decide(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let args = serde_json::Value::Object(arguments.unwrap_or_default());
    let req: DecideRequest = match parse_decide_arguments(args) {
        Ok(r) => r,
        Err(e) => return Ok(error_result(&format!("Invalid tylluan_decide arguments: {e}"))),
    };

    let resp = server.decision_service.decide(req, "mcp").await;
    let is_error = resp.status == DecideStatus::Error;
    let text = serde_json::to_string_pretty(&resp).unwrap_or_default();
    Ok(CallToolResult {
        content: vec![Content::text(text)],
        is_error: Some(is_error),
    })
}


#[cfg(test)]
mod t122_mcp_sensitivity_tests {
    use super::*;

    fn args(sensitivity: Option<&str>) -> serde_json::Value {
        let mut v = serde_json::json!({
            "state": {"task": "t"},
            "questions": {"q": {"kind": "boolean"}}
        });
        if let Some(s) = sensitivity {
            v["sensitivity"] = serde_json::json!(s);
        }
        v
    }

    #[test]
    fn mcp_old_wire_without_sensitivity_still_parses() {
        let req = parse_decide_arguments(args(None)).unwrap();
        assert!(req.sensitivity.is_none());
    }

    #[test]
    fn mcp_sensitivity_is_parsed() {
        let req = parse_decide_arguments(args(Some("clinical"))).unwrap();
        assert_eq!(req.sensitivity.as_deref(), Some("clinical"));
    }

    #[test]
    fn mcp_invalid_sensitivity_is_rejected_by_validation() {
        let req = parse_decide_arguments(args(Some("secret"))).unwrap();
        assert!(crate::router::decision::api_types::validate_request(&req).is_err());
    }
}
