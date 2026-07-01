# ADR-004 — M14-D: Guild Execution Channels

**Status:** Proposed  
**Date:** 2026-07-01  
**Authors:** Qwen (research), Tech Lead (synthesis)  
**Depends on:** M14-C Noise NK (implemented), M14-B Gossip (implemented), M14-A DHT (implemented)

---

## Context

Tylluan nodes form a P2P mesh. Each node runs a local subset of guilds (Python FastMCP processes). When a node needs a capability it doesn't have locally — e.g., a Raspberry Pi 4 (4GB RAM, no GPU) needs `vision` or `comfy_ui` — there is currently no mechanism to delegate that tool call to a peer that has the capability.

**Problem:** How does a node decide where to execute a guild tool when peers are heterogeneous (RPi4 4GB vs workstation 32GB+GPU)? How does it dispatch the call and return the result without a central coordinator, without cloud dependencies, and without breaking the 5-tool sovereign CONTRACT-01?

**Constraints:**
- No cloud dependency in the critical path
- Offline-first: must degrade gracefully when no peers available
- CONTRACT-01 inviolable: exactly 5 sovereign tools (`tylluan_do`, `tylluan_remember`, `tylluan_recall`, `tylluan_think`, `tylluan_graph`). M14-D routes **inside** `tylluan_do`, not as a new tool
- CPU-local inference timeouts apply (no GPU assumed on all nodes)
- Must compose with existing Noise XK transport (`MeshTransport` trait)

---

## Alternatives Considered

### Option A — Local-first reactive (rejected)
Always try local first, fall back to any available peer on failure. Simple, but:
- No proactive optimization: a node with GPU never gets used if local guild succeeds (even slowly)
- No latency awareness: a heavily loaded peer may be slower than local
- **Rejected:** doesn't optimize resource utilization

### Option B — Latency-only routing (rejected)
Ping peers, route to lowest-latency responder. Ignores capabilities:
- A fast peer without the required guild still wins the auction
- Would require guild discovery out-of-band
- **Rejected:** latency alone is insufficient

### Option C — User-controlled routing (rejected)
Expose routing preferences via config or MCP args. Shifts cognitive load to user:
- UX is terrible for agents that need transparent execution
- **Rejected:** violates transparency principle

### Option D — Central gateway (rejected)
A designated "orchestrator" node routes all calls:
- Single point of failure
- Violates P2P offline-first design
- **Rejected:** antithetical to sovereignty

### Option E — Capability-Aware + Latency-Based Hybrid Routing (CHOSEN)
Each node publishes its available guilds and hardware capabilities. Routing decisions combine capability match + observed latency + local load. Execution happens peer-to-peer over Noise NK.

---

## Decision: Capability-Aware Hybrid Routing

### Component 1 — Capability Registry (DHT-backed, Gossip-propagated)

Each node publishes a `CapabilityRecord` to the DHT with TTL=5min:

```rust
struct CapabilityRecord {
    node_id: NodeId,
    guilds: Vec<String>,          // ["vision", "comfy_ui", "code"]
    ram_mb: u32,
    has_gpu: bool,
    load_avg: f32,                // 0.0–1.0, updated every 30s
    addr: SocketAddr,
    timestamp: u64,
}
```

The Gossip protocol (M14-B) propagates `CapabilityRecord` updates using the existing `capabilities` field in `GossipEntry`. TTL ensures stale records expire without explicit deletion.

**Catalog sharing decision: Gossip (Option A)**
- GossipEntry already has `capabilities: Vec<String>` field
- No additional round-trip required: capabilities arrive with peer discovery
- DHT provides fallback lookup when gossip hasn't propagated yet

### Component 2 — Dispatch Algorithm

When `tylluan_do` receives a tool call that routes to a guild:

```
1. Check local guild catalog — if available and load_avg < 0.7, execute locally
2. Query CapabilityRegistry for peers that have the guild
3. Score peers: score = (1 - load_avg) * (1 / latency_ms) * capability_match
4. If best_peer_score > local_score * 1.2: dispatch remotely
5. Else: execute locally
```

