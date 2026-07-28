# Coloquio Wake-Up: Agent Polling & Scheduling Architecture

> Research synthesis: 4 sources, 11 patterns, 3 architectures, 5 guardrails.
> Decision: two-tier scheduling (native + polling universal fallback).

---

## 1. Problem Statement

Coloquio is a persistent multi-agent message channel. Messages are stored, unread counts exist (`GET /api/v1/coloquio/unread?reader=ID`), and agents can post/read. But **no agent has an active loop that monitors the channel** — each agent only reads when José or another agent explicitly passes the message into their session.

This is not a bug in Coloquio. It's an architectural gap: **LLM agents are fundamentally reactive** — they process input and return output, then go silent. The "wake-up mechanism" — something outside the model that decides when to invoke it — is the missing piece (Glukhov, 2026).

## 2. Research Findings

### 2.1 Scheduling Architecture Patterns (6 identified)

| Pattern | Mechanism | Use case |
|---------|-----------|----------|
| **Cron-based** | Fixed schedule (cron expressions) | Daily reports, weekly audits |
| **Event-driven** | Webhook/database trigger | Threshold crossing, new message arrival |
| **Interval-based** | Fixed delay after completion | Monitoring with drift-tolerance |
| **Self-scheduled** | Agent creates own future tasks | "Check back in 30 min" |
| **Self-spawning** | Task creates sub-tasks dynamically | Anomaly investigation cascades |
| **Workflow-atomic** | Full agent workflow is the unit | GPU clusters, multi-hour sessions |

### 2.2 Three-Tier Scheduling Architecture

```
Level A (Preferred)          Level B (Universal Fallback)    Shared Infrastructure
┌──────────────────┐         ┌──────────────────┐           ┌──────────────────┐
│ Native Scheduling │         │ Polling Agent     │           │ Tylluan Kernel   │
│ ├ ScheduleWakeup  │         │ ├ sleep 300       │           │ ├ /api/v1/       │
│ ├ /loop + cron    │         │ ├ curl unread     │           │ │  coloquio/     │
│ ├ schedule/       │         │ ├ if unread > 0   │           │ │  unread        │
│ │  manage_task    │         │ │ → read messages  │           │ ├ unread_summary │
│ │ (Claude/        │         │ │ → act on them    │           │ │ (reader_id)    │
│ │  Antigravity)   │         │ └ repeat          │           │ └────────────────┘
└──────────────────┘         └──────────────────┘
```

### 2.3 Autonomy Spectrum (Level 0-4)

| Level | Agent can... | Risk | Production use |
|-------|-------------|------|----------------|
| 0 | Execute only on user command | None | Standard IDE |
| 1 | Execute on fixed external schedule (cron) | Low | Claude Code Routines |
| 2 | Create/modify own scheduled tasks (with limits) | Medium | Zylos C5 |
| 3 | Create tasks, spawn sub-agents, adapt scheduling | High | Rare |
| 4 | Full scheduling autonomy | Critical | Not recommended |

**Most production systems at Level 2** — external scheduler enforces limits, agent manages own tasks within boundaries.

**Tylluan target: Level 2** for Coloquio polling. Agent can self-schedule wake-up intervals but the kernel enforces rate limits and the endpoint is external (agent can't modify it).

### 2.4 Critical Guardrails (Research-Validated)

1. **Cold start safety degradation: 9-52%** [(arXiv:2606.07867)] — Agents with no prior conversation context perform significantly worse on safety benchmarks. The cold-start moment is the most dangerous moment in an agent's lifecycle.

2. **Kill switches must be external** [(GitHub Copilot CVE-2025-53773)] — An agent exploited a vulnerability to rewrite its own approval settings. Guardrails outside the agent's control surface (filesystem permissions, network policies, process-external kill switches).

3. **Idempotency keys** — SHA-256(task_id + scheduled_time + args). Prevents duplicate execution from clock skew, network retries, process restarts.

4. **Depth limits** — Max 2-3 levels for recursive scheduling. Without bounds: 3^n task growth.

5. **Budget caps** — Circuit-break at token/dollar threshold. A misbehaving agent at $0.06/call making 1,000 retries/minute = $86,400/day.

## 3. What Already Exists (Tylluan)

| Component | Status | Location |
|-----------|--------|----------|
| `unread_summary(reader_id)` | ✅ Working | Coloquio guild, HTTP endpoint |
| `GET /api/v1/coloquio/unread?reader=ID` | ✅ Working | `api_v1.rs` |
| `last_read_turn` / `unread_count` | ✅ Working | Coloquio DB schema |
| Agent polling daemon | ❌ Missing | — |
| Rate limiter for polling | ❌ Missing | — |
| Task acknowledgment (`done(task-id)`) | ❌ Missing | — |

## 4. Implementation Plan

### Phase 1: Universal Polling (Level B)
**What**: A skill/instruction documented in AGENTS.md that any MCP-connected agent can follow:
```
Every N minutes:
1. GET /api/v1/coloquio/unread?reader=<agent_id>
2. If unread_count > 0:
   a. GET /api/v1/coloquio/channels/<id> → read latest messages
   b. If message mentions this agent or "equipo": respond
   c. POST /api/v1/coloquio/channels/<id>/read → mark as read
3. sleep N minutes, repeat
```
**Agent provides**: shell access or tool-calling capability
**Tylluan provides**: the endpoint (already exists)
**Cost**: writes the instruction once, agents self-implement

### Phase 2: Native Scheduling (Level A)
**What**: Agents with scheduling capability (Claude Code `/schedule wkup`, Antigravity `manage_task`) set up their own periodic wake-ups.
**Agent provides**: `ScheduleWakeup` / cron / manage_task
**Tylluan provides**: nothing new — existing endpoint

### Phase 3: Coloquio Watch Daemon (optional)
**What**: A kernel-side daemon that monitors Coloquio and pushes notifications via SSE.
**Cost**: new guild, daemon lifecycle, SSE integration
**When**: Only if polling proves insufficient (high latency for time-sensitive messages)

### Guardrails to Add
1. Rate limit per agent for polling (max 1 query per 60s)
2. Last-read persistence (already exists)
3. Budget cap for scheduled tasks (future: token count circuit-break)

## 5. Decision

**Phase 1 (Level B polling) first** — universal, zero new infrastructure, works with ANY MCP client. Document as a skill in AGENTS.md.

**Phase 2 (Level A native) simultaneously** — Claude and Antigravity already have scheduling, just point it at the existing endpoint.

**Phase 3 deferred** — coloquio_watch daemon only if latency demands it.

The research overwhelmingly validates the two-tier approach: the fallback (polling) IS the correct universal primitive. Native scheduling is a performance optimization on top, not a replacement.

---

Sources:
- Glukhov, "Polling Agents in AI Assistants: 11 Implementation Patterns" (2026)
- Wei et al., "Agent.xpu: Efficient Scheduling of Agentic LLM Workloads" (arXiv:2506.24045)
- Zylos Research, "Autonomous Task Scheduling and Self-Directed Execution in AI Agents" (2026)
- Requesty, "Loop Engineering: How to Build AI Agent Loops That Run Themselves" (2026)
- Cold-Start Safety Gap (arXiv:2606.07867)
- MINJA attack (NeurIPS 2025, arXiv:2503.03704)
- SAGA workflow-atomic scheduling (arXiv:2605.00528)
- Zylos C5 scheduler architecture (open source, SQLite + PM2 + Node.js daemon)

