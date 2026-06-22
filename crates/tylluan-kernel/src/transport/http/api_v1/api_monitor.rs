use axum::{
    Json,
    extract::State,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::transport::http::HttpState;

/// GET /api/v1/dream/status
/// Returns live graph health + NightConsolidation observability metrics.
pub async fn dream_status_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use std::collections::HashMap;

    let stats = state.silva.get_detailed_stats().await.unwrap_or_else(|_| serde_json::json!({}));
    let total_nodes = stats.get("node_count").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let nodes_by_type = stats.get("by_type").cloned().unwrap_or(serde_json::json!({}));

    let all_edges = state.silva.get_all_edges().await.unwrap_or_default();
    let total_edges = all_edges.len();
    let mut edges_by_type: HashMap<String, usize> = HashMap::new();
    let mut edge_node_ids: std::collections::HashSet<String> =
        std::collections::HashSet::with_capacity(total_edges * 2);
    let mut contradictions = 0;
    for e in &all_edges {
        let etype = e.get("type").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        if etype == "contradicts" || etype == "contradiction" { contradictions += 1; }
        *edges_by_type.entry(etype).or_insert(0) += 1;
        if let Some(s) = e.get("source").and_then(|v| v.as_str()) { edge_node_ids.insert(s.to_string()); }
        if let Some(t) = e.get("target").and_then(|v| v.as_str()) { edge_node_ids.insert(t.to_string()); }
    }

    let connected = edge_node_ids.len();
    let orphans = total_nodes.saturating_sub(connected);
    let routing_anchors = nodes_by_type.get("routing_anchor").and_then(|v| v.as_i64()).unwrap_or(0) as usize;
    let knowledge_nodes = total_nodes.saturating_sub(routing_anchors);
    let knowledge_orphans = orphans.saturating_sub(routing_anchors);
    let orphan_pct = if knowledge_nodes > 0 { 100 * knowledge_orphans / knowledge_nodes } else { 0 };

    let nodes_with_embedding = state.silva.count_nodes_with_embedding().await.unwrap_or(0);
    let nodes_with_topic = state.silva.count_nodes_with_topic_key().await.unwrap_or(0);

    let contradiction_pct = if total_edges > 0 { 100 * contradictions / total_edges } else { 0 };
    let embedding_coverage = if total_nodes > 0 { 100 * nodes_with_embedding / total_nodes } else { 0 };
    let topic_key_coverage = if total_nodes > 0 { 100 * nodes_with_topic / total_nodes } else { 0 };

    let canary_assertions = serde_json::json!({
        "orphan_pct_ok": orphan_pct < 35,
        "contradiction_pct_ok": contradiction_pct < 15,
        "embedding_coverage_ok": embedding_coverage > 50,
        "overall_health": orphan_pct < 35 && contradiction_pct < 15 && embedding_coverage > 50
    });

    let summaries_created_24h = tokio::task::block_in_place(|| {
        let conn = state.silva.conn.blocking_lock();
        conn.query_row(
            "SELECT COUNT(*) FROM cluster_summaries WHERE created_at > strftime('%s','now','-1 day')",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap_or(0) as usize
    });

    let uptime_secs = state.start_time.elapsed().as_secs();
    let next_run_in_secs = 3600u64.saturating_sub(uptime_secs % 3600);
    let runs_completed = uptime_secs / 3600;

    Json(serde_json::json!({
        "status": "ok",
        "graph": {
            "nodes": total_nodes,
            "edges": total_edges,
            "orphans": orphans,
            "orphan_pct": orphan_pct,
            "contradiction_pct": contradiction_pct,
            "embedding_coverage": embedding_coverage,
            "nodes_with_embedding": nodes_with_embedding,
            "topic_key_coverage": topic_key_coverage,
            "nodes_with_topic_key": nodes_with_topic,
            "nodes_by_type": nodes_by_type,
            "edges_by_type": edges_by_type,
        },
        "canary_assertions": canary_assertions,
        "summaries_created_24h": summaries_created_24h,
        "canary_summary_inflation": {
            "summaries_24h": summaries_created_24h,
            "threshold": 100,
            "alert": summaries_created_24h > 100
        },
        "night_consolidation": {
            "schedule_secs": 3600,
            "uptime_secs": uptime_secs,
            "runs_completed": runs_completed,
            "next_run_in_secs": next_run_in_secs,
            "components": [
                "dream_cycle (dedup cosine>0.92, decay 30d idle, flag contradictions)",
                "auto_link (file_ref, tool_ref, same_topic, orphan_links, semantic_similarity)",
                "graph_rag (cluster summaries via deep_analysis)",
                "selective_decay",
                "agent_consolidation"
            ]
        }
    }))
}

/// GET /api/v1/dashboard/summary
/// Returns consolidated system telemetry, interoception, hormones, and silva stats in a single call.
pub async fn dashboard_summary_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {

    let diag = state.doctor.diagnose().await;
    let statuses = state.registry.status_all().await.unwrap_or_default();

    let online = statuses.iter().filter(|s| s.running).count();
    let total_tools: usize = statuses.iter().map(|s| s.tools_count).sum();
    let edge_count = state.silva.edge_count().await.unwrap_or(0);
    let node_count = state.silva.node_count().await.unwrap_or(0);

    let golden_signals = serde_json::json!({
        "traffic": {
            "active_guilds": online,
            "total_guilds": statuses.len(),
            "active_tools": total_tools
        },
        "errors": {
            "rate_percent": if diag.status == "healthy" { 0 } else if diag.status == "degraded" { 5 } else { 20 },
            "total_errors": 0,
            "critical": diag.status == "critical"
        },
        "saturation": {
            "memory_percent": diag.system.memory_percent.round(),
            "storage_percent": 0,
            "node_count": node_count,
            "edge_count": edge_count
        },
        "uptime_seconds": state.start_time.elapsed().as_secs(),
        "slo_target": 99.9,
        "status": {
            "guilds_online": online,
            "guilds_total": statuses.len(),
            "nodes": node_count,
            "edges": edge_count
        }
    });

    let graph_density = if node_count > 1 { (edge_count as f64) / ((node_count as f64) * ((node_count - 1) as f64)) } else { 0.0 };
    let (homeostasis, stress_level, active_pheromones, capabilities) = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        let cap = serde_json::json!({
            "embeddings_loaded": srv.matcher.engine().is_some(),
            "reranker_loaded": srv.reranker.is_some(),
            "embedding_model": if srv.matcher.engine().is_some() { "bge-m3" } else { "none" },
            "reranker_model": if srv.reranker.is_some() { "jina-reranker-v1-turbo-en" } else { "none" },
        });
        if let Ok(h) = srv.hormones.lock() {
            let stress = h.stress_level().max(0.0);
            let homeo = (1.0 - stress).max(0.0);
            (homeo, stress, h.active_signals().len(), cap)
        } else {
            (1.0, 0.0, 0, cap)
        }
    } else {
        (1.0, 0.0, 0, serde_json::json!({}))
    };

    let sessions = state.sessions.read().await;
    let knowledge_hunger = if sessions.is_empty() { 0.5_f64 } else {
        (1.0 - ((node_count as f64) / 10000.0_f64).min(1.0)).max(0.0)
    };

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let agent_rhythms = {
        let rhythms: serde_json::Map<String, serde_json::Value> = sessions.values()
            .filter(|s| now_unix.saturating_sub(s.last_active_unix) < 3600)
            .map(|s| {
                let agent_key = s.agent_id.as_deref()
                    .unwrap_or(&s.client_name)
                    .to_string();
                (agent_key, serde_json::json!({
                    "tool_calls": s.tool_count,
                    "last_active_secs_ago": now_unix.saturating_sub(s.last_active_unix),
                    "client": s.client_name,
                }))
            })
            .collect();
        serde_json::Value::Object(rhythms)
    };

    let mut recommendations = vec![];
    if stress_level > 0.5 { recommendations.push("High stress detected — check guild errors"); }
    if knowledge_hunger > 0.7 { recommendations.push("Low knowledge density — consider ingesting more data"); }
    if graph_density < 0.001 { recommendations.push("Sparse knowledge graph — connections are low"); }

    let interoception = serde_json::json!({
        "homeostasis": homeostasis,
        "stress_level": stress_level,
        "knowledge_hunger": knowledge_hunger,
        "graph_density": graph_density,
        "active_pheromones": active_pheromones,
        "agent_rhythms": agent_rhythms,
        "recommendations": recommendations,
        "capabilities": capabilities
    });

    let (h_signals, h_stress, h_novelty, h_saturation, h_energy) = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Ok(h) = srv.hormones.lock() {
            (
                h.active_signals(),
                h.get_intensity("stress"),
                h.get_intensity("novelty"),
                h.get_intensity("saturation"),
                h.energy_level()
            )
        } else {
            (vec![], 0.0, 0.0, 0.0, 1.0)
        }
    } else {
        (vec![], 0.0, 0.0, 0.0, 1.0)
    };
    let hormones = serde_json::json!({
        "signals": h_signals,
        "count": h_signals.len(),
        "stress":     (h_stress * 100.0).round() / 100.0,
        "novelty":    (h_novelty * 100.0).round() / 100.0,
        "saturation": (h_saturation * 100.0).round() / 100.0,
        "energy":     (h_energy * 100.0).round() / 100.0,
        "homeostasis": ((1.0 - h_stress - h_saturation * 0.5).max(0.0) * 100.0).round() / 100.0
    });

    let silva_stats = state.silva.get_detailed_stats().await.unwrap_or_else(|_| {
        serde_json::json!({
            "node_count": node_count,
            "edge_count": edge_count,
            "orphan_pct": 0,
            "orphans": 0,
            "by_type": {}
        })
    });

    let curr_count = {
        let curr_learner = state.doctor.curriculum();
        let curr = curr_learner.lock().unwrap_or_else(|e| e.into_inner());
        curr.get_stats()["total_entries"].as_u64().unwrap_or(0)
    };
    let system_status = serde_json::json!({
        "status": if diag.status == "healthy" { "ok" } else { diag.status.as_str() },
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": state.start_time.elapsed().as_secs(),
        "guilds_online": online,
        "guilds_total": statuses.len(),
        "silva_healthy": diag.storage.silva_db_ok,
        "mailbox_healthy": diag.storage.memory_db_ok,
        "curriculum_entries": curr_count,
        "embeddings_loaded": true,
        "score": if diag.status == "healthy" { 100u32 } else if diag.status == "degraded" { 65 } else { 30 },
        "system": {
            "cpu_usage": diag.system.cpu_usage_percent,
            "memory_percent": diag.system.memory_percent,
            "used_memory_mb": diag.system.used_memory_mb,
            "total_memory_mb": diag.system.total_memory_mb,
            "process_count": diag.system.process_count,
        }
    });

    Json(serde_json::json!({
        "golden_signals": golden_signals,
        "interoception": interoception,
        "hormones": hormones,
        "silva_stats": silva_stats,
        "system_status": system_status
    }))
}

