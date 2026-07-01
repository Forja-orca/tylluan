use rmcp::{Error as McpError, model::*};
use serde_json;
use chrono;
use std::collections::{HashMap, HashSet};

use crate::memory::silva::GraphNode;
use crate::registry::proxy::error_result;
use super::TylluanServer;

pub async fn handle_tylluan_think(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let query = arguments.as_ref().and_then(|a| a.get("query")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let agent_id: Option<String> = arguments.as_ref().and_then(|a| a.get("agent_id")).and_then(|v| v.as_str()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let chain = arguments.as_ref().and_then(|a| a.get("chain")).and_then(|v| v.as_bool()).unwrap_or(false);

    if query.trim().is_empty() {
        return Ok(error_result("tylluan_think requires a non-empty 'query' argument."));
    }

    let agent_id_str = agent_id.unwrap_or_else(|| "anonymous".to_string());

    let embedding = server.matcher.engine().and_then(|e| e.embed(&query).ok());
    let nodes_with_scores = if let Some(ref reranker) = server.reranker {
        server.silva.search_hybrid_reranked(&query, embedding.as_deref(), 20, reranker).await
    } else {
        server.silva.search_hybrid(&query, embedding.as_deref(), 20, None).await
    }.unwrap_or_default();
    
    let mut nodes: Vec<GraphNode> = nodes_with_scores.iter().map(|(n, _)| n.clone()).collect();

    // M20-D: exclude machinery nodes — same filter as handler_recall to prevent routing_anchor
    // and session_digest from appearing in think synthesis
    nodes.retain(|n| n.node_type != "routing_anchor" && n.node_type != "session_digest");

    // BLOQUE A: PageRank Re-ranking
    if !nodes.is_empty() {
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let mut edges = Vec::new();
        for id in &node_ids {
            if let Ok(neighbors) = server.silva.get_context(id, 1).await {
                for n in neighbors {
                    if node_ids.contains(&n.id) && n.id != *id {
                        edges.push((id.clone(), n.id.clone()));
                    }
                }
            }
        }
        
        let pr = calculate_pagerank(&node_ids, &edges, 10, 0.85);
        let max_pr = pr.values().cloned().fold(0.0, f64::max);
        
        let mut ranked = nodes_with_scores.clone();
        ranked.sort_by(|(a, s_a), (b, s_b)| {
            let pr_a = (pr.get(&a.id).cloned().unwrap_or(0.0) / (if max_pr > 0.0 { max_pr } else { 1.0 })) as f32;
            let pr_b = (pr.get(&b.id).cloned().unwrap_or(0.0) / (if max_pr > 0.0 { max_pr } else { 1.0 })) as f32;
            let score_a = 0.6_f32 * s_a + 0.4_f32 * pr_a;
            let score_b = 0.6_f32 * s_b + 0.4_f32 * pr_b;
            score_b.partial_cmp(&score_a).unwrap_or(std::cmp::Ordering::Equal)
        });
        nodes = ranked.into_iter().map(|(n, _)| n).take(8).collect();
    }

    let recent_mail = server.mailbox.get_recent_broadcasts("broadcast", "agent_activity", 24).await.unwrap_or_default();
    let related_mail: Vec<_> = recent_mail.into_iter()
        .filter(|m| m.payload.to_lowercase().contains(&query.to_lowercase()))
        .take(5)
        .collect();

    let mut synthesis = format!("## Pensamiento sobre: {}\n\n", query);

    if nodes.is_empty() && related_mail.is_empty() {
        synthesis.push_str("No prior knowledge or recent activity found on this topic.\n");
    } else {
        let since_24h = chrono::Utc::now().timestamp() - 86400;
        if !nodes.is_empty() {
            synthesis.push_str(&format!("### Conocimiento en SilvaDB ({})\n", nodes.len()));
            for (i, node) in nodes.iter().enumerate() {
                let _ = server.silva.touch_node(&node.id, &agent_id_str, "tylluan_think").await;
                let heat_suffix = if i < 3 {
                    match server.silva.get_trace_count_since(&node.id, since_24h).await {
                        Ok(n) if n > 0 => format!(" [hot: {} accesos (24h)]", n),
                        _ => String::new(),
                    }
                } else { String::new() };
                synthesis.push_str(&format!(
                    "- **{}** (peso: {:.1}){}: {}\n",
                    node.id,
                    node.weight,
                    heat_suffix,
                    if node.content.chars().count() > 150 { format!("{}...", node.content.chars().take(150).collect::<String>()) } else { node.content.clone() }
                ));
            }
        }

        // BLOQUE A: Chain-of-thought expansion
        if chain && !nodes.is_empty() {
            synthesis.push_str("\n### Cadenas de Razonamiento (Chain-of-Thought)\n");
            let mut seen_expanded = HashSet::new();
            for n in &nodes { seen_expanded.insert(n.id.clone()); }
            
            let mut chains_found = 0;
            for node in nodes.iter().take(3) {
                if let Ok(ctx) = server.silva.get_context(&node.id, 3).await {
                    let expanded: Vec<_> = ctx.into_iter()
                        .filter(|n| !seen_expanded.contains(&n.id))
                        .filter(|n| n.node_type != "routing_anchor" && n.node_type != "session_digest")
                        .take(2)
                        .collect();
                    
                    if !expanded.is_empty() {
                        synthesis.push_str(&format!("- Desde **{}**:\n", node.id));
                        for exp in expanded {
                            synthesis.push_str(&format!("  -> *{}*: {}\n", exp.id, exp.content.chars().take(80).collect::<String>()));
                            seen_expanded.insert(exp.id.clone());
                            chains_found += 1;
                        }
                    }
                }
                if chains_found >= 5 { break; }
            }
            if chains_found == 0 {
                synthesis.push_str("No se hallaron ramificaciones profundas relevantes para expandir el contexto.\n");
            }
        }

        if !related_mail.is_empty() {
            synthesis.push_str(&format!("\n### Actividad Reciente ({})\n", related_mail.len()));
            for mail in &related_mail {
                synthesis.push_str(&format!("- De **{}**: {}\n", mail.sender_id, mail.payload));
            }
        }

        let mut connections_found = 0;
        let mut connections_text = String::new();
        let mut node_edge_counts: HashMap<String, usize> = HashMap::new();
        for node in nodes.iter().take(8) {
            if let Ok(ctx) = server.silva.get_context(&node.id, 1).await {
                let neighbor_count = ctx.iter().filter(|n| n.id != node.id).count();
                node_edge_counts.insert(node.id.clone(), neighbor_count);
                if connections_found <= 5 {
                    for neighbor in &ctx {
                        if neighbor.id != node.id
                            && neighbor.node_type != "routing_anchor"
                            && neighbor.node_type != "session_digest" {
                            connections_text.push_str(&format!("  - {} ↔ {}\n", node.id, neighbor.id));
                            connections_found += 1;
                            if connections_found > 5 { break; }
                        }
                    }
                }
            } else {
                node_edge_counts.insert(node.id.clone(), 0);
            }
        }
        if connections_found > 0 {
            synthesis.push_str("\n### Conexiones Relevantes\n");
            synthesis.push_str(&connections_text);
        }

        // GraphRAG: detect clusters among retrieved nodes and load summaries
        let gr_node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        if gr_node_ids.len() >= 3 {
            let mut gr_edges: Vec<(String, String)> = Vec::new();
            for id in &gr_node_ids {
                if let Ok(neighbors) = server.silva.get_context(id, 1).await {
                    for n in neighbors {
                        if gr_node_ids.contains(&n.id) && n.id != *id {
                            gr_edges.push((id.clone(), n.id.clone()));
                        }
                    }
                }
            }
            let mut cluster_sections = String::new();
            let components = find_connected_components(&gr_node_ids, &gr_edges);
            for (ci, component) in components.iter().enumerate() {
                if component.len() < 2 { continue; }
                // Check for existing summary via member_of edges
                let mut summary_text = String::new();
                for member_id in component {
                    if let Ok(ctx) = server.silva.get_context(member_id, 1).await {
                        for neighbor in &ctx {
                            if neighbor.node_type == "summary" || neighbor.id.starts_with("summary:") {
                                summary_text = neighbor.content.chars().take(300).collect();
                                break;
                            }
                        }
                    }
                    if !summary_text.is_empty() { break; }
                }
                let member_preview: Vec<String> = component.iter()
                    .filter_map(|id| nodes.iter().find(|n| n.id == *id))
                    .map(|n| n.id.chars().take(30).collect())
                    .take(4)
                    .collect();
                cluster_sections.push_str(&format!(
                    "- **Cluster {}** ({} nodos): {}\n",
                    ci + 1,
                    component.len(),
                    member_preview.join(", ")
                ));
                if !summary_text.is_empty() {
                    cluster_sections.push_str(&format!("  - Resumen: {}\n", summary_text));
                }
            }
            if !cluster_sections.is_empty() {
                synthesis.push_str("\n### GraphRAG: Clusters Detectados\n");
                synthesis.push_str(&cluster_sections);
            }
        }

        // Gap Analysis: only surface when gaps are a majority (>50%) of retrieved nodes.
        // Sparse connectivity is normal for this graph — flagging every node with <3 edges
        // produces false "knowledge fragmented" warnings even when 8 meaningful nodes were found.
        let gaps: Vec<&GraphNode> = nodes.iter()
            .filter(|n| node_edge_counts.get(&n.id).copied().unwrap_or(0) < 3)
            .collect();
        if !gaps.is_empty() && (nodes.is_empty() || gaps.len() * 2 > nodes.len()) {
            synthesis.push_str("\n### Brechas de Conocimiento\n");
            synthesis.push_str("Los siguientes conceptos tienen pocas conexiones — son áreas donde el conocimiento está fragmentado:\n");
            for gap in gaps.iter().take(5) {
                let edge_n = node_edge_counts.get(&gap.id).copied().unwrap_or(0);
                synthesis.push_str(&format!(
                    "- **{}** ({} conexión{}): {}\n",
                    gap.id,
                    edge_n,
                    if edge_n == 1 { "" } else { "es" },
                    gap.content.chars().take(80).collect::<String>()
                ));
            }
        }

        // Graph structure analysis — deterministic, no LLM needed
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let graph_analysis = server.silva.analyze_subgraph(&node_ids, &query).await
            .unwrap_or_default();
        
        let mut graph_insights = String::new();
        if let Some((hub_id, degree)) = &graph_analysis.hub_node
            && *degree > 0 {
                let hub_content: String = nodes.iter()
                    .find(|n| &n.id == hub_id)
                    .map(|n| n.content.chars().take(80).collect())
                    .unwrap_or_else(|| hub_id.clone());
                let display_name: String = hub_content.chars().take(55).collect();
                let display_name = if hub_content.len() > 55 {
                    format!("{}…", display_name)
                } else {
                    display_name
                };
                let short_id = &hub_id[..hub_id.len().min(20)];
                graph_insights.push_str(&format!(
                    "\n\n### Nodo Hub (más conectado)\n**{}** ({} conexiones)\n*ref: {}*", 
                    display_name, degree, short_id
                ));
            }
        
        if !graph_analysis.connected_path.is_empty() {
            graph_insights.push_str("\n\n### Camino conceptual\n");
            for step in &graph_analysis.connected_path {
                graph_insights.push_str(&format!("-> {}\n", step));
            }
        }
        
        if graph_insights.is_empty() && graph_analysis.node_count > 0 {
            graph_insights = format!(
                "\n\n### Estructura del grafo\n{} nodos analizados — sin conexiones directas entre ellos. Los conceptos son independientes en el grafo actual.",
                graph_analysis.node_count
            );
        }
        synthesis.push_str(&graph_insights);

        synthesis.push_str("\n\n### Conclusión e Insights\n");
        synthesis.push_str("- El sistema posee trazas de este concepto en su memoria a largo plazo.\n");
        if nodes.iter().any(|n| n.weight > 5.0) {
            synthesis.push_str("- Existe un alto grado de consolidación en este dominio.\n");
        }
        if !related_mail.is_empty() {
            synthesis.push_str("- Hay actividad operativa reciente relacionada con esta consulta.\n");
        }
    }

    if let Ok(mut h) = server.hormones.lock() {
        h.emit_novelty(0.3 + (nodes.len() as f64 * 0.05).min(0.5));
    }

    server.notify("tool_call", serde_json::json!({
        "status": "finished",
        "tool": "tylluan_think",
        "agent_id": agent_id_str,
        "intent": query,
        "ok": true,
        "ts": chrono::Utc::now().timestamp_millis()
    }));

    if !nodes.is_empty() {
        let think_intent = format!(
            "Analiza estos {} conocimientos sobre '{}' y genera una síntesis coherente: {}",
            nodes.len(),
            query,
            nodes.iter().take(5)
                .map(|n| n.content.chars().take(100).collect::<String>())
                .collect::<Vec<_>>()
                .join(" | ")
        );

        let synth_guild = server.matcher.match_guild("think step by step and synthesize", None, 0.1, None)
            .map(|m| m.guild_name);

        if let Some(guild_name) = synth_guild
            && server.registry.write().await.ensure_guild_running(&guild_name).await.is_ok() {
                let think_depth = (nodes.len().max(2).min(5)) as i64;
                let synth_params = rmcp::model::CallToolRequestParam {
                    name: "think".into(),
                    arguments: Some({
                        let mut m = serde_json::Map::new();
                        m.insert("problem".to_string(), serde_json::Value::String(think_intent));
                        m.insert("intent".to_string(), serde_json::Value::String(query.clone()));
                        m.insert("depth".to_string(), serde_json::Value::Number(think_depth.into()));
                        m
                    }),
                };
                let synth_result = {
                    let mut reg = server.registry.write().await;
                    if let Some(g) = reg.guilds.get_mut(&guild_name) {
                        Some(g.call_tool(synth_params).await)
                    } else { None }
                };

                if let Some(synth) = synth_result {
                    let synth_text = synth.content.iter()
                        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
                        .collect::<String>();
                    if !synth_text.is_empty() {
                        synthesis.push_str("\n\n## Síntesis\n");
                        synthesis.push_str(&synth_text);
                    }
                }
            }
    }

    // STIGMERGY: Mark agent as actively reasoning
    let agent_node_id = format!("agent:{}", agent_id_str);
    let _ = server.silva.touch_node(&agent_node_id, &agent_id_str, "tylluan_think").await;

    Ok(CallToolResult {
        content: vec![Content::text(synthesis)],
        is_error: Some(false),
    })
}

fn find_connected_components(node_ids: &[String], edges: &[(String, String)]) -> Vec<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for id in node_ids {
        adj.entry(id.as_str()).or_default();
    }
    for (s, t) in edges {
        if node_ids.contains(s) && node_ids.contains(t) {
            adj.entry(s.as_str()).or_default().push(t.as_str());
            adj.entry(t.as_str()).or_default().push(s.as_str());
        }
    }
    let mut visited: HashSet<&str> = HashSet::new();
    let mut components = Vec::new();
    for id in node_ids {
        if visited.contains(id.as_str()) { continue; }
        let mut stack = vec![id.as_str()];
        let mut comp = Vec::new();
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur) { continue; }
            comp.push(cur.to_string());
            if let Some(neighbors) = adj.get(cur) {
                for n in neighbors {
                    if !visited.contains(n) {
                        stack.push(n);
                    }
                }
            }
        }
        components.push(comp);
    }
    components
}

