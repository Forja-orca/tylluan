# DISEÑO — ADR-019 paso 2: permisos por par y por espacio (SIN código) — v2

> **Asignación:** TL, T1171/T1172 (2026-10-14/15). ADR-019 §6 paso 2: "Permisos por par y por espacio, con denegación por defecto y tests de rechazo". "DISEÑO, sin código… Espera mi OK antes de escribir código."
> **v2 (2026-10-15)** — correcciones de la revisión del TL (turno 1175): (1) 4º punto de ingesta `sync_both` — 8 upserts en 4 funciones, helper único de ingesta, test T14; (2) usabilidad de espacios (receta del operador + qué pasa con `default`); (3) denegaciones → `log_audit_entry` (guild_audit_log), no solo `warn`; (4) decisiones §6 aplicadas del TL; (5) frescura (updated_at forjable) → informe de amenaza de Antigravity, fuera de este diseño.
> **v3 (2026-10-15)** — segunda revisión del TL: (a) `"default"` se **RECHAZA** al configurar un par (ya no es literal permitido); (b) endurecer la ingesta: un nodo de par **nunca** cambia `protected` ni `weight` de un existente; (c) error de lectura de frescura → **rechazo** (hoy ese caso hace upsert); (d) **precondición operativa LAN** (dev_mode=false + bind fuera de 127.0.0.1) documentada en §7.
> **Autor:** Deep. **Worktree:** `E:\tylluan-wt-deep` (rama `spike/iroh` con los cambios del spike intactos — TL dijo no tocarlos; este documento es un fichero NUEVO). **Nada en main.**
> **Base de verificación:** `origin/main` = `682d428`; todas las citas re-verificadas contra el árbol del worktree (código sin cambios vs `7350a1d`: los commits posteriores son docs). Los ficheros citados no están tocados por el spike.
> **Estado:** v3 para el OK de José (lo lleva el TL). No implementar hasta ese OK.

---

## 1. Estado verificado hoy (base del diseño)

- **Aprobación binaria sin permisos**: `FederationPeer` no tiene ningún campo de permisos ([`crates/tylluan-kernel/src/federation/mod.rs:11-33`]); tabla `federation_peers` sin columnas de scope ([`mod.rs:59-69`]).
- **Export sin filtro por par**: sirve todo `shareable=1 AND federation_source IS NULL AND quarantined=0` ([`memory/silva/anchors.rs:166-169`]) al primer par aprobado que presente su bearer ([`api_federation.rs:417-427`]).
- **Ingesta sin filtro por espacio — EL INVENTARIO COMPLETO (corregido en v2)**: **8 upserts en 4 funciones** de `api_federation.rs` (grep exhaustivo de `upsert_node_with_provenance` sobre TODO `crates/`; los demás callers — `dream_cycle.rs`, `agent_memory.rs:430`, `correct.rs:133`, `handlers.rs`, `handler_graph.rs`, `handler_remember.rs` — NO son peer-facing, son escrituras locales):
  1. `federation_sync_receive` (receive): upserts **:365** y **:377**.
  2. Pull manual (`federation_sync_pull`): upserts **:566** y **:576** (sig-verify :520-525, protected :527, frescura :541-573).
  3. **`federation_sync_both`**: upserts **:692** y **:697** (sig-verify :661-666, protected :667-668, frescura :680-701) — **faltaba en la v1; el hueco lo encontró el TL**.
  4. `pull_from_peer_internal` (auto-sync): upserts **L1013** y **L1021**.
  Las 4 funciones repiten el mismo cuerpo (verify → protected → meta `federation_source` → frescura → upsert): **duplicación exacta en 4 sitios**.
