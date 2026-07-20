# Federation Threat Model

> **Status:** Draft — reflects current implementation as of v0.13.0.

## Scope

This document covers the threat model for Tylluan's P2P federation layer: peer discovery (DHT Kademlia + mDNS), knowledge sync (push/pull/auto-sync), and remote guild dispatch (Noise NK). It does not cover local attack surface (see [SECURITY.md](SECURITY.md)).

## Trust Model

Tylluan uses an **approval-gate trust model**, not Byzantine fault tolerance:

1. Peers are not discovered automatically — they must be explicitly approved via `POST /api/v1/federation/peers` with a valid `auth_token` and `shared_secret`
2. Once approved, a peer can push knowledge to your node and pull knowledge from it
3. There is no byzantine fault tolerance — an approved peer that turns malicious can inject false memories with valid provenance

This is a deliberate tradeoff: BFT would add significant complexity and latency for a use case (local knowledge mesh) where peer approval is already a manual/trusted operation.

## Threats

### T1: Malicious peer injects false memories

A compromised or malicious approved peer can push fabricated knowledge nodes that appear with valid `federation_source` provenance. The receiving node has no mechanism to distinguish true from false content.

**Mitigations (current):**
- Peer must be explicitly approved (no auto-join)
- Sync is push/pull with `auth_token` and `shared_secret` — impersonation requires compromising the secret
- Protected nodes (`set_protected = true`) are never exported
- Echo-loop prevention: received nodes carry `federation_source` and are excluded from outbound sync by default

**Gaps (not implemented):**
- No content-level trust scoring (e.g., reputation per peer)
- No contradiction detection across peers (a peer could push a node that contradicts an existing one)
- No revocation mechanism — once a node is synced, removing it from the mesh requires manual cleanup

### T2: Provenance forging

A peer could claim authorship of nodes it did not create. The `federation_source` column is set by the receiving node based on which peer sent the data, not by cryptographic attestation.

**Mitigations (current):**
- Ed25519 signatures exist at the identity level (`identity.key`)
- Inbound sync verifies the peer's Ed25519 signature on the sync request itself

**Gaps:**
- Individual nodes are NOT signed by the originating peer — the sync message is signed, but once stored, a node's `federation_source` is a database column, not a cryptographic claim

### T3: Echo loop with data corruption

If two peers have the same `shared_secret` (misconfiguration) and both have auto-sync enabled, a node could theoretically cycle between them indefinitely.

**Mitigations:**
- Echo-loop prevention at SQL level: `get_shareable_nodes()` filters `federation_source IS NULL`
- Node timestamps cap re-propagation: a node's `created_at` is set on first receipt and never updated on re-sync

### T4: DHT poisoning / Sybil attack

The Kademlia DHT is currently used for WAN peer discovery only — it does NOT store knowledge content. A Sybil attacker could inject fake peer entries, but those peers still need explicit approval before syncing.

**Mitigations:**
- DHT stores node IDs and addresses only, not knowledge
- Peer approval gate applies regardless of how the peer was discovered (DHT, mDNS, or manual)
- Ed25519 identity prevents node ID spoofing

### T5: Network-level attacker (MITM)

Sync traffic travels over Noise XK (TCP) or Noise NK (HTTP). Both use ChaCha20-Poly1305 AEAD with per-session ephemeral keys derived from the handshake.

**Mitigations:**
- Noise XK provides mutual authentication and forward secrecy
- Noise NK provides one-way authentication (client knows server's static key)
- All payloads are encrypted and authenticated before hitting the wire

## Known Limitations

| Area | Status | Timeline |
|------|--------|----------|
| Per-node signatures (cryptographic provenance) | Not implemented | Post-v1.0.0 |
| Contradiction detection across peers | Not implemented | Post-v1.0.0 |
| Reputation scoring | Not implemented | Post-v1.0.0 |
| Revocation / recall of synced nodes | Manual only | Post-v1.0.0 |
| Byzantine fault tolerance | Not implemented | No current plan |

## Related

- [SPEC.md](../../SPEC.md) — project scope
- [docs/concepts/FEDERATION_V3.md](FEDERATION_V3.md) — federation protocol spec
- [docs/concepts/SECURITY.md](SECURITY.md) — general security model
