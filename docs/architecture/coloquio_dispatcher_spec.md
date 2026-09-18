# Coloquio Push Dispatcher — Especificación de Implementación

> **Autor:** Claude Code (Tech Lead), consolidando consenso real del equipo
> **Target:** Deep, Buffy, Antigravity, José
> **Status:** Spec cerrada para implementación — turno 561, Coloquio
> **Fecha:** 2026-09-18
> **Precede a:** `docs/architecture/coloquio_push_dispatch_research.md` y
> `event_driven_agent_triggers_research.md` (investigación); este documento
> es el siguiente paso — de "por qué push" a "cómo, exactamente".

Este documento formaliza cuatro piezas de consenso que ya existen dispersas
en Coloquio (turnos 541-561) en una sola spec implementable. No introduce
decisiones nuevas — cita de dónde viene cada una. Nadie implementa nada de
esto sin que primero salga como contrato BWC reclamado en Coloquio (regla
de trabajo a la vista, `WORK_PROTOCOL.md`).

---

## 1. Resumen de las cuatro piezas de consenso

| Pieza | Quién | Turno | Qué dice |
|---|---|---|---|
| Split política/existencia | Deep | T550 | `agents.toml` = quién puede disparar qué; SilvaDB identity = quién existe ahora |
| Señal de convergencia | Deep | T547+ | Dos agentes llegando al mismo hallazgo sin verse = evidencia de alta confianza |
| Autoridad ejecución ≠ verdad | Antigravity/Claude | T547-549 | Quien commitea no es quien tiene razón; gana el argumento con prueba |
| Aprobación criptográfica + atómica | Buffy | T559 | HITL debe vincular la aprobación (SHA-256) a lo que el revisor vio, y ser atómica (CAS) |

---

## 2. Arquitectura: qué existe ya, qué falta

### 2.1 Lo que ya existe en el kernel (verificado, no asumido)

- **Broadcast de eventos**: `server.notifier` / `broadcast_tx` (`http/mod.rs:109,415`)
  emite `coloquio:new_turn` en cada post (`api_coloquio.rs:173`), con
  `channel_id`, `author_id`, `content`, `turn`.
- **Puente de menciones ya filtrado**: `filter_known_mentions()`
  (`api_coloquio.rs`, commit `258b04a`) extrae `@menciones` y las valida
  contra `AgentsContract::agent_ids()`, case-insensitive, falla abierto si
  el registro está vacío. **El dispatcher reutiliza esta misma extracción
  de menciones — no la reimplementa.**
- **Esquema de política**: `WakeConfig` (`security/agents_contract.rs`,
  commit `3e73b9a`) — `[agents.<id>.wake]` con `enabled`, `trusted_authors`,
  `command`. `is_active()` exige las tres cosas juntas. `trusts(author_id)`
  case-insensitive. **Ya existe, no se toca — el dispatcher lo consume.**
- **Identidad auto-bootstrapeada**: `IdentityManager::has_identity(agent_id)`
  (`memory/identity.rs:117`) — cualquier agente que haya hecho una sola
  llamada real al kernel ya tiene un nodo de identidad en SilvaDB
  (`handlers.rs:55-67`, auto-bootstrap en el primer contacto).
- **Cola de aprobación (prototipo local)**: `coloquio_watcher.py`
  `queue_for_approval()`/`approve_action()` (commit `32ac45c`) — JSON plano
  en `~/.tylluan/pending_actions/`. **Este es el prototipo a superar, no a
  reutilizar tal cual** (sección 4).

### 2.2 Lo que falta construir

1. Suscriptor interno del kernel al broadcast (no un script cliente).
2. Fuente de verdad de "agente válido" = existencia en `IdentityManager`
   **O** listado en `agents.toml` (fallback si SilvaDB está vacía —
   mismo patrón fail-open que ya usa `filter_known_mentions`).
3. Cola HITL con vinculación criptográfica y atomicidad (Buffy, sección 4).
4. Registro de convergencia (Deep, sección 5) — nuevo, no tiene precedente
   en el código actual.

---

## 3. Suscriptor interno de despacho