/// GET /api/v1/skill
/// Universal skill document for any agent connecting to TylluanNexus via MCP.
pub async fn skill_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let config = state.config.read().await;
    let port = config.nexus.port;
    let host = config.nexus.host.clone();
    drop(config);

    let guild_count = state.registry.status_all().await.unwrap_or_default().len();

    let skill = serde_json::json!({
        "schema_version": "1.0",
        "system": {
            "name": "TylluanNexus o3",
            "version": "3.0.0",
            "description": "Sovereign cognitive memory kernel. Local CPU-only. No cloud. No OpenAI. BGE-M3 embeddings + Jina Reranker. 90% Recall@5 on LongMemEval-S (frontier tier).",
            "endpoint": format!("http://{}:{}", host, port),
            "mcp_transports": {
                "sse": format!("http://{}:{}/sse", host, port),
                "http_streamable": format!("http://{}:{}/messages", host, port)
            },
            "protocol": "MCP 2024-11-05 / 2025-06-18 (auto-negotiated)"
        },
        "sovereign_tools": [
            {
                "name": "tylluan_do",
                "description": "Execute any task using natural language. The kernel routes the intent to the right guild automatically.",
                "when_to_use": "Bash commands, git ops, file operations, posting to coloquio, web search, code analysis — anything that maps to a guild.",
                "examples": [
                    "tylluan_do('publica en coloquio mision-activa: [STATUS] task complete')",
                    "tylluan_do('run cargo test -p tylluan-kernel --lib')",
                    "tylluan_do('lee el canal mision-activa ultimos 5 mensajes', guild='coloquio')",
                    "tylluan_do('list files in crates/tylluan-kernel/src/')"
                ],
                "tip": "Use the `guild` parameter to force routing when intent is ambiguous."
            },
            {
                "name": "tylluan_remember",
                "description": "Store information in long-term memory (SilvaDB knowledge graph).",
                "when_to_use": "Decisions, findings, facts, conclusions, architectural choices, anything that should survive context resets."
            },
            {
                "name": "tylluan_recall",
                "description": "Search long-term memory using hybrid retrieval: BM25 + BGE-M3 vector + Jina Reranker.",
                "when_to_use": "Any time you need context about the project, past decisions, or what other agents have done."
            },
            {
                "name": "tylluan_think",
                "description": "Deep reasoning over the knowledge graph. Runs PageRank + gap analysis + GraphRAG cluster summaries.",
                "when_to_use": "Strategic analysis, gap detection, cross-domain synthesis, when you need the system to reason about what it knows."
            },
            {
                "name": "tylluan_graph",
                "description": "Inspect and manipulate the SilvaDB knowledge graph directly.",
                "when_to_use": "Graph visualization, node inspection, edge queries, community detection."
            }
        ],
        "active_guilds": {
            "count": guild_count,
            "always_on": ["bash", "git", "filesystem", "monitor", "docker", "code", "coloquio", "memory", "knowledge", "web", "sequential_thinking"],
            "specialised": ["vision", "deep_analysis", "ingest", "scrapling_web", "code_graph"],
            "note": "Full guild list at GET /api/v1/guilds. Guild health at GET /api/v1/guilds/health."
        },
        "key_endpoints": {
            "health": "GET /health",
            "skill": "GET /api/v1/skill (this document)",
            "tools": "GET /api/v1/tools",
            "guilds": "GET /api/v1/guilds",
            "silva_stats": "GET /api/v1/silva/stats",
            "graph_viz": "GET /api/v1/silva/graph",
            "coloquio_channels": "GET /api/v1/coloquio/channels",
            "system_status": "GET /api/v1/system/status",
            "do": "POST /api/v1/do  {intent, guild?, agent_id?, remember?}",
            "memory_search": "POST /api/v1/memory/search  {query, limit?}"
        },
        "collaboration_protocol": {
            "shared_memory": "SilvaDB (silva.db) is shared across all agents. Write with tylluan_remember, read with tylluan_recall.",
            "coloquio": "Multi-agent mailbox at data/mailbox.db. Channel 'mision-activa' is the main coordination channel. Post decisions, progress, and blockers there.",
            "agent_identity": "Set agent_id on every tylluan_remember/tylluan_recall call so your contributions are attributable.",
            "night_consolidation": "NightConsolidation runs hourly: DreamCycle (dedup+decay) + AutoLinker + GraphRAG + Leiden clustering.",
            "sovereignty": "Never call external APIs. All embeddings are BGE-M3 local (ONNX). All reranking is Jina local."
        },
        "benchmarks": {
            "longmemeval_s_50q": {"recall_at_1": "66%", "recall_at_5": "90%", "recall_at_10": "98%"},
            "vs_zep_graphiti": "90.2% R@5 (requires Neo4j) vs TylluanNexus 90.0% (SQLite, CPU-only)",
            "vs_mem0_oss": "32.4% R@5 vs TylluanNexus 90.0% — 58pp advantage with local BGE-M3",
            "note": "Frontier tier without cloud, GPU, or OpenAI dependency."
        }
    });

    Json(skill).into_response()
}

