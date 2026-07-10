# FSRS Integration Design

## Status: Active (v0.13.0)

## Scope
Integration of FSRS-5 (Free Spaced Repetition Scheduler) into SilvaDB's memory decay model, replacing the fixed half-life (`weight *= 0.5^(t/14d)`) with per-memory stability.

## Core Formula

Retrievability (probability of recall at time t since last review):

    R(t) = 2^(-t / S)

Where `S` = stability of the memory (in days). This replaces:

    weight = weight * 0.5^(hours / half_life)

with a biologically grounded model where each node has its own half-life = stability.

## Node-level FSRS fields

| Column | Type | Default | Description |
|--------|------|---------|-------------|
| `fsrs_stability` | REAL | 14.0 | Memory stability in days (T½) |
| `fsrs_difficulty` | REAL | 0.3 | Item difficulty (FSRS-5 D, 0..1) |
| `fsrs_last_review` | INTEGER | 0 | Unix timestamp of last FSRS review |

## Review semantics

Every access to a node (`touch_node`) triggers a review with `Rating::Good`.
This boosts stability proportionally to the time elapsed since the last review:
a node accessed daily stays at low stability; a node accessed after 30 days
gets a large stability boost (FSRS-5 spacing effect).

**v2 option** — `touch_node_with_rating()` accepts an explicit `Rating`.
Future callers deriving rating from retrieval score (cf. HippoRAG, paper 1.5):

| Retrieval score | Rating |
|----------------|--------|
| Top-1, score > 0.9 | Easy |
| Top-3 | Good |
| Top-5 or low score | Hard |
| Not retrieved at all | Again |

Not implemented yet — requires production data to calibrate thresholds.

## Stability erosion (separate mechanism)

Stability erosion reduces `fsrs_stability` for nodes unaccessed for >60 days.
It is a separate call (`apply_stability_erosion(factor)`) from `apply_decay()`.

Erosion formula: `S' = S * (1 - factor)^(elapsed_days / 30)`

| Parameter | Default | Rationale |
|-----------|---------|-----------|
| `factor` | 0.01 (1% per 30 days) | ~1/70th of retrievability decay rate |
| Threshold | 60 days without access | Below 60d the FSRS review handles it |
| Floor | `max(S', 1.0)` | Prevents structural collapse |

This is a deliberate deviation from standard FSRS (where stability only changes
on review). Rationale: in a cognitive memory store with zero human ratings,
totally abandoned memories (>60d without any access) should structurally weaken.
Without erosion, a node touched 10 times in its first week would have stability
>100 days and virtually never decay again — even if never accessed for years.

## Federation semantics (decision, not yet implemented)

**Decision: FSRS parameters are PER-PEER, NOT synced.**

When Peer A sends a memory to Peer B:
- The *content* (text, metadata, embeddings) is synced
- `fsrs_stability`, `fsrs_difficulty`, `fsrs_last_review` are NOT synced
- Peer B initializes FSRS fields to defaults (stability=14, difficulty=0.3)

Rationale:
1. FSRS parameters are local — they reflect a specific peer's access patterns
2. Peer B's model of importance is independent of Peer A's
3. Aligned with "Don't Ask the LLM" deterministic resolution (paper 1.3):
   each peer's memory model is sovereign

This decision is documented here so the federation module doesn't need to
re-derive it. See also: `docs/architecture/SECURITY_FEDERATION.md`.

## Migration from v12

Schema version v13 adds FSRS columns to the `nodes` table:
- Existing DBs: ALTER TABLE adds columns with defaults
- New DBs: CREATE TABLE includes columns

Migration is zero-regression: old nodes start with `stability=14` (equivalent
to the old 14-day half-life) and `last_review=0` (treated as "not yet reviewed
in FSRS" — `apply_decay` skips decay for nodes that haven't had their first
FSRS review). The first access to each node triggers an FSRS review and sets
`last_review`.

## Configurability

| Parameter | Location | Default |
|-----------|----------|---------|
| Stability floor | `decay.rs` / `tylluan_fsrs::MIN_STABILITY_DAYS` | 0.05 days |
| Erosion factor | `apply_stability_erosion(factor)` | 0.01 |
| Erosion threshold | `decay.rs` hardcoded | 60 days |
| Default stability | schema.sql `/ FSRS_DESIGN.md` | 14.0 days |
| Default difficulty | schema.sql | 0.3 |
