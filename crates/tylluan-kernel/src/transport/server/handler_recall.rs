use rmcp::{Error as McpError, model::*};
use serde_json;
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use rusqlite::params;

use crate::memory::silva::{GraphNode, jaccard_similarity};
use crate::memory::idle_lab::{CANDIDATE_POOL_MULT, RERANK_WINDOW};
use crate::registry::proxy::error_result;
use super::TylluanServer;
use crate::memory::mailbox::BlackboardMessage;

// ─── Hot Context: recency-biased short-term memory (Letta-inspired) ────────
// A rolling buffer of recently recalled node IDs. Boosts matching nodes on
// subsequent recalls to create conversational coherence (recency ×2.0).

#[derive(Clone)]
pub struct HotContext {
    buffer: VecDeque<String>,
    max_size: usize,
    boost_factor: f64,
}

impl HotContext {
    pub fn new(max_size: usize) -> Self {
        Self { buffer: VecDeque::with_capacity(max_size), max_size, boost_factor: 2.0 }
    }

    /// Returns the boost factor if the node is in the hot context.
    pub fn boost_for(&self, node_id: &str) -> f64 {
        if self.buffer.iter().any(|id| id == node_id) { self.boost_factor } else { 1.0 }
    }

    /// Mark a node as recently accessed. Moves to front if already present.
    pub fn touch(&mut self, node_id: String) {
        if let Some(pos) = self.buffer.iter().position(|id| *id == node_id) {
            self.buffer.remove(pos);
        }
        if self.buffer.len() >= self.max_size {
            self.buffer.pop_back();
        }
        self.buffer.push_front(node_id);
    }

    /// Full list of hot node IDs (for diagnostics).
    pub fn snapshot(&self) -> Vec<String> {
        self.buffer.iter().cloned().collect()
    }
}

// ─── Jaccard similarity for cache matching ───────────────────────────────────



// ─── LRU cache for similar recall queries ────────────────────────────────────

/// In-memory LRU cache keyed by query text. Hit <2ms vs >400ms without.
#[derive(Clone)]
pub struct RecallCacheEntry {
    pub query: String,
    pub include_archived: bool,
    pub results: Vec<(GraphNode, f32)>,
    pub inserted_at: std::time::Instant,
}

pub struct RecallCache {
    entries: VecDeque<RecallCacheEntry>,
    max_size: usize,
}

const RECALL_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

impl RecallCache {
    pub fn new(max_size: usize) -> Self {
        Self { entries: VecDeque::with_capacity(max_size), max_size }
    }

    /// Look up a query by Jaccard similarity. Returns matching results if
    /// any cached query has similarity > 0.85 and is within TTL.
    pub fn get(&mut self, query: &str, include_archived: bool) -> Option<&Vec<(GraphNode, f32)>> {
        let threshold = 0.85;
        for entry in &self.entries {
            if entry.include_archived == include_archived
                && entry.inserted_at.elapsed() < RECALL_CACHE_TTL
                && jaccard_similarity(&entry.query, query) > threshold
            {
                return Some(&entry.results);
            }
        }
        None
    }

    /// Insert a new entry. Evicts the oldest if at capacity.
    pub fn put(&mut self, query: String, include_archived: bool, results: Vec<(GraphNode, f32)>) {
        if self.entries.len() >= self.max_size {
            self.entries.pop_front();
        }
        self.entries.push_back(RecallCacheEntry {
            query,
            include_archived,
            results,
            inserted_at: std::time::Instant::now(),
        });
    }

    /// Invalidate all cached entries. Called after tylluan_remember to
    /// ensure new memories are immediately visible to subsequent recalls.
    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }
}

/// M31-P1: Memory isolation — if `agent_id` has memory_isolation=true,
/// filter out nodes that don't belong to it. Was duplicated verbatim across
/// the cache-hit and live-query paths; consolidated into one call site.
async fn apply_memory_isolation(scored: &mut Vec<(GraphNode, f32)>, agent_id: &Option<String>) {
    let Some(aid) = agent_id else { return };
    let Ok(config_lock) = crate::config::TylluanConfig::load_cached() else { return };
    let cfg = config_lock.read().await;
    if crate::transport::http::auth::agent_has_memory_isolation(aid, &cfg.security.acl) {
        let aid_pattern = format!("\"agent_id\":\"{aid}\"");
        let before = scored.len();
        scored.retain(|(n, _)| n.metadata.contains(&aid_pattern));
        tracing::info!("🧊 Memory isolation: {before}→{} nodes for agent '{aid}'", scored.len());
    }
}