pub async fn canary_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let mut probe_results = Vec::new();
    let mut passed = 0u32;
    let mut total = 0u32;

    // 1. SilvaDB probe: nodes and edges must exist, orphans tolerable
    {
        total += 1;
        let ncount = state.silva.node_count().await.unwrap_or(0);
        let ecount = state.silva.edge_count().await.unwrap_or(0);
        let routing_anchors = state.silva.count_by_type("routing_anchor").await.unwrap_or(0);
        let knowledge_nodes = ncount.saturating_sub(routing_anchors);
        let orphan_pct = if knowledge_nodes > 0 {
            let edges = state.silva.get_all_edges().await.unwrap_or_default();
            let mut connected = std::collections::HashSet::new();
            for e in &edges {
                if let Some(s) = e.get("source").and_then(|v| v.as_str()) { connected.insert(s.to_string()); }
                if let Some(t) = e.get("target").and_then(|v| v.as_str()) { connected.insert(t.to_string()); }
            }
            let orphans = ncount.saturating_sub(connected.len());
            let knowledge_orphans = orphans.saturating_sub(routing_anchors);
            100 * knowledge_orphans / knowledge_nodes
        } else { 100 };
        let ok = ncount > 0 && orphan_pct < 35;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "silva_db",
            "pass": ok,
            "detail": format!("{} nodes, {} edges, {}% orphans (routing_anchors excluded)", ncount, ecount, orphan_pct)
        }));
    }

    // 2. MCP tools probe: exactly 5 sovereign tools
    {
        total += 1;
        let tool_count = if let Some(ref srv_arc) = state.server {
            let srv = srv_arc.read().await;
            srv.all_tools().await.len()
        } else { 0 };
        let ok = tool_count == 5;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "mcp_tools",
            "pass": ok,
            "detail": format!("{} sovereign tools registered (expected 5)", tool_count)
        }));
    }

    // 3. Guilds probe: at least one guild active
    {
        total += 1;
        let (_, active) = state.registry.guild_stats().await.unwrap_or((0, 0));
        let ok = active > 0;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "guilds",
            "pass": ok,
            "detail": format!("{} active guilds", active)
        }));
    }

    // 4. Embeddings probe: BGE-M3 loaded
    {
        total += 1;
        let loaded = if let Some(ref srv_arc) = state.server {
            if let Ok(srv) = srv_arc.try_read() {
                srv.matcher.engine().is_some()
            } else { false }
        } else { false };
        if loaded { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "embeddings",
            "pass": loaded,
            "detail": if loaded { "BGE-M3 loaded" } else { "embedding engine unavailable" }
        }));
    }

    // 5. Coloquio probe: at least 1 channel
    {
        total += 1;
        let channels = state.coloquio.list_channels().await.unwrap_or_default();
        let ok = !channels.is_empty();
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "coloquio",
            "pass": ok,
            "detail": format!("{} channels", channels.len())
        }));
    }

    // 6. Job queue probe: no job stuck in claimed state > 5 min
    {
        total += 1;
        let stuck = state.jobs.count_stuck().unwrap_or(0);
        let ok = stuck == 0;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "job_queue",
            "pass": ok,
            "detail": format!("{} stuck jobs", stuck)
        }));
    }

    // 7. Drift Guard probe: measures ratio of auto-generated summary nodes vs factual episodic nodes.
    //    High ratio indicates "drift de consolidación" — risk of hallucination feedback loops.
    {
        total += 1;
        let summary_count = state.silva.count_by_type("summary").await.unwrap_or(0);
        let synthesis_count = state.silva.count_by_type("synthesis").await.unwrap_or(0);
        let agent_summary_count = state.silva.count_by_type("agent_summary").await.unwrap_or(0);
        let episode_count = state.silva.count_by_type("episode").await.unwrap_or(0);
        let agent_memory_count = state.silva.count_by_type("agent_memory").await.unwrap_or(0);
        let doc_count = state.silva.count_by_type("document").await.unwrap_or(0);
        let factual_total = episode_count + agent_memory_count + doc_count;
        let synthetic_total = summary_count + synthesis_count + agent_summary_count;
        let drift_ratio = if factual_total > 0 {
            synthetic_total as f64 / factual_total as f64
        } else { 0.0 };
        let ok = drift_ratio < 1.0;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "drift_guard",
            "pass": ok,
            "detail": format!(
                "drift_ratio={:.2} ({} synthetic / {} factual). summary={} synthesis={} agent_summary={} episode={} agent_memory={} document={}",
                drift_ratio, synthetic_total, factual_total,
                summary_count, synthesis_count, agent_summary_count,
                episode_count, agent_memory_count, doc_count
            )
        }));
    }

    // 8. Uptime probe: kernel has been running > 30 seconds (avoids false negatives during startup)
    {
        total += 1;
        let uptime = state.start_time.elapsed().as_secs();
        let ok = uptime > 30;
        if ok { passed += 1; }
        probe_results.push(serde_json::json!({
            "name": "uptime",
            "pass": ok,
            "detail": format!("{}s since start", uptime)
        }));
    }

    let score = if total > 0 { (passed as f64 / total as f64) * 100.0 } else { 0.0 };
    let status = if score >= 85.0 { "healthy" }
                 else if score >= 50.0 { "degraded" }
                 else { "critical" };

    Json(serde_json::json!({
        "status": status,
        "score": score,
        "passed": passed,
        "total": total,
        "probes": probe_results,
        "version": state.version
    }))
}

