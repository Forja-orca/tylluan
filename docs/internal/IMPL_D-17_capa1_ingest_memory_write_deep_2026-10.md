# D-17 — Capa 1 y juez en `memory/write` e `ingest` (y cierre del near-duplicado)

- **Fecha:** 2026-10-21
- **Agente:** Deep (OpenCode)
- **Cola:** `docs/internal/COLA_deep_2026-10-21.md`, ítem **D-17**
- **Hallazgo de origen:** H5 medido por Buffy en `docs/internal/IMPL_b9_h5_ingest_memory_write_buffy_2026-10.md`
- **Base:** `origin/main` @ `043eb71` (worktree `E:\tylluan-wt-deep`, diff sin commitear)
- **Patch:** `E:\tylluan-patches\deep\D-17.patch` (4 archivos, +288/−3)

## Qué estaba roto (medido, no supuesto)

`tylluan_remember` (MCP) tenía las dos capas ASI06 desde antes: capa 1 determinista
(`poison_patterns::matches_injection_pattern`) sobre el contenido crudo antes de tocar la BD,
y capa 2 (juez LLM, `spawn_write_gate_judge`, fire-and-forget) tras escribir. B-9 midió que
**dos rutas REST de escritura no tenían ninguna de las dos**:

| Ruta | Muestras de inyección (capa 1) | Juez (capa 2) |
|------|-------------------------------|----------------|
| `POST /api/v1/memory/write` | 8/8 aceptadas sin marca | nunca lanzado |
| `POST /api/v1/ingest` | 8/8 guardadas sin marca | nunca lanzado |

Las 8 muestras de `benchmarks/injection_corpus_v1.json` que la capa 1 detecta y que ambas rutas
dejaban pasar: `inj_01, inj_02, inj_07, inj_12, inj_19, inj_22, inj_35, inj_36`.

Además quedaba abierto un cuarto camino (hallazgo de D-9): el retorno temprano por
**near-duplicado** (`DCR_THRESHOLD = 0.87`, `handler_remember.rs`) refuerza un nodo existente y
devuelve **antes** de tener `node_id`, así que ni siquiera `tylluan_remember` llegaba al juez por
esa vía.

## Qué cambia

1. **`memory/write` — rechazo duro (misma severidad que `remember`).**
   `matches_injection_pattern(content)` antes de escribir → `403` con
   `{"error":"ACCESS_DENIED","reason":…}`. No se crea nodo, no se gasta juicio.
   Es una escritura de *agente* recordando algo: el contrato es el de `tylluan_remember`.

2. **`ingest` — guardar pero MARCAR (no rechazar).**
   `ingest` guarda *documentos*, y un documento legítimo puede hablar de inyección de prompts
   (un paper, un cheat-sheet de OWASP, este mismo informe). Rechazarlo sería un falso positivo
   sobre contenido válido. Si la capa 1 dispara, el metadata del nodo pasa a llevar:
   - `untrusted_content: true`
   - `untrusted_reason: "ASI06 Layer 1: content matches a known prompt-injection pattern"`

   Es la misma marca que ya lee la capa 2 (`coherence_gate` usa `untrusted_content` para bajar la
   confianza del contexto recuperado), así que la marca es útil fuera de este ítem.

3. **Capa 2 en ambas rutas.** Tras un upsert correcto se llama a
   `spawn_write_gate_judge(state.silva.clone(), node_id, content)` — el mismo helper de
   `handler_remember`, fire-and-forget: no añade latencia a la respuesta ni puede hacer fallar la
   escritura. Respeta el semáforo de C1+C2 (`JUDGE_MAX_CONCURRENCY = 1`, `JUDGE_QUEUE_WAIT = 60 s`,
   `note_busy_skip()` si no hay ranura).

4. **Near-duplicado (DCR) — el cuarto camino.**
   Antes de devolver el "reinforced existing node", se lanza el juez **sobre el nodo que se
   refuerza** (`existing_node.id`, `existing_node.content`), no sobre el texto descartado: el texto
   entrante no entra en memoria, el riesgo real es reforzar un nodo cuyo contenido nunca se juzgó.
   Si el juez lo marca, la cuarentena normal actúa sobre ese nodo.

## Verificación real

- **Rojo primero.** Con los tres archivos de producción revertidos a `origin/main` y el test nuevo
  en su sitio:
  `inj_01: memory/write must reject, got {"node_id":"memory_write:…","status":"ok"}` → `0 passed; 1 failed`.
  Reproduce la medición de B-9 exactamente.
- **Verde.** `cargo test -p tylluan-kernel --test d17_layer1_write_paths` → `1 passed` (72 s; el
  ruido `No module named 'guilds'` es del arranque de guilds en el harness, preexistente).
