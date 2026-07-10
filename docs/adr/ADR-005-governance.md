# ADR-005: Project Governance and Continuity

**Status:** Adopted 2026-07-10
**Author:** Forja-orca

## Context

Tylluan is a single-author project. The bus factor is 1 — if the maintainer stops, the project halts. This is a structural risk for a project that promises sovereign memory: users who depend on Tylluan for long-term knowledge persistence need confidence that the project can survive its author.

## Decision

### 1. License as the ultimate continuity mechanism

MIT license means any user can fork at any time. No CLA or copyright assignment is required — the repo is fork-friendly by design. If the maintainer disappears, the community can continue development under any governance model they choose.

### 2. Architecture Decision Records

All architectural decisions that affect a successor's ability to understand the system are documented as ADRs in `docs/adr/`. This file (ADR-005) is one of them. Future ADRs must cover:

- Why a given architecture was chosen over alternatives
- What was considered and rejected
- What invariants downstream code depends on

ADRs are NOT changelogs — they capture *rationale*, not *history*.

### 3. Single maintainer, open to contribution

The project is currently maintained solely by Forja-orca. Becoming a multi-author project is an explicit goal for v1.0.0, but not a prerequisite — a project with solid ADRs and MIT license can survive a bus factor of 1 long enough to grow.

### 4. No promises that require funding

The project makes no promises that depend on external funding (paid audits, dedicated infrastructure, full-time maintainers) unless a funding source is identified. Items in ROADMAP.md are marked as "stretch" when they require resources the project does not currently have.

## Consequences

- Users evaluating Tylluan for long-term adoption should factor bus factor into their risk assessment
- The MIT license is the backbone of continuity — any user can fork and continue
- ADRs make the project navigable by a successor who did not write the code

## Related

- ROADMAP.md — v1.0.0 goals and stretch items
- SPEC.md — project scope and audience definitions