// ── hormones_handler ────────────────────────────────────────────────────────
pub async fn hormones_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let (signals, stress, novelty, saturation, energy) = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Ok(h) = srv.hormones.lock() {
            let sigs = h.active_signals();
            let s = h.get_intensity("stress");
            let n = h.get_intensity("novelty");
            let sat = h.get_intensity("saturation");
            let e = h.energy_level();
            (sigs, s, n, sat, e)
        } else {
            (vec![], 0.0, 0.0, 0.0, 1.0)
        }
    } else {
        (vec![], 0.0, 0.0, 0.0, 1.0)
    };

    Json(serde_json::json!({
        "signals": signals,
        "count": signals.len(),
        "stress":     (stress * 100.0).round() / 100.0,
        "novelty":    (novelty * 100.0).round() / 100.0,
        "saturation": (saturation * 100.0).round() / 100.0,
        "energy":     (energy * 100.0).round() / 100.0,
        "homeostasis": ((1.0 - stress - saturation * 0.5).max(0.0) * 100.0).round() / 100.0
    })).into_response()
}

// ── agent_profiles_handler ──────────────────────────────────────────────────
pub async fn agent_profiles_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use axum::http::StatusCode;

    let profiles = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            ap.lock().map(|p| p.list_profiles().unwrap_or_default()).unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    // Persist reputation scores to SilvaDB in background (idempotent upsert)
    {
        let profiles = profiles.clone();
        let silva = state.silva.clone();
        tokio::spawn(async move {
            crate::memory::agent_profile::sync_agent_reputation_to_silva(&silva, &profiles).await;
        });
    }

    (StatusCode::OK, Json(serde_json::json!({ "profiles": profiles }))).into_response()
}

