use crate::transport::http::HttpState;
use axum::{
    extract::State,
    http::{StatusCode, header::CONTENT_TYPE, HeaderMap, HeaderValue},
    response::IntoResponse,
};
use std::sync::Arc;

pub async fn metrics_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    let uptime_secs = state.start_time.elapsed().as_secs();
    let (total_guilds, active_guilds) = state.registry.guild_stats().await.unwrap_or((0, 0));
    let node_count = state.silva.node_count().await.unwrap_or(0);
    let edge_count = state.silva.edge_count().await.unwrap_or(0);

    let body = format!(
        "# HELP tylluan_guilds_active Number of guilds currently running\n\
         # TYPE tylluan_guilds_active gauge\n\
         tylluan_guilds_active {}\n\
         # HELP tylluan_guilds_total Total registered guilds\n\
         # TYPE tylluan_guilds_total gauge\n\
         tylluan_guilds_total {}\n\
         # HELP tylluan_memory_nodes Total memory nodes in SilvaDB\n\
         # TYPE tylluan_memory_nodes gauge\n\
         tylluan_memory_nodes {}\n\
         # HELP tylluan_memory_edges Total graph edges in SilvaDB\n\
         # TYPE tylluan_memory_edges gauge\n\
         tylluan_memory_edges {}\n\
         # HELP tylluan_uptime_seconds Seconds since kernel start\n\
         # TYPE tylluan_uptime_seconds counter\n\
         tylluan_uptime_seconds {}\n",
        active_guilds, total_guilds, node_count, edge_count, uptime_secs,
    );

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    (StatusCode::OK, headers, body)
}