```
[kernel boot]
  agents_contract = AgentsContract::load(workspace_root)   // ya existe
  identity_mgr    = IdentityManager::new(silva)             // ya existe
  dispatch_rx     = server.notifier.subscribe()             // ya existe, nuevo suscriptor

[loop, tarea de fondo del kernel — NO un proceso cliente]
  event = dispatch_rx.recv().await   // coloquio:new_turn
  if event.type != "coloquio:new_turn": continue

  mentions = extract_mentions(event.content)   // función YA existente, reutilizada
  for mention in mentions:
      if mention == event.author_id: continue   // auto-exclusión, ya existe

      // Existencia: SilvaDB identity O agents.toml (fallback, no ambos obligatorios)
      is_known = identity_mgr.has_identity(mention).await
                 || agents_contract.agent_ids().any(|a| a.eq_ignore_ascii_case(mention))
      if !is_known: continue   // ya cubierto por filter_known_mentions en el buzón

      wake = agents_contract.active_wake_config(mention)   // ya existe, WakeConfig::is_active()
      if wake.is_none(): continue   // agente sin política de despacho = solo buzón, comportamiento actual

      if !wake.trusts(event.author_id): continue   // fail-closed, ya existe

      // A partir de aquí es la pieza NUEVA de este documento:
      queue_dispatch(mention, event, wake.command)   // sección 4
```

Puntos de diseño explícitos:

- **Cero tools MCP nuevas** (CONTRACT-01). El suscriptor vive dentro del
  proceso del kernel, no expone ningún endpoint nuevo salvo los de
  aprobación (sección 4.3, ya existen como patrón HTTP del resto del
  kernel).
- **No reemplaza el buzón de menciones** — un agente sin `[wake]` activo
  sigue recibiendo solo el correo normal, exactamente el comportamiento
  de hoy. El dispatcher es un añadido, no una migración forzosa.
- **`coloquio_watcher.py` en modo solo-inbox sigue siendo válido** como
  vía local/manual para un agente sin `wake_command` configurado — no se
  elimina, coexiste.

---

## 4. Cola HITL — el patrón de Buffy, con atomicidad

### 4.1 Qué falla en el prototipo de anoche

`coloquio_watcher.py::queue_for_approval()` escribe un JSON plano. Dos
problemas reales, ambos señalados por Buffy (T559), no hipotéticos:

1. **Sin vinculación criptográfica**: nada impide que el contenido del
   archivo cambie entre que se encola y que un humano lo aprueba — el
   humano podría estar aprobando algo distinto de lo que vio.
2. **Sin atomicidad**: dos aprobaciones concurrentes (o una aprobación +
   una expiración) pueden correr sin coordinación — no hay
   compare-and-set.

### 4.2 Diseño corregido

```rust
struct PendingDispatch {
    id: String,                  // uuid
    agent_id: String,            // a quién se despierta
    author_id: String,           // quién lo disparó (ya validado por trusts())
    channel: String,
    turn: i64,
    content_snapshot: String,    // exactamente lo que el revisor va a ver
    content_hash: String,        // SHA-256 de content_snapshot -- lo que el revisor APRUEBA
    command: Vec<String>,        // argv fijo, de WakeConfig -- nunca construido desde el mensaje
    state: DispatchState,        // Pending | Approved | Rejected | Expired
    queued_at: i64,
}

enum DispatchState { Pending, Approved, Rejected, Expired }
```

- **Vinculación**: el endpoint de aprobación recibe `(id, expected_hash)`.
  Si `expected_hash != content_hash` almacenado → rechazo — el humano
  estaba viendo algo que ya no coincide con lo encolado (contenido
  cambiado, o el humano está aprobando un id equivocado por error de
  copia/pega). Esto es literalmente lo que hace `open-multi-agent`
  (verificado por Buffy, T559): la aprobación se ata a un hash de
  exactamente lo que el revisor vio, no solo a un id.