// ── interoception_handler ───────────────────────────────────────────────────
pub async fn interoception_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use axum::http::StatusCode;

    let silva = state.silva.clone();
    let node_count = silva.node_count().await.unwrap_or(0) as f64;
    let edge_count = silva.edge_count().await.unwrap_or(0) as f64;
    let graph_density = if node_count > 1.0 { edge_count / (node_count * (node_count - 1.0)) } else { 0.0 };

    let (homeostasis, stress_level, active_pheromones, _agent_rhythms, capabilities) = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        let cap = serde_json::json!({
            "embeddings_loaded": srv.matcher.engine().is_some(),
            "reranker_loaded": srv.reranker.is_some(),
            "embedding_model": if srv.matcher.engine().is_some() { "bge-m3" } else { "none" },
            "reranker_model": if srv.reranker.is_some() { "jina-reranker-v1-turbo-en" } else { "none" },
        });
        if let Ok(h) = srv.hormones.lock() {
            let signals = h.active_signals();
            let stress = h.stress_level().max(0.0);
            let homeo = (1.0 - stress).max(0.0);
            (homeo, stress, signals.len(), serde_json::json!({}), cap)
        } else {
            (1.0, 0.0, 0, serde_json::json!({}), cap)
        }
    } else {
        (1.0, 0.0, 0, serde_json::json!({}), serde_json::json!({}))
    };

    let sessions = state.sessions.read().await;
    let knowledge_hunger = if sessions.is_empty() { 0.5_f64 } else {
        (1.0 - (node_count / 10000.0_f64).min(1.0)).max(0.0)
    };

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let agent_rhythms = {
        let rhythms: serde_json::Map<String, serde_json::Value> = sessions.values()
            .filter(|s| now_unix.saturating_sub(s.last_active_unix) < 3600)
            .map(|s| {
                let agent_key = s.agent_id.as_deref()
                    .unwrap_or(&s.client_name)
                    .to_string();
                (agent_key, serde_json::json!({
                    "tool_calls": s.tool_count,
                    "last_active_secs_ago": now_unix.saturating_sub(s.last_active_unix),
                    "client": s.client_name,
                }))
            })
            .collect();
        serde_json::Value::Object(rhythms)
    };

    let mut recommendations = vec![];
    if stress_level > 0.5 { recommendations.push("High stress detected \u{2014} check guild errors"); }
    if knowledge_hunger > 0.7 { recommendations.push("Low knowledge density \u{2014} consider ingesting more data"); }
    if graph_density < 0.001 { recommendations.push("Sparse knowledge graph \u{2014} connections are low"); }

    (StatusCode::OK, Json(serde_json::json!({
        "homeostasis": homeostasis,
        "stress_level": stress_level,
        "knowledge_hunger": knowledge_hunger,
        "graph_density": graph_density,
        "active_pheromones": active_pheromones,
        "agent_rhythms": agent_rhythms,
        "recommendations": recommendations,
        "capabilities": capabilities,
        "tunnel": {
            "enabled": state.tunnel_wsl_url.is_some(),
            "wsl_bridge_active": state.tunnel_wsl_url.is_some(),
            "wsl_url": state.tunnel_wsl_url
        }
    }))).into_response()
}

