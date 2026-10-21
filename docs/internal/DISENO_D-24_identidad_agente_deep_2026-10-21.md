# D-24 — Identidad de agente: diseño, sin migración

**Agente:** Deep (OpenCode) · **Fecha:** 2026-10-21 · **Base:** `origin/main` = `59d3eaa`
**Estado:** documento de diseño. **No hay código de migración, ni cambios de config, ni
escrituras sobre datos reales.** Todo lo que sigue son propuestas; la ejecución la decide José.
**Fuente del hallazgo:** `docs/internal/IMPL_T-DSH-8_fts_anonymous_identidades_2026-10-21.md` (b, c).

Líneas citadas = verificadas contra el árbol en `59d3eaa` el 2026-10-21, no heredadas del informe.

---

## 1. El problema, con evidencia

### 1.1 El `agent_id` no sale de la credencial, sale del argumento

| Punto | Código | Valor cuando no se declara |
|---|---|---|
| Entrada MCP (todas las tools del kernel) | `transport/server/handlers.rs:23-27` | `"anonymous"` |
| `tylluan_do` (ruta principal) | `handler_do/mod.rs:1305` | **`""` (vacío)** |
| MCP externo por guild | `handler_do/external_mcp.rs:175` | `"anonymous"` |
| Guilds: capacidades, dry-run | `registry/guild_process.rs:541-544`, `692-695` | `"anonymous"` |
| SSE | `transport/http/sse.rs:387` | `"anonymous"` |
| Jobs de fondo | `transport/server/background_jobs.rs:129,147,317` | `"anonymous"` |
| REST de cuarentena | `api_v1/api_memory.rs:296-301` | `"operator"` / `operator:<rol>` |

Las 490 filas sin atribuir del informe de DSH salen de la fila marcada en negrita:
`handler_do` hace `agent_id.clone().unwrap_or_default()` y el escritor de auditoría acepta
`""` sin protestar (`handler_do/audit.rs:25,77`). No es un fallo de datos: es que el
**valor por defecto de esa ruta es la cadena vacía**, distinta de la de las demás rutas.

### 1.2 El binding token→agente existe y no se usa para rellenar

- `AclConfig.token_agent_bindings: HashMap<String,String>` (`config.rs:1705-1709`), vacío en
  producción.
- `auth::resolve_agent_id_for_token` (`auth.rs:138-140`) y el task-local
  `current_bound_agent_id` (`auth.rs:112-117`) ya están implementados.
- Lo único que hace el kernel con ellos es **rechazar suplantación**
  (`handlers.rs:33-39`: si el token está atado a `X` y declaras `Y`, 403). Si no declaras
  nada, se usa `"anonymous"`: **el binding nunca rellena**.

### 1.3 No hay canonicalización en ningún punto

- El nodo de identidad se construye con el string crudo: `format!("agent:{agent_id}")`
  (`memory/identity.rs:62,83,98`) y se busca con `get_node` exacto.
- `agent_permissions` y `resolve_effective_profile` también van por string exacto
  (`config.rs:1715`, `guild_process.rs:545`).
- Consecuencia verificada: `deep` y `Deep` son **dos identidades distintas**, cada una con su
  nodo, su biografía y sus permisos, y la auditoría las cuenta por separado. Lo mismo con
  `antigravity`/`Antigravity` y `buffy`/`Buffy`.

---

## 2. Helper único de canonicalización

### 2.1 Forma propuesta

Nuevo módulo `crates/tylluan-kernel/src/security/agent_identity.rs` (junto a `trust.rs` y
`guard.rs`, que ya son la capa de decisión de seguridad):

```rust
pub enum AgentSource { BoundToken, Declared, Operator, Anonymous }

pub struct ResolvedAgent {
    pub id: String,          // siempre canónico, nunca vacío
    pub source: AgentSource, // de dónde salió: para la auditoría
}

/// trim + ASCII-lowercase + validación de charset. `None` si es inválido.
pub fn canonical_agent_id(raw: &str) -> Option<String>;

/// Precedencia única. `declared` ya canónico o None.
pub fn resolve(declared: Option<&str>, bound: Option<&str>, is_master: bool) -> ResolvedAgent;
```

Reglas, explícitas para que sean revisables:

1. `trim()` y **solo** `to_ascii_lowercase()` — sin plegado de acentos ni NFKC: un `José` no
   debe convertirse silenciosamente en `jose`, y nombres con caracteres no ASCII se rechazan
   en este campo (el nombre humano va en `human_name`, no en el id).