pub async fn handle_tylluan_recall(
    server: &TylluanServer,
    arguments: Option<serde_json::Map<String, serde_json::Value>>,
) -> Result<CallToolResult, McpError> {
    let query = arguments.as_ref()
        .and_then(|a| a.get("query")).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let limit = arguments.as_ref()
        .and_then(|a| a.get("limit")).and_then(|v| v.as_u64()).unwrap_or(5) as usize;
    let mode = arguments.as_ref()
        .and_then(|a| a.get("mode")).and_then(|v| v.as_str())
        .unwrap_or("personal").to_string();
    let offset_arg = arguments.as_ref()
        .and_then(|a| a.get("offset")).and_then(|v| v.as_u64()).map(|v| v as i64);

    tracing::info!(
        gen_ai.operation.name = "tylluan_recall",
        gen_ai.request.model = "hybrid_memory",
        gen_ai.request.max_results = limit as u64,
        query = %query,
        mode = %mode,
        "Handling tylluan_recall"
    );

    if query.trim().is_empty() {
        return Ok(error_result("tylluan_recall requires a non-empty 'query' argument."));
    }
    let compact = arguments.as_ref()
        .and_then(|a| a.get("compact")).and_then(|v| v.as_bool()).unwrap_or(true);
    let episodic = arguments.as_ref()
        .and_then(|a| a.get("episodic")).and_then(|v| v.as_bool()).unwrap_or(false);
    let include_archived = arguments.as_ref()
        .and_then(|a| a.get("include_archived")).and_then(|v| v.as_bool()).unwrap_or(false);
    let rec_agent_id = arguments.as_ref()
        .and_then(|a| a.get("agent_id")).and_then(|v| v.as_str())
        .map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    // Prepend agent session summary if available (using singleton manager)
    // M34-P0: provenance-based trust gate — federation-sourced nodes carry a
    // disclaimer instead of implicit high-authority framing (OWASP ASI06).
    let session_context: Option<String> = if let Some(ref aid) = rec_agent_id {
        if let Some(ref amm) = server.agent_memory {
            amm.get_summary(aid).await.map(|node| {
                let header = if node.provenance == "federation_peer" {
                    "### Contexto de sesiones anteriores (fuente: peer federado, sin verificar)"
                } else {
                    "### Contexto de sesiones anteriores"
                };
                format!("{header}\n{}\n\n---\n", node.content)
            })
        } else { None }
    } else { None };

    if query.starts_with("@inbox") {
        let agent_id = rec_agent_id.as_deref().unwrap_or("anonymous");
        let messages = if query.starts_with("@inbox:") {
            let thread_id = query.strip_prefix("@inbox:").unwrap_or("");
            server.mailbox.get_thread(agent_id, thread_id).await
        } else {
            server.mailbox.get_messages_for(agent_id, limit).await
        };

        return match messages {
            Ok(msgs) if msgs.is_empty() => Ok(CallToolResult {
                content: vec![Content::text("Inbox is empty.")],
                is_error: Some(false),
            }),
            Ok(msgs) => {
                let text = msgs.iter().map(|m| {
                    let body = BlackboardMessage::from_payload(&m.payload)
                        .map(|bm| format!("[{}] {}", bm.msg_type.to_uppercase(), bm.body))
                        .unwrap_or_else(|| m.payload.chars().take(200).collect());
                    format!("[{} FROM {}] {}", m.status, m.sender_id, body)
                }).collect::<Vec<_>>().join("\n");
                Ok(CallToolResult { content: vec![Content::text(text)], is_error: Some(false) })
            },
            Err(e) => Ok(error_result(&format!("Inbox retrieval failed: {e}"))),
        };
    }

    if query.starts_with("@coloquio") {
        let coloquio = match server.coloquio.as_ref() {
            Some(c) => c,
            None => return Ok(error_result("Coloquio is not available.")),
        };
        let trimmed = query.trim();
        if trimmed == "@coloquio" || trimmed == "@coloquio:list" {
            let channels = match coloquio.list_channels().await {
                Ok(ch) => ch,
                Err(e) => return Ok(error_result(&format!("Coloquio error: {e}"))),
            };
            if channels.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::text("No channels yet. Create one with `crea canal <name>`.")],
                    is_error: Some(false),
                });
            }
            let text = channels.iter().map(|c| {
                format!("- **#{}**: {} messages (last turn: {})", c.channel_id, c.message_count, c.last_turn)
            }).collect::<Vec<_>>().join("\n");
            return Ok(CallToolResult {
                content: vec![Content::text(format!("## Coloquio Channels\n\n{text}"))],
                is_error: Some(false),
            });
        }
        if trimmed == "@coloquio:unread" || trimmed == "@coloquio:whats_new" {
            let reader_id = rec_agent_id.as_deref().unwrap_or("agent");
            let summary = match coloquio.unread_summary(reader_id).await {
                Ok(s) => s,
                Err(e) => return Ok(error_result(&format!("Coloquio error: {e}"))),
            };
            let mut results = Vec::new();
            for item in summary {
                if item.unread_count > 0
                    && let Ok(msgs) = coloquio.get_new_messages(&item.channel_id, reader_id, limit as i64).await
                        && !msgs.is_empty() {
                            results.push(format!("### #{} ({} unread)\n", item.channel_id, item.unread_count));
                            for m in &msgs {
                                results.push(format!("[T{}] @{}: {}", m.turn, m.author_id, m.content));
                            }
                            // Auto-advance reader cursor
                            let max_turn = msgs.iter().map(|m| m.turn).max().unwrap_or(0);
                            if max_turn > 0 {
                                let _ = coloquio.mark_read(&item.channel_id, reader_id, max_turn).await;
                            }
                        }
            }
            if results.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::text("No unread messages.")],
                    is_error: Some(false),
                });
            }
            return Ok(CallToolResult {
                content: vec![Content::text(format!("## Unread Coloquio Messages\n\n{}", results.join("\n")))],
                is_error: Some(false),
            });
        }
        if let Some(search_term) = trimmed.strip_prefix("@coloquio:search:") {
            if search_term.is_empty() {
                return Ok(error_result("Usage: `@coloquio:search:<keyword>`"));
            }
            let channels = match coloquio.list_channels().await {
                Ok(ch) => ch,
                Err(e) => return Ok(error_result(&format!("Coloquio error: {e}"))),
            };
            let mut results = Vec::new();
            for ch in channels.iter().take(10) {
                if let Ok(msgs) = coloquio.search_messages(&ch.channel_id, search_term, 5).await {
                    for msg in msgs {
                        results.push(format!("[#{} T{}] @{}: {}", ch.channel_id, msg.turn, msg.author_id, msg.content));
                    }
                }
            }
            if results.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::text(format!("No matches for '{search_term}' in any channel."))],
                    is_error: Some(false),
                });
            }
            return Ok(CallToolResult {
                content: vec![Content::text(format!("## Search: '{}'\n\n{}", search_term, results.join("\n\n")))],
                is_error: Some(false),
            });
        }
        if let Some(channel_id) = trimmed.strip_prefix("@coloquio:") {
            let parts: Vec<&str> = channel_id.split(':').collect();
            let cid = parts[0];
            if cid.is_empty() {
                return Ok(error_result("Usage: `@coloquio:<channel_id>` or `@coloquio:<channel_id>:<keyword>`"));
            }
            if parts.len() > 1 && !parts[1].is_empty() {
                if parts[1] == "unread" || parts[1] == "whats_new" {
                    let reader_id = rec_agent_id.as_deref().unwrap_or("agent");
                    let msgs = match coloquio.get_new_messages(cid, reader_id, limit as i64).await {
                        Ok(msgs) => msgs,
                        Err(e) => return Ok(error_result(&format!("Coloquio read error: {e}"))),
                    };
                    if msgs.is_empty() {
                        return Ok(CallToolResult {
                            content: vec![Content::text(format!("No new messages in #{cid}"))],
                            is_error: Some(false),
                        });
                    }
                    let text = msgs.iter().map(|m| {
                        format!("[T{}] **@{}**: {}", m.turn, m.author_id, m.content)
                    }).collect::<Vec<_>>().join("\n\n");
                    // Auto-advance reader cursor
                    let max_turn = msgs.iter().map(|m| m.turn).max().unwrap_or(0);
                    if max_turn > 0 {
                        let _ = coloquio.mark_read(cid, reader_id, max_turn).await;
                    }
                    return Ok(CallToolResult {
                        content: vec![Content::text(format!("## #{cid} — Unread Messages\n\n{text}"))],
                        is_error: Some(false),
                    });
                } else {
                    let keyword = parts[1];
                    let msgs = match coloquio.search_messages(cid, keyword, limit as i64).await {
                        Ok(msgs) => msgs,
                        Err(e) => return Ok(error_result(&format!("Coloquio search error: {e}"))),
                    };
                    if msgs.is_empty() {
                        return Ok(CallToolResult {
                            content: vec![Content::text(format!("No matches for '{keyword}' in #{cid}"))],
                            is_error: Some(false),
                        });
                    }
                    let text = msgs.iter().map(|m| {
                        format!("[T{}] @{}: {}", m.turn, m.author_id, m.content)
                    }).collect::<Vec<_>>().join("\n\n");
                    return Ok(CallToolResult {
                        content: vec![Content::text(format!("## #{cid} — Search: '{keyword}'\n\n{text}"))],
                        is_error: Some(false),
                    });
                }
            }
            // Tail-Offset Querying:
            let offset = match offset_arg {
                Some(off) => off,
                None => {
                    let last_turn = coloquio.get_last_turn(cid).await.unwrap_or(0);
                    if last_turn > limit as i64 {
                        last_turn - limit as i64
                    } else {
                        0
                    }
                }
            };
            let msgs = match coloquio.get_thread(cid, limit as i64, offset).await {
                Ok(msgs) => msgs,
                Err(e) => return Ok(error_result(&format!("Coloquio read error: {e}"))),
            };
            if msgs.is_empty() {
                return Ok(CallToolResult {
                    content: vec![Content::text(format!("#{cid} is empty."))],
                    is_error: Some(false),
                });
            }
            let text = msgs.iter().map(|m| {
                format!("[T{}] **@{}**: {}", m.turn, m.author_id, m.content)
            }).collect::<Vec<_>>().join("\n\n");
            return Ok(CallToolResult {
                content: vec![Content::text(format!("## #{cid} — Messages (Limit: {limit}, Offset: {offset})\n\n{text}"))],
                is_error: Some(false),
            });
        }
    }

    if query.starts_with("@pending") || query.starts_with("@completed") || query == "@context" {
        let target_agent = if query.starts_with("@pending:") {
            Some(query.strip_prefix("@pending:").unwrap_or(""))
        } else {
            None
        };

        if query == "@context" {
            let conn_guard = server.silva.conn_lock();
            if let Ok(conn) = conn_guard.try_lock() {
                use rusqlite::params;
                let two_hours_ago = chrono::Utc::now() - chrono::Duration::hours(2);
                let mut stmt = conn.prepare(
                    "SELECT agent_id, content, created_at FROM nodes WHERE created_at > ?1 AND agent_id IS NOT NULL ORDER BY created_at DESC"
                ).ok();
                let mut context_lines: Vec<String> = vec![];
if let Some(ref mut s) = stmt {
                    let rows = s.query_map(params![two_hours_ago.to_rfc3339()], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?))
                    });
                    if let Ok(rows) = rows {
                        let mut by_agent: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
                        for r in rows.flatten() {
                            let (aid, content, created) = r;
                            by_agent.entry(aid).or_insert_with(|| (content.chars().take(50).collect(), created));
                        }
                        for (aid, (content_snippet, _)) in by_agent {
                            context_lines.push(format!("- {aid}: {content_snippet}"));
                        }
                    }
                }
                let context_text = if context_lines.is_empty() {
                    "No activity in the last 2 hours.".to_string()
                } else {
                    format!("Actividad de agentes (ultimas 2h):\n{}", context_lines.join("\n"))
                };
                return Ok(CallToolResult { content: vec![Content::text(context_text)], is_error: Some(false) });
            }
        }

        let conn_guard = server.silva.conn_lock();
        if let Ok(conn) = conn_guard.try_lock() {
            let now = chrono::Utc::now();
            let yesterday = now - chrono::Duration::hours(24);

            let (status_filter, date_filter) = if query.starts_with("@pending") {
                ("pending", None)
            } else {
                ("completed", Some(yesterday.to_rfc3339()))
            };

            // M31 audit fix: assignee (and, previously, the date filter) were
            // interpolated directly into the SQL string via format!() -- an
            // agent_id containing '%', '_', or a quote could alter the LIKE
            // pattern or break out of the string literal entirely. status_filter
            // is safe to inline (one of two hardcoded literals, never user input);
            // assignee and dt are now bound parameters instead.
            enum RecallParams { Assignee(String), Date(String), None }
            let (sql, bind): (String, RecallParams) = if let Some(assignee) = target_agent {
                (
                    format!(
                        "SELECT id, content, metadata FROM nodes WHERE type = 'task' AND metadata LIKE '%\"status\":\"{status_filter}\"%' AND metadata LIKE ('%\"assigned_to\":\"' || ?1 || '\"%') ORDER BY CAST(json_extract(metadata, '$.priority') AS INTEGER) DESC, created_at ASC"
                    ),
                    RecallParams::Assignee(assignee.to_string()),
                )
            } else if let Some(dt) = date_filter {
                (
                    "SELECT id, content, metadata FROM nodes WHERE type = 'task' AND metadata LIKE '%\"status\":\"completed\"%' AND updated_at > ?1 ORDER BY updated_at DESC".to_string(),
                    RecallParams::Date(dt),
                )
            } else {
                (
                    "SELECT id, content, metadata FROM nodes WHERE type = 'task' AND metadata LIKE '%\"status\":\"pending\"%' ORDER BY CAST(json_extract(metadata, '$.priority') AS INTEGER) DESC, created_at ASC".to_string(),
                    RecallParams::None,
                )
            };

            let stmt = conn.prepare(&sql);
            let mut task_lines: Vec<String> = vec![];
            let mut count = 0;

            if let Ok(mut s) = stmt {
                let query_result = match &bind {
                    RecallParams::Assignee(a) => s.query(params![a]),
                    RecallParams::Date(d) => s.query(params![d]),
                    RecallParams::None => s.query([]),
                };
                let mut rows = match query_result {
                    Ok(r) => r,
                    Err(_) => return Ok(error_result("Query failed")),
                };
                while let Ok(Some(row)) = rows.next() {
                    count += 1;
                    let id: String = row.get(0).unwrap_or_default();
                    let content: String = row.get(1).unwrap_or_default();
                    let meta_str: String = row.get(2).unwrap_or("{}".to_string());

                    let meta: serde_json::Value = serde_json::from_str(&meta_str).unwrap_or_default();
                    let created_by = meta.get("created_by").and_then(|v| v.as_str()).unwrap_or("?");
                    let assigned_to = meta.get("assigned_to").and_then(|v| v.as_str()).unwrap_or("sin asignar");
                    let priority = meta.get("priority").and_then(|v| v.as_i64()).unwrap_or(5);

                    let _age = if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(id.split(':').nth(1).unwrap_or("0")) {
                        let dur = now.signed_duration_since(parsed.with_timezone(&chrono::Utc));
                        if dur.num_hours() > 0 { format!("{}h", dur.num_hours()) } else { format!("{}min", dur.num_minutes()) }
                    } else { "?".to_string() };

                    let priority_label = if priority <= 2 { "[ALTA]" } else if priority <= 4 { "[MEDIA]" } else { "[BAJA]" };
                    task_lines.push(format!("[{}] {} | asig: {} | de: {} | \"{}\"", count, priority_label, assigned_to, created_by, content.chars().take(80).collect::<String>()));
                }
            }

            let response = if query.starts_with("@pending") {
                if let Some(ref agent) = target_agent {
                    format!("TAREAS PENDIENTES PARA {} ({})\n\n{}", agent, count, task_lines.join("\n"))
                } else {
                    format!("TAREAS PENDIENTES ({})\n\n{}", count, task_lines.join("\n"))
                }
            } else {
                format!("TAREAS COMPLETADAS (24h) ({})\n\n{}", count, task_lines.join("\n"))
            };

            return Ok(CallToolResult { content: vec![Content::text(response)], is_error: Some(false) });
        }
    }

    let effective_query = match &rec_agent_id {
        Some(aid) => format!("agent: {aid} {query}"),
        None => query.clone(),
    };

    // Jaccard LRU cache: skip expensive embedding + search if similar query exists
    let mut cache = server.recall_cache.lock().await;
    let cached_docs = cache.get(&effective_query, include_archived).cloned();
    drop(cache);

    // Cascade mode: skip the eager dense embed (2-8s CPU on cache miss) —
    // search_recall_cascade computes it only when lexical signals don't agree
    // enough to answer alone. Legacy path keeps the eager embed.
    let cascade = server.recall_cascade_enabled && mode != "dual";
    let mut query_embedding = if cascade {
        None
    } else {
        server.matcher.engine().and_then(|e| {
            tokio::task::block_in_place(|| {
                server.silva.query_embed_cache
                    .get_or_embed(&effective_query, |q| e.embed(q))
            })
            .ok()
        })
    };

    if let Some(cached) = cached_docs {
        let mut scored: Vec<(GraphNode, f32)> = cached;
        let aid = rec_agent_id.as_deref().unwrap_or("anonymous");

        // ADR-011 Coherence Gate: the cache stores raw (pre-gate) candidates,
        // so a cache hit must still be gated — otherwise a poisoned node
        // that made it into the cache once would bypass the gate forever.
        let (gated, gate_stats) = crate::security::coherence_gate::CoherenceGate::filter(
            scored, &server.silva, query_embedding.as_deref(),
        ).await;
        scored = gated;
        let gate_warning = gate_stats.should_warn();

        // Layer 4 hybrid: fire-and-forget 3-way classification on ALL survivors.
        // Calls the LLM only for those that trigger any ambiguity zone (A/B/C/D).
        // Supersedes the older observe_layer4()/reason_about_flagged() path (full
        // v3/v4 reasoning prompt, 15-20s latency) that used to run here too on the
        // same penalized-nodes subset — removed to stop double-calling the LLM on
        // every recall (found during the 2026-07-30 connection audit).
        //
        // Opt-in gate ([security] coherence_gate_hybrid_enabled, default false,
        // 2026-08-28): this fires on every tylluan_recall, any agent, any time
        // of day — when its trigger zone activates it calls llama_backend,
        // which auto-starts a real llama-server subprocess with zero user
        // opt-in. Contributed to a 4-day Unsloth training run being killed.
        if server.coherence_gate_hybrid_enabled {
            crate::security::coherence_gate::CoherenceGate::hybrid_classify(
                &effective_query,
                &scored,
                server.silva.clone(),
                query_embedding.map(|e| e.to_vec()),
                &gate_stats.penalized_nodes.iter().map(|(n, _)| n.id.clone()).collect::<Vec<_>>(),
            );
        }

        if !include_archived {
            let archived_ids = server.silva.archived_lifecycle_ids_among(
                &scored.iter().map(|(node, _)| node.id.clone()).collect::<Vec<_>>(),
            ).await.unwrap_or_default();
            scored.retain(|(node, _)| !archived_ids.contains(&node.id));
        }

        for (node, _) in &scored {
            let _ = server.silva.reinforce_node(&node.id, 1.02).await;
            let _ = server.silva.touch_node(&node.id, aid, "recall").await;
        }

        let total_found = scored.len();

        // Filter out decayed nodes for display (weight < 0.15), keeping at least one if all are decayed.
        // Reranker candidates use a lower threshold (0.05) so weak-but-relevant nodes (e.g. Jina episode
        // at w=0.12) survive to be scored by Jina before being cut.
        let min_weight = 0.05;
        if scored.iter().any(|(d, _)| d.weight >= min_weight) {
            scored.retain(|(d, _)| d.weight >= min_weight);
        }

        // M20-D: Filter out machinery nodes that pollute recall results
        scored.retain(|(d, _)| d.node_type != "routing_anchor" && d.node_type != "session_digest");

        apply_memory_isolation(&mut scored, &rec_agent_id).await;

        scored.truncate(limit);
        let showing = scored.len();

        if let Some(ref aid_val) = rec_agent_id {
            let task_hash: String = format!("{effective_query}|{aid_val}").bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
                .to_string();
            for (rank, (node, _)) in scored.iter().enumerate() {
                let _ = server.silva.log_recall_feedback(
                    &node.id, aid_val, &task_hash, &effective_query, rank as i64,
                ).await;
            }
        }

        let header = if gate_warning {
            format!(
                "### Recall Results (Found {total_found}, Showing top {showing})\n\n⚠️ {} resultados filtrados por control de coherencia\n\n",
                gate_stats.eliminated + gate_stats.penalized
            )
        } else {
            format!("### Recall Results (Found {total_found}, Showing top {showing})\n\n")
        };
        // M40-P4: confidence and source/author/evidence are fetched on demand
        // (not stored on GraphNode, see SilvaDB::get_confidence's doc comment),
        // so this loop can't be a plain sync .map() over scored.
        let mut statuses = Vec::with_capacity(scored.len());
        let mut source_infos: Vec<(Option<String>, Option<String>, Option<String>)> = Vec::with_capacity(scored.len());
        for (d, _) in &scored {
            let confidence = server.silva.get_confidence(&d.id).await;
            statuses.push(crate::memory::silva::memory_status(d.conflicted, d.valid_until, confidence));
            source_infos.push(server.silva.get_source_info(&d.id).await);
        }
        let summary_body = scored.iter().zip(statuses.iter()).zip(source_infos.iter())
            .map(|(((d, score), status), (source, author, evidence_url))| {
                let age = d.created_at.as_deref().unwrap_or("?");
                let truncated_content = truncate_adaptive(&d.content, *score, compact);
                let mut tag = format!("- [score={:.2} weight={:.2} type={} age={} status={}", score, d.weight, d.node_type, age, status);
                if let Some(s) = source.as_deref() {
                    tag.push_str(&format!(" source={s}"));
                }
                if let Some(a) = author.as_deref() {
                    tag.push_str(&format!(" author={a}"));
                }
                tag.push_str("] ");
                let mut line = format!("{tag}{truncated_content}");
                if let Some(url) = evidence_url.as_deref() {
                    line.push_str(&format!("\n  \u{21b3} evidence: {url}"));
                }
                line
            }).collect::<Vec<_>>().join("\n");
        let summary = format!("{header}{summary_body}\n\n---\n🔍 Cache hit");
        if rec_agent_id.is_some() && !scored.is_empty() {
            let now = chrono::Utc::now().timestamp();
            for (node, _) in &scored {
                let _ = server.silva.record_agent_access(&node.id, now).await;
            }
        }

        let prefix = session_context.unwrap_or_default();
        return Ok(CallToolResult { content: vec![Content::text(format!("{prefix}{summary}"))], is_error: Some(false) });
    }

    // M6: Dual-level retrieval (LightRAG pattern) — opt-in via mode="dual"
    let mut candidates = if mode == "dual" {
        match crate::memory::dual_retrieval::dual_retrieve(
            &server.silva, &effective_query, query_embedding.as_deref(), limit * 3,
        ).await {
            Ok(mut r) => {
                if !include_archived {
                    let archived_ids = server.silva.archived_lifecycle_ids_among(
                        &r.merged.iter().map(|(node, _)| node.id.clone()).collect::<Vec<_>>(),
                    ).await.unwrap_or_default();
                    r.merged.retain(|(node, _)| !archived_ids.contains(&node.id));
                }
                r.merged
            },
            Err(_) => vec![],
        }
    } else {
        vec![]
    };

    if candidates.is_empty() {
        // Stage 1: gather broad candidate pool from SilvaDB + HybridMemory (always)
        let candidate_pool = (limit * CANDIDATE_POOL_MULT.load(Ordering::Relaxed)).max(100);
        let filter = if episodic { Some("episodic") } else { None };
        if cascade {
            // Two-stage cascade: lexical-first with agreement gate; stage 2
            // embeds internally and hands the embedding back for the gate.
            if let Ok((cands, emb)) = server.silva.search_recall_cascade(
                &effective_query,
                candidate_pool,
                filter,
                include_archived,
            ).await {
                candidates = cands;
                query_embedding = emb;
            }
        }
        if candidates.is_empty() {
            candidates = server.silva.search_hybrid_for_recall(
                    &effective_query,
                    query_embedding.as_deref(),
                    candidate_pool,
                    filter,
                    false,
                    include_archived,
                )
                .await
                .unwrap_or_default();
        }

        if let Ok(hybrid) = server.memory.search(&effective_query, query_embedding.as_deref(), limit.max(10)).await {
            for doc in hybrid {
                let is_dup = candidates.iter().any(|(n, _)| jaccard_similarity(&n.content, &doc.content) > 0.85);
                if !is_dup {
                    candidates.push((GraphNode {
                        id: format!("hybrid:{}", doc.id),
                        node_type: "memory_document".into(),
                        content: doc.content,
                        metadata: doc.metadata,
                        weight: 1.0,
                        protected: false,
                        conflicted: false,
                        topic_key: None,
                        created_at: None,
                        updated_at: None,
                        last_touched: chrono::Utc::now(),
                        valid_from: None,
                        valid_until: None,
                        shareable: false,
                        content_hash: "".to_string(),
                        provenance: "".to_string(),
                    }, doc.score));
                }
            }
            candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        }
    }

    // Stage 2: Jina cross-encoder rerank on top-50 candidates (if available)
    // This corrects the RRF score ordering with a true relevance signal.
    let docs: Result<Vec<(GraphNode, f32)>, anyhow::Error> = if let Some(ref reranker) = server.reranker {
        let rerank_pool = candidates.iter().take(RERANK_WINDOW.load(Ordering::Relaxed)).collect::<Vec<_>>();
        let texts: Vec<&str> = rerank_pool.iter().map(|(n, _)| n.content.as_str()).collect();
        let ranked = reranker.rerank(&effective_query, &texts).unwrap_or_else(|_| {
            (0..texts.len()).map(|i| (i, 0.0f32)).collect()
        });
        let reranked: Vec<(GraphNode, f32)> = ranked.into_iter()
            .filter_map(|(idx, logit)| {
                // Normalize cross-encoder logit to (0,1) with sigmoid before mixing with RRF scores
                let norm = 1.0f32 / (1.0 + (-logit).exp());
                rerank_pool.get(idx).map(|(n, _)| ((*n).clone(), norm))
            })
            .collect();
        tracing::info!("🔀 Jina reranker: {} candidates → {} reranked", rerank_pool.len(), reranked.len());
        Ok(reranked)
    } else {
        Ok(candidates)
    };

    match docs {
        Ok(docs) if docs.is_empty() => Ok(CallToolResult {
            content: vec![Content::text("No memories found for that query.")],
            is_error: Some(false),
        }),
        Ok(docs) => {
            // Cache the full results before filtering
            server.recall_cache.lock().await.put(effective_query.clone(), include_archived, docs.clone());
            let mut scored: Vec<(GraphNode, f32)> = docs;
            let aid = rec_agent_id.as_deref().unwrap_or("anonymous");

            // ADR-011 Coherence Gate: eliminate/penalize before anything downstream
            // (stigmergy reinforcement, hot context, eventual generative consumption)
            // trusts these nodes further.
            let (gated, gate_stats) = crate::security::coherence_gate::CoherenceGate::filter(
                scored, &server.silva, query_embedding.as_deref(),
            ).await;
            scored = gated;
            let gate_warning = gate_stats.should_warn();

            // Layer 4 hybrid: fire-and-forget 3-way classification on ALL survivors.
            // Supersedes the older observe_layer4() call that used to run here too
            // on the same penalized-nodes subset — removed to stop double-calling
            // the LLM on every recall (2026-07-30 connection audit).
            //
            // Opt-in gate ([security] coherence_gate_hybrid_enabled, default
            // false, 2026-08-28) — see the sibling call site above for why.
            if server.coherence_gate_hybrid_enabled {
                crate::security::coherence_gate::CoherenceGate::hybrid_classify(
                    &effective_query,
                    &scored,
                    server.silva.clone(),
                    query_embedding.map(|e| e.to_vec()),
                    &gate_stats.penalized_nodes.iter().map(|(n, _)| n.id.clone()).collect::<Vec<_>>(),
                );
            }

            if gate_stats.eliminated > 0 || gate_stats.penalized > 0 {
                tracing::info!(
                    "🛡️ Coherence Gate: {} eliminated, {} penalized of {} candidates",
                    gate_stats.eliminated, gate_stats.penalized, gate_stats.total
                );
            }

            // STIGMERGY: Reinforce recalled nodes
            for (node, _) in &scored {
                let _ = server.silva.reinforce_node(&node.id, 1.02).await;
                let _ = server.silva.touch_node(&node.id, aid, "recall").await;
            }
            tracing::info!("🧠 Rejuvenated {} nodes via recall (agent={})", scored.len(), aid);

            let total_found = scored.len();

            // Filter out decayed nodes for display (weight < 0.15). Reranker threshold is 0.05
            // so weak-but-relevant nodes survive to be scored before display cutoff is applied.
            let min_weight = 0.05;
            if scored.iter().any(|(d, _)| d.weight >= min_weight) {
                scored.retain(|(d, _)| d.weight >= min_weight);
            }

            // M20-D: Filter out machinery nodes that pollute recall results
            scored.retain(|(d, _)| d.node_type != "routing_anchor" && d.node_type != "session_digest");

            apply_memory_isolation(&mut scored, &rec_agent_id).await;

            if let Some(aid_val) = rec_agent_id.as_ref() {
                let aid_pattern = format!("\"agent_id\":\"{aid_val}\"");
                let query_mentions_agent = query.to_lowercase().contains(&aid_val.to_lowercase());
                for (node, score) in &mut scored {
                    // Only boost agent_memory nodes when the query explicitly mentions the agent.
                    // Boosting all nodes with a matching agent_id caused mega-nodes (sprint summaries
                    // that mention every keyword) to crowd out specialized nodes.
                    if node.node_type == "agent_memory" && query_mentions_agent
                        && (node.metadata.contains(&aid_pattern) || node.content.contains(&format!("agent: {aid}")))
                    {
                        *score += 0.25;
                    }
                }
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }

            // HOT CONTEXT: Boost recently recalled nodes (recency ×2.0)
            {
                let mut hc = server.hot_context.lock().await;
                let mut boosted_any = false;
                for (node, score) in &mut scored {
                    let boost = hc.boost_for(&node.id);
                    if boost > 1.0 {
                        *score = (*score as f64 * boost) as f32;
                        boosted_any = true;
                    }
                }
                if boosted_any {
                    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    tracing::info!("🔥 Hot Context boost applied ({} hot nodes)", hc.snapshot().len());
                }
                // Insert top-3 results into hot context for next query
                for (node, _) in scored.iter().take(3) {
                    hc.touch(node.id.clone());
                }
            }

            // ADR-011 LightReranker: reorder by learned score when model available.
            // Additive, opt-in — if no trained model exists, scored is unchanged.
            if let Some(ref reranker) = server.light_reranker
                && reranker.is_active()
            {
                    // Pre-compute agent affinity for each candidate.
                    // Only queries recall_feedback when reranker is active (model exists).
                    let mut affinity_map: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
                    let agent_ref = rec_agent_id.as_deref().unwrap_or("");
                    for (node, _) in &scored {
                        if !affinity_map.contains_key(&node.id) {
                            let affinity = server.silva.agent_affinity_for_memory(&node.id, agent_ref).await.unwrap_or(0.0);
                            affinity_map.insert(node.id.clone(), affinity);
                        }
                    }
                    let candidates: Vec<(crate::router::light_reranker::RerankFeatures, usize)> = scored.iter()
                        .enumerate()
                        .map(|(idx, (node, score))| {
                            let days = chrono::Utc::now()
                                .signed_duration_since(
                                    node.created_at
                                        .as_deref()
                                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                        .map(|dt| dt.naive_utc().and_utc())
                                        .unwrap_or_else(chrono::Utc::now),
                                )
                                .num_days()
                                .max(0) as f32;
                            (crate::router::light_reranker::RerankFeatures {
                                score_rrf: *score,
                                score_graph: *score,
                                recency_score: 1.0 / (1.0 + days),
                                agent_affinity: *affinity_map.get(&node.id).unwrap_or(&0.0),
                            }, idx)
                        })
                        .collect();
                    let order = reranker.rerank(candidates);
                    if order.len() == scored.len() {
                        scored = order.into_iter().filter_map(|i| {
                            if i < scored.len() { Some(scored[i].clone()) } else { None }
                        }).collect();
                        tracing::info!("🎯 LightReranker reordered {} candidates", scored.len());
                    }
            }

            // Always truncate to the requested limit at the very end
            scored.truncate(limit);
            let showing = scored.len();

            // ADR-011 Signal Loop: log which memories were actually shown to this
            // agent, so NightConsolidation's FeedbackSignalPhase can later resolve
            // whether they were referenced. Fire-and-forget, never blocks recall.
            if let Some(ref aid_val) = rec_agent_id {
                let task_hash: String = format!("{effective_query}|{aid_val}").bytes()
                    .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
                    .to_string();
                for (rank, (node, _)) in scored.iter().enumerate() {
                    let _ = server.silva.log_recall_feedback(
                        &node.id, aid_val, &task_hash, &effective_query, rank as i64,
                    ).await;
                }
            }

            let header = if gate_warning {
                format!(
                    "### Recall Results (Found {total_found}, Showing top {showing})\n\n⚠️ {} resultados filtrados por control de coherencia\n\n",
                    gate_stats.eliminated + gate_stats.penalized
                )
            } else {
                format!("### Recall Results (Found {total_found}, Showing top {showing})\n\n")
            };

            // WER — Weight Exposure in Recall (R21-4)
            // Expose score, weight, node_type and created_at so agents can audit
            // ALD decay, TMS contradictions, and retrieval quality directly.
            // M40-P4: status and source/author/evidence are fetched on demand
            // (see the cache-hit path above for why it can't be a plain sync .map()).
            let mut statuses = Vec::with_capacity(scored.len());
            let mut source_infos: Vec<(Option<String>, Option<String>, Option<String>)> = Vec::with_capacity(scored.len());
            for (d, _) in &scored {
                let confidence = server.silva.get_confidence(&d.id).await;
                statuses.push(crate::memory::silva::memory_status(d.conflicted, d.valid_until, confidence));
                source_infos.push(server.silva.get_source_info(&d.id).await);
            }
            let summary_body = scored.iter().zip(statuses.iter()).zip(source_infos.iter())
                .map(|(((d, score), status), (source, author, evidence_url))| {
                    let age = d.created_at.as_deref().unwrap_or("?");
                    let truncated_content = truncate_adaptive(&d.content, *score, compact);
                    let mut tag = format!("- [score={:.2} weight={:.2} type={} age={} status={}", score, d.weight, d.node_type, age, status);
                    if let Some(s) = source.as_deref() {
                        tag.push_str(&format!(" source={s}"));
                    }
                    if let Some(a) = author.as_deref() {
                        tag.push_str(&format!(" author={a}"));
                    }
                    tag.push_str("] ");
                    let mut line = format!("{tag}{truncated_content}");
                    if let Some(url) = evidence_url.as_deref() {
                        line.push_str(&format!("\n  \u{21b3} evidence: {url}"));
                    }
                    line
                })
                .collect::<Vec<_>>().join("\n");
            let mut summary = format!("{header}{summary_body}");

            if mode == "collective" {
                let receiver = rec_agent_id.as_deref().unwrap_or("collective");
                if rec_agent_id.is_some() {
                    let broadcasts = server.mailbox
                        .get_recent_broadcasts(receiver, "broadcast", 2).await
                        .unwrap_or_default();
                    if !broadcasts.is_empty() {
                        let shared: String = broadcasts.iter()
                            .filter_map(|m| {
                                serde_json::from_str::<serde_json::Value>(&m.payload).ok()
                                    .and_then(|v| v.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()))
                                    .map(|c| format!("- [colectivo, de {}] {}", m.sender_id, c))
                            })
                            .collect::<Vec<_>>().join("\n");
                        if !shared.is_empty() {
                            summary.push_str("\n\n📡 Conocimiento compartido por el colectivo:\n");
                            summary.push_str(&shared);
                        }
                    }
                }
            }

            if mode == "context"
                && !scored.is_empty() {
                    let neighbors = server.silva.get_context(&scored[0].0.id, 1).await.unwrap_or_default();
                    let root_id = &scored[0].0.id;
                    let related: String = neighbors.iter()
                        .filter(|n| n.id != *root_id)
                        .take(4)
                        .map(|n| format!("- {}: {}", n.id, n.content.chars().take(80).collect::<String>()))
                        .collect::<Vec<_>>().join("\n");
                    if !related.is_empty() {
                        summary.push_str("\n\n### Conceptos relacionados\n");
                        summary.push_str(&related);
                    }
                }

            let using_embeddings = server.matcher.engine().is_some();
            let footer = if using_embeddings {
                "\n\n---\n🔍 Búsqueda: semántica (BGE-M3)"
            } else {
                "\n\n---\n⚠️ Búsqueda: solo texto (embeddings no cargados)"
            };
            let prefix = session_context.unwrap_or_default();
            let full_summary = format!("{prefix}{summary}{footer}");

            // ADR-012 Fase 1: actualizar last_agent_access para los nodos devueltos
            // (solo si hay agent_id y nodos devueltos)
            if rec_agent_id.is_some() && !scored.is_empty() {
                let now = chrono::Utc::now().timestamp();
                for (node, _) in &scored {
                    let _ = server.silva.record_agent_access(&node.id, now).await;
                }
            }

            Ok(CallToolResult {
                content: vec![Content::text(full_summary)],
                is_error: Some(false),
            })
        }
        Err(e) => Ok(error_result(&format!("Memory search failed: {e}"))),
    }
}
fn truncate_adaptive(content: &str, score: f32, compact: bool) -> String {
    if !compact {
        return content.to_string();
    }
    let char_limit = if score >= 0.85 {
        1000
    } else if score < 0.50 {
        250
    } else {
        500
    };

    if content.chars().count() > char_limit {
        format!("{}...[+{}c]", content.chars().take(char_limit).collect::<String>(), content.chars().count() - char_limit)
    } else {
        content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jaccard_similarity_basic() {
        assert!((jaccard_similarity("", "") - 0.0).abs() < 1e-9);
        assert!((jaccard_similarity("hello world", "hello world") - 1.0).abs() < 1e-9);
        assert!((jaccard_similarity("hello world", "world hello") - 1.0).abs() < 1e-9);
        assert!((jaccard_similarity("hello world", "hello there") - 1.0 / 3.0).abs() < 1e-9);
        assert!((jaccard_similarity("abc def", "ghi jkl") - 0.0).abs() < 1e-9);
        assert!((jaccard_similarity("hello world foo", "hello world bar") - 2.0 / 4.0).abs() < 1e-9);
    }

    #[test]
    fn test_truncate_adaptive() {
        let long_text = "a".repeat(1200);
        
        // High score (>= 0.85) -> 1000 char limit
        let res_high = truncate_adaptive(&long_text, 0.90, true);
        assert_eq!(res_high.chars().take(1000).count(), 1000);
        assert!(res_high.contains("[+200c]"));

        // Medium score -> 500 char limit
        let res_med = truncate_adaptive(&long_text, 0.70, true);
        assert_eq!(res_med.chars().take(500).count(), 500);
        assert!(res_med.contains("[+700c]"));

        // Low score (< 0.50) -> 250 char limit
        let res_low = truncate_adaptive(&long_text, 0.40, true);
        assert_eq!(res_low.chars().take(250).count(), 250);
        assert!(res_low.contains("[+950c]"));

        // Compact false -> no truncation
        let res_no_compact = truncate_adaptive(&long_text, 0.40, false);
        assert_eq!(res_no_compact.len(), 1200);
    }

    async fn test_server() -> TylluanServer {
        use crate::registry::guild_process::GuildRegistry;
        use crate::router::catalog::builtin_catalog;
        use crate::router::matcher::GuildMatcher;
        use crate::memory::hybrid::HybridMemory;
        use crate::memory::silva::SilvaDB;
        use crate::memory::mailbox::Mailbox;
        use crate::memory::agent_nodes::AgentNodeRouter;
        use std::path::PathBuf;
        use std::sync::Arc;
        use tokio::sync::RwLock;
        use tokio::sync::broadcast;

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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_handler_recall_episodic_filter() {
        let server = test_server().await;
        
        // 1. Insertar dos tipos de nodos en SilvaDB
        server.silva.upsert_node("n1", "episodic", "Mensaje de coloquio episódico de testeo", "{}").await.unwrap();
        server.silva.upsert_node("n2", "lesson", "Lección de testeo general", "{}").await.unwrap();

        // 2. Invocar recall con episodic = true
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("episódico".to_string()));
        args.insert("episodic".to_string(), serde_json::Value::Bool(true));
        
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        let text = result.content[0].as_text().unwrap();
        
        // Debe encontrar el episódico pero no el de lección
        assert!(text.text.contains("episódico"), "Debe retornar el nodo episódico: {text:?}");
        assert!(!text.text.contains("general"), "No debe retornar el nodo de lección: {text:?}");
        
        // 3. Invocar recall sin episodic (por defecto busca todo)
        let mut args_all = serde_json::Map::new();
        args_all.insert("query".to_string(), serde_json::Value::String("testeo".to_string()));
        
        let result_all = handle_tylluan_recall(&server, Some(args_all)).await.unwrap();
        let text_all = result_all.content[0].as_text().unwrap();
        assert!(text_all.text.contains("episódico") && text_all.text.contains("general"), "Debe contener ambos: {text_all:?}");
    }

    // ── Edge case tests ───────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_empty_query_returns_error() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        assert!(result.is_error.unwrap_or(false), "empty query must return error");
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("requires a non-empty"), "error must mention empty query");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_no_results_returns_empty_message() {
        let server = test_server().await;
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("xyznonexistent_99".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        assert!(!result.is_error.unwrap_or(false), "no results is not an error");
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("No memories found"), "must say no memories found");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_output_exposes_memory_status() {
        // M40-P4: an agent asking "why do you believe this" should see the status
        // in the recall output itself, not have to guess or call a separate tool.
        let server = test_server().await;
        server.silva.upsert_node("s1", "concept", "confirmed status memory status test node", "{}").await.unwrap();
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("confirmed status memory status test".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("status=confirmed"), "a freshly written, unconflicted node must show status=confirmed: {text:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_output_exposes_source_author_evidence() {
        // M40-P4 (second cut): source/author/evidence_url should appear in recall
        // output when set, and be absent when not set.
        let server = test_server().await;
        use crate::memory::silva::nodes::NodeWriteOptions;
        server.silva.upsert_node_with_validity(
            "src1", "concept", "source author evidence test node for recall",
            "{}",
            NodeWriteOptions::new("agent_generated")
                .source(Some("user_conversation"))
                .author(Some("agent:deep"))
                .evidence_url(Some("https://example.com/evidence/1")),
        ).await.unwrap();
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("source author evidence test".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("source=user_conversation"), "recall output must include source: {text:?}");
        assert!(text.text.contains("author=agent:deep"), "recall output must include author: {text:?}");
        assert!(text.text.contains("evidence: https://example.com/evidence/1"), "recall output must include evidence URL: {text:?}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_filters_routing_anchor_nodes() {
        let server = test_server().await;
        server.silva.upsert_node("r1", "routing_anchor", "routing anchor data content", "{}").await.unwrap();
        server.silva.upsert_node("n1", "concept", "normal concept node for routing anchor test", "{}").await.unwrap();

        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("routing anchor".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        let text = result.content[0].as_text().unwrap();
        // routing_anchor must be filtered out; result should not error
        assert!(!text.text.contains("routing_anchor"), "routing_anchor must be filtered from recall");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_filters_session_digest_nodes() {
        let server = test_server().await;
        server.silva.upsert_node("d1", "session_digest", "session digest content for test filtering", "{}").await.unwrap();
        server.silva.upsert_node("n1", "concept", "normal concept node for digest test", "{}").await.unwrap();

        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("session digest".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        let text = result.content[0].as_text().unwrap();
        assert!(!text.text.contains("session_digest"), "session_digest must be filtered from recall");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_honors_limit_parameter() {
        let server = test_server().await;
        for i in 0..5 {
            let id = format!("limit_n{i}");
            server.silva.upsert_node(&id, "concept", &format!("limit test node {i}"), "{}").await.unwrap();
        }
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("limit test".to_string()));
        args.insert("limit".to_string(), serde_json::Value::Number(serde_json::Number::from(2u32)));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        assert!(!result.is_error.unwrap_or(false));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("Showing top 2"), "limit=2 must show top 2");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn recall_with_agent_id_does_not_error() {
        let server = test_server().await;
        server.silva.upsert_node("a1", "concept", "agent scoped data recall test", "{}").await.unwrap();
        let mut args = serde_json::Map::new();
        args.insert("query".to_string(), serde_json::Value::String("recall".to_string()));
        args.insert("agent_id".to_string(), serde_json::Value::String("test-agent-1".to_string()));
        let result = handle_tylluan_recall(&server, Some(args)).await.unwrap();
        assert!(!result.is_error.unwrap_or(false), "agent_id must not cause errors");
    }
}