// ── metrics_history_handler ─────────────────────────────────────────────────
pub async fn metrics_history_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    use axum::http::StatusCode;

    let ring = state.metrics_ring.read().await;
    let snapshots: Vec<_> = ring.snapshots().into_iter().cloned().collect();
    (
        StatusCode::OK,
        crate::transport::http::Utf8Json(serde_json::json!({
            "snapshots": snapshots,
            "interval_secs": 5,
            "capacity": 60,
        })),
    )
        .into_response()
}

/// POST /api/v1/silva/graphrag-trigger
/// Manually run one GraphRAG pass and return detailed results.
pub async fn graphrag_trigger_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    use axum::http::StatusCode;

    let t0 = std::time::Instant::now();
    let rag = crate::memory::graph_rag::GraphRagManager::new(state.silva.clone());

    let targets = match rag.identify_summarization_targets(3).await {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({
            "error": format!("identify_summarization_targets failed: {}", e)
        }))).into_response(),
    };

    let target_count = targets.len();
    let mut saved = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for target in &targets {
        let member_ids: Vec<String> = target.nodes.iter().map(|n| n.id.clone()).collect();
        let summary: String = target.nodes.iter()
            .map(|n| n.content.chars().take(150).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n---\n");

        if summary.len() > 20 {
            match rag.save_summary(&target.cluster_id, &summary, member_ids).await {
                Ok(_) => saved += 1,
                Err(e) => errors.push(format!("cluster {}: {}", &target.cluster_id[..8], e)),
            }
        }
    }

    let elapsed = t0.elapsed().as_millis();
    (StatusCode::OK, Json(serde_json::json!({
        "targets_found": target_count,
        "summaries_saved": saved,
        "errors": errors,
        "elapsed_ms": elapsed
    }))).into_response()
}