- **Push global**: un único body para TODOS los pares — manual ([`api_federation.rs:181-199`]), **`sync_both` ([:624-650])** y auto-sync ([L1107-1130]).
- **Dispatch P2P sin allow-list**: el handler ejecuta `reg.call_tool` para cualquier `guild`+`tool` ([`transport/http/mod.rs:1005-1037`]). La identidad del llamante **está disponible en el punto de invocación** (`pipe.peer_id` en [`p2p.rs:197`], handler llamado en [`p2p.rs:247-248`]) pero no se pasa al handler.
- **Auditoría de despachos YA EXISTE y está viva** (corrección a la v1 de esta auditoría, ya señalada por el TL en `9c50725`): `log_audit_entry` ([`transport/server/handler_do/audit.rs:25`], INSERT del camino principal en [`audit.rs:71`]) escribe filas hash-chained en `guild_audit_log` (misma DB que `audit_db_path`, [`audit.rs:15`], seam `TYLLUAN_AUDIT_DB`) en cada dispatch de `tylluan_do`. Las denegaciones del paso 2 usarán ESTE mismo canal (§2.6).
- **Pares existentes: 0** — verificado en vivo: `GET /api/v1/federation/peers` → `[]`.
- **Coloquio no federado**: sin código de federación de canales (coincide con Antigravity, `AUDIT_peer_capabilities_2026-10.md` §2.4).

## 2. Modelo conceptual

### 2.1 Espacio (space) — resolución y ASIGNACIÓN POR EL OPERADOR (punto 2 del TL)

**Resolución (sin cambios vs v1):** para un nodo, en este orden: 1) `topic_key` no vacío → ese valor; 2) `metadata["space"]` (string no vacío) → ese valor; 3) ninguno → `"default"`. Sin migración, sin columna nueva. Sin wildcards (`"*"` prohibido explícitamente).

**Asignación hoy — receta verificada (era el hueco señalado por el TL):**
- **Vía MCP `tylluan_remember`: SÍ hay camino limpio.** El schema acepta `metadata` (object) ([`transport/server/tools.rs:197`]); el handler lo incorpora tal cual al metadata del nodo ([`handler_remember.rs:90-100`]) y lo persiste en el write del nodo ([`handler_remember.rs:302`], `upsert_node_with_validity(&nid, "memory", …, &metadata, …)`).
  **Receta exacta:** `tylluan_remember(content="…", metadata={"space": "proyecto-x"})` → el nodo queda en el espacio `proyecto-x` y `space_of(node)` lo resuelve.
- **Vía HTTP `POST /api/v1/memory/write`: NO permite asignar espacio hoy** — construye `metadata` como `{"agent_id": …}` fijo ([`api_memory.rs:33`]) y no lee `metadata` del request. Se documenta explícitamente; ampliar el endpoint (aceptar `metadata` o un campo `space`) queda como candidato post-paso-2 (no lo asumo aquí).
- **`topic_key`**: no hay herramienta de operador que lo fije directamente (lo pueblan flujos internos/anclas); se mantiene como primer criterio de resolución documentado como comportamiento del sistema.

**Decisión del TL (revisión 2): `"default"` NO se puede listar.** La validación del endpoint de permisos lo **RECHAZA** (400). Motivo: casi todos los nodos sin etiqueta caen en `default`, y compartirlo por accidente sería fácil — para compartir hay que **etiquetar de forma explícita** (`metadata.space`) al escribir (receta de arriba). La resolución interna sigue siendo topic_key → metadata.space → `default` (necesaria para clasificar cada nodo), pero `default` no es un espacio compartible.

### 2.2 Dirección — definida DESDE el nodo local N respecto al par P
- `push` = N puede ENVIAR ese espacio a P (saliente: push propio y export servido a P).
- `pull` = N acepta ese espacio EMPUJADO/ENTREGADO por P (entrante: receive y pull ejecutado por N).
- `both` = ambas. `none` = ninguna (equivalente a no listar). **Confirmado por el TL (§6.1, turno 1175): se mantienen los nombres del ADR.**

