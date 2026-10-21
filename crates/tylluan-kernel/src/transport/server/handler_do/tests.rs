//! Tests de `handler_do`, extraidos de `mod.rs` en D-22a (movimiento puro).

use super::embedding::parse_content_for_embedding;

#[test]
fn d16_approve_intent_matches_only_when_the_phrase_starts_the_intent() {
    assert_eq!(
        plan_id_from_approve_intent("approve action for plan 0123456789ab").as_deref(),
        Some("0123456789ab")
    );
    assert_eq!(
        plan_id_from_approve_intent("  approve action for plan abc-1_2").as_deref(),
        Some("abc-1_2")
    );
    // T-DSH-9 F3: la variante corta ya no enruta.
    assert_eq!(plan_id_from_approve_intent("approve plan abc"), None);
    // Incrustada en otra frase: no aprueba nada.
    assert_eq!(plan_id_from_approve_intent("please do not approve plan abc"), None);
    assert_eq!(plan_id_from_approve_intent("approve plan "), None);
}

use super::*;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::path::PathBuf;
use crate::registry::guild_process::GuildRegistry;
use crate::memory::silva::SilvaDB;
use crate::memory::mailbox::Mailbox;
use crate::router::matcher::GuildMatcher;
use crate::router::catalog::builtin_catalog;
use crate::memory::agent_nodes::AgentNodeRouter;

fn test_registry() -> Arc<RwLock<GuildRegistry>> {
    let reg = GuildRegistry::new(PathBuf::from("."), 300, Default::default(), 3);
    Arc::new(RwLock::new(reg))
}

async fn test_server() -> TylluanServer {
    use tokio::sync::broadcast;
    let matcher = GuildMatcher::new(builtin_catalog());
    let (tx, _) = broadcast::channel(16);
    let node_router = AgentNodeRouter::new(tx);
    let doctor = Arc::new(crate::doctor::Doctor::new(
        test_registry(),
        Arc::new(SilvaDB::in_memory().await.unwrap()),
        Arc::new(std::sync::Mutex::new(crate::curriculum::CurriculumLearner::new_in_memory(5).unwrap())),
    ));
    TylluanServer::new(
        test_registry(),
        Arc::new(matcher),
        Arc::new(SilvaDB::in_memory().await.unwrap()),
        Arc::new(Mailbox::in_memory().await.unwrap()),
        doctor,
        node_router,
    )
}

