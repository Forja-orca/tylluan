use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use crate::transport::http::HttpState;

pub async fn tylluan_graph_get_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Sovereign server not initialized"}))).into_response(),
    };
    let server = server_arc.read().await;
    match crate::transport::server::handler_graph::handle_tylluan_graph(&server, None).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn tylluan_graph_handler(
    State(state): State<Arc<HttpState>>,
    Json(args): Json<serde_json::Value>,
) -> impl IntoResponse {
    let server_arc = match state.server.as_ref() {
        Some(s) => s,
        None => return (StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"error": "Sovereign server not initialized"}))).into_response(),
    };
    let server = server_arc.read().await;
    match crate::transport::server::handler_graph::handle_tylluan_graph(&server, args.as_object().cloned()).await {
        Ok(res) => (StatusCode::OK, Json(res)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    }
}

pub async fn agents_list_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let profiles = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            if let Ok(p) = ap.lock() {
                p.list_profiles().unwrap_or_default()
            } else { vec![] }
        } else { vec![] }
    } else { vec![] };

    let reputation: Vec<serde_json::Value> = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            match ap.lock() {
                Ok(g) => g.get_domain_reputation().unwrap_or_default(),
                Err(_) => vec![],
            }
        } else { vec![] }
    } else { vec![] };

    let mut domain_map: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for rep in &reputation {
        if let Some(aid) = rep.get("agent_id").and_then(|v: &serde_json::Value| v.as_str()) {
            domain_map.entry(aid.to_string()).or_default().push(rep.clone());
        }
    }

    let agents: Vec<serde_json::Value> = profiles.iter().map(|p| {
        let domains = domain_map.get(&p.agent_id).cloned().unwrap_or_default();
        let identity_node = format!("agent_identity_{}", p.agent_id);
        serde_json::json!({
            "agent_id": p.agent_id,
            "role": p.role,
            "total_calls": p.total_calls,
            "first_seen": p.first_seen,
            "last_intent": p.last_intent,
            "competencies": p.competencies,
            "identity_node": identity_node,
            "domains": domains
        })
    }).collect();

    (StatusCode::OK, Json(serde_json::json!({ "agents": agents, "count": agents.len() }))).into_response()
}

use crate::memory::silva::SilvaDB;

