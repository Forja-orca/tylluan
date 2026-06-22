use axum::{
    Json,
    extract::{State, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use serde_json;
use crate::transport::http::HttpState;

#[derive(serde::Deserialize)]
pub struct SuggestQuery { domain: Option<String> }

fn calculate_reputation(success_rate: f64, avg_latency_ms: f64, total_calls: u64) -> f64 {
    let score = if total_calls == 0 {
        0.5
    } else if success_rate >= 0.85 {
        0.75 + (success_rate - 0.85) * 1.66
    } else if success_rate >= 0.50 {
        0.40 + success_rate * 0.35
    } else {
        success_rate * 0.40
    };
    let score = if avg_latency_ms > 5000.0 { score - 0.10 } else { score };
    score.max(0.0).min(1.0)
}

pub async fn collective_pulse_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let mailbox = &state.mailbox;
    let silva = &state.silva;

    let broadcasts_1h = mailbox.broadcast_count_last_hours(1).await.unwrap_or(0);
    let node_count = silva.node_count().await.unwrap_or(0);
    let edge_count = silva.edge_count().await.unwrap_or(0);

    // Active agents = those with identity nodes touched in last hour
    let active_agents: Vec<String> = silva
        .get_identity_nodes().await
        .unwrap_or_default()
        .into_iter()
        .filter(|n| {
            let ts = n.updated_at.as_deref().or(n.created_at.as_deref()).unwrap_or("");
            chrono::DateTime::parse_from_rfc3339(ts)
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.and_utc().fixed_offset()))
                .map(|dt| chrono::Utc::now().signed_duration_since(dt).num_minutes() < 60)
                .unwrap_or(false)
        })
        .map(|n| n.id.trim_start_matches("agent:").to_string())
        .collect();

    (StatusCode::OK, Json(serde_json::json!({
        "active_agents": active_agents,
        "active_count": active_agents.len(),
        "broadcasts_last_hour": broadcasts_1h,
        "graph": { "nodes": node_count, "edges": edge_count },
        "ts": chrono::Utc::now().to_rfc3339(),
    }))).into_response()
}

pub async fn collective_timeline_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let conn_guard = state.silva.conn_lock();
    let conn = conn_guard.lock().await;
    
    let sql = "SELECT node_id, agent_id, touched_at, trace_type FROM node_traces ORDER BY touched_at DESC LIMIT 50";
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()}))).into_response(),
    };
    
    let events: Vec<serde_json::Value> = match stmt.query_map([], |row| {
        let node_id: String = row.get(0)?;
        let agent_id: String = row.get(1)?;
        let ts: i64 = row.get(2)?;
        let trace_type: String = row.get(3)?;
        Ok(serde_json::json!({
            "id": format!("{}_{}", node_id, ts),
            "type": trace_type,
            "content": format!("Agente {} interactuÃ³ con {} ({})", agent_id, node_id, trace_type),
            "weight": 1.0,
            "updated_at": chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.to_rfc3339()).unwrap_or_default(),
        }))
    }) {
        Ok(rows) => rows.filter_map(|r| r.ok()).collect(),
        Err(e) => {
            tracing::warn!("collective_timeline_handler query failed: {}", e);
            vec![]
        }
    };

    (StatusCode::OK, Json(serde_json::json!({
        "events": events,
        "count": events.len(),
        "ts": chrono::Utc::now().to_rfc3339(),
    }))).into_response()
}

pub async fn collective_heatmap_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let heatmap = tokio::task::block_in_place(|| -> anyhow::Result<Vec<serde_json::Value>> {
        let conn_arc = state.silva.conn_lock();
        let conn = conn_arc.blocking_lock();
        let sql = "SELECT strftime('%Y-%m-%d', datetime(touched_at, 'unixepoch')) as date, COUNT(*) as count
                   FROM node_traces
                   WHERE touched_at >= strftime('%s', 'now', '-1 year')
                   GROUP BY date
                   ORDER BY date ASC";
        let mut stmt = conn.prepare(sql)?;
        let rows: Vec<serde_json::Value> = stmt.query_map([], |row| {
            Ok(serde_json::json!({ "date": row.get::<_, String>(0)?, "count": row.get::<_, i64>(1)? }))
        })?.filter_map(|r| r.ok()).collect();
        Ok(rows)
    }).unwrap_or_default();

    (StatusCode::OK, Json(serde_json::json!({
        "heatmap": heatmap,
        "window_hours": 8760,
        "ts": chrono::Utc::now().to_rfc3339(),
    }))).into_response()
}

