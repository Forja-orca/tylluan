use rmcp::{Error as McpError, model::*};
use tracing::info;
use serde_json;
use chrono;

use crate::registry::proxy::error_result;
use super::utils::json_pretty;
use super::TylluanServer;

pub async fn handle_tylluan_graph(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let command = arguments.as_ref().and_then(|a| a.get("command").or_else(|| a.get("action"))).and_then(|v| v.as_str()).unwrap_or("stats").to_string();
    let agent_id = arguments.as_ref().and_then(|a| a.get("agent_id")).and_then(|v| v.as_str()).unwrap_or("tylluan_graph").to_string();

    // STIGMERGY: Mark agent as actively interacting with the graph
    let agent_node_id = format!("agent:{agent_id}");
    let _ = server.silva.touch_node(&agent_node_id, &agent_id, "tylluan_graph").await;

    match command.as_str() {
        "add_triple" => {
            let subject = arguments.as_ref().and_then(|a| a.get("subject")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let predicate = arguments.as_ref().and_then(|a| a.get("predicate")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let object = arguments.as_ref().and_then(|a| a.get("object")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if subject.is_empty() || predicate.is_empty() || object.is_empty() {
                return Ok(error_result("add_triple requires 'subject', 'predicate', and 'object'."));
            }
            let metadata = serde_json::json!({"timestamp": chrono::Utc::now().to_rfc3339(), "source": "tylluan_graph", "agent": agent_id}).to_string();
            let _ = server.silva.upsert_node_with_provenance(&subject, "concept", &subject, &metadata, "agent_generated").await;
            let _ = server.silva.upsert_node_with_provenance(&object, "concept", &object, &metadata, "agent_generated").await;
            match server.silva.add_edge(&subject, &object, &predicate, 1.0, &metadata).await {
                Ok(_) => {
                    info!("🌲 tylluan_graph: added triple {} -[{}]-> {}", subject, predicate, object);
                    server.edge_added(&subject, &object, &predicate, 1.0);
                    let _ = server.silva.touch_node(&subject, &agent_id, "add_triple").await;
                    let _ = server.silva.touch_node(&object, &agent_id, "add_triple").await;

                    if let Ok(mut h) = server.hormones.lock() {
                        h.emit_novelty(0.2);
                    }

                    let node_count = server.silva.get_detailed_stats().await.map(|s| s["node_count"].as_i64().unwrap_or(0)).unwrap_or(0);

                    Ok(CallToolResult {
                        content: vec![Content::text(serde_json::json!({
                            "added": true,
                            "triple": { "subject": subject, "predicate": predicate, "object": object },
                            "total_nodes": node_count
                        }).to_string())],
                        is_error: Some(false)
                    })
                }
                Err(e) => Ok(error_result(&format!("Failed to add triple: {e}"))),
            }
        }
        "query" => {
            let subject = arguments.as_ref().and_then(|a| a.get("subject")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if subject.is_empty() { return Ok(error_result("query requires 'subject'.")); }

            let _ = server.silva.touch_node(&subject, &agent_id, "query").await;

            // FASE 2 (2026-08-20, convención warnings de docs/reference/MCP_WARNINGS_CONVENTION.md):
            // 'query' es búsqueda híbrida, no lookup exacto. Si el subject no es
            // un nodo real, los resultados son matches semánticos, NO triples
            // exactos del subject — eso es un aviso que el caller debe conocer,
            // no un error (los resultados siguen siendo útiles).
            let resolved = server.silva.existing_node_ids(&[subject.clone()]).await.unwrap_or_default();
            let mut warnings: Vec<serde_json::Value> = Vec::new();
            if !resolved.contains(&subject) {
                warnings.push(serde_json::json!({
                    "code": "NODE_NOT_FOUND",
                    "severity": "warn",
                    "message": format!("'{subject}' is not a real node ID — results below are semantic matches via hybrid search, not exact triples of the subject."),
                    "suggestion": "Use tylluan_graph(command='add_triple') to create the node, or list_neighbors/stats to discover valid IDs."
                }));
            }

            let nodes_with_scores = server.silva.search_hybrid(&subject, None, 5, None, false).await.unwrap_or_default();
            let mut triples = Vec::new();

            for (node, _) in nodes_with_scores {
                if let Ok(ctx) = server.silva.get_context(&node.id, 1).await {
                    for neighbor in ctx {
                        if neighbor.id != node.id {
                            triples.push(serde_json::json!({
                                "subject": node.id,
                                "predicate": "relates_to",
                                "object": neighbor.id
                            }));
                        }
                    }
                }
            }

            let mut payload = serde_json::json!({
                "query": subject,
                "results": triples,
                "count": triples.len()
            });
            if !warnings.is_empty() {
                payload["warnings"] = serde_json::Value::Array(warnings);
            }

            Ok(CallToolResult {
                content: vec![Content::text(payload.to_string())],
                is_error: Some(false)
            })
        }
        "list_neighbors" => {
            let entity = arguments.as_ref().and_then(|a| a.get("entity")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if entity.is_empty() { return Ok(error_result("list_neighbors requires 'entity'.")); }
            let mut neighbors = Vec::new();
            if let Ok(context_nodes) = server.silva.get_context(&entity, 1).await {
                for node in context_nodes {
                    if node.id != entity {
                        neighbors.push(serde_json::json!({ "id": node.id, "type": node.node_type, "content_preview": node.content.chars().take(100).collect::<String>() }));
                    }
                }
            }
            Ok(CallToolResult { content: vec![Content::text(json_pretty(&serde_json::json!({ "entity": entity, "neighbors": neighbors, "edge_count": neighbors.len() })))], is_error: Some(false) })
        }
        "stats" => {
            match server.silva.get_detailed_stats().await {
                Ok(stats) => Ok(CallToolResult {
                    content: vec![Content::text(serde_json::to_string_pretty(&stats).unwrap_or_default())],
                    is_error: Some(false)
                }),
                Err(e) => Ok(error_result(&format!("Failed to get stats: {e}"))),
            }
        }
        "query_path" => {
            let subject = arguments.as_ref().and_then(|a| a.get("subject")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let object = arguments.as_ref().and_then(|a| a.get("object")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let max_depth = arguments.as_ref()
                .and_then(|a| a.get("max_depth"))
                .or_else(|| arguments.as_ref().and_then(|a| a.get("depth")))
                .and_then(|v| v.as_u64())
                .unwrap_or(6)
                .min(12) as usize;
            if subject.is_empty() || object.is_empty() { return Ok(error_result("query_path requires 'subject' and 'object'.")); }
            match server.silva.shortest_path(&subject, &object, max_depth).await {
                Ok(path) => {
                    let path = path.unwrap_or_default();
                    Ok(CallToolResult {
                        content: vec![Content::text(json_pretty(&serde_json::json!({
                            "source": subject,
                            "target": object,
                            "found": !path.is_empty(),
                            "max_depth": max_depth,
                            "hops": path.len().saturating_sub(1),
                            "path": path
                        })))],
                        is_error: Some(false)
                    })
                }
                Err(e) => Ok(error_result(&format!("Failed to query path: {e}"))),
            }
        }
        "retrograde_extract" => {
            let limit = arguments.as_ref()
                .and_then(|a| a.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as usize;
            
            // STIGMERGY: Record the intent to refine memory
            let _ = server.silva.touch_node(&agent_node_id, &agent_id, "retrograde_extract").await;

            let silva_clone = server.silva.clone();
            let reg_clone = server.registry.clone();
            
            tokio::spawn(async move {
                let edges_added = silva_clone.retrograde_extract_triples(limit, |snippet: String| {
                    let reg = reg_clone.clone();
                    async move {
                        let params = rmcp::model::CallToolRequestParam {
                            name: "extract_triples".into(),
                            arguments: Some(serde_json::json!({"text": snippet, "max_triples": 5}).as_object().cloned().unwrap_or_default()),
                        };
                        let mut r = reg.write().await;
                        if let Some(guild) = r.guilds.get_mut("knowledge") {
                            let res = guild.call_tool(params).await;
                            Ok(res.content.into_iter()
                                .filter_map(|c: rmcp::model::Content| c.as_text().map(|t| t.text.clone()))
                                .next().unwrap_or_default())
                        } else {
                            Err(anyhow::anyhow!("Knowledge guild not found"))
                        }
                    }
                }).await.unwrap_or(0);
                
                tracing::info!("✅ retrograde_extract: complete — {} edges added", edges_added);
            });

            Ok(CallToolResult {
                content: vec![Content::text(serde_json::json!({
                    "status": "started",
                    "message": format!("Retrograde extraction started for up to {} nodes. Edges will accumulate in background — check tylluan_graph stats to monitor.", limit),
                }).to_string())],
                is_error: Some(false),
            })
        }
        "expand" => {
            let node_id = arguments.as_ref().and_then(|a| a.get("node_id")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            let depth = arguments.as_ref().and_then(|a| a.get("depth")).and_then(|v| v.as_u64()).unwrap_or(1).min(3) as usize;
            if node_id.is_empty() { return Ok(error_result("expand requires 'node_id'.")); }

            // FASE 2 (2026-08-20, convención warnings de docs/reference/MCP_WARNINGS_CONVENTION.md):
            // expand es lookup EXACTO por ID — un node_id que no existe como
            // nodo real no puede distinguirse de un nodo aislado legítimo
            // (mismo bug que tenía ppr). Warning NODE_NOT_FOUND: el early return
            // con el JSON completo mantiene el contrato (center/depth/nodes)
            // mientras da la señal diagnóstica.
            let resolved = server.silva.existing_node_ids(&[node_id.clone()]).await.unwrap_or_default();
            if !resolved.contains(&node_id) {
                return Ok(CallToolResult {
                    content: vec![Content::text(serde_json::json!({
                        "nodes": [],
                        "edges": [],
                        "center": node_id,
                        "depth": depth,
                        "warnings": [{
                            "code": "NODE_NOT_FOUND",
                            "severity": "warn",
                            "message": format!("'{node_id}' is not a real node ID — nothing to expand."),
                            "suggestion": "Use tylluan_graph(command='stats') or list_neighbors to discover valid node IDs."
                        }]
                    }).to_string())],
                    is_error: Some(false),
                });
            }

            let nodes = server.silva.get_context(&node_id, depth).await.unwrap_or_default();
            let node_ids: std::collections::HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
            let mut result_nodes = Vec::new();
            let now = chrono::Utc::now().timestamp();

            for node in nodes {
                let heat = server.silva.get_trace_count_since(&node.id, now - 86400).await.unwrap_or(0) as usize;
                result_nodes.push(serde_json::json!({
                    "id": node.id,
                    "type": node.node_type,
                    "content_preview": node.content.chars().take(120).collect::<String>(),
                    "heat": heat,
                    "weight": node.weight
                }));
            }

            let all_edges = server.silva.get_all_edges().await.unwrap_or_default();
            let relevant_edges: Vec<_> = all_edges.into_iter().filter(|e| {
                let s = e["source"].as_str().unwrap_or("");
                let t = e["target"].as_str().unwrap_or("");
                node_ids.contains(s) && node_ids.contains(t)
            }).collect();

            Ok(CallToolResult {
                content: vec![Content::text(serde_json::json!({
                    "nodes": result_nodes,
                    "edges": relevant_edges,
                    "center": node_id,
                    "depth": depth
                }).to_string())],
                is_error: Some(false)
            })
        }
        "ppr" | "pagerank" => {
            let seeds_val = arguments.as_ref().and_then(|a| a.get("seeds"));
            let seeds: Vec<String> = match seeds_val {
                Some(serde_json::Value::Array(arr)) => {
                    arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
                }
                _ => return Ok(error_result("personalized pagerank (ppr) requires a list of 'seeds' (array of strings).")),
            };
            if seeds.is_empty() {
                return Ok(error_result("personalized pagerank (ppr) requires a non-empty list of 'seeds'."));
            }

            let alpha = arguments.as_ref()
                .and_then(|a| a.get("alpha"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.85);

            let top_k = arguments.as_ref()
                .and_then(|a| a.get("top_k"))
                .and_then(|v| v.as_u64())
                .unwrap_or(10) as usize;

            // FASE 1 (2026-08-19, consenso de equipo en Coloquio -- Deep, Antigravity,
            // Claude Code): distinguir "seed no resuelve a ningun nodo real" de
            // "seed real pero sin conexiones", con la convencion `warnings` acordada
            // como patron de referencia para las otras 4 tools soberanas. Antes,
            // un seed invalido (p.ej. el nombre de una tool en vez de un ID de
            // memoria) producia `results: []` silencioso e indistinguible de un
            // subgrafo vacio legitimo -- is_error: false en ambos casos.
            let resolved = server.silva.existing_node_ids(&seeds).await.unwrap_or_default();
            let unresolved: Vec<&String> = seeds.iter().filter(|s| !resolved.contains(*s)).collect();
            let mut warnings: Vec<serde_json::Value> = Vec::new();
            if !unresolved.is_empty() {
                warnings.push(serde_json::json!({
                    "code": "NODE_NOT_FOUND",
                    "severity": "warn",
                    "message": format!(
                        "{} of {} seed(s) are not real node IDs and were never expanded: {}",
                        unresolved.len(), seeds.len(),
                        unresolved.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                    "suggestion": "Seeds must be real node IDs (e.g. 'agent_memory:...', 'lesson:...'), not tool or guild names. Use tylluan_graph(command='stats') or list_neighbors to discover valid IDs."
                }));
            }
            if resolved.is_empty() {
                // No seed resolved at all -- running PPR would be pure noise
                // (BFS frontier can never expand), so skip the computation and
                // surface the warning directly instead of a misleadingly
                // "successful" empty result.
                return Ok(CallToolResult {
                    content: vec![Content::text(serde_json::json!({
                        "action": "ppr",
                        "seeds": seeds,
                        "results": [],
                        "warnings": warnings
                    }).to_string())],
                    is_error: Some(false),
                });
            }

            match server.silva.personalized_pagerank_local(&seeds, alpha, 20, top_k).await {
                Ok(res) => {
                    let results_json: Vec<serde_json::Value> = res.into_iter().map(|(node_id, score)| {
                        serde_json::json!({
                            "node_id": node_id,
                            "score": score
                        })
                    }).collect();

                    let mut payload = serde_json::json!({
                        "action": "ppr",
                        "seeds": seeds,
                        "results": results_json
                    });
                    if !warnings.is_empty() {
                        payload["warnings"] = serde_json::Value::Array(warnings);
                    }

                    Ok(CallToolResult {
                        content: vec![Content::text(payload.to_string())],
                        is_error: Some(false),
                    })
                }
                Err(e) => Ok(error_result(&format!("Failed to calculate personalized pagerank: {e}"))),
            }
        }
        _ => Ok(error_result(&format!("Unknown tylluan_graph command: {command}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::agent_nodes::AgentNodeRouter;
    use crate::memory::hybrid::HybridMemory;
    use crate::memory::mailbox::Mailbox;
    use crate::memory::silva::SilvaDB;
    use crate::registry::guild_process::GuildRegistry;
    use crate::router::catalog::builtin_catalog;
    use crate::router::matcher::GuildMatcher;
    use crate::transport::server::TylluanServer;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::broadcast;
    use tokio::sync::RwLock;

    async fn test_server() -> TylluanServer {
        let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
        let test_reg = Arc::new(RwLock::new(reg));
        let matcher = GuildMatcher::new(builtin_catalog());
        let (tx, _) = broadcast::channel(16);
        let node_router = AgentNodeRouter::new(tx);
        let doctor = Arc::new(crate::doctor::Doctor::new(
            test_reg.clone(),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
        ));
        TylluanServer::new(
            test_reg,
            Arc::new(matcher),
            Arc::new(HybridMemory::in_memory().await.unwrap()),
            Arc::new(SilvaDB::in_memory().await.unwrap()),
            Arc::new(Mailbox::in_memory().await.unwrap()),
            doctor,
            node_router,
        )
    }

    async fn call_graph(
        server: &TylluanServer,
        args: serde_json::Map<String, serde_json::Value>,
    ) -> serde_json::Value {
        let result = handle_tylluan_graph(server, Some(args)).await
            .expect("handler must not fail");
        assert_eq!(result.is_error, Some(false), "expected non-error result");
        serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn expand_nonexistent_node_returns_warning() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("command".into(), serde_json::Value::String("expand".into()));
        args.insert("node_id".into(), serde_json::Value::String("nonexistent:node".into()));

        let payload = call_graph(&server, args).await;
        let warnings = payload["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1, "must surface exactly one warning");
        assert_eq!(warnings[0]["code"], "NODE_NOT_FOUND");
        assert_eq!(payload["nodes"].as_array().unwrap().len(), 0);
        assert_eq!(payload["center"], "nonexistent:node");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn expand_existing_isolated_node_has_no_warning() {
        let server = test_server().await;
        server.silva.upsert_node("g1", "concept", "a real concept", "{}").await.unwrap();

        let mut args = serde_json::Map::new();
        args.insert("command".into(), serde_json::Value::String("expand".into()));
        args.insert("node_id".into(), serde_json::Value::String("g1".into()));

        let payload = call_graph(&server, args).await;
        assert!(payload["warnings"].is_null(), "isolated real node is a legitimate empty result");
        assert_eq!(payload["center"], "g1");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn expand_missing_node_id_is_error() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("command".into(), serde_json::Value::String("expand".into()));

        let result = handle_tylluan_graph(&server, Some(args)).await.unwrap();
        assert_eq!(result.is_error, Some(true), "missing node_id must be a schema error, not a warning");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn query_nonexistent_subject_returns_warning() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("command".into(), serde_json::Value::String("query".into()));
        args.insert("subject".into(), serde_json::Value::String("nonexistent:subject".into()));

        let payload = call_graph(&server, args).await;
        let warnings = payload["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 1, "must surface exactly one warning");
        assert_eq!(warnings[0]["code"], "NODE_NOT_FOUND");
        assert_eq!(payload["count"], 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn query_existing_subject_has_no_warning() {
        let server = test_server().await;
        server.silva.upsert_node("s1", "concept", "s1 subject concept", "{}").await.unwrap();
        server.silva.upsert_node("o1", "concept", "o1 object concept", "{}").await.unwrap();
        server.silva.add_edge("s1", "o1", "relates_to", 1.0, "{}").await.unwrap();

        let mut args = serde_json::Map::new();
        args.insert("command".into(), serde_json::Value::String("query".into()));
        args.insert("subject".into(), serde_json::Value::String("s1".into()));

        let payload = call_graph(&server, args).await;
        assert!(payload["warnings"].is_null(), "existing subject must not warn");
        assert!(payload["count"].as_i64().unwrap() > 0, "must find the edge");
    }
}
