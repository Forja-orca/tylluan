//! API v1 route table (CONTRACT-05). Split from api_v1.rs: route registration
//! lives here, handlers stay in their feature modules (mod.rs and api_*).
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, delete, any},
};
use std::sync::Arc;

use crate::transport::http::HttpState;

use super::*;

// @CONTRACT: HTTP-API-V1 (CONTRACT-05)
// No eliminar rutas existentes ??? el dashboard depende de sus firmas
// Add new routes at the end of the block, never reorder
// See CONTRACTS.md section CONTRACT-05 for stable route list
pub fn api_v1_routes() -> Router<Arc<HttpState>> {
    Router::new()
        .route("/api/v1/do", post(do_intent_handler))
        .route("/api/v1/guilds", get(guilds_list_handler))
        .route("/api/v1/guilds/health", get(guilds_health_handler))
        .route("/api/v1/guilds/register", post(guild_register_handler))
        .route("/api/v1/guilds/request", post(guild_request_handler))
        .route("/api/v1/guilds/dispatch/execute", post(guild_dispatch_execute_handler))
        .route("/api/v1/guilds/dispatch/remote", post(guild_dispatch_remote_handler))
        .route("/api/v1/guilds/peers", get(guild_peers_handler))
        .route("/api/v1/guilds/{name}/start", post(guild_start_handler))
        .route("/api/v1/guilds/{name}/stop", post(guild_stop_handler))
        .route("/api/v1/guilds/{name}/reset-backoff", post(guild_reset_backoff_handler))
        .route("/api/v1/guilds/{guild_name}/tools/{tool_name}", post(guild_tool_call_handler))
        .route("/api/v1/doctor", get(doctor_diagnose_handler))
        .route("/api/v1/doctor/repair", post(doctor_repair_handler))
        .route("/api/v1/silva/stats", get(silva_stats_handler))
        .route("/api/v1/silva/recent", get(silva_recent_handler))
        .route("/api/v1/silva/edge", post(silva_add_edge_handler))
        .route("/api/v1/silva/node", post(silva_create_node_handler))
        .route("/api/v1/silva/graph", get(silva_graph_handler))
        .route("/api/v1/silva/export", get(knowledge_export_handler))
        .route("/api/v1/silva/save-cluster-summary", post(silva_save_summary_handler))
        .route("/api/v1/silva/analyze", any(silva_analyze_handler))
        .route("/api/v1/silva/communities", post(silva_communities_handler))
        .route("/api/v1/silva/shared/{agent_a}/{agent_b}", get(silva_shared_knowledge_handler))
        .route("/api/v1/silva/consolidate", post(silva_consolidate_handler))
        .route("/api/v1/silva/graphrag-trigger", post(graphrag_trigger_handler))
        .route("/api/v1/silva/contradictions", get(list_contradictions_handler))
        .route("/api/v1/dream/status", get(dream_status_handler))
        .route("/api/v1/silva/nodes/{node_id}", delete(silva_delete_node_handler))
        .route("/api/v1/sessions", get(list_sessions_handler))
        .route("/api/v1/sessions/resume", get(sessions_resume_handler).post(sessions_resume_action_handler))
        .route("/api/v1/repo-map", get(repo_map_handler))
        .route("/api/v1/sessions/{session_id}", get(session_detail_handler).delete(revoke_session_handler))
        .route("/api/v1/system/sessions", get(list_sessions_handler))
        .route("/api/v1/mailbox", get(mailbox_list_handler))
        .route("/api/v1/interoception", get(interoception_handler))
        .route("/api/v1/graph/viz", get(silva_graph_handler))
        .route("/api/v1/graph/scope", get(graph_scope_handler))

        .route("/api/v1/ingest/upload", post(ingest_upload_handler))
        .route("/api/v1/ingest/files/{filename}", get(serve_ingested_file_handler))
        // 100MB body limit for multipart uploads
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .route("/api/v1/docker/ps", get(docker_containers_handler))
        .route("/api/v1/docker/containers", get(docker_containers_handler))
        .route("/api/v1/system/status", get(system_status_handler))
        .route("/api/v1/memory/reindex", post(reindex_handler))
        .route("/api/v1/memory/write", post(memory_write_handler))
        .route("/api/v1/memory/search", any(memory_search_handler))
        // M31-P1: Audit trail query by agent_id
        .route("/api/v1/audit", get(audit_log_handler))
        .route("/api/v1/audit/trail", get(audit_trail_handler))
        .route("/api/v1/tools", get(tools_list_handler))
        .route("/api/v1/capabilities", get(capabilities_handler))
        .route("/api/v1/audit/logs", get(audit_logs_handler))
        .route("/api/v1/audit/verify", get(api_audit::audit_verify))
        // ADR-011 §2.5: Coherence Gate + Signal Loop observability
        .route("/api/v1/security/coherence-gate/stats", get(api_security::coherence_gate_stats))
        .route("/api/v1/security/friction/stats", get(api_security::friction_stats_handler))
        .route("/api/v1/security/scopes", get(get_security_scopes_handler).post(save_security_scopes_handler))
        .route("/api/v1/memory/recall-feedback/stats", get(api_security::recall_feedback_stats))
        .route("/api/v1/config", get(get_config_handler).post(save_config_handler))
        .route("/api/v1/config/device", post(set_inference_device_handler))
        .route("/api/v1/config/device/status", get(device_status_handler))
        .route("/api/v1/config/inference-llama", post(set_inference_llama_config_handler))
        .route("/api/v1/config/sandbox-profile", post(set_sandbox_profile_handler))
        .route("/api/v1/config/sandbox-profile/guild", post(set_guild_sandbox_override_handler))
        .route("/api/v1/config/sandbox-profile/session", post(set_session_sandbox_override_handler))
        .route("/api/v1/config/sandbox-profile/guild/{guild}", delete(delete_guild_sandbox_override_handler))
        .route("/api/v1/config/sandbox-profile/session/{agent_id}", delete(delete_session_sandbox_override_handler))
        .route("/api/v1/models", get(models_handler))
        .route("/api/v1/setup-hint", get(setup_hint_handler))
        .route("/api/v1/setup-hint/apply", post(setup_hint_apply_handler))
        .route("/api/v1/skills", get(project_skills_list_handler).post(project_skills_save_handler))
        .route("/api/v1/jobs", get(background_jobs_list_handler))
        .route("/api/v1/bash", post(bash_execute_handler)) // DEPRECATED - usar tylluan_do

        .route("/api/v1/security/events", get(security_events_handler))
        .route("/api/v1/inference/providers", get(list_inference_providers_handler).post(add_inference_provider_handler))
        .route("/api/v1/inference/providers/{name}/test", post(test_inference_provider_handler))
        .route("/api/v1/external-providers", get(list_external_providers_handler))
        .route("/api/v1/external-providers/{name}/test", post(test_external_provider_handler))
        .route("/api/v1/mcp/external", get(list_mcp_servers_handler).post(add_mcp_server_handler))
        .route("/api/v1/mcp/external/discover", post(discover_mcp_servers_handler))
        .route("/api/v1/mcp/external/{name}", delete(remove_mcp_server_handler).put(update_mcp_server_handler))
        .route("/api/v1/system/signals", get(golden_signals_handler))

        .route("/api/v1/maintenance/status", get(maintenance_status_handler))
        .route("/api/v1/system/maintenance/status", get(maintenance_status_handler))
        .route("/api/v1/maintenance/export", post(maintenance_export_handler))
        .route("/api/v1/maintenance/vacuum", post(maintenance_vacuum_handler))
        .route("/api/v1/maintenance/checkpoint", post(maintenance_checkpoint_handler))
        .route("/api/v1/maintenance/decay", post(maintenance_decay_handler))
        .route("/api/v1/maintenance/purge", post(maintenance_purge_handler))
        .route("/api/v1/maintenance/onnx-clean", post(maintenance_onnx_clean_handler))
        .route("/api/v1/maintenance/logs-compact", post(maintenance_logs_compact_handler))
        .route("/api/v1/maintenance/purge-lessons", post(maintenance_purge_lessons_handler))
        .route("/api/v1/maintenance/clean-orphans", post(maintenance_clean_orphans_handler))
        .route("/api/v1/guilds/{name}/test", post(guild_test_handler))
        .route("/api/v1/test-connection", post(test_connection_handler))
        .route("/api/v1/config/wsl", post(update_wsl_config_handler))
        // Recovered endpoints (were lost in http.rs ??? http/ migration)
        .route("/api/v1/slo/summary", get(slo_summary_handler))
        .route("/api/v1/guilds/utilization", get(guilds_utilization_handler))
        .route("/api/v1/memory/retention", get(memory_retention_handler))
        .route("/api/v1/guild-start", post(guild_start_alt_handler))

        // Compatibility aliases
        .route("/api/v1/health/golden-signals", get(golden_signals_handler))

        .route("/api/v1/mailbox/send", post(mailbox_send_handler))
        .route("/api/v1/blackboard", get(blackboard_handler))
        .route("/api/v1/guilds/{guild_name}/call/{tool_name}", post(guild_tool_call_handler))
        .route("/memory/graph", get(silva_graph_handler))
        // Cognitive forest endpoints

        .route("/api/v1/hormones", get(hormones_handler))
        .route("/api/v1/agent-profiles", get(agent_profiles_handler))
        .route("/api/v1/collective/pulse", get(collective_pulse_handler))
        .route("/api/v1/collective/timeline", get(collective_timeline_handler))
        .route("/api/v1/collective/heatmap", get(collective_heatmap_handler))
        .route("/api/v1/collective/reputation", get(collective_reputation_handler))
        .route("/api/v1/metrics", get(metrics_handler))
        .route("/api/v1/agents", get(agents_list_handler))
        .route("/api/v1/collective/suggest", get(collective_suggest_handler))

        .route("/api/v1/agent-memories/{agent_id}", get(agent_memories_handler))
        .route("/api/v1/agent-memories/{agent_id}/summary", get(agent_memories_summary_handler))
        .route("/api/v1/silva/traces", get(silva_traces_handler))
        .route("/api/v1/agent-memories/{agent_id}", delete(agent_memories_delete_handler))
        .route("/api/v1/session-digest", post(session_digest_handler))
        .route("/api/v1/health/detailed", get(health_detailed_handler))
        .route("/api/v1/canary", get(canary_handler))
        .route("/api/v1/logs", get(logs_handler))
        .route("/api/v1/sandbox/sessions", get(sandbox_sessions_handler))
        .route("/api/v1/sandbox/files/{path}", get(sandbox_files_handler))

        // --- Blackboard Coordination Protocol ---
        .route("/api/v1/blackboard/plan", post(blackboard_plan_handler))
        .route("/api/v1/blackboard/tasks/{agent}", get(blackboard_agent_tasks_handler))
.route("/api/v1/blackboard/tasks/{msg_id}/done", post(blackboard_task_done_handler))
        .route("/api/v1/tylluan_graph", post(tylluan_graph_handler).get(tylluan_graph_get_handler))

        // Unified ingest pipeline (R15)
        .route("/api/v1/ingest", post(ingest_handler))

        // Metrics history ring buffer
        .route("/api/v1/metrics/history", get(metrics_history_handler))

        // Admin endpoints
        .route("/api/v1/admin/reload", post(admin_reload_handler))
        .route("/api/v1/admin/meta-prune", post(meta_prune_handler))

        // Federation (M3)
        .route("/api/v1/federation/peers", get(federation_list_peers).post(federation_add_peer))
        .route("/api/v1/federation/peers/{name}", delete(federation_remove_peer))
        .route("/api/v1/federation/peers/{name}/approve", post(federation_approve_peer))
        .route("/api/v1/federation/sync", post(federation_sync_push))
        .route("/api/v1/federation/sync/receive", post(federation_sync_receive))
        // M11-B: Pull sync + bidirectional
        .route("/api/v1/federation/sync/export", get(federation_sync_export))
        .route("/api/v1/federation/sync/pull", post(federation_sync_pull))
        .route("/api/v1/federation/sync/both", post(federation_sync_both))
        // M11-C: Provenance query
        .route("/api/v1/federation/identity", get(federation_identity))
        .route("/api/v1/federation/nodes", get(federation_nodes_query))
        // M12-C: NAT traversal
        .route("/api/v1/nat/external-address", get(nat_external_address))
        .route("/api/v1/federation/ping", get(federation_ping))
        .route("/api/v1/federation/sharing/disable", post(federation_sharing_disable))
        .route("/api/v1/federation/sharing/enable", post(federation_sharing_enable))
        .route("/api/v1/federation/sharing/status", get(federation_sharing_status))
        .route("/api/v1/silva/node/{id}/shareable", post(silva_set_shareable_handler))
        // Routing anchors (M3 anchor routing)
        .route("/api/v1/routing/anchors", get(routing_anchors_list).post(routing_anchors_seed))
        .route("/api/v1/routing/anchors/reembed", post(routing_anchors_reembed))
        .route("/api/v1/silva/edge/search", post(silva_edge_search_handler))
        // Coloquio ??? shared async group chat (M7)
        .route("/api/v1/coloquio/channels", get(coloquio_list_channels).post(coloquio_create_channel))
        .route("/api/v1/coloquio/channels/{id}", get(coloquio_get_thread).delete(coloquio_delete_channel))
        .route("/api/v1/coloquio/channels/{id}/messages", get(coloquio_get_thread))
        .route("/api/v1/coloquio/channels/{id}/post", post(coloquio_post_message))
        .route("/api/v1/coloquio/channels/{id}/search", get(coloquio_search))
        .route("/api/v1/coloquio/channels/{id}/turn/{turn}", get(coloquio_get_turn))
        .route("/api/v1/coloquio/unread", get(coloquio_unread))
        .route("/api/v1/coloquio/channels/{id}/new", get(coloquio_new_messages))
        .route("/api/v1/coloquio/channels/{id}/read", post(coloquio_mark_read))
        .route("/api/v1/coloquio/channels/{id}/typing", post(coloquio_post_typing))
        .route("/api/v1/coloquio/repair-msgids", post(coloquio_repair_msgids))
        .route("/api/v1/coloquio/documents", get(coloquio_list_docs).post(coloquio_create_doc))
        .route("/api/v1/coloquio/documents/{id}", get(coloquio_get_doc).put(coloquio_update_doc).delete(coloquio_delete_doc))
        .route("/api/v1/coloquio/documents/{id}/append", post(coloquio_append_doc))
        .route("/api/v1/coloquio/documents/{id}/versions", get(coloquio_list_doc_versions))
        .route("/api/v1/coloquio/documents/{id}/versions/{version}", get(coloquio_get_doc_version))
        .route("/api/v1/dashboard/summary", get(dashboard_summary_handler))
        .route("/api/v1/autoresearch/summary", get(autoresearch_summary_handler))
        .route("/api/v1/autoresearch/start", post(autoresearch_start_handler))
        .route("/api/v1/autoresearch/stop", post(autoresearch_stop_handler))
        .route("/api/v1/autoresearch/evaluate", post(autoresearch_evaluate_handler))
        .route("/api/v1/skill", get(skill_handler))
        .route("/api/v1/admin/shutdown", post(admin_shutdown_handler))
        .route("/api/v1/admin/emergency-kill", post(admin_emergency_kill_handler))
        .route("/api/v1/admin/kill-guild/{name}", post(admin_kill_guild_handler))
        .route("/api/v1/canvas/ws", get(canvas_ws_handler))
        .route("/api/v1/canvas/{channel}/nodes", post(canvas_create_node_handler))
        // M23-Fractal: tool discovery endpoint
        .route("/api/v1/tools/explore", get(tools_explore_handler))
        // Agent Node Router
        .route("/api/v1/nodes", get(nodes_list_handler))
        .route("/api/v1/nodes/{agent_id}/register", post(nodes_register_handler))
        .route("/api/v1/nodes/{agent_id}/send", post(nodes_send_handler))
        .route("/api/v1/nodes/broadcast", post(nodes_broadcast_handler))
        .route("/api/v1/nodes/{agent_id}/inbox", get(nodes_inbox_handler))
        .route("/api/v1/nodes/{agent_id}/program", get(nodes_get_program_handler).put(nodes_set_program_handler))
        .route("/api/v1/nodes/{agent_id}/unregister", post(nodes_unregister_handler))
        // Agent Journal — crash-safe checkin/recover
        .route("/api/v1/journal", get(journal_list))
        .route("/api/v1/journal/{agent_id}/checkin", post(journal_checkin))
        .route("/api/v1/journal/{agent_id}/recover", get(journal_recover))
        // M9 Autonomous Mode — Agent Registry
        .route("/api/v1/agents/session/start", post(agent_session_start_handler))
        .route("/api/v1/agents/session/stop", post(agent_session_stop_handler))
        .route("/api/v1/agents/session", get(agent_session_list_handler))
        .route("/api/v1/agents/stats", get(agent_stats_handler))
        .route("/api/v1/agents/heartbeat", post(agent_heartbeat_handler))
        // M10 Bounded Work Contracts
        .route("/api/v1/work-contracts", post(contract_create_handler))
        .route("/api/v1/work-contracts/active", get(contract_active_handler))
        .route("/api/v1/work-contracts/{id}", get(contract_get_handler))
        .route("/api/v1/work-contracts/{id}/tick", post(contract_tick_handler))
        .route("/api/v1/work-contracts/{id}/deliver", post(contract_deliver_handler))
        .route("/api/v1/work-contracts/{id}/vote", post(contract_vote_handler))
        .route("/api/v1/work-contracts/{id}/close", post(contract_close_handler))
        // M14-A: Mesh DHT peer discovery
        .route("/api/v1/mesh/peers", get(mesh_peers_handler))
        .route("/api/v1/mesh/refresh", post(mesh_refresh_handler))
        // M14-B: Gossip Protocol — peer knowledge exchange
        .route("/api/v1/gossip", post(gossip_handler))
        // Eval benchmarks (M26-B)
        .route("/api/v1/eval/run", post(api_eval::eval_run_handler))
        .route("/api/v1/eval/results", get(api_eval::eval_list_handler))
        // Embed endpoint — returns BGE-M3 1024-dim vector for a text string.
        // Used by Python guilds (night_reasoner route_intent) to compare
        // intent vs guild description similarity without loading a second copy.
        .route("/api/v1/embed", post(embed_handler))
        // Fase 1 circuito LLM examples (CoherenceGate → dataset): exporta los
        // ejemplos estructurados a NDJSON con split train/heldout por node_id.
        .route("/api/v1/llm-examples/export", get(llm_examples_export_handler))
}
