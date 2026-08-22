# ADR-003: M14-D Latency-Aware Routing & Cross-Datacenter Federation

## Status
**Partially implemented** — 2026-07-02 (revisado 2026-08-22 en auditoría full-proyecto, Coloquio #mision-activa T196)

El despacho M14-D en sí (latency-aware routing, ver [M14D_dispatch_spec.md](M14D_dispatch_spec.md)) está en producción y cerrado según ROADMAP_O3.md. Las secciones de federación cross-datacenter/DHT de este ADR no tienen evidencia confirmada de implementación — pendiente de que José o el equipo lo verifique explícitamente antes de marcar el documento completo como Accepted.

## Context

### El problema
Tylluan v0.9.0 tiene DHT Kademlia (256 K-buckets), Gossip protocol, y Noise XK/NK implementados. Sin embargo, estos componentes están diseñados para redes de miles/millones de nodos (estilo BitTorrent/IPFS).

Nuestro caso de uso fundacional es **el médico de África**: un profesional que lleva Tylluan en un USB, lo ejecuta en máquinas de distintos pueblos (5-10 peers típicos), y sincroniza cuando tiene conexión. No necesita routing cross-datacenter a escala global. Necesita:
1. **Descubrimiento LAN** automático cuando los peers están en la misma red
2. **Selección de peers** basada en latencia real (no en hops de DHT)
3. **Partial replication** — solo sincronizar lo que cada peer necesita
4. **Offline-first** — funcionar sin conexión, sincronizar cuando haya

### Lo que ya tenemos (v0.9.0)
- ✅ DHT Kademlia con 256 K-buckets (sobredimensionado para 5-10 peers)
- ✅ Gossip protocol con anti-entropy cursors
- ✅ Noise XK/NK para cifrado end-to-end
- ✅ SilvaDB (SQLite WAL) como knowledge base
- ✅ BGE-M3 embeddings (2.2GB, requiere 4GB+ RAM)
- ✅ 272 tests verdes

### Lo que falta
- ❌ LAN auto-discovery (UDP broadcast/mDNS)
- ❌ Partial replication (subscription model)
- ❌ Latency-aware peer selection
- ❌ Fault tolerance validada (M6-full en progreso)

### Restricciones técnicas
- **Hardware objetivo:** Raspberry Pi 4 (4GB RAM, ARM64)
- **Red:** Intermitente, alta latencia (>500ms RTT común)
- **Escala:** 5-100 peers máximo (no 10k+)
- **Offline-first:** Debe funcionar sin conexión indefinidamente

## Decision

### Opciones consideradas

#### Opción 1: Full DHT con latency-aware routing (libp2p-style)
**Descripción:** Implementar routing basado en latencia real sobre DHT Kademlia existente. Cada peer mide RTT a sus vecinos y ajusta la routing table para preferir peers de baja latencia.

**Pros:**
- Reutiliza DHT existente
- Escala a miles de nodos si el proyecto crece
- Bien documentado en libp2p/IPFS

**Contras:**
- Complejidad algorítmica alta (O(log n) lookups)
- Overhead de mantenimiento de K-buckets innecesario para 5-10 peers
- No resuelve partial replication
- Requiere M6-full primero para validar bajo estrés

**Veredicto:** ❌ RECHAZADA — sobredimensionado para nuestro caso de uso

#### Opción 2: Simple peer selection based on RTT (Syncthing-style)
**Descripción:** Mantener lista de peers conocidos (configurados manualmente o descubiertos via LAN). Medir RTT a cada peer periodicamente. Priorizar sync con peers de menor latencia.

**Pros:**
- Simple de implementar (O(n) donde n = número de peers)
- Bajo overhead computacional
- Fácil de debuggear
- Alineado con Syncthing (probado en producción)

**Contras:**
- No escala más allá de ~100 peers
- Requiere configuración inicial o LAN discovery
- No resuelve partial replication por sí solo

**Veredicto:** ✅ APROBADA como mecanismo de peer selection

#### Opción 3: Partial replication con subscription model (Scuttlebutt-style)
**Descripción:** Cada peer se suscribe a feeds específicos (ej: "conocimiento médico", "herramientas IA del equipo"). Solo replica los mensajes de esos feeds. Usa Epidemic Broadcast Trees (EBT) para sync eficiente.

**Pros:**
- Ahorra ancho de banda y almacenamiento
- Permite "especialización" de peers (algunos solo tienen datos médicos, otros solo herramientas)
- Alineado con Scuttlebutt (probado en producción desde 2016)
- Resuelve el problema del médico: no necesita toda la red, solo sus colegas

**Contras:**
- Complejidad de implementación media (EBT no es trivial)
- Requiere metadata de suscripciones
- No resuelve peer selection por sí solo

**Veredicto:** ✅ APROBADA como mecanismo de replication

### Decisión final

**Combinar Opción 2 + Opción 3:**

1. **Peer selection:** Simple RTT-based selection (Syncthing-style)
   - Medir RTT a peers conocidos cada 60s
   - Priorizar sync con los 3 peers de menor latencia
   - Fallback a peers de mayor latencia si los primeros no responden

2. **Replication:** Partial replication con subscription model (Scuttlebutt-style)
   - Cada peer tiene lista de suscripciones (feeds/channels)
   - Solo replica mensajes de feeds suscritos
   - Usa EBT para sync eficiente (push-pull con vectores de estado)

3. **LAN discovery:** UDP broadcast (Scuttlebutt-style)
   - Broadcast UDP cada 1s en red local
   - Mensaje contiene: IP, port, public key
   - Auto-conexión cuando se detecta nuevo peer

### Justificación

Esta combinación:
- ✅ Resuelve el caso de uso del médico (5-10 peers, offline-first)
- ✅ Bajo overhead computacional (compatible con RPi4)
- ✅ Ahorra ancho de banda (partial replication)
- ✅ Simple de implementar y debuggear
- ✅ Alineado con proyectos probados (Scuttlebutt, Syncthing)
- ✅ No requiere M6-full para empezar (pero M6-full valida la decisión)

## Consequences

### Positivas
1. **Menor complejidad:** Eliminamos DHT Kademlia como componente crítico (lo mantenemos como fallback opcional)
2. **Menor uso de recursos:** Partial replication reduce almacenamiento y ancho de banda en 60-80%
3. **Mejor UX:** LAN auto-discovery elimina configuración manual
4. **Offline-first validado:** Scuttlebutt-style funciona en redes intermitentes desde 2016
5. **Escalabilidad adecuada:** 5-100 peers es el sweet spot para nuestro caso de uso

### Negativas
1. **No escala a miles de nodos:** Si Tylluan crece más allá de 100 peers, necesitamos migrar a DHT completo
2. **Configuración inicial:** Los peers necesitan conocerse (manualmente o via LAN)
3. **EBT complejidad:** Implementar Epidemic Broadcast Trees requiere 2-3 sesiones de trabajo
4. **Dependencia de M6-full:** Necesitamos validar fault tolerance antes de confiar en este diseño

### Neutrales
1. **DHT Kademlia se mantiene:** No lo eliminamos, solo lo dejamos como fallback opcional
2. **Gossip protocol se reutiliza:** EBT es una variante de gossip, no reemplazamos todo
3. **Noise XK/NK se mantiene:** El cifrado no cambia

## Implementation Plan

### Fase 1: LAN Discovery (1 sesión)
- Implementar UDP broadcast en `tylluan-link/src/discovery.rs`
- Formato: `{ip, port, pubkey}` en JSON
- Broadcast cada 1s en puerto 8008 (Scuttlebutt convention)
- Auto-conexión cuando se detecta nuevo peer

### Fase 2: RTT-based Peer Selection (1 sesión)
- Medir RTT a peers conocidos cada 60s
- Mantener lista ordenada por latencia
- Priorizar sync con top-3 peers
- Fallback a peers de mayor latencia si los primeros no responden

### Fase 3: Partial Replication (2-3 sesiones)
- Definir modelo de suscripciones (feeds/channels)
- Implementar EBT (Epidemic Broadcast Trees)
- Integrar con SilvaDB (solo replicar nodos de feeds suscritos)
- Tests de sync con 3-4 peers

### Fase 4: Validación con M6-full (1 sesión)
- Integrar con fault_dst.rs (M6-full)
- Testear bajo particiones de red
- Validar convergencia de EBT
- Medir overhead de RTT measurements

**Total:** 5-6 sesiones (2-3 semanas)

## Alternatives Considered

### Alternative A: Mantener DHT completo + añadir partial replication
**Descripción:** Mantener DHT Kademlia como está y añadir partial replication encima.

**Por qué no:** DHT es overkill para 5-10 peers. Añade complejidad innecesaria y overhead computacional.

### Alternative B: Usar libp2p completo
**Descripción:** Reemplazar tylluan-link con libp2p (que ya tiene DHT, gossip, discovery).

**Por qué no:** libp2p es enorme (100k+ LOC), requiere dependencias pesadas, y no está optimizado para offline-first. Tylluan-link es más simple y alineado con nuestra filosofía.

### Alternative C: No hacer M14-D, enfocarse en M6-full
**Descripción:** Pausar M14-D indefinidamente y enfocarse solo en fault tolerance.

**Por qué no:** Sin peer selection y partial replication, el caso de uso del médico no funciona. M6-full valida la infraestructura, pero M14-D la hace útil.

## References

1. **Scuttlebutt Protocol Guide** — https://ssbc.github.io/scuttlebutt-protocol-guide/
   - LAN discovery via UDP broadcast
   - Partial replication con EBT
   - Probado en producción desde 2016

2. **Syncthing Documentation** — https://docs.syncthing.net/
   - Local Discovery Protocol v4
   - Block Exchange Protocol v1
   - Device selection based on availability

3. **libp2p Specs** — https://github.com/libp2p/specs
   - Kademlia DHT
   - Gossipsub
   - No usado directamente, pero referencia para comparación

4. **Tylluan ROADMAP.md** — v0.5.0 M14-D
   - "Cross-datacenter federation — latency-aware routing, regional clusters"
   - Originalmente diseñado para escala AWS, ahora rediseñado para caso de uso del médico

5. **ADR-001, ADR-002** — Decisiones previas de arquitectura
   - ADR-001: SilvaDB como knowledge base
   - ADR-002: Noise XK/NK para cifrado

## Notes

- Este ADR está **propuesto**, no aceptado. Requiere aprobación de Jose y del equipo.
- M6-full (fault_dst.rs) debe completarse antes de implementar Fase 3-4.
- LAN discovery (Fase 1) puede empezar inmediatamente, no depende de M6-full.
- DHT Kademlia se mantiene como fallback opcional para futuros casos de uso de mayor escala.