fn calculate_pagerank(
    node_ids: &[String],
    edges: &[(String, String)],
    iterations: usize,
    damping: f64,
) -> HashMap<String, f64> {
    let n = node_ids.len();
    if n == 0 { return HashMap::new(); }
    
    let mut pr: HashMap<String, f64> = node_ids.iter().map(|id| (id.clone(), 1.0 / n as f64)).collect();
    let mut out_degree: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    
    for (s, t) in edges {
        if node_ids.contains(s) && node_ids.contains(t) {
            adj.entry(s.clone()).or_default().push(t.clone());
            *out_degree.entry(s.clone()).or_insert(0) += 1;
        }
    }
    
    for _ in 0..iterations {
        let mut next_pr: HashMap<String, f64> = node_ids.iter().map(|id| (id.clone(), (1.0 - damping) / n as f64)).collect();
        let mut dangling_sum = 0.0;
        
        for id in node_ids {
            if *out_degree.get(id).unwrap_or(&0) == 0 {
                dangling_sum += pr[id];
            } else {
                if let Some(neighbors) = adj.get(id) {
                    for neighbor in neighbors {
                        if let Some(score) = next_pr.get_mut(neighbor) {
                            *score += damping * pr[id] / out_degree[id] as f64;
                        }
                    }
                }
            }
        }
        
        for id in node_ids {
            if let Some(score) = next_pr.get_mut(id) {
                *score += damping * dangling_sum / n as f64;
            }
        }
        
        pr = next_pr;
    }
    
    pr
}