/// Core of `GET /api/v1/stigmergy/zones`, split out of the handler so its
/// contract (12 default zones + live work_traces inside the 16h window, heat
/// math, contention_risk matrix) is testable against a real SilvaDB with
/// deterministic time -- mirrors the work_traces tests in silva/tests.rs.
pub async fn stigmergy_zones_core(db: &SilvaDB, now_unix: i64) -> serde_json::Value {
    let default_zones: Vec<(&str, &str, &str, Vec<&str>)> = vec![
        ("crates/tylluan-kernel/transport", "kernel", "Sovereign MCP transport handlers, SSE event loop, HTTP router & rate limiter", vec!["crates/tylluan-kernel/router", "crates/tylluan-link/p2p"]),
        ("dashboard/src/components", "dashboard", "React dashboard UI, consolidated tab suites, metric primitives, and observability panels", vec!["dashboard/src/hooks", "packages/tylluan-ui-core"]),
        ("docs/reference/adr", "docs", "Architecture Decision Records (ADR-001..015), declarative contracts, and specifications", vec!["docs/roadmap", "docs/internal"]),
        ("crates/tylluan-kernel/memory", "kernel", "SilvaDB semantic graph, FSRS-5 spaced consolidation, decay.rs stigmergy math, and agent profiles", vec!["crates/tylluan-kernel/router"]),
        ("crates/tylluan-link/gossip", "link", "Gossip protocol anti-entropy sync, LRU vector stores, and peer capability registry", vec!["crates/tylluan-link/p2p"]),
        ("guilds/core", "guilds", "Python ecosystem tools, vision moondream, check_coloquio, and worker coordinators", vec!["guilds/vision"]),
        ("crates/tylluan-link/p2p", "link", "Noise XK session pools, direct TCP socket dispatch, and NAT traversal handlers", vec!["crates/tylluan-kernel/transport"]),
        ("docs-site/src", "docs", "Next.js 3010 interactive architecture visualizer, 3D maps, and interactive graphs", vec!["docs/reference/adr"]),
        ("crates/tylluan-kernel/config", "kernel", "tylluan.toml declarative configuration parser, identity keys, and environment guards", vec![]),
        ("crates/tylluan-kernel/router", "kernel", "Intent router, embeddings batching, catalog scoring, and capability discovery", vec!["crates/tylluan-kernel/memory"]),
        ("guilds/vision", "guilds", "Local OCR, screenshot capture, and visual reasoning pipeline", vec!["guilds/core"]),
        ("docs/roadmap", "docs", "Technical roadmaps (ROADMAP_O3), milestone trackers, and specification drafts", vec!["docs/reference/adr"]),
    ];

    let mut uris_set: std::collections::HashSet<String> = default_zones.iter().map(|(id, _, _, _)| (*id).to_string()).collect();

    let cutoff = now_unix - (16 * 3600);
    let extra_uris = tokio::task::block_in_place(|| {
        let conn_arc = db.conn_lock();
        let conn = conn_arc.blocking_lock();
        let mut stmt = match conn.prepare("SELECT DISTINCT target_uri FROM work_traces WHERE touched_at >= ?1") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([cutoff], |r| r.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect::<Vec<String>>()
    });
    for u in extra_uris {
        uris_set.insert(u);
    }

    let all_uris: Vec<String> = uris_set.into_iter().collect();
    let heat_map = db.work_traces_heat_batch(&all_uris, Some(now_unix)).await.unwrap_or_default();

    let zones_details = tokio::task::block_in_place(|| {
        let conn_arc = db.conn_lock();
        let conn = conn_arc.blocking_lock();
        let mut map: std::collections::HashMap<String, (Vec<serde_json::Value>, Vec<String>, usize, i64)> = std::collections::HashMap::new();

        if let Ok(mut stmt) = conn.prepare(
            "SELECT target_uri, trace_id, agent_id, trace_type, weight, touched_at 
             FROM work_traces 
             WHERE touched_at >= ?1 
             ORDER BY touched_at DESC",
        ) {
            let rows_res = stmt.query_map([cutoff], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            });
            if let Ok(rows) = rows_res {
                for r in rows.filter_map(|x| x.ok()) {
                    let (uri, trace_id, agent_id, trace_type, weight, touched_at) = r;
                    let entry = map.entry(uri).or_insert_with(|| (Vec::new(), Vec::new(), 0, 0));
                    entry.2 += 1; // total_traces
                    if entry.3 == 0 || touched_at > entry.3 {
                        entry.3 = touched_at; // last_touched_at
                    }
                    if !agent_id.is_empty() && !entry.1.contains(&agent_id) {
                        entry.1.push(agent_id.clone());
                    }
                    if entry.0.len() < 10 {
                        entry.0.push(serde_json::json!({
                            "trace_id": trace_id,
                            "agent_id": agent_id,
                            "trace_type": trace_type,
                            "weight": weight,
                            "touched_at": touched_at,
                        }));
                    }
                }
            }
        }
        map
    });

    let default_desc_map: std::collections::HashMap<&str, (&str, &str, Vec<&str>)> = default_zones
        .into_iter()
        .map(|(id, sub, desc, neigh)| (id, (sub, desc, neigh)))
        .collect();

    let mut zones_list: Vec<serde_json::Value> = Vec::new();
    for uri in &all_uris {
        let heat = heat_map.get(uri).copied().unwrap_or(0.0);
        let (subsystem, description, neighbor_zones) = if let Some(&(sub, desc, ref neigh)) = default_desc_map.get(uri.as_str()) {
            (sub.to_string(), desc.to_string(), neigh.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
        } else {
            let sub = if uri.starts_with("crates/") {
                "kernel"
            } else if uri.starts_with("dashboard/") {
                "dashboard"
            } else if uri.starts_with("guilds/") {
                "guilds"
            } else if uri.starts_with("docs/") {
                "docs"
            } else {
                "other"
            };
            (sub.to_string(), format!("Workspace resource: {uri}"), Vec::new())
        };

        let (traces, active_agents, total_traces, last_touched_at) = zones_details.get(uri).cloned().unwrap_or_default();
        let contention_risk = if heat >= 1.5 && active_agents.len() > 1 {
            "high"
        } else if heat >= 1.0 {
            "moderate"
        } else {
            "low"
        };

        zones_list.push(serde_json::json!({
            "zone_id": uri,
            "subsystem": subsystem,
            "description": description,
            "heat": (heat * 100.0).round() / 100.0,
            "half_life_hours": 4.0,
            "active_agents": active_agents,
            "total_traces": total_traces,
            "last_touched_at": last_touched_at,
            "traces": traces,
            "neighbor_zones": neighbor_zones,
            "contention_risk": contention_risk,
        }));
    }

    zones_list.sort_by(|a, b| {
        let ha = a.get("heat").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let hb = b.get("heat").and_then(|v| v.as_f64()).unwrap_or(0.0);
        hb.partial_cmp(&ha).unwrap_or(std::cmp::Ordering::Equal)
    });

    let ts = chrono::DateTime::from_timestamp(now_unix, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339();
    serde_json::json!({
        "zones": zones_list,
        "count": zones_list.len(),
        "half_life_hours": 4.0,
        "window_hours": 16,
        "ts": ts,
    })
}

/// `GET /api/v1/stigmergy/zones` — thin wrapper over the testable core.
pub async fn stigmergy_zones_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let body = stigmergy_zones_core(&state.silva, now_unix).await;
    (StatusCode::OK, Json(body)).into_response()
}

#[cfg(test)]
mod stigmergy_zones_tests {
    use super::*;
    use crate::memory::silva::SilvaDB;

    async fn test_silva() -> SilvaDB {
        SilvaDB::in_memory().await.expect("test silva init")
    }

    async fn touch(db: &SilvaDB, agent: &str, zone: &str, ago_secs: i64, now: i64) {
        let conn_arc = db.conn_lock();
        let conn = conn_arc.lock().await;
        conn.execute(
            "INSERT INTO work_traces (target_uri, target_kind, agent_id, trace_type, weight, touched_at)
             VALUES (?1, 'file', ?2, 'direct', 1.0, ?3)",
            rusqlite::params![zone, agent, now - ago_secs],
        )
        .expect("insert work trace");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zones_contract_twelve_default_zones() {
        let db = test_silva().await;
        let now = 1_800_000_000i64;
        let body = stigmergy_zones_core(&db, now).await;
        assert_eq!(body["count"].as_u64(), Some(12), "must expose the 12 default zones");
        assert_eq!(body["half_life_hours"].as_f64(), Some(4.0));
        assert_eq!(body["window_hours"].as_u64(), Some(16));
        assert!(body["zones"].is_array());
        let ids: Vec<&str> = body["zones"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|z| z["zone_id"].as_str())
            .collect();
        assert!(ids.contains(&"crates/tylluan-kernel/transport"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zones_live_traces_rank_hot_zone_first_with_real_heat() {
        let db = test_silva().await;
        let now = 1_800_000_000i64;
        // Hot zone: 3 agents recently; cold zone: 1 old trace.
        touch(&db, "buffy", "crates/tylluan-kernel/transport", 600, now).await;
        touch(&db, "deep", "crates/tylluan-kernel/transport", 900, now).await;
        touch(&db, "mimo", "crates/tylluan-kernel/transport", 1200, now).await;
        touch(&db, "buffy", "docs/reference/adr", 60_000, now).await;

        let body = stigmergy_zones_core(&db, now).await;
        let zones = body["zones"].as_array().unwrap();
        let hot = zones
            .iter()
            .find(|z| z["zone_id"] == "crates/tylluan-kernel/transport")
            .expect("hot zone present");
        let cold = zones
            .iter()
            .find(|z| z["zone_id"] == "docs/reference/adr")
            .expect("cold zone present");

        assert!(hot["heat"].as_f64().unwrap_or(0.0) > cold["heat"].as_f64().unwrap_or(0.0));
        // Zones are sorted by heat descending.
        assert_eq!(zones[0]["zone_id"], "crates/tylluan-kernel/transport");
        assert_eq!(hot["total_traces"].as_u64(), Some(3));
        let agents: Vec<&str> = hot["active_agents"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(agents.contains(&"buffy") && agents.contains(&"deep") && agents.contains(&"mimo"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zones_window_cuts_traces_older_than_16h() {
        let db = test_silva().await;
        let now = 1_800_000_000i64;
        // 20h old: outside the 16h window, must not count.
        touch(&db, "buffy", "crates/tylluan-kernel/memory", 72_000, now).await;
        // 1h old: inside.
        touch(&db, "deep", "crates/tylluan-kernel/memory", 3_600, now).await;

        let body = stigmergy_zones_core(&db, now).await;
        let mem = body["zones"]
            .as_array()
            .unwrap()
            .iter()
            .find(|z| z["zone_id"] == "crates/tylluan-kernel/memory")
            .expect("memory zone present");
        assert_eq!(mem["total_traces"].as_u64(), Some(1), "20h-old trace must be cut by the 16h window");
        let agents: Vec<&str> = mem["active_agents"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(!agents.contains(&"buffy"));
        assert!(agents.contains(&"deep"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn zones_contention_risk_matrix() {
        let db = test_silva().await;
        let now = 1_800_000_000i64;
        // high: heat >= 1.5 && agents > 1 (3 fresh traces, 3 agents).
        touch(&db, "a1", "crates/tylluan-link/gossip", 60, now).await;
        touch(&db, "a2", "crates/tylluan-link/gossip", 90, now).await;
        touch(&db, "a3", "crates/tylluan-link/gossip", 120, now).await;
        // moderate: heat >= 1.0, single agent.
        touch(&db, "a1", "crates/tylluan-kernel/config", 60, now).await;
        touch(&db, "a1", "crates/tylluan-kernel/config", 90, now).await;
        // low: single stale-ish trace.
        touch(&db, "a1", "docs/roadmap", 3_600, now).await;

        let body = stigmergy_zones_core(&db, now).await;
        let risk = |zone: &str| -> String {
            body["zones"]
                .as_array()
                .unwrap()
                .iter()
                .find(|z| z["zone_id"] == zone)
                .map(|z| z["contention_risk"].as_str().unwrap_or("").to_string())
                .unwrap_or_default()
        };
        assert_eq!(risk("crates/tylluan-link/gossip"), "high");
        assert_eq!(risk("crates/tylluan-kernel/config"), "moderate");
        assert_eq!(risk("docs/roadmap"), "low");
    }
}