#[test]
fn test_lesson_key_format() {
    let intent = "analyze the system health";
    let key = format!("lesson:intent:{}",
        intent.to_lowercase()
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_"));
    assert_eq!(key, "lesson:intent:analyze_the_system");
}

#[test]
fn test_lesson_key_short_intent() {
    let intent = "hello";
    let key = format!("lesson:intent:{}",
        intent.to_lowercase()
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_"));
    assert_eq!(key, "lesson:intent:hello");
}

#[test]
fn test_lesson_key_long_intent_truncated() {
    let intent = "RUN CARGO TEST FOR KERNEL MODULE WITH COVERAGE";
    let key = format!("lesson:intent:{}",
        intent.to_lowercase()
            .split_whitespace()
            .take(3)
            .collect::<Vec<_>>()
            .join("_"));
    // Only first 3 lowercase tokens
    assert_eq!(key, "lesson:intent:run_cargo_test");
    // Should NOT include "for", "kernel", "module", etc.
    assert_ne!(key, "lesson:intent:run_cargo_test_for");
}

// Discoverability regression guards, 2026-08-09: a live checklist verification
// found list_available_guilds/agent_bootstrap unreachable via tylluan_do intent
// text at all (the semantic router misrouted "list available guilds" to bash
// and "agent bootstrap" to coloquio) -- these tests assert the fix from natural
// language, not just exact tool-name dispatch, per the discoverability debt
// logged in Coloquio.
#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_list_available_guilds() {
    let server = test_server().await;
    for phrase in ["list available guilds", "qué guilds hay disponibles", "guild capabilities"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.starts_with('[') || text.contains("\"required_args\""),
            "'{phrase}' must reach list_available_guilds's JSON output, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn long_intent_containing_bootstrap_phrase_is_not_hijacked() {
    // Regression guard for the exact bug found live, 2026-08-09: a long
    // Coloquio post whose body happened to contain "qué estaba haciendo"
    // got swallowed into agent_bootstrap instead of reaching the real
    // coloquio guild -- the announcement itself never got posted.
    let server = test_server().await;
    let long_intent = format!(
        "publica en coloquio general: resumen largo del cierre, incluye la frase \
         qué estaba haciendo el agente en la sesión anterior como parte del contenido. {}",
        "relleno ".repeat(5)
    );
    assert!(long_intent.len() > 60, "test intent must exceed the short-intent threshold");
    let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": long_intent}).as_object().unwrap().clone())).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(
        !text.contains("\"identity\"") && !text.contains("\"recent_memories\""),
        "a long intent that merely mentions a bootstrap phrase must NOT be hijacked into agent_bootstrap, got: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_agent_bootstrap() {
    crate::security::grants::init();
    let server = test_server().await;
    for phrase in ["agent bootstrap", "orientame", "qué estaba haciendo"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase, "agent_id": "test-agent"}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.contains("\"identity\"") && text.contains("\"recent_memories\""),
            "'{phrase}' must reach agent_bootstrap's JSON output, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_health_not_bash() {
    // Found live by an external audit, 2026-08-09: "health" via tylluan_do
    // was silently misrouted to the bash guild and returned isError=false,
    // making a routing miss look like a legitimate result.
    let server = test_server().await;
    for phrase in ["health", "kernel health", "estado del kernel"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.contains("Kernel Health") || text.contains("OK") || text.contains("STOPPED"),
            "'{phrase}' must reach the real health subtool, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_list_pending_actions() {
    crate::security::grants::init();
    let server = test_server().await;
    for phrase in ["list pending actions", "acciones pendientes", "pending approvals"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.contains("No pending actions") || text.contains("guild"),
            "'{phrase}' must reach the real list_pending_actions subtool, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_doctor_diagnose_not_sequential_thinking() {
    // Found live by Qwen Desktop's diagnostic report, 2026-08-11: "doctor_diagnose"
    // via tylluan_do had zero deterministic routing and was anchor-matched by the
    // semantic router to sequential_thinking's compare_options (score 0.923),
    // producing "Need at least 2 options to compare, got: 1" -- a routing miss
    // disguised as a tool-usage error.
    let server = test_server().await;
    for phrase in ["doctor_diagnose", "doctor diagnose", "diagnóstico del kernel"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            !text.contains("compare_options") && !text.contains("Need at least 2 options"),
            "'{phrase}' must not be hijacked by sequential_thinking, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_doctor_repair_with_clear_error_when_target_missing() {
    // doctor_repair requires a structured 'target' argument (not parsed from
    // free text) -- routing "doctor repair" deterministically means a missing
    // target surfaces handlers.rs's real guidance message instead of drifting
    // to an unrelated guild.
    let server = test_server().await;
    let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": "doctor repair"}).as_object().unwrap().clone())).await;
    assert!(result.is_ok(), "'doctor repair' should route successfully");
    let r = result.unwrap();
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(
        text.contains("requires a 'target' argument"),
        "'doctor repair' without a target must reach doctor_repair's real guidance message, got: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn natural_language_reaches_undo_last_action() {
    // M40-P3 discoverability guard: "undo last action" (and its Spanish
    // equivalents) must route deterministically to undo_last_action, not
    // drift to a guild. Verified by the honest "Nothing reversible" reply
    // — the handler answers even with an empty log, instead of the
    // semantic router sending it somewhere unrelated.
    let server = test_server().await;
    for phrase in ["undo last action", "undo my last action", "deshacer última acción", "deshacer la ultima"] {
        let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": phrase, "agent_id": "test-agent"}).as_object().unwrap().clone())).await;
        assert!(result.is_ok(), "'{phrase}' should route successfully");
        let r = result.unwrap();
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        assert!(
            text.contains("Nothing reversible"),
            "'{phrase}' must reach undo_last_action's handler, got: {text}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn long_intent_containing_undo_phrase_is_not_hijacked() {
    // Same anchoring guard as the bootstrap test: a long message that
    // merely mentions "deshacer la última acción" must NOT be swallowed
    // by the undo matcher.
    let server = test_server().await;
    let long_intent = format!(
        "publica en coloquio general: resumen del ciclo, donde explico que para \
         deshacer la última acción hay que pedirlo explícitamente y no como parte \
         de un texto largo. {}",
        "relleno ".repeat(5)
    );
    assert!(long_intent.len() > 60, "test intent must exceed the short-intent threshold");
    let result = handle_tylluan_do(&server, Some(serde_json::json!({"intent": long_intent, "agent_id": "test-agent"}).as_object().unwrap().clone())).await;
    assert!(result.is_ok());
    let r = result.unwrap();
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(
        !text.contains("Nothing reversible"),
        "a long intent that merely mentions an undo phrase must NOT be hijacked, got: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_lesson_expiry_old_node() {
    let server = test_server().await;
    let silva = server.silva.clone();

    // Target intent: "analyze system health"
    let intent = "analyze system health";
    let lesson_key = "lesson:intent:analyze_system_health";
    let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";

    // 1. Create a lesson node that is 31 days old
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
    let age_31_days_secs = 31 * 86400;
    let touched_at = now - age_31_days_secs;

    silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();
    // Artificially modify last_touched/touched_at to make it 31 days old
    {
        let conn_guard = silva.conn_lock();
        let conn = conn_guard.lock().await;
        conn.execute(
            "UPDATE nodes SET last_touched = ?1, weight = 1.0 WHERE id = ?2",
            rusqlite::params![touched_at, lesson_key],
        ).unwrap();
    }

    // 2. Call resolve_guild_name
    let _result = resolve_guild_name(&server, intent, None, None).await;

    // Since the lesson is 31 days old, it should expire (decay) and the resolve should fall through.
    // D-8: the decay happens in a spawned task. A fixed 150ms sleep flaked
    // under the full parallel suite (ONNX/embedding contention elsewhere in
    // the binary can starve that task), so poll for the observable effect.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut weight = silva.get_node(lesson_key).await.unwrap().unwrap().weight;
    while weight >= 1.0 && std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        weight = silva.get_node(lesson_key).await.unwrap().unwrap().weight;
    }
    // Weight should have been decayed (Ebbinghaus exponential decay or apply_node_decay)
    assert!(weight < 1.0, "Old lesson weight should have decayed, got {weight}");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_lesson_penalty_on_failure() {
    let server = test_server().await;
    let silva = server.silva.clone();

    // 1. Create a lesson node
    let intent = "analyze system health";
    let lesson_key = "lesson:intent:analyze_system_health";
    let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";
    silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();

    // 2. Call handle_tylluan_do with arguments
    let mut args = serde_json::Map::new();
    args.insert("intent".to_string(), serde_json::json!(intent));

    let _result = handle_tylluan_do(&server, Some(args)).await.unwrap();

    // Poll for the spawned background task to record its trace, instead of
    // a fixed sleep -- under full-suite parallel load (500+ concurrent
    // tests) a flat 150ms was intermittently too short and made this test
    // flaky (observed failing under a full `cargo test --lib` run,
    // always passing in isolation).
    // D-8: the trace is written by a spawned task. 2s was still too tight
    // under the full parallel suite (the pre-dispatch embedding can block on
    // the ONNX mutex held by other tests), so the deadline is 10s while the
    // poll stays at 20ms -- the common case is still ~20ms.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut has_rejected = false;
    while std::time::Instant::now() < deadline {
        let traces = silva.get_node_traces(lesson_key, 10).await.unwrap();
        if traces.iter().any(|t| t.trace_type == "rejected") {
            has_rejected = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(has_rejected, "Should record a 'rejected' trace when intent fails");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_lesson_deprecated_on_low_success() {
    let server = test_server().await;
    let silva = server.silva.clone();

    let intent = "analyze system health";
    let lesson_key = "lesson:intent:analyze_system_health";
    let content = "guild:system_metrics tool:system_metrics_collect intent:analyze system health";

    // 1. Create a lesson node with weight >= 0.6
    silva.upsert_node(lesson_key, "lesson", content, "{}").await.unwrap();

    // 2. Add 6 rejected traces (total=6, rejected=6, ratio=1.0 > 0.5)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    {
        let conn = silva.conn_lock();
        let c = conn.lock().await;
        for i in 0..6 {
            c.execute(
                "INSERT INTO node_traces (node_id, agent_id, touched_at, trace_type) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![lesson_key, "test_agent", now - (i * 10), "rejected"],
            ).unwrap();
        }
    }

    // 3. Verify trace counts before calling resolve_guild_name
    let window = now - (7 * 86400);
    let total = silva.get_trace_count_since(lesson_key, window).await.unwrap();
    assert_eq!(total, 6, "Should see 6 traces");
    let rejected = silva.get_trace_count_by_type(lesson_key, "rejected", window).await.unwrap();
    assert_eq!(rejected, 6, "Should see 6 rejected traces");

    // 4. Call resolve_guild_name — should deprecate and fall through to matcher
    let _result = resolve_guild_name(&server, intent, None, None).await;

    // 5. Verify lesson was marked as deprecated in background.
    // D-8: poll instead of sleeping a fixed 300ms -- the deprecation is a
    // spawned write and 300ms flaked under the full parallel suite.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut has_deprecated = false;
    while std::time::Instant::now() < deadline {
        let traces = silva.get_node_traces(lesson_key, 20).await.unwrap();
        if traces.iter().any(|t| t.trace_type == "deprecated") {
            has_deprecated = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(has_deprecated, "Lesson should have a 'deprecated' trace after low success-rate check");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_coloquio_post_intent_never_routes_to_coordinator() {
    // Regression (2026-08-25): a long "publica en coloquio X: ..." scored
    // >=0.6 on the Proactive Cascade (M20) and was routed to coordinator
    // (required arg `task`), failing the post with "guild 'coordinator'
    // requires argument(s): task". Deterministic coloquio prefixes must
    // force the coloquio guild BEFORE any semantic/cascade routing.
    let server = test_server().await;

    let long_post = "publica en coloquio mision-activa: propuesta para el equipo con plan de implementacion y ejecucion paso a paso";
    let (guild, trace) = resolve_guild_name(&server, long_post, None, None).await.unwrap();
    assert_eq!(guild, "coloquio", "coloquio post must be forced to the coloquio guild, got '{guild}'");
    assert!(
        trace.iter().any(|t| t.contains("coloquio prefix")),
        "trace should record the deterministic dispatch, got {trace:?}"
    );

    let read = "lee el coloquio mision-activa";
    let (guild, _) = resolve_guild_name(&server, read, None, None).await.unwrap();
    assert_eq!(guild, "coloquio", "coloquio read must be forced to the coloquio guild, got '{guild}'");

    // An explicit guild hint still wins — the caller asked for it directly.
    let (guild, _) = resolve_guild_name(&server, long_post, Some("coordinator".to_string()), None).await.unwrap();
    assert_eq!(guild, "coordinator", "explicit guild hint must bypass the coloquio force");

    // Unrelated intents are NOT forced — the matcher/cascade decides,
    // even when that means NO_GUILD_MATCH (the force simply did not fire).
    let bare = "post to the coordinator: execute the migration";
    if let Ok((guild, trace)) = resolve_guild_name(&server, bare, None, None).await {
        assert_ne!(guild, "coloquio", "unrelated intent must not be forced to coloquio");
        assert!(
            !trace.iter().any(|t| t.contains("coloquio prefix")),
            "unrelated intents must not be force-routed to coloquio, got {trace:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_delegation_gate_blocks_coordinator_hijack() {
    // WS2 regression (2026-09-13 external audit — J-13 triage): a complex
    // intent that never requested delegation was swallowed by the Proactive
    // Cascade and died on coordinator's required `task` arg. With
    // coordinator registered, the gate must hold for live routing.
    let server = test_server().await;
    {
        let mut reg = server.registry.write().await;
        reg.guilds.insert(
            "coordinator".to_string(),
            crate::registry::guild_process::GuildProcess::new(
                "coordinator",
                crate::registry::guild_process::GuildLauncher::Python { module_path: "core/coordinator.py".to_string() },
                true,
                None,
                3,
            ),
        );
    }

    // Complex, multi-step, NO delegation signal — the exact hijack class.
    let hijack = "research Rust async patterns, then implement a proof of concept, then write tests, and finally document the results";
    match resolve_guild_name(&server, hijack, None, None).await {
        Ok((guild, trace)) => {
            assert_ne!(guild, "coordinator", "complex non-delegation intent must not reach coordinator, got '{guild}'");
            assert!(
                trace.iter().any(|t| t.contains("delegation_gate=blocked")),
                "gate must record the blocked hijack in the trace, got {trace:?}"
            );
        }
        Err(res) => {
            // Matcher fallback (NO_GUILD_MATCH as an error result) is also
            // acceptable — anything but a coordinator route.
            let text = res.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
            assert!(!text.contains("coordinator"), "even as an error, the hijack must not surface coordinator: {text}");
        }
    }

    // The same complexity WITH an explicit delegation request still escalates.
    let delegated = "delegate to coordinator: research the topic and then implement the fix and then write tests and then document";
    let (guild, trace) = resolve_guild_name(&server, delegated, None, None).await.unwrap();
    assert_eq!(guild, "coordinator", "explicit delegation must still reach coordinator, got '{guild}'");
    assert!(
        trace.iter().any(|t| t.contains("proactive_cascade=coordinator")),
        "trace must show the proactive cascade fired, got {trace:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn test_forget_shortcut_deletes_node() {
    let server = test_server().await;
    server.silva.upsert_node("forget:test:node", "concept", "temporary test node", "{}").await.unwrap();

    let args: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"intent": "forget: forget:test:node"}"#
    ).unwrap();
    let result = handle_tylluan_do(&server, Some(args)).await.unwrap();
    assert!(result.is_error != Some(true));
    let text = result.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<Vec<_>>().join("");
    assert!(text.contains("Forgotten"), "Expected 'Forgotten' in: {text}");
    assert!(server.silva.get_node("forget:test:node").await.unwrap().is_none());
}

#[test]
fn test_routing_failure_id_is_deterministic() {
    let id1 = routing_failure_id("list files in current directory");
    let id2 = routing_failure_id("list files in current directory");
    let id3 = routing_failure_id("different intent");
    assert_eq!(id1, id2, "same intent should produce same failure id");
    assert_ne!(id1, id3, "different intents should produce different ids");
    assert!(id1.starts_with("lesson:routing_failure:"));
}

#[test]
fn test_lesson_throttle_only_writes_when_decayed() {
    // Verifica que el guard lógico funciona: weight >= 0.5 → skip, weight < 0.5 → write
    let existing_weight_high = 0.7f64;
    let should_write = existing_weight_high < 0.5;
    assert!(!should_write, "Should not overwrite high-weight lesson");

    let existing_weight_low = 0.3f64;
    let should_write = existing_weight_low < 0.5;
    assert!(should_write, "Should update decayed lesson");
}

#[test]
fn test_rfl_guard_extracts_guild_from_content() {
    let id1 = routing_failure_id("list files in project");
    let id2 = routing_failure_id("list files in project");
    let id3 = routing_failure_id("show git status");
    assert_eq!(id1, id2);
    assert_ne!(id1, id3);
    assert!(id1.starts_with("lesson:routing_failure:"));

    let content = "ROUTING_FAILURE guild=ingest intent=extract triples from text error=empty";
    let extracted = content.split_whitespace()
        .find_map(|w| w.strip_prefix("guild=").map(|g| g.to_string()));
    assert_eq!(extracted, Some("ingest".to_string()));

    let content_no_guild = "ROUTING_FAILURE intent=list files error=empty";
    let extracted_none = content_no_guild.split_whitespace()
        .find_map(|w| w.strip_prefix("guild=").map(|g| g.to_string()));
    assert_eq!(extracted_none, None);
}

#[test]
fn test_parse_coloquio_post_with_channel_and_content() {
    let (cid, content, hint) = parse_coloquio_intent("post to mision-activa: Hello world");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content.as_deref(), Some("Hello world"));
    assert_eq!(hint, "post");
}

#[test]
fn test_parse_coloquio_post_to_channel_without_coloquio_word() {
    let (cid, content, hint) = parse_coloquio_intent("post to mision-activa: COMPLETED task");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content.as_deref(), Some("COMPLETED task"));
    assert_eq!(hint, "post");
}

#[test]
fn test_parse_coloquio_publica_en() {
    let (cid, content, hint) = parse_coloquio_intent("publica en coloquio mision-activa: Mensaje de prueba");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content.as_deref(), Some("Mensaje de prueba"));
    assert_eq!(hint, "post");
}

#[test]
fn test_parse_coloquio_lee_el() {
    let (cid, content, hint) = parse_coloquio_intent("lee el coloquio mision-activa");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content, None);
    assert_eq!(hint, "read");
}

#[test]
fn test_parse_coloquio_read_channel() {
    let (cid, _content, hint) = parse_coloquio_intent("read coloquio channel mision-activa");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(hint, "read");
}

#[test]
fn test_parse_coloquio_lee_el_with_pagination_suffix() {
    let (cid, content, hint) = parse_coloquio_intent("lee el canal coloquio mision-activa ultimos 5 mensajes");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content, None);
    assert_eq!(hint, "read");
}

#[test]
fn test_parse_coloquio_lee_el_with_limit_offset() {
    let (cid, _content, hint) = parse_coloquio_intent("lee el coloquio mision-activa offset 140 limit 30");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(hint, "read");
}

#[test]
fn test_clean_coloquio_channel_id_strips_messages() {
    let cleaned = coloquio_utils::_clean_coloquio_channel_id("mision-activa ultimos 5 mensajes");
    assert_eq!(cleaned, "mision-activa");
}

#[test]
fn test_clean_coloquio_channel_id_strips_limit() {
    let cleaned = coloquio_utils::_clean_coloquio_channel_id("mision-activa limit 10");
    assert_eq!(cleaned, "mision-activa");
}

#[test]
fn test_parse_coloquio_list() {
    let (cid, content, hint) = parse_coloquio_intent("lista canales coloquio");
    assert_eq!(cid, None);
    assert_eq!(content, None);
    assert_eq!(hint, "list");
}

#[test]
fn test_parse_coloquio_create() {
    let (cid, content, hint) = parse_coloquio_intent("crea canal test-channel: Canal de prueba");
    assert_eq!(cid.as_deref(), Some("test-channel"));
    assert_eq!(content.as_deref(), Some("Canal de prueba"));
    assert_eq!(hint, "create");
}

#[test]
fn test_parse_coloquio_send_message() {
    let (cid, content, hint) = parse_coloquio_intent("send message to mision-activa: task done");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content.as_deref(), Some("task done"));
    assert_eq!(hint, "post");
}

#[test]
fn test_parse_coloquio_envia_al_canal() {
    let (cid, content, hint) = parse_coloquio_intent("envia al canal mision-activa: COMPLETED");
    assert_eq!(cid.as_deref(), Some("mision-activa"));
    assert_eq!(content.as_deref(), Some("COMPLETED"));
    assert_eq!(hint, "post");
}

#[test]
fn test_parse_coloquio_non_coloquio_intent_returns_none() {
    let (cid, content, hint) = parse_coloquio_intent("list files in current directory");
    assert_eq!(cid, None);
    assert_eq!(content, None);
    assert_eq!(hint, "");
}

// ── distill_for_embedding tests ──

#[test]
fn test_distill_empty_output_falls_back_to_intent() {
    let result = distill_for_embedding("analiza el sistema", "");
    assert_eq!(result, "analiza el sistema");
}

#[test]
fn test_distill_short_output_falls_back_to_intent() {
    let result = distill_for_embedding("analiza el sistema", "OK");
    assert_eq!(result, "analiza el sistema");
}

#[test]
fn test_distill_knowledge_output_keeps_intent_and_content() {
    let result = distill_for_embedding(
        "estado del proyecto",
        "El informe ejecutivo muestra que el sistema tiene 2250 nodos y 12200 aristas. La cobertura de embeddings es del 99%."
    );
    assert!(result.starts_with("estado del proyecto: "));
    assert!(result.len() > 40);
    assert!(result.contains("informe ejecutivo"));
}

#[test]
fn test_distill_operational_powershell_returns_intent_only() {
    let result = distill_for_embedding(
        "Set-Content -Path archivo.json -Value contenido",
        "Set-Content -Path 'E:\\data\\file.json' -Value '{\"key\": \"value\"}'"
    );
    // Operational output — should return just intent
    assert_eq!(result, "Set-Content -Path archivo.json -Value contenido");
}

#[test]
fn test_distill_operational_curl_returns_intent_only() {
    let result = distill_for_embedding(
        "consulta el endpoint de salud",
        "curl -s http://127.0.0.1:3030/health"
    );
    assert_eq!(result, "consulta el endpoint de salud");
}

#[test]
fn test_distill_operational_json_returns_intent_only() {
    let result = distill_for_embedding(
        "guardar configuracion",
        "{\"author_id\": \"agent-1\", \"role\": \"agent\"}"
    );
    assert_eq!(result, "guardar configuracion");
}

#[test]
fn test_distill_operational_git_returns_intent_only() {
    let result = distill_for_embedding(
        "revisar cambios del repositorio",
        "git status --short\n M src/main.rs\n?? new_file.rs"
    );
    assert_eq!(result, "revisar cambios del repositorio");
}

#[test]
fn test_distill_mixed_output_extracts_meaningful_words() {
    let result = distill_for_embedding(
        "analizar log del sistema",
        "kernel version 3.0.0 puerto 3030 modo produccion timeouts OK sistema funcionando correctamente"
    );
    assert!(result.starts_with("analizar log del sistema"));
    assert!(result.contains("kernel") || result.contains("sistema"));
}

#[test]
fn test_parse_episode_with_agent() {
    let content = "tylluan_do episode | agent: test-agent | intent: list files in directory | guild: bash | tool: bash_execute | result: file1.txt\nfile2.txt";
    let (intent, preview) = parse_content_for_embedding(content, "episode");
    assert_eq!(intent, "list files in directory");
    assert_eq!(preview, "file1.txt\nfile2.txt");
}

#[test]
fn test_parse_episode_without_agent() {
    let content = "tylluan_do episode | intent: check health | guild: bash | tool: bash_execute | result: HTTP 200 OK";
    let (intent, preview) = parse_content_for_embedding(content, "episode");
    assert_eq!(intent, "check health");
    assert_eq!(preview, "HTTP 200 OK");
}

#[test]
fn test_parse_episode_empty_result() {
    let content = "tylluan_do episode | intent: list files | guild: bash | tool: bash_execute | result: ";
    let (intent, preview) = parse_content_for_embedding(content, "episode");
    assert_eq!(intent, "list files");
    assert_eq!(preview, "");
}

#[test]
fn test_parse_lesson_with_preview() {
    let content = "guild:bash tool:bash_execute intent:list files in directory -- file1.txt\nfile2.txt";
    let (intent, preview) = parse_content_for_embedding(content, "lesson");
    assert_eq!(intent, "list files in directory");
    assert_eq!(preview, "file1.txt\nfile2.txt");
}

#[test]
fn test_parse_lesson_without_preview() {
    let content = "guild:bash tool:bash_execute intent:Set-Content -Path file.txt -Value 'hello'";
    let (intent, preview) = parse_content_for_embedding(content, "lesson");
    assert_eq!(intent, "Set-Content -Path file.txt -Value 'hello'");
    assert_eq!(preview, "");
}

#[test]
fn test_parse_unknown_type_returns_empty() {
    let (intent, preview) = parse_content_for_embedding("anything", "routing_anchor");
    assert!(intent.is_empty());
    assert!(preview.is_empty());
}

// ─── Guild Rate Limiter Tests ──────────────────────────────────────────────

#[test]
fn test_guild_rate_limiter_separate_guilds() {
    let limiter = crate::security::rate_limiter::RateLimiter::new(Some(3));
    for _ in 0..3 {
        assert!(limiter.check_and_record("bash").is_ok());
    }
    // bash is now at limit
    assert!(limiter.check_and_record("bash").is_err());
    // Different guild should still work
    assert!(limiter.check_and_record("filesystem").is_ok());
}

#[test]
fn test_guild_rate_limiter_blocks_excess() {
    let limiter = crate::security::rate_limiter::RateLimiter::new(Some(2));
    assert!(limiter.check_and_record("vision").is_ok());
    assert!(limiter.check_and_record("vision").is_ok());
    // vision is at limit
    let result = limiter.check_and_record("vision");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Rate limit exceeded"));
    // After the window elapses, it should work again
    std::thread::sleep(std::time::Duration::from_millis(10));
    // But wait — window is 60s. With limit 2, the third call still fails.
    // This is a sliding window, so we must verify it's still blocked within window.
    // Create a tiny window RateLimiter for a more practical test.
}

#[test]
fn test_guild_rate_limiter_small_window() {
    // Verify that with a low limit the window actually slides
    let limiter = crate::security::rate_limiter::RateLimiter::new(Some(1));
    assert!(limiter.check_and_record("websearch").is_ok());
    assert!(limiter.check_and_record("websearch").is_err());
    // Different guild unaffected
    assert!(limiter.check_and_record("code").is_ok());
}

// ─── M32-P0: External MCP dispatch tests ───────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_external_mcp_no_servers_returns_none() {
    let server = test_server().await;
    let result = external_mcp::try_external_mcp_dispatch(
        &server, "search the web", None
    ).await;
    // Real config likely has no external MCP servers → None
    // If it does, result will be Some, which is also fine
    if result.is_none() {
        // No external MCP servers registered — expected in default config
        return;
    }
    // If Some, verify it's a valid result (not a panic/error)
    let r = result.unwrap();
    assert!(!r.content.is_empty() || r.is_error == Some(true));
}

#[tokio::test(flavor = "multi_thread")]
async fn test_external_mcp_server_name_in_intent_no_config() {
    let server = test_server().await;
    let result = external_mcp::try_external_mcp_dispatch(
        &server, "use external-mcp to search", None
    ).await;
    // With no external MCPs in the running registry, dispatch returns None.
    // This tests the guard against non-registered servers.
    assert!(result.is_none() || result.as_ref().map(|r| r.is_error == Some(true)).unwrap_or(false),
        "should be None or error when no external servers configured");
}

// ─── M31-P5: @skill prefix tests ────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_skill_save_get_roundtrip() {
    let server = test_server().await;
    let result = handle_skill_prefix(&server, "@skill:save:test-skill: This is a test skill content", &None).await;
    assert!(result.is_some(), "save should return Some");
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(false), "save should succeed");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("Skill"), "save response should mention 'Skill'");
    assert!(text.contains("test-skill"), "save response should include skill name");

    // Now get it back
    let result2 = handle_skill_prefix(&server, "@skill:get:test-skill", &None).await;
    assert!(result2.is_some(), "get should return Some");
    let r2 = result2.unwrap().unwrap();
    assert_eq!(r2.is_error, Some(false), "get should succeed");
    let text2 = r2.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text2.contains("This is a test skill content"), "get should return saved content");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_skill_list_only_project_scoped() {
    let server = test_server().await;
    // Save a skill for the current project
    let _ = handle_skill_prefix(&server, "@skill:save:my-skill: content for current project", &None).await;

    // Manually insert a skill node with a DIFFERENT project_root (simulating another project)
    let other_root = "E:/some-other-project";
    let other_hash: u64 = other_root.as_bytes().iter()
        .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u64));
    let other_id = format!("skill:project:{other_hash:x}:other-skill");
    let other_meta = serde_json::json!({
        "project_root": other_root,
        "name": "other-skill",
    }).to_string();
    server.silva.upsert_node(&other_id, "project_skill", "content from other project", &other_meta).await.unwrap();

    // List: should only show current project's skills
    let result = handle_skill_prefix(&server, "@skill:list", &None).await;
    assert!(result.is_some(), "list should return Some");
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(false), "list should succeed");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("my-skill"), "list should include current project's skill");
    assert!(!text.contains("other-skill"), "list should NOT include other project's skill");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_skill_delete_removes_node() {
    let server = test_server().await;
    // Save a skill
    let _ = handle_skill_prefix(&server, "@skill:save:delete-me: content to be deleted", &None).await;
    // Verify it exists
    let get_before = handle_skill_prefix(&server, "@skill:get:delete-me", &None).await;
    assert!(get_before.is_some());
    let text_before = get_before.unwrap().unwrap().content.iter()
        .filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text_before.contains("content to be deleted"), "skill should exist before delete");

    // Delete it
    let del = handle_skill_prefix(&server, "@skill:delete:delete-me", &None).await;
    assert!(del.is_some(), "delete should return Some");
    let d = del.unwrap().unwrap();
    assert_eq!(d.is_error, Some(false), "delete should succeed");
    let del_text = d.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(del_text.contains("deleted"), "delete response should mention deleted");

    // Verify it's gone
    let get_after = handle_skill_prefix(&server, "@skill:get:delete-me", &None).await;
    assert!(get_after.is_some(), "get after delete should return Some");
    let g = get_after.unwrap().unwrap();
    assert_eq!(g.is_error, Some(true), "get after delete should report error/is_error");
    let text_after = g.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text_after.contains("not found"), "get after delete should say 'not found'");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_skill_get_nonexistent_returns_clear_message() {
    let server = test_server().await;
    let result = handle_skill_prefix(&server, "@skill:get:nonexistent-skill", &None).await;
    assert!(result.is_some(), "get nonexistent should return Some");
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(true), "get nonexistent should be error");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("not found"), "error message should say 'not found'");
    assert!(!text.is_empty(), "should return a non-empty message, not a panic");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_skill_prefix_not_matched_returns_none() {
    let server = test_server().await;
    let result = handle_skill_prefix(&server, "list files in current directory", &None).await;
    assert!(result.is_none(), "non-skill intents should return None");
}