- **Atomicidad**: transición de estado vía `UPDATE ... WHERE state='Pending'`
  en SQLite — el primer `UPDATE` que aplica gana; los siguientes ven
  `rows_affected == 0` y devuelven "ya resuelto", nunca ejecutan dos
  veces. *Corrección tras verificar antes de publicar (misma disciplina
  que le pedimos a Antigravity con sus citas): `security/grants.rs`, que
  cité primero como precedente SQLite, en realidad no usa SQLite — es un
  `HashMap` en memoria protegido por `RwLock`, donde `.remove(id)` da
  "ya resuelto" en la segunda llamada (`grants.rs::resolve`, línea 127).
  Mismo principio de "primero en llegar gana", mecanismo distinto (en
  memoria, no persistido, se pierde en un restart) — vale como
  precedente conceptual, no como código a copiar; la cola de dispatch sí
  necesita SQLite porque debe sobrevivir a un reinicio del kernel
  mientras un humano decide.*
- **Almacenamiento**: tabla SQLite nueva (`pending_dispatches`), no
  archivos JSON en `~/.tylluan/` — hereda las garantías de atomicidad de
  SQLite en vez de reinventarlas sobre el sistema de ficheros.

### 4.3 Aprobación

Igual que el resto de flujos de riesgo del kernel: un endpoint HTTP
(`POST /api/v1/dispatches/{id}/approve`, body `{expected_hash}`), no un
comando de terminal exclusivo — así el dashboard de Antigravity puede
construir el panel de aprobación con un clic que ella misma propuso
anoche, sin depender de que alguien tenga una terminal abierta (la misma
lección de todo lo de anoche: no depender de que un humano abra una
sesión concreta).

---

## 5. Registro de convergencia (propuesta de Deep, sin precedente en código)

No existe todavía nada de esto en el kernel — es la pieza más nueva y la
que menos detalle tiene, deliberadamente, porque necesita discusión antes
de diseño fino:

- Cuando dos agentes distintos reportan el mismo hallazgo (mismo
  `file:line` o mismo hash de contenido de hallazgo) **sin que uno haya
  leído el reporte del otro primero** (verificable por orden de turnos en
  Coloquio + ausencia de mención cruzada antes del hallazgo), el sistema
  podría marcarlo como `convergencia_independiente=true` en el nodo de
  memoria correspondiente.
- Uso propuesto: elevar la prioridad/confianza de ese hallazgo en
  recall futuro, y opcionalmente saltarse la cola HITL para acciones que
  de otro modo requerirían aprobación (un hallazgo con dos linajes
  independientes de por medio pesa más que la palabra de uno solo).

**Esto queda fuera del primer contrato de implementación** — es la pieza
que necesita su propia ronda de discusión (cómo se detecta "sin haberse
visto" de forma fiable, qué umbral de similitud cuenta como "mismo
hallazgo") antes de tener spec de código.

---

## 6. Plan de contratos (orden sugerido)

1. **BWC-1**: `PendingDispatch` + tabla SQLite + lógica CAS (sección 4.2).
   Sin endpoint HTTP todavía, solo el tipo y la persistencia. Tests:
   doble-aprobación concurrente debe dejar solo una ejecución.
2. **BWC-2**: endpoint `POST /api/v1/dispatches/{id}/approve` +
   `GET /api/v1/dispatches` (para que el dashboard liste pendientes).
3. **BWC-3**: suscriptor interno (sección 3) — conecta el broadcast
   existente con la cola de BWC-1, sin ejecutar nada todavía (dry-run:
   solo loguea qué encolaría).
4. **BWC-4**: activar ejecución real (`subprocess`/`Command::new` con el
   `command` fijo de `WakeConfig`) — el único paso que de verdad dispara
   procesos, y por tanto el que más revisión cruzada necesita antes de
   fusionar.
5. **(Aparte, sin bloquear lo anterior)**: diseño del registro de
   convergencia (sección 5) — discusión, no código todavía.

Cada BWC se reclama en Coloquio antes de escribir una línea, se entrega
vía `/deliver`, se verifica de forma independiente por alguien que no lo
escribió, y se cierra con voto — exactamente el ciclo que ya probamos
anoche con el fix de buzones fantasma y funcionó.

---

## 7. Qué NO cambia

- `coloquio_watcher.py` en modo solo-inbox sigue siendo libre de activar.
- `--exec` del watcher sigue apagado — decisión de José, no se toca aquí.
- Ningún agente sin `[wake]` configurado nota ninguna diferencia.
- CONTRACT-01 intacto: cero tools MCP nuevas en todo este diseño.
