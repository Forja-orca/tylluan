# Propuesta: Inferencia Compartida en Malla de Confianza (Shared Inference Mesh)

**Estado**: DISEÑO — refactorizado bajo principios de soberanía y cooperación no-transaccional.
**Autor**: sesión Claude Code + José, 2026-08-08.
**Contexto**: Tylluan no es un mercado, un motor de inferencia comercial ni una economía de tokens. El propósito de compartir cómputo es reducir la huella de datos global y permitir que nodos modestos compartan capacidad con su red de confianza de forma limpia, natural e interoperable.

## 0. Tesis Reorientada

Se elimina totalmente la idea de un `CreditLedger`, priorización por aportación, fichas o métricas transaccionales. **La inferencia compartida opera bajo la misma regla que la memoria en M14: confianza explícita binaria (`FederationPeer.approved`)**.

1. **Confianza Binaria**: Si un nodo está en tu lista de peers aprobados (`FederationPeer.approved`), le prestas cómputo cuando lo necesita. Si no está aprobado, no hay comunicación. Sin contabilidad, sin arbitraje de mercado.
2. **Capacidades en Malla**: Un nodo anuncia sus modelos o recursos disponibles en la tabla de capacidades DHT (`inference:<model_id>:<quant>`).
3. **Ruteo Transparente**: `DispatchRouter` (ya existente en `tylluan-link`) rutea la solicitud de inferencia al peer aprobado que sirve dicho modelo (estilo `exo`), utilizando el canal cifrado Noise NK/XK ya probado.

## 1. Arquitectura de Colaboración Limpia

```
┌──────────────┐    1. Anuncia capability        ┌──────────────┐
│  Nodo A      │    "inference:llama-8b-q4"     │ DHT (Kademlia│
│ (con GPU/RAM)│ ──────────────────────────────▶│  malla M14)  │
└──────┬───────┘                                └──────┬───────┘
       │                                               │
       │ 2. Escucha peticiones de peers aprobados      │ 3. Lookup de
       │    vía Noise NK (tylluan-link::p2p)           │    capability
       ▼                                               ▼
┌──────────────┐   4. DispatchRouter decide        ┌──────────────┐
│ llama-server │◀── RemoteTcp{peer_id, addr} ──────│  Nodo B      │
│ (o guild)    │    (Peer verificado en PeerDb)    │ (nodo modesto│
└──────────────┘                                   │  sin GPU)    │
                                                   └──────────────┘
```

## 2. Componentes y Simplificación

### 2.1 Capability Structuring (Extensión de `CapabilityRegistry`)
Se utiliza el `CapabilityRegistry` de `crates/tylluan-link/src/capability.rs` sin alterar su diseño:
- Capability String: `inference:<model_id>` (ejemplo: `inference:llama-3-8b-instruct` o `inference:bge-m3`).
- El nodo A indica qué modelos tiene listos para inferencia en su entorno local.

### 2.2 Reutilización Completa de `FederationPeer.approved`
No hay SQLite nuevo ni tablas de créditos. Se valida estrictamente contra la base de datos de federación existente:
- Si el campo `approved: bool` del `FederationPeer` cargado vía `PeerDb::load_all()` es `true`, el nodo A atiende la solicitud de inferencia del nodo B. (Nota de verificación: no existe hoy un método `is_approved()` dedicado — habría que añadirlo como conveniencia, o filtrar sobre `load_all()`.)
- Si no está aprobado, la conexión Noise se rechaza en la capa de transporte P2P (`p2p.rs`).

### 2.4 Higiene del Donante — no es contabilidad, es límite local
A diferencia de la sincronización de memoria (coste marginal ~cero para quien presta), la inferencia ocupa la GPU/CPU del nodo donante en exclusiva mientras dura la sesión. Sin ningún límite, un peer ruidoso podría saturar el hardware de quien presta. Solución local, no mutua: cada nodo define su propio `max_concurrent_sessions` y `max_vram_share` en su config — el donante decide cuánto da, nadie lleva la cuenta de lo que ya dio. Es control unilateral de generosidad, no arbitraje entre pares.

### 2.3 Ruteo de Modelos Enteros vs Tensor-Split
- **Enfoque Principal (Model-Level Dispatch)**: El nodo B delega la ejecución de inferencia completa de un modelo determinado a Nodo A. Esto minimiza el tráfico de red (un request HTTP/gRPC cifrado por inferencia, cero latencia intra-token).
- **Sin Tensor-Split Complejo**: No se fuerza a particionar tensores token por token a través de Internet/LAN salvo que dos máquinas estén en un clúster local especializado.

## 3. Por qué este marco es el correcto para Tylluan

| Aspecto | Enfoque de Mercado (Descartado) | Enfoque Tylluan (Malla de Confianza) |
|---|---|---|
| **Mecanismo** | `CreditLedger`, balance `contributed_ms - consumed_ms` | `FederationPeer.approved` (binario) |
| **Filosofía** | Transaccional, "quien más aporta tiene prioridad" | Cooperativo, "ayuda natural dentro de la malla" |
| **Complejidad** | Tablas de crédito, prevención de fraude/Sybil, balanceo | 0 líneas de contabilidad nuevas, 100% M14 existente |
| **Incentivo** | Económico / Fichas | Acceso natural a inteligencia compartida y sostenibilidad |

## 4. Siguiente Paso Real

1. **Prueba de Concepto (Spike)** — **HECHO, 2026-08-08**: `benchmarks/spikes/inference_mesh/README.md` + `crates/tylluan-link/tests/inference_mesh_spike.rs` (3/3 tests reales pasando). Confirmado: el enrutamiento por capability de modelo entero funciona con cero código nuevo en `DispatchRouter`.
2. **Hallazgo real del spike**: `DispatchRouter`/`CapabilityRegistry` no conocen `FederationPeer.approved` — son dos capas hoy desconectadas. El filtro de confianza binaria descrito en la sección 2.2 de este documento **no existe todavía en código**; debe añadirse en el punto de ingestión del gossip de capacidades (`registry.ingest()`), no dentro del router. Ver spike para el detalle exacto.
3. **Validación de Latencia** (pendiente): confirmar que el envío de prompts y retorno de stream de tokens vía `p2p.rs` (Noise NK) se percibe fluido desde el cliente IDE/Dashboard — requiere 2 nodos reales, no DST.