/// GET /api/v1/autoresearch/summary
/// Returns the current state of the AutoResearch background loops, including active mutations, metrics, and calibration lineage.
pub async fn autoresearch_summary_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    use std::sync::atomic::Ordering;
    use crate::memory::idle_lab::{CANDIDATE_POOL_MULT, RERANK_WINDOW, SEMANTIC_WEIGHT, DEDUP_COSINE};

    let active = crate::memory::autoresearch::AUTORESEARCH_ACTIVE.load(Ordering::Relaxed);

    let pool_mult = CANDIDATE_POOL_MULT.load(Ordering::Relaxed);
    let rerank_win = RERANK_WINDOW.load(Ordering::Relaxed);
    let sem_weight = SEMANTIC_WEIGHT.load(Ordering::Relaxed);
    let dedup_cos = DEDUP_COSINE.load(Ordering::Relaxed);

    // Parse TSV history to build lineage and current metrics
    let tsv = std::fs::read_to_string("data/idle_lab_results.tsv").unwrap_or_default();
    let mut lineage: Vec<serde_json::Value> = Vec::new();
    let mut best_r1 = 0.65f64;
    let mut best_r5 = 0.90f64;
    for (step, line) in tsv.lines().filter(|l| !l.starts_with("timestamp")).enumerate() {
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 8 {
            let r1: f64 = cols[5].parse().unwrap_or(0.0);
            let r5: f64 = cols[6].parse().unwrap_or(0.0);
            let kept = cols[7] == "KEPT";
            if kept { best_r1 = r1; best_r5 = r5; }
            lineage.push(serde_json::json!({
                "step": step + 1,
                "target": format!("pool_mult={} rerank_win={}", cols[1], cols[2]),
                "val": cols[1].parse::<f64>().unwrap_or(0.0),
                "recall_1": r1,
                "status": if kept { "Committed" } else { "Reverted" }
            }));
        }
    }
    lineage.reverse(); // most recent first
    lineage.truncate(8);

    let summary = serde_json::json!({
        "status": if active { "Running" } else { "Idle" },
        "current_mutation": serde_json::Value::Null,
        "progress": {
            "current_step": lineage.len(),
            "total_steps": 100,
            "last_improvement_at": 0
        },
        "metrics": {
            "baseline": { "recall_1": 0.65, "recall_5": 0.90, "latency_ms": 202.0 },
            "current": { "recall_1": best_r1, "recall_5": best_r5, "latency_ms": 202.0 }
        },
        "lineage": lineage,
        "active": active,
        "current_params": {
            "candidate_pool_mult": pool_mult,
            "rerank_window": rerank_win,
            "semantic_weight": sem_weight,
            "dedup_cosine": dedup_cos,
        },
        "note": "IdleLab runs during NightConsolidation (hourly). Activate via POST /api/v1/autoresearch/start"
    });
    Json(summary)
}

