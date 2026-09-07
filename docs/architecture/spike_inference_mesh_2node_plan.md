# Spike: Inference Mesh — 2 nodos reales (localhost)

**Estado:** PLAN DE EJECUCIÓN — no ejecutado todavía.
**Fecha:** 2026-09-07
**Origen:** extensión del spike DST existente (`benchmarks/spikes/inference_mesh/`, `crates/tylluan-link/tests/inference_mesh_spike.rs`, 3/3 tests pasando en simulación) + hallazgo del PROPOSAL (sección 4.2: `FederationPeer.approved` no está cableado en `registry.ingest()`).

## Por qué este plan existe

El spike DST confirma que el enrutamiento por capability funciona con cero código nuevo en `DispatchRouter`. Pero DST no mide latencia real de red, fluidez de streaming, ni el comportamiento de Noise NK/XK sobre TCP real. Para Tylluan, donde la inferencia compartida vive o muere por la experiencia del usuario IDE, la latencia percibida y la fluidez del stream son las métricas que importan — no el routing en sí.

## Prerrequisito (bloqueante)

**`FederationPeer.approved` no está cableado en `registry.ingest()`.**

El PROPOSAL (sección 2.2, hallazgo #2 del spike) documenta esto explícitamente: `DispatchRouter`/`CapabilityRegistry` no conocen `FederationPeer.approved`. El filtro de confianza binaria **no existe todavía en código**. Debe añadirse antes de este spike real, o el spike no prueba la ruta de producción (sin filtro, cualquier peer puede consumir inferencia — no es el comportamiento objetivo).

Punto de implementación: `tylluan-link/src/capability.rs` → `registry.ingest()`, filtrando contra `PeerDb::load_all()` (o su equivalente en el contexto de ingestión).

## Setup de 2 instancias locales

### Estructura de directorios

```
data/
├── node-a/
│   ├── silva.db
│   ├── tylluan.toml
│   └── models/
└── node-b/
    ├── silva.db
    ├── tylluan.toml
    └── models/
```

### Configuración: `node-a/tylluan.toml`

```toml
[nexus]
port = 4000                    # API principal de Node A

[federation]
# Node A se registra como peer de B
[[peers]]
id = "node-b"
address = "127.0.0.1:4010"     # API de Node B
approved = true

[mesh]
enabled = true
listen_port = 5000              # P2P listener (Noise NK)

[sharing]
enabled = true
capability = "inference:llama-3-8b-instruct:q4"  # Node A dona este modelo
```

### Configuración: `node-b/tylluan.toml`

```toml
[nexus]
port = 4010                    # API principal de Node B

[federation]
# Node B se registra como peer de A
[[peers]]
id = "node-a"
address = "127.0.0.1:4000"
approved = true

[mesh]
enabled = true
listen_port = 5001              # P2P listener distinto

[sharing]
# Node B NO dona capacidad — solo consume
enabled = false
```

### Pasos de arranque

```bash
# Terminal 1: Node A (con GPU/RAM, donante)
cargo run -p tylluan-kernel -- --data-dir data/node-a

# Terminal 2: Node B (consumidor, sin GPU)
cargo run -p tylluan-kernel -- --data-dir data/node-b

# Verificar peering
curl -s http://127.0.0.1:4000/api/v1/federation/peers | jq '.[].approved'
# → true para node-b
curl -s http://127.0.0.1:4010/api/v1/federation/peers | jq '.[].approved'
# → true para node-a
```

## Qué medir

### Métrica 1: Latencia de dispatch remoto vs local

| Caso | Descripción | End-to-end (ms) | Setup |
|------|-------------|-----------------|-------|
| **Local** | Inferencia directa en Node A (sin red) | Baseline | `tylluan_do` → `llama_backend` en Node A |
| **Remoto** | Inferencia dispatchada de B → A via Noise NK | Medir | `tylluan_do` en Node B → `DispatchRouter` → Node A |
| **Delta** | Overhead puro de la red + Noise | Remoto - Local | |

Protocolo de medición:
1. 10 invocaciones de `tylluan_do` con el mismo prompt en Node A (local baseline).
2. 10 invocaciones del mismo prompt en Node B (dispatch remoto vía mesh).
3. Calcular p50, p95, p99 de cada caso. El delta p50 es el overhead de red real.

### Métrica 2: Fluidez de stream de tokens

El dispatch remoto debe percibirse fluido desde el cliente IDE:
- Tiempo al primer token (TTFT) remoto vs local.
- Tokens por segundo (TPS) promedio del stream.
- Pausas visibles > 200ms en el stream remoto (que indicarían buffering excesivo).

### Métrica 3: Comportamiento de Noise NK/XK sobre TCP

- Handshake inicial: latencia de establishment del canal cifrado.
- Re-conexión tras timeout: ¿el canal se reconstruye transparentemente?
- Saturación: ¿qué pasa si 3+ peers piden inferencia simultánea al mismo donante? (Controlado por `max_concurrent_sessions` unilateral del donante — PROPOSAL sección 2.4).

## Criterios de éxito

| Métrica | Objetivo | Fail |
|---------|----------|------|
| Delta p50 latencia | < 50ms overhead vs local | > 200ms |
| TTFT remoto | < 300ms después del handshake | > 1s |
| TPS stream | > 80% del TPS local | < 50% |
| Pausas visibles | 0 pausas > 200ms en 10 invocaciones | > 2 pausas |
| Handshake Noise NK | < 100ms (localhost) | > 500ms |
| Re-conexión | Transparente, sin intervención manual | Error visible al usuario |

## Criterios de fallo

- Si el overhead puro de dispatch remoto supera 200ms en localhost, la malla no es viable sin compresión o pre-fetching de capabilities.
- Si el stream se traba visiblemente (> 200ms pausas), el canal Noise NK con TCP tiene un cuello de botella que necesita investigación antes de escalar.
- Si `FederationPeer.approved` no puede cablearse limpiamente en `registry.ingest()`, la arquitectura de confianza tiene un problema de acoplamiento que debe resolverse antes de avanzar.

## Escalado posterior

1. **2 máquinas físicas en la misma LAN** — primer salto real de red (WiFi/ethernet). Mide overhead real de la red local.
2. **2 máquinas en diferentes ubicaciones** — segundo salto. Mide latencia de Internet real + comportamiento de Noise NK bajo jitter.
3. **3+ nodos** — prueba la malla completa con DHT Kademlia routing multi-hop.
4. **Producción** — solo después de pasar todos los criterios de éxito en los 3 pasos anteriores.

## Archivos relacionados

- `PROPOSAL_distributed_inference_credit_mesh.md` — diseño de la malla de confianza
- `benchmarks/spikes/inference_mesh/README.md` — spike DST existente
- `crates/tylluan-link/tests/inference_mesh_spike.rs` — tests DST (3/3)
- `crates/tylluan-link/src/capability.rs` — `CapabilityRegistry`
- `crates/tylluan-link/src/p2p.rs` — canal Noise NK/XK