- **Lib.** `cargo test -p tylluan-kernel --lib` → `1132 passed; 0 failed; 4 ignored` (47.63 s).
  Sin tests de lib nuevos: los cuatro caminos se comprueban por HTTP real contra el router de
  producción, que es donde se midió el fallo.
- **Clippy.** `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` → `Finished`, EXIT 0.

Qué cubre el test nuevo (`crates/tylluan-kernel/tests/d17_layer1_write_paths.rs`, un solo
`#[tokio::test]` a propósito, porque mide un contador global y una variable de entorno):

1. La capa 1 detecta **exactamente** el conjunto que midió B-9 (si el corpus cambia, avisa).
2. Las 8 en `memory/write` → `403 ACCESS_DENIED`, **0 nodos** en SilvaDB con ese texto, y el
   contador de juicios **no se mueve** (un rechazo no gasta juicio).
3. Las 8 en `ingest` → `200`, nodo guardado con `untrusted_content: true` y razón que cita Layer 1,
   y **8 juicios** lanzados.
4. Las 40 benignas de `benign_corpus_v1.json` → `memory/write` 200 y `ingest` 200, sin marca y sin
   rechazo (0 falsos positivos), con `+2×40` juicios: total exacto `+88`.

El juez se apunta a `TYLLUAN_KERNEL_URL=http://127.0.0.1:9` (puerto muerto) desde el test: se
verifica **que se lanza** (contador), no el veredicto del LLM, y así el test nunca habla con el
kernel vivo de la máquina ni puede mover cuarentenas reales.

## Impact

- **Contrato HTTP:** `POST /api/v1/memory/write` pasa a poder devolver `403` con
  `{"error":"ACCESS_DENIED"}`. **Es un cambio de comportamiento para clientes**: cualquier cliente
  que escribiera contenido que case con un patrón de inyección (p. ej. un digest de Coloquio que
  cite literalmente un ataque) ahora recibe 403 en vez de 200. Los que escriben contenido normal no
  cambian. `POST /api/v1/ingest` **no** cambia de código de estado: solo añade `untrusted_content`
  al metadata del nodo (aditivo).
- **Carga del juez:** cada escritura aceptada en estas dos rutas (y cada refuerzo por near-duplicado
  de `tylluan_remember`) ahora encola un juicio. Con `JUDGE_MAX_CONCURRENCY = 1` esto compite por la
  ranura única de C1+C2; cuando se satura, el juicio **se salta** (no bloquea) y queda contado en
  `judge_telemetry().busy_skips`. Es el diseño aceptado en D-14, pero el volumen de juicios sube.
- **Datos existentes:** no hay migración. Los nodos ya guardados por estas rutas siguen sin marca;
  el ítem cierra la escritura *nueva*, no reescribe histórico.
- **Permisos/ACL:** sin cambios. No se toca `dev_mode`, ni tokens, ni `host`/`port`.
- **No-depredación:** no se añaden dependencias, ni procesos, ni rutas de arranque; no se toca
  `README.md`/`AGENTS.md`/`STATUS.md`. Sin commit (regla de la flota: Deep entrega diff).

## No verificado (honestidad de alcance)

- **No hay test dedicado del camino DCR/near-duplicado.** Ese branch necesita un motor de
  embeddings real (`server.matcher.engine()`), que en los tests del harness devuelve `None`; se
  verificó por lectura y por el orden del código (el `spawn_write_gate_judge` va antes del `return`),
  no por ejecución. Buffy puede cubrirlo en B-12 si le resulta barato con un motor falso.
- **No hay medición del juez en vivo** (veredicto LLM real, cuarentena real): este ítem verifica que
  el juicio se lanza, no qué decide. Requiere kernel + `llama-server` reales.
- **`POST /api/v1/ingest/upload` no se toca**: no escribe nodos en Rust, delega en los guilds
  Python `ingest`/`vision`, cuyo pipeline queda fuera de esta cola.
- **`api_v1/a2a.rs:544`, `api_federation.rs:155` y `api_coloquio.rs:408`** también escriben nodos sin
  capa 1. No estaban en D-17 (B-9 midió solo estas dos rutas + el DCR); quedan como hallazgo abierto
  para que el TL decida si merecen ítem propio.
- **Colisión de `node_id` en `memory/write`:** el id es `memory_write:<millis>`, así que dos
  escrituras en el mismo milisegundo se pisan (el comentario dice "unique id per call" y no lo es).
  Detectado al escribir 48 nodos en bucle; **no se arregla aquí** (fuera de alcance, y arreglarlo
  cambia IDs que otros clientes podrían estar leyendo).

## Siguiente

`D-18` según la cola.