2. Charset permitido `[a-z0-9._-]`, longitud 1..=64, sin `..`, sin `/`, sin `\`, sin espacios
   internos. Si no cumple: **400 con el motivo**, nunca un fallback silencioso.
3. `anonymous` y `operator` son **reservados**: un cliente no puede declararlos. Se asignan
   solo por la precedencia de abajo.
4. Precedencia: `bound` (token atado) > `declared` > `operator` si la credencial es el token
   maestro > `anonymous`.
5. `bound` y `declared` se comparan **después** de canonicalizar. Hoy
   `handlers.rs:34` compara en crudo: un token atado a `deep` rechaza `Deep` con 403, que es
   el síntoma de 1.3 en la puerta de seguridad.

### 2.2 Dónde se invoca (una sola puerta, cuatro llamadas)

El punto clave del diseño: **canonicalizar y reescribir el argumento en la entrada**, no en
cada consumidor.

| Dónde | Qué hace |
|---|---|
| `TylluanServer::handle_kernel_tool` (`handlers.rs:23`) | resuelve y **sustituye `arguments["agent_id"]`** por el canónico; el journal, la ACL, el bootstrap de identidad y la auditoría leen ya el valor bueno |
| `handler_do` (`mod.rs:1305`) | usa el valor ya reescrito; se elimina el `unwrap_or_default()` |
| Entradas REST (`api_v1/*`, dispatch) | mismo resolver con `current_auth_kind()` (master → `operator`), luego reescribe el JSON que baja a guilds |
| `log_audit_entry` (`audit.rs:25`) | última defensa: canonicaliza y **rechaza `""`** (sustituye por `anonymous`/`operator`), con `tracing::warn!` si tuvo que corregir |

Por qué reescribir el argumento y no solo auditar: los guilds leen
`params.arguments["agent_id"]` directamente (`guild_process.rs:541-544`), y el perfil de
sandbox se resuelve por ese string. Si la normalización se quedara en la capa MCP, el guild
seguiría viendo `Deep` y eligiendo otro perfil de sandbox que el que la ACL evaluó. Reescribir
en la entrada deja **un solo valor** en todo el recorrido.

Efecto observable declarado: el payload que recibe el guild lleva el id canónico, no el que
escribió el cliente. Se registra un `tracing::info!(target: "agent_identity")` cuando la
canonicalización cambia el valor, para que el cambio sea rastreable.

---

## 3. Binding token→`agent_id`: adopción sin que nadie pierda acceso

La regla de José manda: los tokens se pueden rotar, **él nunca se queda fuera**. El diseño
cumple eso por construcción: **los bindings solo añaden restricciones a tokens que ya existen;
no hay ninguna ruta en la que un binding quite acceso al token maestro.**

### 3.1 Config

```toml
[security.acl.token_agent_bindings]
"<token-de-deep>" = "deep"
```

- `token_agent_bindings` está vacío por defecto (`config.rs:1724`): sin entradas, el
  comportamiento es exactamente el de hoy.
- El token maestro **no se ata nunca**. No está en `acl.tokens` como los demás y su
  identidad es `operator` por `current_auth_kind()`; atarlo lo convertiría en un agente y
  rompería la aprobación humana. Regla explícita para el revisor.
- El cambio se aplica con `TylluanConfig::reload()` (`config.rs:2150`), que ya existe y ya se
  dispara desde `api_admin.rs:500,609,693,1332`: **no hace falta reiniciar el kernel**.

### 3.2 Fases (cada una con su vuelta atrás)

| Fase | Acción | Verificación | Rollback |
|---|---|---|---|
| 0 | Acordar el nombre canónico de cada agente en `.tylluan/agents.toml` (ya es la fuente declarativa, `security/agents_contract.rs:107-110`) | revisión humana | ninguna escritura |
| 1 | Atar **un** token no crítico (`buffy` o `deep`) a su id | `whoami` con ese token devuelve el id esperado; llamar con otro `agent_id` → 403 (comportamiento actual, `handlers.rs:34`) | borrar esa línea del TOML + reload |
| 2 | Repetir por agente, uno a uno, anunciando en Coloquio | `audit.agent_id` deja de ser `anonymous` para ese token | idem, inmediato |
| 3 | Retirar el token compartido **solo con José delante** | ninguna llamada con el token viejo en 24 h | volver a añadirlo a `acl.tokens` (no se borra hasta este paso) |

Ninguna fase requiere tocar `.tylluan-token`, ni el token maestro, ni `dev_mode`.

### 3.3 Qué NO se propone (y por qué)

- **No** rellenar el `agent_id` con el binding cuando el cliente declara otro: eso convertiría
  el binding en una suplantación silenciosa. Se mantiene el 403.
- **No** atar tokens por IP ni por user-agent: cambia con VPN/proxy y generaría bloqueos
  inexplicables.
- **No** migrar `agent_permissions` a otra clave: al canonizar en la entrada, las claves
  existentes en minúsculas empiezan a funcionar solas; las que estén en mayúsculas se listan
  en la fase 0 y se renombran a mano (son pocas y es config, no datos).

---

## 4. Propagar `agent_id` al audit de guilds (las 490 filas vacías)

1. **Causa**: `handler_do/mod.rs:1305` (`unwrap_or_default()`), no un fallo de la tabla.
   Se corrige con el resolver de §2 en esa misma línea; `audit.rs:25` añade la guarda que
   impide escribir `""` aunque alguien vuelva a pasar un `Option<String>` vacío.
2. **Las filas históricas no se tocan.** `guild_audit_log` es una cadena de hashes
   (`prev_hash`, `hash`; insert en `audit.rs:77` y siguientes). Un `UPDATE` de relleno
   rompería la cadena y con ella la única garantía de integridad que tiene la auditoría.
   Las 490 filas se quedan como están y se documentan como *no atribuibles (pre-D-24)*.
3. **Lectura**: en los agregados (`api_v1/api_audit.rs`, `memory/agent_memory.rs:38`), mapear
   `agent_id = ''` a `unknown` en la respuesta, sin escribir nada. Así el dashboard deja de
   mostrar una categoría vacía sin reescribir historia.
4. **Guilds**: con la reescritura de §2.2 no hace falta tocar `guild_process.rs`: recibe el
   valor canónico en `arguments`. Si en el futuro `GuildDispatchRequest`
   (`tylluan-link/src/dispatch.rs:17-30`, hoy solo lleva `sender_id`) necesita el agente para
   auditoría remota, se añade un campo **opcional** `agent_id`; no se reutiliza `sender_id`,
   que identifica el nodo, no la persona/agente.

---

## 5. Fusión de `agent:deep` y `agent:Deep`

### 5.1 Regla de oro

`LIKE` en SQLite es **case-insensitive para ASCII** por defecto: `WHERE id LIKE 'agent:deep'`
encuentra también `agent:Deep` y convertiría una fusión controlada en un borrado masivo.
La fusión usa **igualdad exacta** para tocar (`id = ?1`) y `GLOB` (sensible a mayúsculas) para
enumerar.

### 5.2 Procedimiento propuesto (lo ejecuta José, no este informe)

1. **Copia de la base** (`silva.db` + `-wal`/`-shm`) antes de nada. Es la única vuelta atrás
   real de una fusión.
2. Enumerar: `SELECT id, type, protected, created_at FROM nodes WHERE id GLOB 'agent:*'`.
3. Agrupar en un script por `id[6..].to_ascii_lowercase()` (el prefijo `agent:` son 6 bytes).
   Grupos de tamaño 1: nada que hacer.
4. Para cada grupo con >1, elegir canónico: el que tenga `type='identity'` y `metadata` con
   `born_at` más antiguo; a igualdad, el de `id` todo minúsculas. Los demás son duplicados.
5. Fusionar cada duplicado con `merge_node_into(dup, canonico)`
   (`memory/silva/nodes.rs:884`). **Requisito: D-23 mergeado** — antes de D-23 esa función
   dejaba huérfano el docid FTS del nodo fusionado; ahora lo limpia dentro de la transacción.
6. Antes de fusionar, conservar lo que se perdería: si el duplicado tiene `philosophy`,
   `purpose` o `human_name` no vacíos y el canónico no, copiarlos al canónico
   (`upsert_node` sobre el canónico) y anotar `metadata.merged_from = [...]` con los ids
   absorbidos. La fusión de aristas la hace `merge_node_into` (`UPDATE OR IGNORE` + limpieza
   de auto-loops).
7. Verificar después: `SELECT COUNT(*) FROM nodes WHERE id GLOB 'agent:*'` y
   `fts_desync() == 0`; `whoami` de cada agente devuelve una sola identidad.

### 5.3 Qué se gana y qué no

Se gana: un nodo por agente, auditoría y permisos por un solo id, y la posibilidad de atar
tokens con un nombre estable. **No** se reescriben las filas históricas de `guild_audit_log`
ni los episodios ya guardados con el id antiguo: el histórico se queda como está, y la
canonicalización de §2 impide que se generen más variantes a partir de ahora.

---

## 6. Verificación que debería acompañar a la implementación (no incluida aquí)

- Unit tests del helper: `Deep`→`deep`, ` deep `→`deep`, `Deep/../x`→`None`, `""`→`None`,
  `anonymous`/`operator` declarados→`None`, cadena de 65→`None`.
- Test de precedencia: bound gana; bound+declarado distinto→403; master→`operator`.
- Test de auditoría: ninguna ruta escribe `""` (recorrer los call sites de `log_audit_entry`).
- Test de fusión en memoria: dos nodos `agent:deep`/`agent:Deep` → uno, `merged_from` con el
  absorbido, `fts_desync()==0`.
- Gate de documentación y `clippy -D warnings` como en el resto de la cola.

---

## 7. Decisiones que quedan para José

1. **¿Se adopta la canonicalización a minúsculas?** (recomendado sí; es lo único que evita
   que las tres parejas sigan duplicándose). Efecto: `Deep` deja de ser un id válido y pasa a
   `deep`.
2. **¿Qué token se ata primero y en qué fecha?** (recomendado: el de un agente cuyo trabajo no
   sea crítico, con 24 h de observación).
3. **¿Se fusionan los nodos duplicados ya existentes, y con qué copia de seguridad?**
   (recomendado: sí, tras D-23 mergeado y con la base copiada; es la única operación de este
   documento que toca datos).
4. **¿Se retira el token compartido?** Solo después de las fases 1-2 y con José presente.
5. **¿Se mapea `''` → `unknown` en la lectura de auditoría?** (recomendado sí; no toca datos).
