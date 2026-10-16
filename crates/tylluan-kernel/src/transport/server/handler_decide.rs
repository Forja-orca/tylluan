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

pub async fn handle_tylluan_decide(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let args = serde_json::Value::Object(arguments.unwrap_or_default());
    let req: DecideRequest = match serde_json::from_value(args) {
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
