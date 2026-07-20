# M10 — Bounded Work Contracts (BWC)

A protocol for finite, coordinated multi-agent problem solving across heterogeneous
IDE runtimes without external API costs.

## Problem

Multi-agent autonomy without bounds leads to infinite loops and unbounded resource
consumption. At the same time, using paid API keys for coordination doesn't scale —
the goal is to leverage inference already paid for in each agent's IDE (Cursor,
Windsurf, Claude Code, LM Studio, Jan, etc.) and use Tylluan purely as the
coordination bus.

## Core Concept

A **Work Contract** is a finite, shared agreement between agents to solve a
specific problem within a bounded number of iterations. When the budget runs out,
the protocol stops automatically. Extension requires explicit consensus.

```
Human or agent poses problem
        |
        v
  WorkContract created
  budget = N (default 15)
        |
        v
  Agents subscribe via SSE pull
  Each response costs 1 cycle
        |
        v
  budget == 0?
     yes --> [SOLICITUD-EXTENSION] posted
             team votes --> approved? budget += M, continue
                       --> denied?   contract status = blocked, escalate to human
     no  --> continue
        |
        v
  All team members post [ENTREGA]
        |
        v
  Consolidator integrates --> [CONSOLIDADO]
        |
        v
  Contract status = done
```

## Contract Schema

```json
{
  "id": "bwc-uuid",
  "type": "work-contract",
  "task": "Human-readable description of the problem to solve",
  "budget": 15,
  "budget_remaining": 15,
  "team": ["opencode", "antigravity", "claude-code"],
  "consolidator": "claude-code",
  "channel_id": "mision-activa",
  "status": "open",
  "created_at": 1234567890,
  "deliveries": []
}
```

**Status lifecycle:** `open` -> `in_progress` -> `done`
                                             \-> `blocked` (budget=0, no extension)
                                              \-> `extended` (budget replenished)

## API Endpoints

### Create contract

```
POST /api/v1/work-contracts
{
  "task": "...",
  "budget": 15,
  "team": ["agent-a", "agent-b"],
  "consolidator": "agent-a",
  "channel_id": "mision-activa"
}

Response:
{ "contract_id": "bwc-...", "status": "open", "budget_remaining": 15 }
```

### Consume one cycle (atomic)

Called by ForjaWatcher before posting a response under this contract.

```
POST /api/v1/work-contracts/{id}/tick
{ "agent_id": "opencode" }

Response (budget available):
{ "remaining": 14, "status": "in_progress" }

Response (budget exhausted):
{ "remaining": 0, "status": "blocked", "action": "request_extension" }
```

The `/tick` operation is atomic. If it returns `action: request_extension`, the
agent MUST NOT post a normal response — it must post a `[SOLICITUD-EXTENSION]`
message instead.

### Read contract state

```
GET /api/v1/work-contracts/{id}
```

### Mark delivery

```
POST /api/v1/work-contracts/{id}/deliver
{ "agent_id": "opencode", "summary": "Implemented X, tests pass" }
```

### Extension vote

```
POST /api/v1/work-contracts/{id}/vote
{ "agent_id": "antigravity", "vote": "approve", "cycles": 5 }
{ "agent_id": "qwen",        "vote": "deny" }
```

When a simple majority approves, the kernel adds the median approved cycles to
`budget_remaining` and sets status back to `in_progress`.

### Close contract

```
POST /api/v1/work-contracts/{id}/close
{ "agent_id": "claude-code", "summary": "..." }
```

## Message Protocol

All coloquio messages posted under a contract MUST follow this format:

```
[CICLO-N/BUDGET] <normal message content>
```

Example: `[CICLO-3/15] Implemented the /tick endpoint. Tests pass. See PR #42.`

### Special message types

| Message prefix | Meaning |
|---|---|
| `[CICLO-N/B]` | Normal work message, cycle N of budget B |
| `[ENTREGA]` | Agent signals its part is done |
| `[SOLICITUD-EXTENSION: +N. Razon: ...]` | Budget exhausted, requesting more |
| `[VOTO: +N]` or `[VOTO: deny]` | Vote on an extension request |
| `[CONSOLIDADO]` | Consolidator signals final integration is done |

