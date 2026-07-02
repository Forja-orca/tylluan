# ADR-005: M14-F — Real P2P Guild Dispatch over Noise XK

## Status
**Proposed** — 2026-07-02

## Context

### El problema
M14-D ([ADR-004](./M14D_dispatch_spec.md)) implementó dispatch HTTP vía Noise NK (stateless, 1-message handshake). Cada request encripta/desencripta independientemente. Esto funciona para federación HTTP ocasional, pero es ineficiente para comunicación frecuente entre dos peers:

1. **Handshake repetido:** NK genera ephemeral key por request — overhead criptográfico en cada llamada.
2. **Sin sesión permanente:** No hay detección de conexión caída, heartbeat, ni reuso de canal.
3. **Latencia alta:** Cada dispatch HTTP requiere TCP connect + NK encrypt + HTTP parse + NK decrypt.

M14-F extiende el dispatch para operar sobre **sesiones TCP persistentes** usando Noise XK (3-message handshake, bidireccional, handshake amortizado).

### Estado actual (v0.11.0-dev)
- ✅ Noise XK handshake: `noise_accept` / `noise_connect` en `crates/tylluan-link/src/noise.rs`
- ✅ Noise XK transporte: `NoisedPipe.session` (TransportState) con `async_encrypt_write` / `async_decrypt_read`
- ✅ GossipEngine sync sobre MeshTransport trait
- ✅ DispatchRequest/Response estructuras + Noise NK encrypt (dispatch.rs:49-103)
- ❌ No hay sesión TCP persistente entre peers
- ❌ No hay pool de conexiones reutilizables
- ❌ No hay forma de saber la dirección TCP de un peer (solo node_id)

## Design Decisions

### 1. Signatura de `execute_remote_tcp()`

```rust
pub async fn execute_remote_tcp(
    request: GuildDispatchRequest,
    peer_addr: SocketAddr,
    peer_pubkey_hex: &str,
    identity: &NodeIdentity,
) -> Result<GuildDispatchResponse, DispatchError>
```

**Flujo de bytes sobre XK:**

```
Initiator (caller)                     Responder (peer)
    │                                         │
    ├── TCP connect ──────────────────────►   │
    │                                         │
    │   ╔═══════════════════════════╗          │
    │   ║ Noise XK Handshake (3 msg)║          │
    │   ║ →e, es                    ║          │
    │   ║ ←e, ee                    ║          │
    │   ║ →s, se                    ║          │
    │   ╚═══════════════════════════╝          │
    │                                         │
    │   ╔═══════════════════════════╗          │
    │   ║ Transport mode            ║          │
    │   ║ [len:2][encrypted request]║──────►   │
    │   ║                          ║          │
    │   ║ [len:2][encrypted resp]  ║◄─────────│
    │   ╚═══════════════════════════╝          │
    │                                         │
    │   TCP close / return to pool             │
```

Cada payload usa length-prefixed framing (u16 big-endian + encrypted bytes), idéntico al patrón existente en `noise.rs:206-221`.

**Response timeouts:** El timeout del request se pasa desde `GuildDispatchRequest.timeout_secs`. Si expira, la sesión se cierra y se reporta `DispatchError::Timeout`.

### 2. Session Pool vs Open/Close por Request

**Decisión: Session Pool con keep-alive (Opción A para v0.11.0)**

| Aspecto | Open/Close por Request | Session Pool |
|---------|----------------------|--------------|
| Handshake | 3 msg por llamada | 3 msg solo la primera vez |
| Memoria | 0 state | N sockets, N TransportState |
| Complejidad | Baja | Media |
| Latencia p50 (estimada) | ~15ms (handshake) + payload | ~2ms (payload only) |

Para el caso de uso médico (llamadas frecuentes a la workstation), el pool es necesario. Propuesta:

```rust
pub struct P2pSessionPool {
    pool: HashMap<String, Vec<PooledSession>>,  // peer_pubkey → [sessions]
    max_per_peer: usize,         // default 4
    keepalive_secs: u64,         // default 30
}

struct PooledSession {
    transport: NoisedPipe,
    write_half: OwnedWriteHalf,
    read_half: OwnedReadHalf,
    last_used: Instant,
}
```

- Al llamar `execute_remote_tcp()`, buscar sesión viva en pool.
- Si no hay, hacer TCP connect + Noise XK handshake, añadir al pool.
- Si todas las sesiones están ocupadas, crear nueva (hasta `max_per_peer`).
- Sesiones inactivas > `keepalive_secs` se cierran y eliminan del pool.
- Background task cada 10s hace prune del pool.

### 3. Endpoint: Opciones A/B/C

**Decisión: Opción A — Transparente (v0.11.0), reservar Opción B para v0.12.0**