The 1.2× threshold prevents unnecessary remote dispatch when local is nearly as good.

### Component 3 — Remote Execution Protocol

Wire format over Noise NK (ChaCha20-Poly1305 encrypted, length-prefixed):

```rust
// Request (msgpack or JSON, negotiated at handshake)
struct GuildDispatchRequest {
    request_id: Uuid,
    guild_name: String,
    tool_name: String,
    args: serde_json::Value,
    timeout_ms: u32,
}

// Response
struct GuildDispatchResponse {
    request_id: Uuid,
    result: Option<serde_json::Value>,
    error: Option<String>,
    execution_ms: u32,
}
```

Transport: reuse existing `MeshTransport` trait. The Noise XK session from M14-C provides encryption. Guild dispatch uses a dedicated port or multiplexed stream ID to avoid conflicting with gossip/sync traffic.

**Wire format recommendation:** JSON over Noise (simpler, self-describing). Migrate to msgpack if p99 latency > 50ms in production.

### Component 4 — Fallback Strategy

| Condition | Action |
|-----------|--------|
| Offline (no peers) | Queue request, retry when peer reconnects; return `"queued"` to agent |
| Peer unresponsive (timeout) | Circuit breaker: mark peer degraded for 60s, try next peer |
| No peer has guild | Fail fast with clear error: `"Guild X not available on any connected peer"` |
| Local overloaded + no peers | Execute locally anyway (best-effort) |

Circuit breaker state is in-memory only (resets on restart). Persistent circuit state would require SQLite write on every peer interaction — not worth it at this stage.

### Timeout Values

Following the CLAUDE.md invariant: **never reduce timeouts for CPU-local inference**.

- Guild dispatch request timeout: 180s (same as `heavy_guild_ms`)
- Peer score cache TTL: 30s
- CapabilityRecord DHT TTL: 300s (5min)
- Circuit breaker cooldown: 60s

---

## Implementation Plan

### Phase 1 — Capability Registry (2 sessions)
- Extend `GossipEntry` with `ram_mb`, `has_gpu`, `load_avg` fields
- Add `CapabilityRegistry` struct (in-memory, DHT-backed flush)
- Background task: publish own record every 60s

### Phase 2 — Dispatch Algorithm (2 sessions)
- `DispatchRouter::route(guild_name, args)` → `DispatchDecision::Local | Remote(NodeId)`
- Scoring function with load + latency + capability
- Circuit breaker for degraded peers

### Phase 3 — Remote Execution Protocol (3 sessions)
- `GuildDispatchRequest` / `GuildDispatchResponse` types in `tylluan-common`
- Handler on receiver side: validates guild exists, executes, returns response
- Integration with `MeshTransport` (reuse Noise XK session)

### Phase 4 — Fallback & UX (1 session)
- Queue implementation for offline peers
- Dashboard: show active remote dispatches
- `tylluan_recall` subtool: `guild_peers` → list peers and their available guilds

**Total:** ~8 sessions. Phases 1–2 can start immediately. Phase 3 requires Phase 1 complete.

---

## Impact on CONTRACT-01

CONTRACT-01 (5 sovereign tools) is **preserved**. M14-D routes inside `tylluan_do`'s intent routing:

```
tylluan_do("run vision analysis on image.png")
  → intent matcher → "vision" guild
  → DispatchRouter::route("vision", args)
  → Local or Remote(peer_id)
  → result returned to caller
```

The MCP client sees one tool call, one result. The routing is transparent.

---

## Open Questions

1. **Stream multiplexing:** Should guild dispatch share the Noise XK session used for gossip sync, or open a dedicated channel? Dedicated is simpler to reason about but doubles handshake overhead for new peers.

2. **Result caching:** If two agents request the same guild call within 5s, should the second reuse the first's result? Useful for expensive vision/inference calls, risky for side-effectful tools (bash, filesystem).

3. **Billing / attribution:** In a multi-user scenario, who "pays" for remote compute? Not needed for v1 (all trusted peers) but worth noting for future access control.

4. **Partial failure:** If a remote guild returns a partial result before timeout, should we return partial or fail? FastMCP doesn't have a streaming result primitive today.