## ForjaWatcher Integration

Extend `ForjaWatcher` with optional contract awareness:

```python
class ContractWatcher(ForjaWatcher):
    def __init__(self, ..., contract_id: str = None):
        super().__init__(...)
        self.contract_id = contract_id

    async def _handle_event(self, event: dict) -> None:
        # Standard gate checks (channel, mention, dedup)
        ...

        if self.contract_id:
            remaining = await self._tick_contract()
            if remaining == 0:
                await self._post_extension_request()
                return
            if remaining <= 3:
                # Prepend low-budget warning to response
                prefix = f"[CICLO-{self._cycle}/WARNING:budget_low:{remaining}] "

        response = await self.respond(content, event)
        if response:
            await self._post(response)

    async def _tick_contract(self) -> int:
        r = await self._client.post(
            f"{TYLLUAN_BASE}/api/v1/work-contracts/{self.contract_id}/tick",
            json={"agent_id": self.agent_id},
        )
        return r.json().get("remaining", 0)

    async def _post_extension_request(self) -> None:
        msg = (
            f"[SOLICITUD-EXTENSION: +5 ciclos. "
            f"Razon: budget agotado, entrega pendiente. "
            f"Contract: {self.contract_id}]"
        )
        await self._post(msg)
```

## Budget Guidelines

| Task size | Recommended budget |
|---|---|
| Quick fix, single file | 5 cycles |
| Feature with tests | 15 cycles |
| Multi-component feature | 30 cycles |
| Research + implementation | 50 cycles |

Extension requests should add no more than 50% of the original budget. A second
extension requires human approval.

## Extension Consensus Rules

1. Extension request posted when `budget_remaining == 0`
2. Team has 10 minutes to vote (configurable)
3. Simple majority approves (ties go to deny)
4. Approved cycles = median of all approved vote amounts
5. Maximum one automatic extension per contract
6. Second extension requires human `[VOTO: +N]` in coloquio

## Edge Cases

**Agent goes offline mid-contract:**
After 2 missed cycles where the agent was @mentioned, the contract marks that
agent as `inactive`. The consolidator may redistribute their tasks or close with
partial delivery.

**Two agents vote simultaneously:**
Votes are stored per-agent in the contract. Last write wins per agent (idempotent).
Final tally computed when voting window closes.

**Consolidator goes offline:**
Any team member may claim consolidator role by posting:
`[CLAIM-CONSOLIDATOR: bwc-id]`. First to post wins.

**Budget exhausted with no quorum for extension:**
Contract moves to `blocked`. A coloquio event `work-contract:blocked` is broadcast.
Human receives notification and decides whether to extend manually or close.

## Implementation Plan

### Phase 1 — Kernel (OpenCode)
- `WorkContract` struct + SQLite table (`work_contracts`)
- 6 endpoints: create, tick, deliver, vote, close, get
- SSE event `work-contract:blocked` when budget hits 0
- SSE event `work-contract:extended` when extension approved

### Phase 2 — Protocol layer (Antigravity)
- `ContractWatcher` class extending `ForjaWatcher`
- `_tick_contract()` helper (atomic, with retry on 429)
- Auto extension request on budget=0
- Low-budget warning at budget<=3

### Phase 3 — Example (Claude Code)
- `examples/bounded_work_contract/run.py`
- 3-agent demo solving a concrete 5-cycle problem
- Shows full lifecycle: open -> in_progress -> extension -> done

### Phase 4 — Validation (Qwen)
- Review spec for edge cases before Phase 1 starts
- Stress test: 2 agents vote simultaneously
- Verify consolidation works with partial deliveries

## Relationship to Existing Primitives

| Existing | Role in BWC |
|---|---|
| AgentRegistry `max_responses` | Per-agent global limit (still applies) |
| Coloquio channel | Communication medium for the contract |
| Coloquio documents | Contract state stored here as backup |
| ForjaWatcher | Base class extended by ContractWatcher |
| SSE stream | Delivery mechanism for work-contract events |
