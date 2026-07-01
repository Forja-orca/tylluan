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
    let agent_node_id = format!("agent:{}", agent_id);
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
            let _ = server.silva.upsert_node(&subject, "concept", &subject, &metadata).await;
            let _ = server.silva.upsert_node(&object, "concept", &object, &metadata).await;
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
                Err(e) => Ok(error_result(&format!("Failed to add triple: {}", e))),
            }
        }
        "query" => {
            let subject = arguments.as_ref().and_then(|a| a.get("subject")).and_then(|v| v.as_str()).unwrap_or("").to_string();
            if subject.is_empty() { return Ok(error_result("query requires 'subject'.")); }

            let _ = server.silva.touch_node(&subject, &agent_id, "query").await;

            let nodes_with_scores = server.silva.search_hybrid(&subject, None, 5, None).await.unwrap_or_default();
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

            Ok(CallToolResult {
                content: vec![Content::text(serde_json::json!({
                    "query": subject,
                    "results": triples,
                    "count": triples.len()
                }).to_string())],
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
                Err(e) => Ok(error_result(&format!("Failed to get stats: {}", e))),
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
                Err(e) => Ok(error_result(&format!("Failed to query path: {}", e))),
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

            match server.silva.personalized_pagerank_local(&seeds, alpha, 20, top_k).await {
                Ok(res) => {
                    let results_json: Vec<serde_json::Value> = res.into_iter().map(|(node_id, score)| {
                        serde_json::json!({
                            "node_id": node_id,
                            "score": score
                        })
                    }).collect();

                    Ok(CallToolResult {
                        content: vec![Content::text(serde_json::json!({
                            "action": "ppr",
                            "seeds": seeds,
                            "results": results_json
                        }).to_string())],
                        is_error: Some(false),
                    })
                }
                Err(e) => Ok(error_result(&format!("Failed to calculate personalized pagerank: {}", e))),
            }
        }
        _ => Ok(error_result(&format!("Unknown tylluan_graph command: {}", command))),
    }
}