### 2.3 Dispatch allow-list
Entradas con coincidencia exacta `"guild.tool"` (p. ej. `"git.status"`). Lista vacía = **denegar todo** (default). Sin comodines en v1 (**confirmado por el TL, §6.3**).
Validación en la API de escritura (**decisión del TL, §6.2**): entrada que no exista en el catálogo actual → `warn` (no error: el catálogo cambia). **Además: entradas de las familias `bash.*`, `filesystem.*` o `code.*` generan `warn` FUERTE al escribir los permisos** (el sandbox está apagado por defecto, `SandboxConfig::default().enabled = false`, `config.rs:1563`) **y toda ejecución denegada queda auditada** (§2.6). El gate de dispatch no cambia por familia: la allow-list es la lista.

### 2.4 Coloquio
`coloquio_channels: []` se **almacena** desde el paso 2 (esquema estable) pero es **INERTE**: no existe enforcement hasta el paso 6 (ADR §3.4/§6). Documentado así para no prometer nada.

### 2.5 Invariante transversal
Todo parseo de permisos es **fail-closed**: campo ausente, JSON inválido o desconocido ⇒ deny-all + `warn`. Un error de parseo jamás concede. Se aplica idéntico en los 4 puntos de ingesta, el dispatch y el push/export.

### 2.6 Auditoría de denegaciones (punto 3 del TL)
Toda denegación por permisos — dispatch rechazado, espacio no permitido en export/receive/pull/push — emite **dos** salidas:
1. `tracing::warn!` (ya previsto en v1).
2. **Fila en `guild_audit_log` vía `handler_do::log_audit_entry`** ([`transport/server/handler_do/audit.rs:25`], hash-chained, mismo store que los despachos de `tylluan_do`; seam `TYLLUAN_AUDIT_DB` para tests). Mapeo propuesto de campos:
   - `intent` = `"peer {peer_name}: {flujo} denied — {razón}"` (p. ej. `"peer torre-b: dispatch denied — tool not in allow-list"`, `"peer torre-b: receive denied — space 'proyecto-x' not allowed"`).
   - `guild`/`tool` = los reales del intento (dispatch) o `"federation"`/nombre del flujo (sync).
   - `agent_id` = nombre del par.
   - `success = false`; `preview` = razón corta; `latency_ms = 0`; `human_intervention = false`.
   - Volumen: una fila por denegación (acotado por tamaño de mensaje/sync); es el mismo store que ya recibe cada dispatch normal.

## 3. Esquema propuesto

### 3.1 Almacenamiento (una columna JSON nueva, mismo patrón de migración que `ed25519_pubkey`)
En `federation_peers` ([`federation/mod.rs:59-74`], el ALTER existente en `:72` es el precedente):
```sql
ALTER TABLE federation_peers ADD COLUMN permissions TEXT NOT NULL DEFAULT '{}';
```
Forma del JSON (versión 1):
```json
{
  "sync_spaces": [ {"space": "proyecto-x", "direction": "both"} ],
  "dispatch_allow": ["git.status"],
  "coloquio_channels": []
}
```
- `'{}'`, `"sync_spaces": []` y `"dispatch_allow": []` ⇒ deny-all.
- En Rust: `#[serde(default)] pub permissions: PeerPermissions` en `FederationPeer` (`mod.rs:11-33`), `Default` = todo vacío; deserialización por función dedicada que loguea y degrada a deny-all si falla (§2.5).

### 3.2 Mutación de permisos
- Endpoint nuevo: `PUT /api/v1/federation/peers/{name}/permissions` (misma protección admin que el resto de operaciones de peers; no exento en [`auth.rs:192-200`] `is_public_bypass_path`). Body = §3.1; validación estricta (nombres no vacíos, `direction` ∈ enum, formato `guild.tool`; `space == "default"` → **400, rechazado** (§2.1); `warn` de catálogo + warn fuerte por familia peligrosa §2.3).
- `PeerDb`: `set_permissions(name, perms)` + `insert` extendido para persistir la columna (hoy :87-99).
- **TOML: fuera de alcance.** El seed de pares nace con deny-all.

### 3.3 Migración de pares existentes
- `DEFAULT '{}'` deja **deny-all** a toda fila previa (hoy: 0 filas aprobadas — verificado). Test explícito T2.

## 4. Puntos exactos de aplicación — con helpers únicos (punto 1 del TL)

