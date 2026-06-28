# Federation v0.3.0 — Spec

## Current State (v0.2.0 baseline)

| Component | Status |
|-----------|--------|
| ChaCha20-Poly1305 encrypt/decrypt | ✅ Done (`federation/mod.rs`) |
| Peer add/remove/approve (HTTP) | ✅ Done (`api_federation.rs`) |
| Push sync (manual, one-shot) | ✅ Done |
| mDNS advertiser + discovery | ✅ Done (not wired at startup) |
| Peer persistence (DB) | ❌ Missing — peers live in `tylluan.toml` |
| Pull sync | ❌ Missing |
| Node provenance tracking | ❌ Missing |
| Scheduled auto-sync | ❌ Missing |

## What v0.3.0 Is NOT

The ROADMAP lists NAT traversal (Libp2p/WireGuard), Ed25519 asymmetric signing, and DHT peer discovery. These are v0.4.0. v0.3.0 makes federation **work reliably** on LAN/VPN. v0.4.0 makes it work on the internet without a shared secret.

---

## M11-A: Peer Persistence (DB) — Break TOML dependency

**Problem:** Peers added at runtime are stored in `tylluan.toml` via `persist_federation_peers()`. This races with manual edits and creates merge conflicts. Peers are also lost if the file is reset.

**Solution:** SQLite `data/peers.db`. TOML `[federation.peers]` becomes read-only seed — loaded once at first boot if DB is empty.

### Schema

```sql
CREATE TABLE federation_peers (
    name          TEXT PRIMARY KEY,
    url           TEXT NOT NULL,
    auth_token    TEXT NOT NULL,   -- HTTP bearer token for talking TO this peer
    shared_secret TEXT NOT NULL,   -- ChaCha20 key (separate from auth)
    approved      INTEGER NOT NULL DEFAULT 0,
    last_sync     INTEGER,
    added_at      INTEGER NOT NULL
);
```

**Security fix:** Current design uses `peer.token` for both bearer auth AND ChaCha20 key. This is wrong — one secret for two purposes means rotating the auth token breaks encryption. `auth_token` and `shared_secret` are now separate fields. `AddFederationPeerRequest` gains `shared_secret: Option<String>` (defaults to auth_token if omitted, for backwards compat).

### Bootstrap

On startup in `HttpState::new()`:
1. Open `peers.db`, create schema
2. If DB is empty AND `config.federation_peers` is non-empty → migrate TOML peers into DB
3. Load peers from DB into `config.federation_peers` (replaces TOML load)
4. Stop writing to TOML for peer mutations

### API changes

`GET /api/v1/federation/peers` — add `approved`, `added_at`, `shared_secret_set: bool` to response.

---

## M11-B: Pull Sync — Bidirectional federation

**Problem:** Only push exists. Peer A can push to Peer B, but Peer B cannot pull from Peer A. A new peer joining the network needs to explicitly push to everyone.

### New endpoints

```
GET  /api/v1/federation/sync/export           → returns shareable nodes (encrypted for caller)
POST /api/v1/federation/sync/pull?peer={name} → fetches from named peer's /export
```

**`/sync/export` (receive side):**
- Auth: bearer token matching an approved peer's `auth_token`
- Returns: ChaCha20-encrypted JSON array of shareable nodes (same format as push)
- Response: `application/octet-stream`

**`/sync/pull` (initiator side):**
- Looks up peer by name in DB
- `GET {peer.url}/api/v1/federation/sync/export` with bearer = `peer.auth_token`
- Decrypts with `peer.shared_secret`
- Upserts received nodes into local SilvaDB (skip protected)
- Updates `last_sync` in DB

### Sync modes

| Mode | Who initiates | When to use |
|------|--------------|-------------|
| Push | Local | "I have new knowledge, tell peers" |
| Pull | Local | "I want what peers know" |
| Both | Local | Full bidirectional (push then pull) |

New endpoint: `POST /api/v1/federation/sync/both?peer={name}` — push then pull in sequence.

---