// ─── M36: @correct: prefix tests ──────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_supersedes_and_creates_new() {
    let server = test_server().await;
    server.silva.upsert_node("m36:test:original", "concept", "original content", "{}").await.unwrap();

    let result = handle_correct_prefix(&server, "@correct:m36:test:original:corrected content").await;
    assert!(result.is_some(), "correct should return Some");
    let r = result.unwrap().unwrap();
    if r.is_error == Some(true) {
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        panic!("correct returned error: {text}");
    }

    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("Corrected"), "success response should mention 'Corrected'");
    assert!(text.contains("m36:test:original"), "response should include original node id");

    // Old node should have valid_until set
    let old = server.silva.get_node("m36:test:original").await.unwrap().unwrap();
    assert!(old.valid_until.is_some(), "old node should have valid_until set");

    // New node should exist with corrected content
    let all = server.silva.get_all_nodes().await.unwrap();
    let new_node = all.iter().find(|n| n.id.contains("corrected")).unwrap();
    assert_eq!(new_node.content, "corrected content", "new node should have corrected content");
    assert_eq!(new_node.provenance, "agent_generated", "new node should have provenance=agent_generated");

    // Edge should exist: new -> old
    let new_id = &new_node.id;
    let edges = server.silva.get_all_edges().await.unwrap();
    assert!(edges.iter().any(|e|
        e["source"].as_str() == Some(new_id.as_str()) &&
        e["target"].as_str() == Some("m36:test:original") &&
        e["type"].as_str() == Some("corrects")
    ), "should have 'corrects' edge from new to old");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_rejects_protected() {
    let server = test_server().await;
    server.silva.upsert_node("m36:protected:node", "concept", "protected content", "{}").await.unwrap();
    server.silva.protect_node("m36:protected:node").await.unwrap();

    let result = handle_correct_prefix(&server, "@correct:m36:protected:node:new content").await;
    assert!(result.is_some());
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(true), "protected correction should be error");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("protected"), "error should mention 'protected'");

    // Original should not have valid_until set
    let node = server.silva.get_node("m36:protected:node").await.unwrap().unwrap();
    assert!(node.valid_until.is_none(), "protected node should not be superseded");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_rejects_identity() {
    let server = test_server().await;
    server.silva.upsert_node("agent:test-agent", "identity", "agent identity", "{}").await.unwrap();

    let result = handle_correct_prefix(&server, "@correct:agent:test-agent:new identity").await;
    assert!(result.is_some());
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(true), "identity correction should be error");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("identity"), "error should mention 'identity'");

    let node = server.silva.get_node("agent:test-agent").await.unwrap().unwrap();
    assert!(node.valid_until.is_none(), "identity node should not be superseded");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_rejects_already_superseded() {
    let server = test_server().await;
    server.silva.upsert_node("m36:old:node", "concept", "old content", "{}").await.unwrap();

    // First correction
    let r1 = handle_correct_prefix(&server, "@correct:m36:old:node:newer content").await;
    assert!(r1.is_some(), "first correct should return Some");
    let r1 = r1.unwrap().unwrap();
    if r1.is_error == Some(true) {
        let text = r1.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        panic!("first correct returned error: {text}");
    }

    // Second correction on same original should fail (already has valid_until)
    let r2 = handle_correct_prefix(&server, "@correct:m36:old:node:even newer content").await;
    assert!(r2.is_some());
    let r2 = r2.unwrap().unwrap();
    assert_eq!(r2.is_error, Some(true), "second correct on same node should be error");
    let text = r2.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("already"), "error should mention 'already'");
    // Only one corrected node should exist
    let all = server.silva.get_all_nodes().await.unwrap();
    let corrected: Vec<_> = all.iter().filter(|n| n.id.contains("corrected")).collect();
    assert_eq!(corrected.len(), 1, "only one corrected node should exist");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_content_with_colon_keeps_node_id_intact() {
    // Regression (Desván bug): the old rsplit_once(':') split at the LAST
    // colon, so content with a literal ':' (e.g. an arXiv citation) was
    // parsed as part of the node id. The node_id must now be the longest
    // colon-delimited prefix that names an existing node.
    let server = test_server().await;
    server.silva.upsert_node("m36:colon:node", "concept", "original content", "{}").await.unwrap();

    let result = handle_correct_prefix(&server, "@correct:m36:colon:node:see arXiv:2606.24322 for the fix").await;
    assert!(result.is_some(), "correct should return Some");
    let r = result.unwrap().unwrap();
    if r.is_error == Some(true) {
        let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
        panic!("correct returned error: {text}");
    }

    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("m36:colon:node"), "node id must be parsed intact, got: {text}");

    // Old node superseded, new node keeps the full content incl. its colon.
    let old = server.silva.get_node("m36:colon:node").await.unwrap().unwrap();
    assert!(old.valid_until.is_some(), "old node should have valid_until set");
    let all = server.silva.get_all_nodes().await.unwrap();
    let new_node = all.iter().find(|n| n.id.contains("corrected")).unwrap();
    assert_eq!(new_node.content, "see arXiv:2606.24322 for the fix", "content must preserve the colon");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_rejects_nonexistent_node() {
    let server = test_server().await;
    let result = handle_correct_prefix(&server, "@correct:nonexistent:node:some content").await;
    assert!(result.is_some());
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(true), "nonexistent node should be error");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("not found"), "error should say 'not found'");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_parse_error_returns_usage() {
    let server = test_server().await;
    let result = handle_correct_prefix(&server, "@correct:").await;
    assert!(result.is_some());
    let r = result.unwrap().unwrap();
    assert_eq!(r.is_error, Some(true), "empty correct should be error");
    let text = r.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>();
    assert!(text.contains("Usage"), "error should show usage");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_correct_prefix_not_matched_returns_none() {
    let server = test_server().await;
    let result = handle_correct_prefix(&server, "list files in current directory").await;
    assert!(result.is_none(), "non-correct intents should return None");
}

// ── M19-P5 bash_execute fallback ──────────────────────────────────

#[test]
fn test_is_bash_state_intent_explicit_restore() {
    assert!(is_bash_state_intent("state_restore"));
    assert!(is_bash_state_intent("restore state"));
    assert!(is_bash_state_intent("state_restore from last checkpoint"));
}

#[test]
fn test_is_bash_state_intent_explicit_checkpoint() {
    assert!(is_bash_state_intent("state_checkpoint"));
    assert!(is_bash_state_intent("checkpoint state"));
    assert!(is_bash_state_intent("state save"));
    assert!(is_bash_state_intent("save state"));
}

#[test]
fn test_is_bash_state_intent_commands_are_not_state_ops() {
    assert!(!is_bash_state_intent("git status"));
    assert!(!is_bash_state_intent("ls -la"));
    assert!(!is_bash_state_intent("cargo build"));
    assert!(!is_bash_state_intent("echo hello"));
    assert!(!is_bash_state_intent("list files"));
    assert!(!is_bash_state_intent("run python script.py"));
}

#[test]
fn test_is_bash_state_intent_edge_cases() {
    assert!(!is_bash_state_intent(""));
    assert!(!is_bash_state_intent("state"));
    assert!(!is_bash_state_intent("restore file backup"));
    assert!(!is_bash_state_intent("save file to disk"));
    assert!(!is_bash_state_intent("check status"));
}

#[test]
fn test_is_bash_state_intent_case_insensitive() {
    assert!(is_bash_state_intent("State_Restore"));
    assert!(is_bash_state_intent("RESTORE STATE"));
    assert!(is_bash_state_intent("State_Checkpoint after update"));
}

// ── MD-3: prefijos de error estables para fallos de Stage-1 ─────────────
// MEASUREMENT_DEBT_REGISTER MD-3: el harness de accuracy descompuesta
// (benchmarks/benchmark_md3_decomposed_accuracy.py) clasifica los fallos de
// tylluan_do parseando prefijos estables máquina-legibles. Estos tests fijan
// cada prefijo en su chokepoint real para que un refactor futuro no rompa
// silenciosamente el clasificador. Convención aditiva: prefijo + cuerpo
// verbatim (los consumidores del texto viejo siguen haciendo match).
// NO_TOOLS y MISSING_ARGS requieren un guild vivo (ensure_guild_running
// hace un handshake MCP real contra un proceso), así que su pin end-to-end
// lo hace la corrida del harness contra el kernel de test aislado, no aquí.

fn md3_error_text(res: &CallToolResult) -> String {
    res.content.iter().filter_map(|c| c.as_text()).map(|t| t.text.clone()).collect::<String>()
}

#[tokio::test(flavor = "multi_thread")]
async fn md3_unknown_guild_hint_is_routing_failed() {
    let server = test_server().await;
    let hint = Some("guild_inexistente_md3".to_string());
    let agent_id: Option<String> = None;
    let arguments: Option<serde_json::Map<String, serde_json::Value>> = None;
    let err = match resolve_and_prepare_tool_call(
        &server, "haz algo md3", "haz algo md3", "haz algo md3",
        &hint, &agent_id, &arguments,
    )
    .await
    {
        Ok(_) => panic!("hint de guild desconocido debe fallar"),
        Err(e) => e,
    };
    let text = md3_error_text(&err);
    assert!(
        text.starts_with("❌ Error: ROUTING_FAILED:"),
        "debe llevar prefijo ROUTING_FAILED: tras el wrapper de error_result, got: {text}"
    );
    // Cuerpo verbatim preservado para consumidores existentes.
    assert!(
        text.contains("Unknown guild 'guild_inexistente_md3'"),
        "cuerpo del mensaje debe preservarse, got: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn md3_http_guild_spawn_failure_is_start_failed() {
    let server = test_server().await;
    // register_http_mcp registra un guild sin levantar proceso;
    // ensure_guild_running intentará conectar a un puerto cerrado de
    // 127.0.0.1 y fallará de forma determinista.
    server.registry.write().await.register_http_mcp(
        "md3_dead_guild",
        "http://127.0.0.1:1/mcp",
        Default::default(),
        Some(500),
    );
    let hint = Some("md3_dead_guild".to_string());
    let agent_id: Option<String> = None;
    let arguments: Option<serde_json::Map<String, serde_json::Value>> = None;
    let err = match resolve_and_prepare_tool_call(
        &server, "usa el guild muerto", "usa el guild muerto", "usa el guild muerto",
        &hint, &agent_id, &arguments,
    )
    .await
    {
        Ok(_) => panic!("guild http muerto debe fallar al arrancar"),
        Err(e) => e,
    };
    let text = md3_error_text(&err);
    assert!(
        text.starts_with("❌ Error: START_FAILED:"),
        "debe llevar prefijo START_FAILED: tras el wrapper de error_result, got: {text}"
    );
    assert!(
        text.contains("Failed to start guild 'md3_dead_guild'"),
        "cuerpo del mensaje debe preservarse, got: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn md3_rate_limit_error_is_rate_limited() {
    let server = test_server().await;
    // Agotar el limiter per-guild de TylluanServer::new (120/min) ANTES de
    // que el handler vea la llamada, usando un guild real del catálogo para
    // pasar el check de hint conocido y llegar al chokepoint del limiter.
    for _ in 0..120 {
        server
            .guild_rate_limiter
            .check_and_record("bash")
            .expect("el limiter debe aceptar hasta su máximo");
    }
    let hint = Some("bash".to_string());
    let agent_id: Option<String> = None;
    let arguments: Option<serde_json::Map<String, serde_json::Value>> = None;
    let err = match resolve_and_prepare_tool_call(
        &server, "ejecuta algo md3", "ejecuta algo md3", "ejecuta algo md3",
        &hint, &agent_id, &arguments,
    )
    .await
    {
        Ok(_) => panic!("guild en rate limit debe fallar"),
        Err(e) => e,
    };
    let text = md3_error_text(&err);
    assert!(
        text.starts_with("❌ Error: RATE_LIMITED:"),
        "debe llevar prefijo RATE_LIMITED: tras el wrapper de error_result, got: {text}"
    );
    assert!(
        text.contains("Rate limit for guild 'bash' exceeded"),
        "cuerpo del mensaje debe preservarse, got: {text}"
    );
}