/// POST /api/v1/autoresearch/start
/// Enables the AutoResearch daemon.
pub async fn autoresearch_start_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    use std::sync::atomic::Ordering;
    crate::memory::autoresearch::AUTORESEARCH_ACTIVE.store(true, Ordering::Relaxed);
    Json(serde_json::json!({ "status": "Started", "active": true }))
}

/// POST /api/v1/autoresearch/stop
/// Disables the AutoResearch daemon.
pub async fn autoresearch_stop_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    use std::sync::atomic::Ordering;
    crate::memory::autoresearch::AUTORESEARCH_ACTIVE.store(false, Ordering::Relaxed);
    Json(serde_json::json!({ "status": "Stopped", "active": false }))
}

/// POST /api/v1/autoresearch/evaluate
/// Runs a single organic optimization experiment on demand and returns the status.
pub async fn autoresearch_evaluate_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let data_dir = std::path::PathBuf::from("data");
    let idle_lab = crate::memory::idle_lab::IdleLab::new(state.silva.clone(), &data_dir);
    let engine = state.matcher.engine();
    let reranker_arc = if let Some(ref s) = state.server {
        let s_read = s.read().await;
        s_read.reranker.clone()
    } else {
        None
    };

    // Run exactly 1 experiment. It will mutate the atomic parameters and log to the TSV file.
    idle_lab.run_experiments(engine, reranker_arc.as_deref(), 1).await;

    Json(serde_json::json!({
        "status": "Success",
        "experiment_run": true
    }))
}