## M11-C: Node Provenance

**Problem:** After receiving nodes from a peer, there is no record of origin. Local and federated nodes are indistinguishable. Debugging, auditing, and selective purging are impossible.

### Schema change (SilvaDB)

```sql
ALTER TABLE silva_nodes ADD COLUMN federation_source TEXT;
-- NULL = local, "peer-name" = received from federation
```

Migration: `ALTER TABLE silva_nodes ADD COLUMN federation_source TEXT;` — safe, NULL default for all existing rows.

### Behavior

- `federation_sync_receive` / `pull`: set `federation_source = peer.name` on each upserted node
- `get_shareable_nodes()`: exclude nodes where `federation_source IS NOT NULL` by default — don't re-federate received nodes (prevents echo loops)
- New query param: `GET /api/v1/federation/sync/export?include_received=true` — opt-in to relay

### New endpoint

```
GET /api/v1/federation/nodes?source={peer_name}  → list nodes received from a peer
GET /api/v1/federation/nodes?source=local        → list local-origin nodes only
```

---

## M11-D: Scheduled Auto-Sync

**Problem:** Sync is entirely manual. Peers drift apart as soon as new knowledge is added locally without triggering a push.

### Config

```toml
[federation]
auto_sync_interval_secs = 3600  # 0 = disabled
auto_sync_mode = "push"         # "push" | "pull" | "both"
```

### Implementation

Background `tokio::spawn` in `HttpState::new()` (after peers are loaded):

```rust
if interval_secs > 0 {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            // call federation_sync_push logic inline
        }
    });
}
```

Dashboard: add `next_sync: Option<u64>` to `GET /api/v1/federation/peers` response.

---

## M11-E: Integration Tests

New file: `crates/tylluan-kernel/tests/federation_audit.rs`

| Test | What it checks |
|------|---------------|
| `test_encrypt_decrypt_roundtrip` | Already exists in `federation/mod.rs` |
| `test_peer_db_roundtrip` | Add peer to PeerDb, reload, verify fields |
| `test_peer_approval_required` | Unapproved peer blocked from push/pull |
| `test_shared_secret_separate_from_auth` | encrypt uses shared_secret, not auth_token |
| `test_provenance_tagged_on_receive` | Received nodes have `federation_source` set |
| `test_no_echo_loop` | Received nodes excluded from export by default |
| `test_auto_sync_config_zero_disables` | interval=0 → no background task spawned |

Add to CI `e2e-security-tests` job:

```yaml
cargo test -p tylluan-kernel --test federation_audit
```

---

## Implementation Order

| Milestone | Assignee | Blocker | Est. |
|-----------|----------|---------|------|
| M11-A Peer DB | Claude Code | — | 1 session |
| M11-B Pull sync | OpenCode | M11-A (needs peer DB to find shared_secret) | 1 session |
| M11-C Provenance | Claude Code | M11-B (can run parallel with A) | 1 session |
| M11-D Auto-sync | OpenCode | M11-A | 1 session |
| M11-E Tests | All | M11-A through D | 1 session |

M11-A and M11-C can run in parallel. M11-B and M11-D both depend on M11-A.

---

## Out of Scope (v0.3.0)

- **NAT traversal** (Libp2p/WireGuard) — requires external network infra
- **Ed25519 node signing** — good for provenance authenticity, but requires key exchange UI
- **DHT peer discovery** — overkill until there are 3+ real users
- **Conflict resolution beyond last-write-wins** — CRDT is complex; last-write-wins is safe for knowledge nodes that don't mutate frequently
- **mDNS wiring at startup** — already implemented; decision to wire it up pending security review of auto-discovery

---

## Security Invariants (unchanged from v0.2.0)

- `approved = false` peers are never synced to or from
- Approval requires human operator action (explicit POST to `/approve`)
- mDNS auto-discovered peers start as `approved = false`
- Protected nodes (`node.protected = true`) are never exported
- `host = "0.0.0.0"` + `dev_mode = true` is still blocked by `security/guard.rs`