#### Opción A (elegida para v0.11.0): Transparente en `dispatch_remote`
El router existente (`dispatch.rs:103-180`) se extiende para detectar que el peer destino tiene una dirección TCP conocida. Si la tiene, usa `execute_remote_tcp()` en vez de `execute_via_http()`.

```rust
// En DispatchRouter.route():
fn route(&self, request: &GuildDispatchRequest, caps: &CapabilityRegistry) -> RoutingDecision {
    if let Some(peer) = self.find_peer_for_capability(&request.guild, &request.tool) {
        if peer.tcp_addr.is_some() {
            return RoutingDecision::RemoteTcp(peer.node_id, peer.tcp_addr.unwrap());
        }
        return RoutingDecision::RemoteHttp(peer.node_id);
    }
    RoutingDecision::Local
}
```

**Ventaja:** El caller no cambia — `forja_do` sigue funcionando igual. El routing es transparente.
**Desventaja:** El caller no sabe si la ejecución será remota o local. Para v0.11.0 esto es aceptable.

#### Opción B (reservada v0.12.0): Endpoint explícito `dispatch/send`
Nuevo tool `forja_send(to: str, intent: str)` que toma una `peer_id` explícita, salta el router, fuerza ejecución remota.

#### Opción C: Híbrida — no implementar
Demasiada complejidad para v0.11.0. Re-evaluar si el caso de uso lo requiere.

### 4. Localización de `tcp_addr`

**Decisión: Campo `addr` en `GossipEntry` + flag `supports_p2p` en `HardwareCaps`**

Actualmente `GossipEntry` ya tiene un campo `addr: String` (e.g. `"10.0.0.1:9000"`). Este campo se usa para discovery pero no está vinculado al dispatch.

Propuesta:
1. En `GossipEntry.addr` se almacena la dirección TCP del listener P2P.
2. En `HardwareCaps` se añade `supports_p2p: bool` (default false).
3. El kernel publica su `tcp_addr` y `supports_p2p: true` al arrancar el listener P2P.
4. `DispatchRouter` solo considera peers con `supports_p2p == true` para `RoutingDecision::RemoteTcp`.

```rust
// En gossip/mod.rs o hardware.rs
pub struct HardwareCaps {
    pub cpu_cores: u32,
    pub ram_mb: u64,
    pub gpu_available: bool,
    pub supports_p2p: bool,         // NEW — v0.11.0
    pub tcp_port: Option<u16>,      // NEW — v0.11.0 (addr se deriva del GossipEntry)
}
```

**Por qué no en HardwareCaps directamente:** `GossipEntry.addr` ya existe y viaja con las entradas de gossip. Añadir `supports_p2p` a HardwareCaps permite filtrar en el router sin parsear strings.

**Por qué no derivado:** Un peer puede cambiar de puerto o IP sin cambiar su node_id. Tenerlo explícito evita ambigüedad.

## Implementation Plan (v0.11.0)

| Paso | Archivo | Descripción |
|------|---------|-------------|
| 1 | `tylluan-link/src/p2p.rs` | `start_p2p_listener()`, `connect_to_peer()`, `execute_remote_tcp()` |
| 2 | `tylluan-link/src/p2p.rs` | `P2pSessionPool` — pool reutilizable con keep-alive y prune |
| 3 | `tylluan-link/src/gossip/state.rs` | Añadir `supports_p2p + tcp_port` a `HardwareCaps` |
| 4 | `tylluan-link/src/dispatch.rs` | Extender `DispatchRouter.route()` para `RoutingDecision::RemoteTcp` |
| 5 | `tylluan-kernel/src/main.rs` | Arrancar `start_p2p_listener()` si config lo habilita |
| 6 | `tylluan-link/tests/p2p_dst.rs` | Tests: roundtrip, state replication, 3-node star |

## Open Questions
- ¿Firewall/NAT traversal? → Diferir a M14-G (STUN/TURN ya implementado en nat.rs)
- ¿mTLS vs Noise XK? → Noise XK sigue siendo más ligero. Re-evaluar si auditoría externa lo requiere.
- ¿Compresión de payload? → Diferir. Los payloads de dispatch son pequeños (< 1KB típicamente).

## References
- ADR-003: M14-D Cross-Datacenter Federation
- ADR-004: M14-D Guild Dispatch Protocol (this document)
- M14-C: Noise Protocol (XK + NK) — `crates/tylluan-link/src/noise.rs`
- `crates/tylluan-link/src/dispatch.rs` — DispatchRouter, DispatchQueue actual
- `crates/tylluan-link/src/gossip/state.rs` — GossipEntry, HardwareCaps
- `crates/tylluan-link/tests/mesh_simulation.rs` — Topology simulation patrón para tests P2P
