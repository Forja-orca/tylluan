//! MCP Apps extension metadata and the first Tylluan UI resource.
//!
//! The HTTP transport owns the MCP Apps wire representation because the
//! workspace currently pins rmcp 0.1.x, whose `Tool` model has no `_meta`
//! field.  Keeping this here lets Streamable HTTP expose the standard
//! extension without widening the five-tool sovereign contract or forcing a
//! transport-wide rmcp upgrade.

use serde_json::{json, Value};

pub const MCP_APPS_EXTENSION: &str = "io.modelcontextprotocol/ui";
pub const MCP_APP_MIME: &str = "text/html;profile=mcp-app";
pub const GRAPH_APP_URI: &str = "ui://tylluan/knowledge-graph-canvas";

pub const GRAPH_APP_HTML: &str = include_str!("mcp_apps/knowledge_graph.html");

/// Return true when a request explicitly advertises the MCP Apps extension
/// with the MIME type required by the stable Apps specification.
///
/// Legacy MCP puts capabilities under `params.capabilities`; stateless MCP
/// carries them under `params._meta.io.modelcontextprotocol/clientCapabilities`.
pub fn client_supports_mcp_apps(payload: &Value) -> bool {
    let params = payload.get("params").unwrap_or(&Value::Null);
    let capabilities = params
        .get("_meta")
        .and_then(|meta| meta.get("io.modelcontextprotocol/clientCapabilities"))
        .or_else(|| params.get("capabilities"));

    capabilities
        .and_then(|caps| caps.get("extensions"))
        .and_then(|extensions| extensions.get(MCP_APPS_EXTENSION))
        .and_then(|extension| extension.get("mimeTypes"))
        .and_then(Value::as_array)
        .is_some_and(|mime_types| mime_types.iter().any(|mime| mime == MCP_APP_MIME))
}

pub fn graph_tool_meta() -> Value {
    json!({
        "ui": {
            "resourceUri": GRAPH_APP_URI,
            "visibility": ["model", "app"]
        }
    })
}

pub fn graph_resource_descriptor() -> Value {
    json!({
        "uri": GRAPH_APP_URI,
        "name": "tylluan_knowledge_graph_canvas",
        "description": "Interactive, self-contained graph view for tylluan_graph results.",
        "mimeType": MCP_APP_MIME,
        "_meta": {
            "ui": {
                "csp": {
                    "connectDomains": [],
                    "resourceDomains": [],
                    "frameDomains": [],
                    "baseUriDomains": []
                },
                "prefersBorder": true
            }
        }
    })
}

/// Serialize rmcp tools and add the Apps manifest only to the graph tool.
/// The exact five sovereign tools and their schemas remain unchanged.
pub fn tools_json(tools: &[rmcp::model::Tool], apps_enabled: bool) -> Value {
    let mut tools = tools
        .iter()
        .map(|tool| serde_json::to_value(tool).unwrap_or_else(|_| json!({})))
        .collect::<Vec<_>>();

    if apps_enabled
        && let Some(graph) = tools
            .iter_mut()
            .find(|tool| tool.get("name").and_then(Value::as_str) == Some("tylluan_graph"))
    {
        graph["_meta"] = graph_tool_meta();
    }

    Value::Array(tools)
}

pub fn graph_structured_content(result: &rmcp::model::CallToolResult) -> Option<Value> {
    result.content.iter().find_map(|content| {
        content
            .as_text()
            .and_then(|text| serde_json::from_str::<Value>(&text.text).ok())
    })
}
