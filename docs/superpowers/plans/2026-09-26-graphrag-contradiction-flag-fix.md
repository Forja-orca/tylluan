# GraphRAG Contradiction False-Positive & NightConsolidation Cap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the perpetual conflict-flag/resolve loop that is pegging production CPU (~2000% sustained), by excluding legitimate one-to-many edge types from the contradiction detector, purging the legacy corrupted nodes that loop caused, and capping NightConsolidation's parallelism so future bursts stay bounded.

**Architecture:** Three independent, sequentially-safe changes to `tylluan-kernel`: (1) a one-line SQL predicate fix plus a regression test in `flag_contradiction_nodes()`; (2) a startup-time idempotent cleanup pass for already-corrupted `graphrag_summary:*` nodes; (3) a new `[night]` config section wired into the existing `PhaseOrchestrator` semaphore, defaulting to today's behavior (uncapped) so nothing changes until José opts in.

**Tech Stack:** Rust (tokio, rusqlite), existing `tylluan-kernel` crate conventions (serde config structs, `#[tokio::test(flavor = "multi_thread")]`).

**Spec:** `docs/superpowers/specs/2026-09-26-graphrag-contradiction-flag-fix-design.md`

## Global Constraints

- Never start, stop, or restart `tylluan-nexus.exe` — only José does that (CLAUDE.md standing rule). All tasks compile/test only; no live process is touched.
- No `cargo build`/`cargo test` while another agent may be committing in the same shared checkout — check Coloquio before running builds (CLAUDE.md standing rule).
- `cargo clippy -- -D warnings` must stay clean on every task (repo convention).
- New config fields must default to the exact current behavior (no default-driven change to production once rebuilt) — see spec's Parte 3.
- Commit format: conventional commits (`fix:`, `feat:`, `test:`), with `## Impact` in the body since these touch `crates/tylluan-kernel/src/memory/` (transport/guilds/integrations/tylluan*.toml rule from CLAUDE.md doesn't strictly apply here, but `memory/` is core — include `## Impact` anyway per house habit).
- Only Claude Code (the orchestrating session) commits/pushes — per this repo's `feedback_only_claude_commits_git` convention, not a blocker for task execution, just don't push without the tech lead's own commit step.

---

### Task 1: Exclude `member_of` and `remembers` from `flag_contradiction_nodes`

**Files:**
- Modify: `crates/tylluan-kernel/src/memory/silva/nodes.rs:750-819` (function `flag_contradiction_nodes`)
- Test: same file, `#[cfg(test)] mod tests` block (append near other `flag_contradiction_nodes` tests if any exist, otherwise at the end of the test module)

**Interfaces:**
- Consumes: `SilvaDB::add_edge(&self, source: &str, target: &str, edge_type: &str, weight: f64, metadata: &str) -> Result<()>` (existing, used to set up the test fixture)
- Consumes: `SilvaDB::upsert_node(&self, id: &str, node_type: &str, content: &str, metadata: &str) -> Result<()>` (existing, used to create test nodes)
- Consumes: `SilvaDB::get_node(&self, id: &str) -> Result<Option<GraphNode>>` (existing, used to assert on `conflicted`)
- Produces: `flag_contradiction_nodes()` with its `WHERE` clause updated — no signature change, callers unaffected.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block of `crates/tylluan-kernel/src/memory/silva/nodes.rs`:

```rust
    // Regression test for the CPU-runaway incident found live 2026-09-26:
    // flag_contradiction_nodes() flagged every GraphRAG summary node and
    // every agent identity node as conflicted=1 on every pass, because it
    // treated the intentional one-to-many fan-out of `member_of` and
    // `remembers` edges as a factual contradiction (>1 distinct target for
    // the same predicate). ConsensusEngine::resolve_conflicts() then
    // unmarked them, and the next pass re-marked them — a perpetual loop
    // that pinned CPU at ~2000% sustained. This locks in the exclusion so
    // it can't silently regress.
    #[tokio::test(flavor = "multi_thread")]
    async fn flag_contradiction_nodes_ignores_member_of_fan_out() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("summary_hub", "summary", "a cluster summary", "{}").await.unwrap();
        for i in 0..5 {
            let member_id = format!("member_{i}");
            db.upsert_node(&member_id, "memory", "a member node", "{}").await.unwrap();
            db.add_edge("summary_hub", &member_id, "member_of", 1.0, "{}").await.unwrap();
        }

        db.flag_contradiction_nodes().await.unwrap();

        let node = db.get_node("summary_hub").await.unwrap().unwrap();
        assert!(
            !node.conflicted,
            "member_of fan-out (one summary, many members) must never be flagged as a contradiction"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn flag_contradiction_nodes_ignores_remembers_fan_out() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("agent_memory:claude-code", "agent_identity", "agent identity", "{}").await.unwrap();
        for i in 0..5 {
            let memory_id = format!("memory:{i}");
            db.upsert_node(&memory_id, "memory", "a remembered thing", "{}").await.unwrap();
            db.add_edge("agent_memory:claude-code", &memory_id, "remembers", 1.0, "{}").await.unwrap();
        }

        db.flag_contradiction_nodes().await.unwrap();

        let node = db.get_node("agent_memory:claude-code").await.unwrap().unwrap();
        assert!(
            !node.conflicted,
            "remembers fan-out (one agent identity, many remembered things) must never be flagged as a contradiction"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn flag_contradiction_nodes_still_flags_real_contradictions() {
        // Genuine contradiction: same predicate, two different targets, both
        // with NULL valid_from — the exclusion must not swallow real cases.
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("claim_a", "concept", "a claim", "{}").await.unwrap();
        db.upsert_node("target_1", "concept", "target one", "{}").await.unwrap();
        db.upsert_node("target_2", "concept", "target two", "{}").await.unwrap();
        db.add_edge("claim_a", "target_1", "asserts", 1.0, "{}").await.unwrap();
        db.add_edge("claim_a", "target_2", "asserts", 1.0, "{}").await.unwrap();

        db.flag_contradiction_nodes().await.unwrap();

        let node = db.get_node("claim_a").await.unwrap().unwrap();
        assert!(
            node.conflicted,
            "a genuine same-predicate contradiction must still be flagged"
        );
    }
```

- [ ] **Step 2: Run tests to verify the first two fail, the third passes**

Run: `cargo test -p tylluan-kernel --lib flag_contradiction_nodes -- --nocapture`
Expected: `flag_contradiction_nodes_ignores_member_of_fan_out` and `flag_contradiction_nodes_ignores_remembers_fan_out` FAIL (both assert `!node.conflicted` but the current code sets it true); `flag_contradiction_nodes_still_flags_real_contradictions` PASSes already.

- [ ] **Step 3: Fix the SQL predicate**

In `crates/tylluan-kernel/src/memory/silva/nodes.rs`, change the query inside `flag_contradiction_nodes` (currently at line ~755-759):

```rust
            // Fetch all edges (except related_to) ordered by source, type, valid_from
            let mut stmt = conn.prepare(
                "SELECT source, type, target, valid_from FROM edges
                 WHERE type != 'related_to'
                 ORDER BY source, type, valid_from ASC NULLS LAST"
            )?;
```

to:

```rust
            // Fetch all edges ordered by source, type, valid_from. Excludes
            // predicates that are intentionally one-to-many by design, not
            // factual contradictions: `related_to` (generic association),
            // `member_of` (a GraphRAG cluster summary legitimately links to
            // every one of its members — graph_rag.rs's save_summary), and
            // `remembers` (an agent identity node legitimately links to
            // every thing it has remembered — main.rs / handler_remember.rs).
            // Regression: see flag_contradiction_nodes_ignores_member_of_fan_out
            // and flag_contradiction_nodes_ignores_remembers_fan_out below —
            // omitting either exclusion re-creates the 2026-09-26 CPU
            // runaway where every GraphRAG summary and every agent identity
            // node was flagged conflicted=1 on every pass, forever.
            let mut stmt = conn.prepare(
                "SELECT source, type, target, valid_from FROM edges
                 WHERE type != 'related_to' AND type != 'member_of' AND type != 'remembers'
                 ORDER BY source, type, valid_from ASC NULLS LAST"
            )?;
```

- [ ] **Step 4: Run tests to verify all three pass**

Run: `cargo test -p tylluan-kernel --lib flag_contradiction_nodes -- --nocapture`
Expected: all 3 tests PASS.

- [ ] **Step 5: Run the full kernel test suite**

Run: `cargo test -p tylluan-kernel --lib`
Expected: PASS, same count as before plus 3 (verify current baseline count first with `git stash` if unsure — do not assume a specific number, compare before/after).

- [ ] **Step 6: Clippy check**

Run: `cargo clippy -p tylluan-kernel --lib -- -D warnings`
Expected: clean, no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/tylluan-kernel/src/memory/silva/nodes.rs
git commit -m "fix(silva): exclude member_of and remembers from contradiction detection

flag_contradiction_nodes() treated one-source-many-targets fan-out on
member_of (GraphRAG summary->members) and remembers (agent identity->
memories) as factual contradictions, flagging every such node
conflicted=1 on every pass. ConsensusEngine::resolve_conflicts() then
unflagged them, and the next pass re-flagged them -- a perpetual loop
that pinned production CPU at ~2000% sustained (PID 44364, 84 CPU-days
in 4 wall-clock days, 323 threads -- same signature as the 2026-08-30
incident, but a different root cause: that fix guarded new nesting,
this fixes the false-positive that kept legacy and fresh nodes cycling
through the conflict queue forever).

## Impact
Touches crates/tylluan-kernel/src/memory/silva/ (core memory subsystem).
Changes which nodes flag_contradiction_nodes() marks conflicted -- only
narrows it (member_of and remembers are now excluded, matching the
existing related_to exclusion). No change to any other edge type's
contradiction detection."
```

---

### Task 2: Purge legacy corrupted `graphrag_summary` nodes on startup

**Files:**
- Modify: `crates/tylluan-kernel/src/memory/silva/schema.rs` (add a new function, call it from wherever `init_schema()` or the equivalent startup migration step is invoked)
- Test: same file's `#[cfg(test)] mod tests` block, or `crates/tylluan-kernel/src/memory/silva/tests.rs` if that's where schema-level tests live (check both, follow whichever the existing GraphRAG regression tests use — `graph_rag.rs`'s own `#[cfg(test)] mod tests` is the closest precedent for this kind of node-shape test)

**Interfaces:**
- Consumes: `rusqlite::Connection` (via `self.conn.blocking_lock()`, same pattern as `flag_contradiction_nodes`)
- Produces: `pub async fn purge_legacy_nested_graphrag_summaries(&self) -> Result<(usize, usize, usize)>` on `SilvaDB` — returns `(nodes_deleted, edges_deleted, cluster_summaries_deleted)` so the caller can log the counts.

- [ ] **Step 1: Write the failing test**

Add to `crates/tylluan-kernel/src/memory/silva/nodes.rs`'s test module (co-locate with the Task 1 tests — same file already has the SilvaDB test helpers):

```rust
    // Regression test for the legacy-cleanup half of the 2026-09-26 CPU
    // runaway fix: nodes created before the flag_contradiction_nodes fix
    // (Task 1) are already corrupted with nested graphrag_summary: ids and
    // must be purged on startup, without touching a valid single-level
    // summary or its member_of edges.
    #[tokio::test(flavor = "multi_thread")]
    async fn purge_legacy_nested_graphrag_summaries_removes_only_nested() {
        let db = SilvaDB::in_memory().await.unwrap();

        // A valid, single-level summary -- must survive.
        db.upsert_node("graphrag_summary:cluster:hub_a", "summary", "a valid summary", "{}").await.unwrap();
        db.upsert_node("member_a", "memory", "a member", "{}").await.unwrap();
        db.add_edge("graphrag_summary:cluster:hub_a", "member_a", "member_of", 1.0, "{}").await.unwrap();

        // A legacy corrupted, doubly-nested summary -- must be purged.
        let nested_id = "graphrag_summary:cluster:graphrag_summary:cluster:hub_b";
        db.upsert_node(nested_id, "summary", "a corrupted nested summary", "{}").await.unwrap();
        db.upsert_node("member_b", "memory", "a member", "{}").await.unwrap();
        db.add_edge(nested_id, "member_b", "member_of", 1.0, "{}").await.unwrap();

        let (nodes_deleted, edges_deleted, _) = db.purge_legacy_nested_graphrag_summaries().await.unwrap();

        assert_eq!(nodes_deleted, 1, "exactly the one nested node must be purged");
        assert!(edges_deleted >= 1, "the nested node's member_of edge must be purged too");

        assert!(
            db.get_node("graphrag_summary:cluster:hub_a").await.unwrap().is_some(),
            "the valid single-level summary must survive"
        );
        assert!(
            db.get_node(nested_id).await.unwrap().is_none(),
            "the nested legacy summary must be gone"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn purge_legacy_nested_graphrag_summaries_is_idempotent() {
        let db = SilvaDB::in_memory().await.unwrap();
        db.upsert_node("graphrag_summary:cluster:graphrag_summary:cluster:hub_c", "summary", "corrupted", "{}").await.unwrap();

        let first = db.purge_legacy_nested_graphrag_summaries().await.unwrap();
        assert_eq!(first.0, 1);

        let second = db.purge_legacy_nested_graphrag_summaries().await.unwrap();
        assert_eq!(second, (0, 0, 0), "a second run with nothing left to purge must be a clean no-op");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tylluan-kernel --lib purge_legacy_nested_graphrag_summaries -- --nocapture`
Expected: FAIL with "no method named `purge_legacy_nested_graphrag_summaries` found" (compile error).

- [ ] **Step 3: Implement the purge function**

Add to `crates/tylluan-kernel/src/memory/silva/nodes.rs` (same `impl SilvaDB` block that already contains `flag_contradiction_nodes` and `mark_conflicted` — keep related node-hygiene functions together):

```rust
    /// Purge legacy nodes corrupted by the pre-2026-09-26 GraphRAG nesting
    /// bug: a node id that starts with `graphrag_summary:` and contains
    /// that same prefix a second time (e.g.
    /// `graphrag_summary:cluster:graphrag_summary:cluster:hub`). These are
    /// pure bug artifacts with no semantic value -- the 2026-08-30 guard in
    /// GraphRagManager::save_summary() stops new ones from being created,
    /// but never cleaned up the ones that already existed. Idempotent: a
    /// second call with nothing matching returns (0, 0, 0) and does no
    /// writes. Returns (nodes_deleted, edges_deleted, cluster_summaries_deleted).
    pub async fn purge_legacy_nested_graphrag_summaries(&self) -> Result<(usize, usize, usize)> {
        const NESTED_PATTERN: &str = "graphrag_summary:%graphrag_summary:%";
        tokio::task::block_in_place(|| {
            let conn = self.conn.blocking_lock();

            let nested_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM nodes WHERE id LIKE ?1",
                params![NESTED_PATTERN],
                |r| r.get(0),
            )?;
            if nested_count == 0 {
                return Ok((0, 0, 0));
            }

            let edges_deleted = conn.execute(
                "DELETE FROM edges WHERE source LIKE ?1 OR target LIKE ?1",
                params![NESTED_PATTERN],
            )?;
            let cluster_summaries_deleted = conn.execute(
                "DELETE FROM cluster_summaries WHERE cluster_id LIKE '%graphrag_summary:%'",
                [],
            )?;
            let nodes_deleted = conn.execute(
                "DELETE FROM nodes WHERE id LIKE ?1",
                params![NESTED_PATTERN],
            )?;

            info!(
                "🧹 SilvaDB: purged {} legacy nested graphrag_summary nodes, {} edges, {} cluster_summaries rows",
                nodes_deleted, edges_deleted, cluster_summaries_deleted
            );

            Ok((nodes_deleted, edges_deleted, cluster_summaries_deleted))
        })
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p tylluan-kernel --lib purge_legacy_nested_graphrag_summaries -- --nocapture`
Expected: both tests PASS.

- [ ] **Step 5: Wire the purge into kernel startup**

Find where the kernel calls schema initialization at boot — search `crates/tylluan-kernel/src/main.rs` for the call to `init_schema` or `SilvaDB::new`/`SilvaDB::open` (the exact call site depends on the current boot sequence; locate it with `grep -n "init_schema\|SilvaDB::new\|SilvaDB::open" crates/tylluan-kernel/src/main.rs`). Immediately after SilvaDB is constructed and schema migrations have run, add:

```rust
    match silva.purge_legacy_nested_graphrag_summaries().await {
        Ok((nodes, edges, summaries)) if nodes > 0 => {
            info!("🧹 Startup cleanup: purged {nodes} legacy nested GraphRAG nodes ({edges} edges, {summaries} cluster_summaries rows)");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Startup cleanup: failed to purge legacy nested GraphRAG nodes: {e}"),
    }
```

Use the exact variable name the boot sequence already uses for the `SilvaDB` instance (likely `silva`, matching Task 1's convention and `PhaseContext.silva` — confirm by reading the 20 lines around the located call site before inserting).

- [ ] **Step 6: Full test suite + clippy**

Run: `cargo test -p tylluan-kernel --lib`
Expected: PASS, baseline count + 5 (3 from Task 1 + 2 from Task 2).

Run: `cargo clippy -p tylluan-kernel --lib -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/tylluan-kernel/src/memory/silva/nodes.rs crates/tylluan-kernel/src/main.rs
git commit -m "fix(silva): purge legacy nested graphrag_summary nodes on startup

The 2026-08-30 guard in GraphRagManager::save_summary() stopped new
nested-id summaries from being created, but never cleaned up the
thousands that already existed (first occurrence found in kernel.log:
2026-07-05). Combined with the flag_contradiction_nodes false-positive
fixed in the previous commit, these legacy nodes were cycling through
the conflict queue on every consensus pass, forever.

purge_legacy_nested_graphrag_summaries() runs once at boot, deletes
nodes/edges/cluster_summaries rows matching the double-nesting pattern
(id LIKE 'graphrag_summary:%graphrag_summary:%'), and is a no-op once
the corruption is gone -- safe to leave running on every future boot.

## Impact
Touches crates/tylluan-kernel/src/memory/silva/ (core memory subsystem)
and the kernel boot sequence in main.rs. Deletes rows matching a very
specific corrupted-id pattern only -- a valid single-level summary
(graphrag_summary:cluster:<hub>) never matches. José has not yet
restarted production with this fix; the purge will run on next boot."
```

---

### Task 3: Add a configurable cap on NightConsolidation phase parallelism

**Files:**
- Modify: `crates/tylluan-kernel/src/config.rs` (add `NightConfig` struct + field on `TylluanConfig`)
- Modify: `crates/tylluan-kernel/src/memory/night/mod.rs:65-124` (`PhaseOrchestrator` — thread the cap through)
- Modify: `crates/tylluan-kernel/src/main.rs:1859-1892` (pass the configured cap when constructing the orchestrator, and use the configured interval)
- Test: `crates/tylluan-kernel/src/memory/night/mod.rs`'s existing `#[cfg(test)] mod tests` block (it already has orchestrator tests per the earlier read, e.g. `orchestrator_empty_phase_list` — add alongside those)

**Interfaces:**
- Consumes: `TylluanConfig` (existing top-level config struct, `#[serde(default)] pub night: NightConfig` field pattern — same as `EvalConfig`)
- Produces: `NightConfig { max_parallel_phases: Option<usize>, interval_secs: u64 }` with `impl Default`
- Produces: `PhaseOrchestrator::new(phases: Vec<Box<dyn Phase>>, max_parallel_override: Option<usize>) -> Self` — **signature change**, every existing call site must pass the new second argument (main.rs:1859 and every test call site in night/mod.rs's test module: lines ~198, 236, 245, 268, 538 per the earlier read).

- [ ] **Step 1: Write the failing test**

Add to `crates/tylluan-kernel/src/memory/night/mod.rs`'s `#[cfg(test)] mod tests` block:

```rust
    struct CountingPhase {
        name: &'static str,
        current_concurrent: Arc<std::sync::atomic::AtomicUsize>,
        max_concurrent: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Phase for CountingPhase {
        fn name(&self) -> &'static str { self.name }
        async fn run(&self, _ctx: &PhaseContext) -> PhaseReport {
            let cur = self.current_concurrent.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            self.max_concurrent.fetch_max(cur, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            self.current_concurrent.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            PhaseReport { name: self.name, duration_ms: 25, ok: true, detail: "ok".into() }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_all_respects_configured_parallelism_cap() {
        let ctx = test_phase_context().await;
        let max_concurrent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let current_concurrent = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let phases: Vec<Box<dyn Phase>> = (0..8).map(|i| {
            let max_concurrent = max_concurrent.clone();
            let current_concurrent = current_concurrent.clone();
            Box::new(CountingPhase {
                name: Box::leak(format!("phase_{i}").into_boxed_str()),
                current_concurrent,
                max_concurrent,
            }) as Box<dyn Phase>
        }).collect();

        // Cap at 2, even though 8 phases exist and the machine likely has
        // more than 2 cores available.
        let orch = PhaseOrchestrator::new(phases, Some(2));
        orch.run_all(&ctx).await;

        let observed_max = max_concurrent.load(std::sync::atomic::Ordering::SeqCst);
        assert!(
            observed_max <= 2,
            "configured cap of 2 must never be exceeded, observed {observed_max}"
        );
    }
```

This test needs a `CountingPhase` test helper — check whether `night/mod.rs`'s test module already defines one (the earlier read showed a `max_concurrent` atomic being used around line 266-271, in a test that already counts concurrency for the *uncapped* default case). If a `CountingPhase`-shaped helper already exists there, reuse it and its exact field names instead of redefining — read that existing test in full before writing this one, and adjust the snippet above to match its real struct name and fields.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p tylluan-kernel --lib run_all_respects_configured_parallelism_cap -- --nocapture`
Expected: FAIL to compile — `PhaseOrchestrator::new` still takes one argument.

- [ ] **Step 3: Add `NightConfig` to `config.rs`**

Add near `EvalConfig` (same file, following its exact pattern):

```rust
/// NightConsolidation scheduling and concurrency settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NightConfig {
    /// Caps how many NightConsolidation phases run concurrently. `None`
    /// (the default) preserves the original behavior: capped only by
    /// `min(available_parallelism(), phase_count)`, which on a many-core
    /// machine can still mean a dozen-plus phases racing for CPU at once
    /// every cycle. Set to a small number (e.g. 2-4) on a shared or
    /// latency-sensitive machine to bound each NightConsolidation burst.
    /// Root cause context: 2026-09-26 incident where uncapped parallelism
    /// combined with a separate contradiction-flagging bug (see nodes.rs
    /// flag_contradiction_nodes) produced sustained ~2000% CPU.
    #[serde(default)]
    pub max_parallel_phases: Option<usize>,

    /// Seconds between NightConsolidation cycles. Default 1800 (30 min),
    /// matching the hardcoded interval this replaces.
    #[serde(default = "default_night_interval_secs")]
    pub interval_secs: u64,
}

impl Default for NightConfig {
    fn default() -> Self {
        Self {
            max_parallel_phases: None,
            interval_secs: default_night_interval_secs(),
        }
    }
}

fn default_night_interval_secs() -> u64 { 1800 }
```

Then add the field to `TylluanConfig` (near `eval`, in the same struct block read earlier):

```rust
    #[serde(default)]
    pub night: NightConfig,
```

- [ ] **Step 4: Thread the cap through `PhaseOrchestrator`**

In `crates/tylluan-kernel/src/memory/night/mod.rs`, change the struct and constructor:

```rust
pub struct PhaseOrchestrator {
    phases: Vec<Arc<dyn Phase>>,
    max_parallel_override: Option<usize>,
}

impl PhaseOrchestrator {
    pub fn new(phases: Vec<Box<dyn Phase>>, max_parallel_override: Option<usize>) -> Self {
        Self {
            phases: phases.into_iter().map(Arc::from).collect(),
            max_parallel_override,
        }
    }

    /// Runs every phase concurrently, capped to the machine's real core
    /// count -- or to `max_parallel_override` when the operator has set
    /// `[night] max_parallel_phases` in tylluan.toml, whichever is smaller.
    pub async fn run_all(&self, ctx: &PhaseContext) {
        let start = Instant::now();
        let mut max_parallel = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(self.phases.len().max(1));
        if let Some(override_cap) = self.max_parallel_override {
            max_parallel = max_parallel.min(override_cap.max(1));
        }
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_parallel));
```

(Keep the rest of `run_all` — the `info!` log, spawn loop, and completion log — unchanged; only the `max_parallel` computation and the struct/constructor above change.)

- [ ] **Step 5: Update every existing call site**

`run_night_consolidation_loop` (`crates/tylluan-kernel/src/main.rs:1844`) currently takes no config parameter — verified: it's spawned from inside `async fn main()` (line 1730) where, by that point, `config` is already `Arc<RwLock<TylluanConfig>>` (established at line 1289: `let config = Arc::new(RwLock::new(config.clone()));`, later read via `config.read().await` elsewhere in `main()`). Add a 7th parameter and thread it through:

```rust
async fn run_night_consolidation_loop(
    silva: Arc<SilvaDB>,
    agent_profiles: Option<Arc<Mutex<AgentProfileStore>>>,
    curriculum: Arc<Mutex<CurriculumLearner>>,
    server: Arc<RwLock<TylluanServer>>,
    data_dir: PathBuf,
    matcher: Arc<GuildMatcher>,
    config: Arc<RwLock<tylluan_kernel::config::TylluanConfig>>,
) {
    use tylluan_kernel::memory::night::{
        PhaseOrchestrator, PhaseContext,
        DreamPhase, OuroborosPhase, AutoLinkPhase, GraphRagPhase,
        DecayPhase, AgentPhase, CurriculumPhase, IdleLabPhase, FeedbackSignalPhase,
        LifecyclePhase, DeepEvalPhase, SlmSocietyPhase,
    };

    let night_config = { config.read().await.night.clone() };

    let orchestrator = PhaseOrchestrator::new(vec![
        Box::new(DreamPhase),
        Box::new(OuroborosPhase),
        Box::new(AutoLinkPhase),
        Box::new(GraphRagPhase),
        Box::new(DecayPhase),
        Box::new(AgentPhase),
        Box::new(CurriculumPhase),
        Box::new(IdleLabPhase),
        Box::new(FeedbackSignalPhase),
        Box::new(LifecyclePhase),
        Box::new(DeepEvalPhase),
        Box::new(SlmSocietyPhase),
    ], night_config.max_parallel_phases);
```

(`NightConfig` needs `Clone` — it already derives `Clone` per Step 3's exact struct definition above, so `.night.clone()` compiles as written.)

Change the interval setup two lines below (currently `let mut interval = tokio::time::interval(Duration::from_secs(1800));`):

```rust
    let mut interval = tokio::time::interval(Duration::from_secs(night_config.interval_secs));
```

Update the call site at `crates/tylluan-kernel/src/main.rs:1730` to pass the config:

```rust
    tokio::spawn(run_night_consolidation_loop(
        silva.clone(),
        agent_profiles.clone(),
        curriculum.clone(),
        server_arc.clone(),
        data_dir.to_path_buf(),
        matcher.clone(),
        config.clone(),
    ));
```

Then update every test call site inside `crates/tylluan-kernel/src/memory/night/mod.rs`'s test module that currently calls `PhaseOrchestrator::new(phases)` (the earlier read found these at approximately lines 198, 236, 245, 268, 538) to `PhaseOrchestrator::new(phases, None)` — `None` preserves each existing test's original uncapped behavior exactly.

- [ ] **Step 6: Run the new test and the full suite**

Run: `cargo test -p tylluan-kernel --lib run_all_respects_configured_parallelism_cap -- --nocapture`
Expected: PASS.

Run: `cargo test -p tylluan-kernel --lib`
Expected: PASS, baseline + 6 (5 from Tasks 1-2 + 1 from this task).

- [ ] **Step 7: Clippy**

Run: `cargo clippy -p tylluan-kernel --lib -- -D warnings`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add crates/tylluan-kernel/src/config.rs crates/tylluan-kernel/src/memory/night/mod.rs crates/tylluan-kernel/src/main.rs
git commit -m "feat(night): add [night] max_parallel_phases config cap

PhaseOrchestrator::run_all() sized its semaphore off
available_parallelism() with no operator override -- on a many-core
machine, every 30-minute NightConsolidation cycle could race up to a
dozen-plus phases at once. Combined with the flag_contradiction_nodes
false-positive (fixed in an earlier commit), this produced sustained
~2000% CPU bursts.

[night] max_parallel_phases (default None, unchanged behavior) lets an
operator cap concurrent phases on a shared or latency-sensitive
machine. [night] interval_secs (default 1800, matching the previous
hardcoded value) makes the cycle cadence configurable too.

## Impact
Touches crates/tylluan-kernel/src/config.rs, memory/night/mod.rs
(PhaseOrchestrator::new signature change -- all call sites updated in
this commit), and main.rs's boot sequence. Default config produces
byte-identical behavior to before this commit; only an explicit
[night] section in tylluan.toml changes anything."
```

---

## Final Verification

After all three tasks:

- [ ] Run `bash scripts/verify.sh --rust` (the repo's own gate, per CLAUDE.md — do not hand-pick a narrower command)
- [ ] Confirm the new test count matches: baseline + 6 (Task 1 = 3 tests, Task 2 = 2 tests, Task 3 = 1 test)
- [ ] Update `STATUS.md`/`README.md` test count via `scripts/check_test_count.sh --fix` if the gate flags drift
- [ ] Do NOT restart `tylluan-nexus.exe` — José has explicitly said the kernel stays up until the team has everything resolved; report completion and let José decide when to rebuild/restart.