**Refactor mínimo aceptado**: el mismo gate copiado 4 veces es como aparecen los agujeros. Dos helpers nuevos en `federation/permissions.rs`, y las rutas existentes los llaman:

- **`ingest_from_peer(state, peer, envelope, flow) -> IngestOutcome`** — encapsula el cuerpo hoy duplicado: sig-verify → protected → **gate `may_receive_space(space_of(node))` (pull|both)** → meta `federation_source` → frescura → upsert. Devuelve `Received` / `Skipped(reason)` / `Denied(reason)` (Denied ⇒ §2.6). **Las 4 funciones de ingesta llaman SOLO a este helper** (receive :365/:377, pull manual :566/:576, sync_both :692/:697, pull_internal L1013/L1021 dejan de tener upsert propio).
  - **Endurecimiento del upsert (añadido, revisión 2):** la ingesta de un par **NUNCA** cambia `protected` ni el peso de un nodo existente. Hoy el SQL común hace `protected = excluded.protected` (el nodo entrante no trae el campo → cae a 0 y puede desproteger un nodo local, [`nodes.rs:195`]) y `weight = MAX(nodes.weight, excluded.weight)` (sube a 1.0, [`nodes.rs:191-194`]). El helper usará un modo de escritura dedicado (p. ej. `NodeWriteOptions.from_peer`) cuya rama ON CONFLICT usa `protected = nodes.protected` y `weight = nodes.weight`. Los nodos **nuevos** de par siguen entrando `protected = 0, weight = 1.0` — el «hueco de día 1» del peso (el Coherence Gate solo penaliza procedencia si `weight < 1.0`, [`coherence_gate.rs:300`]) queda como **seguimiento aparte del ADR**, fuera del paso 2.
  - **Error de lectura de frescura = rechazo (añadido, revisión 2):** hoy un `Err` de `get_freshness_info` cae en el brazo `_ =>` que **hace upsert** (p. ej. [`api_federation.rs:575-579`]). El helper lo convierte en `Skipped(reason = "freshness read error")` + `warn`, sin upsert.
- **`push_body_for_peer(state, peer) -> Vec<SignedEnvelope>`** — construye el body **por par**: shareable→filtro `federation_source`→**gate `may_send_space` (push|both) por nodo**→sign. Lo llaman los 3 flujos de envío (push manual :181-199, sync_both :624-650, auto-sync L1107-1130); par con ningún espacio permitido → body vacío y se le omite.
- Helpers de predicado: `space_of(node)`, `may_send_space`, `may_receive_space`, `may_dispatch`, `peer_by_x25519` (mismo cálculo que `approved_x25519`, [`transport/http/mod.rs:1046-1053`]).

| # | Flujo | Punto de código (hoy) | Cambio de diseño | Deny |
|---|---|---|---|---|
| A | **Dispatch P2P** (entrante) | `transport/http/mod.rs:1005-1037` + firma `P2pHandlerFn` [`p2p.rs:144`] + invocación [`p2p.rs:247-248`] | Pasar `caller_x25519` (`pipe.peer_id`, `p2p.rs:197`) al handler; resolver par + `may_dispatch(guild, tool)` **antes** de `CallToolRequestParam` | `GuildDispatchResponse{success:false, …}` + `warn` + **`log_audit_entry`** (§2.6) |
| B | **Export** (servimos a P) | `api_federation.rs:401-464` (bearer→par :417-427; filtro :429-445) | Filtrar `nodes_for_export` por `may_send_space` adicional al filtro shareable/echo | Nodos excluidos; vacío → export vacío + **audit** por nodo excluido |
| C | **Receive** (P nos empuja) | `:257-395` → **helper** | `ingest_from_peer(…, "receive")` | `Skipped` + `Denied` ⇒ **audit** |
| D | **Pull** (nosotros) | manual `:519-577` y `pull_from_peer_internal :942-1031` → **ambas vía helper** | `ingest_from_peer(…, "pull")` en las DOS rutas | idem C |
| E | **Push** (enviamos) | manual `:175-235` + auto-sync `L1107-1130` → **helper** | `push_body_for_peer` | idem B |
| F | **sync_both** (punto NUEVO de la v2) | `:600-715`: push :624-650 + pull :661-702 (`:692`/`:697`) | push → `push_body_for_peer`; pull → `ingest_from_peer(…, "both")` | cubierto por B+C (sin ruta propia) |