pub async fn collective_reputation_handler(
    State(state): State<Arc<HttpState>>,
) -> impl IntoResponse {
    let stats = state.registry.guild_call_stats().await.unwrap_or_default();
    let guilds = state.registry.status_all().await.unwrap_or_default();
    let running_map: std::collections::HashMap<&str, bool> = guilds.iter().map(|g| (g.name.as_str(), g.running)).collect();

    let reputation: Vec<serde_json::Value> = stats.iter().map(|s| {
        let score = calculate_reputation(s.success_rate, s.avg_latency_ms, s.total_calls);
        let tier = if s.total_calls == 0 { "dormant" }
                    else if s.success_rate >= 0.85 { "reliable" }
                    else if s.success_rate >= 0.50 { "unstable" }
                    else { "degraded" };
        
        serde_json::json!({
            "guild": s.guild_name,
            "agent_id": format!("agent:{}", s.guild_name),
            "score": (score * 100.0).round() / 100.0,
            "total_calls": s.total_calls,
            "successful_calls": s.successful_calls,
            "success_rate": s.success_rate,
            "avg_latency_ms": (s.avg_latency_ms * 100.0).round() / 100.0,
            "running": running_map.get(s.guild_name.as_str()).copied().unwrap_or(false),
            "tier": tier
        })
    }).collect();

    let mut by_domain: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for row in &reputation {
        let guild = row["guild"].as_str().unwrap_or("").to_string();
        let builders = ["bash", "git", "code", "filesystem", "docker", "playwright"];
        let domain = if builders.contains(&guild.to_lowercase().as_str()) {
            "builders"
        } else {
            "scholars"
        };
        by_domain.entry(domain.to_string()).or_default().push(row.clone());
    }

    let total: f64 = reputation.iter()
        .filter(|r| r["total_calls"].as_u64().is_some_and(|c| c > 0))
        .map(|r| r["score"].as_f64().unwrap_or(0.0))
        .sum();
    let count = reputation.iter()
        .filter(|r| r["total_calls"].as_u64().is_some_and(|c| c > 0))
        .count() as f64;
    let avg_reputation = if count > 0.0 { total / count } else { 0.0 };

    (StatusCode::OK, Json(serde_json::json!({
        "reputation": reputation,
        "by_domain": by_domain,
        "avg_reputation": (avg_reputation * 100.0).round() / 100.0,
        "ts": chrono::Utc::now().to_rfc3339(),
    }))).into_response()
}

pub async fn collective_suggest_handler(
    State(state): State<Arc<HttpState>>,
    Query(q): Query<SuggestQuery>,
) -> impl IntoResponse {
    let domain = q.domain.unwrap_or_default();
    if domain.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "domain param required" }))).into_response();
    }

    let reputation: Vec<serde_json::Value> = if let Some(ref srv_arc) = state.server {
        let srv = srv_arc.read().await;
        if let Some(ref ap) = srv.agent_profiles {
            match ap.lock() {
                Ok(g) => match g.get_domain_reputation() { Ok(v) => v, Err(_) => vec![] },
                Err(_) => vec![],
            }
        } else { vec![] }
    } else { vec![] };

    let domain_lower = domain.to_lowercase();
    let mut candidates: Vec<&serde_json::Value> = reputation.iter()
        .filter(|r: &&serde_json::Value| r.get("domain").and_then(|v: &serde_json::Value| v.as_str()).map(|d| d.to_lowercase().contains(&domain_lower)).unwrap_or(false))
        .collect();

    candidates.sort_by(|a: &&serde_json::Value, b: &&serde_json::Value| {
        let ra = a.get("rate").and_then(|v: &serde_json::Value| v.as_f64()).unwrap_or(0.0);
        let rb = b.get("rate").and_then(|v: &serde_json::Value| v.as_f64()).unwrap_or(0.0);
        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
    });

    if let Some(best) = candidates.first() {
        (StatusCode::OK, Json(serde_json::json!({
            "domain": domain,
            "best_agent": best.get("agent_id"),
            "rate": best.get("rate"),
            "total_calls": best.get("total"),
            "confidence": candidates.first().and_then(|c: &&serde_json::Value| c.get("rate")).and_then(|r: &serde_json::Value| r.as_f64()).unwrap_or(0.0),
            "alternatives": candidates.iter().skip(1).take(3).collect::<Vec<_>>()
        }))).into_response()
    } else {
        (StatusCode::OK, Json(serde_json::json!({
            "domain": domain,
            "best_agent": null,
            "message": "No reputation data for this domain yet"
        }))).into_response()
    }
}
