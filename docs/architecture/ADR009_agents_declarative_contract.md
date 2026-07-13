# ADR-009 — M19-P5: AGENTS.md as a Declarative Agent Contract

**Status:** Proposed
**Date:** 2026-07-13
**Authors:** Tech Lead (Claude)
**Depends on:** Existing `AclConfig` (`config.rs`, roles/tokens/default_role), `resolve_acl_role()` / `acl_can_access()` (`transport/http/auth.rs`)
**Implements:** M19-P5 in `docs/roadmap/ROADMAP_O3.md`

---

## Context

Tylluan already has a working ACL system: `tylluan.toml`'s `[security.acl]` maps **bearer tokens → roles**, and roles map to allowed guilds (`AclConfig.roles: HashMap<String, Vec<String>>`, wildcard `"*"` for admin). This is operator-controlled — a human edits `tylluan.toml` to say "this token gets role X."

What's missing: a way for an **individual agent** (Claude Code, OpenCode, a CI bot, a future autonomous agent) to declare its own identity and intended permission profile **in the repo it's working in**, so the kernel can recognize "this agent is DeepSeek-OpenCode, it should default to role `contributor`" without an operator manually wiring a token-to-role mapping for every agent ahead of time. `AGENTS.md` already exists as a human-readable convention (agent instructions, fleet roster) but is markdown prose — not something the kernel parses or enforces.

## Non-goals

- This is **not** a replacement for the token-based ACL. Tokens remain the actual authentication mechanism (`Authorization: Bearer ...`). This ADR adds an **agent_id → default role** lookup that applies only when a request's token maps to `default_role` (i.e., an unrecognized/generic token) and the caller supplies an `agent_id` — it never overrides an explicit, already-configured token-to-role mapping.
- This does **not** make markdown machine-parsed. The contract is a separate, structured sibling file. `AGENTS.md` stays human prose; a new `.tylluan/agents.toml` is what the kernel reads.
- No enforcement changes to `acl_can_access()` — this ADR only adds a new *source* of role assignment, reusing the existing role-to-guild-list resolution unchanged.

## Design

### 1. New file: `.tylluan/agents.toml` (repo-local, not `tylluan.toml`)

```toml
# .tylluan/agents.toml — declarative agent profiles for this repo.
# Committed to the repo (unlike tylluan.toml's [security.acl] tokens,
# which are operator/secret config and stay out of version control).

[agents.claude-code]
role = "admin"
description = "Tech lead — orchestration, planning, cross-cutting fixes"

[agents.deepseek-opencode]
role = "contributor"
description = "Rust/CLI implementation — bounded, briefed tasks"

[agents.antigravity]
role = "contributor"
description = "Dashboard/UI — browser-based, MCP client"

[agents.ci-bot]
role = "readonly"
description = "Automated CI checks, no write operations"
```

`role` values reference role names already defined in `tylluan.toml`'s `[security.acl.roles]` table (e.g. `contributor = ["bash", "filesystem", "git"]`) — this file assigns agents to *existing* roles, it does not define new ones. If a `role` here has no matching entry in `[security.acl.roles]`, the kernel logs a warning at startup and falls back to `default_role` for that agent (fails safe, never silently grants more access than the fallback).

### 2. Kernel loading

- New `AgentsContract` struct in `config.rs`, loaded once at startup from `.tylluan/agents.toml` (missing file → empty map, not an error — this feature is fully optional).
- Stored in `HttpState` alongside the existing `config: Arc<RwLock<TylluanConfig>>` (not merged into `TylluanConfig` itself, since this is repo-local declarative data, not runtime-mutable operator config).

### 3. Resolution order in `bearer_auth_middleware` (`transport/http/auth.rs`)

Current: `resolve_acl_role(token) -> acl.tokens.get(token).cloned().unwrap_or(default_role)`.

New: if the resolved role **is** `default_role` (i.e. the token wasn't explicitly mapped) **and** the request carries an `agent_id` (header or query param, same extraction already used for `agent_rate_limiter`), look up `agents_contract.get(agent_id)` and use its role instead — but only if that role is a valid, defined role in `acl.roles`. An explicitly-mapped token always wins; this only fills the gap for generic/dev-mode tokens.

```
resolve_role(token, agent_id, acl, contract):
    if let Some(role) = acl.tokens.get(token):
        return role                          # explicit token mapping always wins
    if let Some(agent_id) = agent_id:
        if let Some(profile) = contract.get(agent_id):
            if acl.roles.contains_key(profile.role):
                return profile.role          # agent's declared role, if valid
    return acl.default_role                  # unchanged fallback
```

### 4. Why not put this in `tylluan.toml` directly

`tylluan.toml` mixes secrets-adjacent operator config (`.tylluan-token`, `[security.acl.tokens]`) with runtime settings, and is often gitignored or has a `.example.toml` template precisely because parts of it shouldn't be committed verbatim. Agent role *declarations* are the opposite: they're meant to be committed, reviewed in PRs, and visible to every contributor — exactly like `AGENTS.md` and `CONTRIBUTING.md` already are. A separate file keeps that boundary clean instead of asking operators to remember which half of `tylluan.toml` is safe to commit.

## Consequences

- **Positive:** a new agent (human-onboarded or a fresh AI agent instance) gets a sane default permission profile by declaring itself in a repo file, reviewable in a PR — no operator has to pre-provision a token-to-role mapping before the agent's first request.
- **Positive:** fully backward compatible — no `.tylluan/agents.toml` file means zero behavior change from today.
- **Risk:** an `agent_id` is client-supplied (same caveat as the existing `agent_rate_limiter` — see `transport/http/auth.rs`'s comment on client-controlled `X-Agent-Id`). This ADR deliberately does **not** let agent_id claim a role that exceeds `default_role`'s trust boundary in `dev_mode=false` unless the token itself is already valid — it only *narrows or redirects within* what an already-authenticated caller could do, since role resolution happens after bearer-token validation, not instead of it. An attacker who doesn't have a valid token gets no benefit from spoofing `agent_id`.
- **Follow-up not in scope here:** a `tylluan agents` CLI subcommand to list/validate `.tylluan/agents.toml` against the configured roles (nice-to-have, not required for M19-P5 closure).

## Implementation checklist (for M19-P5 kernel work)

- [ ] `AgentsContract` struct + loader in `config.rs` (or a new `agents_contract.rs` module)
- [ ] Wire into `HttpState` construction (`transport/http/mod.rs`)
- [ ] Extend `resolve_acl_role()` in `transport/http/auth.rs` per the resolution order above
- [ ] `.tylluan/agents.toml.example` template (mirrors `tylluan.example.toml`'s pattern)
- [ ] Tests: valid agent role applied, invalid role name falls back safely, explicit token mapping takes precedence over agent_id, missing file is a no-op
- [ ] Document in `AGENTS.md` itself: link to this ADR, note that `.tylluan/agents.toml` is the machine-readable counterpart