**Fuera de este choke point (no es tarea del paso 2):** `POST /api/v1/guilds/dispatch/execute` es operator-only (bearer maestro; `auth.rs:192-200` no lo exime — Antigravity) y A2A no es peer-facing. Si aparece una vía peer-facing nueva, DEBE usar los helpers.

## 5. Tests de rechazo (lista pedida — a implementar en el paso de código)

T1-T13 de la v1 (sin cambios) + **T14 y T15 nuevos**:

1. **T1 default-deny nuevo par**: add+approve sin permisos → export `[]`; receive descarta todo (`received=0, skipped=n`); dispatch rechazado.
2. **T2 migración legacy**: fila `approved=1` sin columna `permissions` → tras `ALTER` sigue deny-all en los 4 flujos.
3. **T3 export por espacio**: 2 espacios, uno permitido (push) → export contiene SOLO ese espacio.
4. **T4 export por dirección**: espacio `pull` → NO se sirve en export.
5. **T5 receive espacio no permitido**: → `skipped`, sin insert, sin `federation_source`.
6. **T6 receive permitido (control positivo)**: `pull|both` → insert correcto y procedencia intacta.
7. **T7 receive con dirección `push`-only**: rechaza entrante.
8. **T8 push por par**: 2 pares con permisos distintos → cada body solo con sus espacios.
9. **T9 dispatch deny vacío**: `dispatch_allow: []` → `success=false`, `call_tool` NO invocado.
10. **T10 dispatch allow exacta**: `["git.status"]` ejecuta; `bash.execute` rechazado.
11. **T11 permisos corruptos**: JSON inválido → deny-all + `warn`, sin panic, sin concesión parcial.
12. **T12 coloquio inerte**: `coloquio_channels` con valor → ningún cambio observable.
13. **T13 helper de identidad**: `peer_by_x25519` (mapea la clave del handshake; desconocida → None → rechazo).
14. **T14 `sync_both` (NUEVO, pedido por el TL)**: (a) push de `sync_both` con 2 pares/2 espacios → cada body solo con lo permitido a su par; (b) pull de `sync_both` con espacio no permitido → `skipped`, **los upserts :692/:697 no ocurren** (quedan tras el gate del helper); (c) control positivo con `both` → received correcto.
15. **T15 auditoría de denegación (NUEVO, punto 3 del TL)**: una denegación de dispatch y una de receive → **fila nueva en `guild_audit_log`** (store vía seam `TYLLUAN_AUDIT_DB`, `success=false`, razón correcta, cadena de hash válida con `verify_audit_chain`).
16. **T16 endurecimiento del SQL de ingesta (NUEVO, revisión 2)**: (a) nodo existente `protected=1` + push del par → sigue `protected=1` tras el conflicto; (b) nodo existente con `weight < 1.0` + push del par → **conserva su peso** (no sube a 1.0); (c) nodo nuevo de par → entra `protected=0`, `weight=1.0`.
17. **T17 fresco rechazo por error de lectura (NUEVO, revisión 2)**: `get_freshness_info` devuelve `Err` (mock/seam) → `Skipped`, **sin upsert**, + `warn`.

## 6. Decisiones y fuera de alcance

- **6.1 (RESUELTA, TL 1175):** push = saliente / pull = entrante desde el nodo local; se mantienen los nombres del ADR.
- **6.2 (RESUELTA, TL 1175):** validación de catálogo → `warn`; familias `bash.*`/`filesystem.*`/`code.*` → `warn` fuerte al escribir + **auditoría de la ejecución denegada** (sandbox off por defecto). El gate no cambia por familia.
- **6.3 (RESUELTA, TL 1175):** sin wildcards en v1; `"default"` es nombre literal (§2.1) y se documenta qué comparte.
- **6.4 (RESUELTA — informe de amenaza de Antigravity + revisión del TL):** `updated_at` forjable es **inocuo hoy**: la prioridad remota es la constante `10`, no algo que decida el par ([`api_federation.rs:551`]); solo importaría si el paso 2 la cambiara — y este diseño **no la cambia**.
- **6.5 (matiz del TL, verificado):** «dispatch/execute sin allow-list» como vía de un par es exagerado: con `dev_mode=true` el host se fuerza a `127.0.0.1` ([`config.rs:1924-1935`]) y sin `dev_mode` exige el token maestro ([`auth.rs:192-200`]). No es una vía peer-facing alcanzable — la allow-list del dispatch A (§4) sigue siendo la corrección correcta para cuando el kernel sea alcanzable por LAN (§7).
- **NO VERIFICADO:** superficies del paso 3 (invitación) que tocarán el mismo schema; aceptación de `metadata` por `/memory/write` como cambio futuro; comportamiento exacto de recepción con `quarantined` local (mitigado: el nodo entrante nunca cambia `protected`/`weight` tras T16).
- **Nota informativa (no cambia el diseño):** existe `owner_scope` (columna real desde schema v16; `api_memory.rs:31/:34`) como eje de partición alternativo; el ADR pide "espacios" y la resolución de §2.1 no requiere migración — si alguna vez se quiere alinear `space` con `owner_scope`, es una decisión aparte.

## 7. Precondición operativa del paso 2 (nueva, revisión 2)

Para que un equipo sincronice por la LAN, el kernel debe **escuchar fuera de `127.0.0.1`**, y eso obliga a **`dev_mode = false` con token real** (la guarda actual fuerza el host a `127.0.0.1` cuando `dev_mode=true`, con `CRITICAL_SECURITY_TRIGGER`, [`config.rs:1924-1935`]; la guía de arranque ofrece las tres opciones, [`main.rs:399-402`]). La configuración actual del kernel de producción (`dev_mode = true`, `127.0.0.1`) **no sirve para redes por equipo**: es requisito del paso 2. Sin él, los gates de permisos siguen siendo la corrección correcta, pero operan como defensa en profundidad para un kernel aún no alcanzable por la red.

## 8. Verificación de este diseño

- **Citas v1 re-verificadas (2026-10-15):** chequeo automático de 20 citas → 20/20 tras afinar 2 en la revisión anterior (`approved_x25519` con ruta explícita; `pull_from_peer_internal :942-1031`).
- **Citas NUEVAS de la v2 verificadas en esta entrega:** `sync_both :600-715` con upserts :692/:697 (leída entera); grep exhaustivo de callers de `upsert_node_with_provenance` en TODO `crates/` → 8 peer-facing en 4 funciones, resto locales; `log_audit_entry` + `audit_db_path` ([`handler_do/audit.rs:15/:25`], INSERT chained leído); receta de espacio: `tools.rs:197` + `handler_remember.rs:90-100/:302` (metadata persiste en el nodo) y contraprueba `api_memory.rs:33` (HTTP no acepta metadata).
- **Citas de la v3 verificadas (revisión 2):** [`nodes.rs:185-207`] (SQL ON CONFLICT leído: `weight = MAX(nodes.weight, excluded.weight)` :191-194; `protected = excluded.protected` :195), [`coherence_gate.rs:300`] (penalización de procedencia solo si `weight < 1.0`), [`config.rs:1924-1935`] (dev_mode fuerza host), [`main.rs:399-402`] (guía de arranque), [`api_federation.rs:551`] (REMOTE_PEER_PRIORITY = 10).
- **"0 pares"** verificado en vivo (`GET /api/v1/federation/peers` → `[]`).
- Al implementar: `cargo clippy -p tylluan-kernel -- -D warnings`, tests T1-T17, `scripts/check_no_predation.sh` (toca `transport/` → sección `## Impact` obligatoria en el commit).
