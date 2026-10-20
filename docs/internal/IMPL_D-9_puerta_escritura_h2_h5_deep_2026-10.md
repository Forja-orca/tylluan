# D-9 — Huecos H2 y H5 de la puerta de escritura ASI06 (Deep, 2026-10-20)

**Estado: no verificado** por terceros. Diff **sin commitear** en
`E:\tylluan-wt-deep`, base `origin/main` = `cad1477`.
Parche: `E:\tylluan-patches\deep\D-9.patch` (5 archivos).
Fuente: `docs/internal/IMPL_b4_coherencegate_puerta_escritura_buffy_2026-10.md` (Buffy).

**Recuento de B-4 (lo que pide el criterio): NO medido.** Ver §4 — no lo
prometo, lo declaro pendiente y dejo el procedimiento exacto.

## 1. H2 — la capa 2 no corría en escrituras anónimas

### Qué pasaba

`spawn_write_gate_judge` se invocaba **dentro** de `let node_id = if let Some(ref
aid) = rem_agent_id { … }` (`handler_remember.rs`). Con `agent_id`, el juez se
disparaba; sin `agent_id`, el nodo se escribía por la rama anónima y **la capa 2
nunca existía**. El comentario del código decía "fire-and-forget judge for every
remember"; la condición del `if` lo contradecía.

### Qué se hizo

La llamada se movió a **un único punto después** de resolverse `node_id`, con lo
que cubre las tres formas de escritura (agente, tarea pendiente y anónima). Sigue
siendo fire-and-forget: no añade latencia a la respuesta ni puede hacer fallar la
escritura.

Para poder afirmarlo sin depender de un LLM vivo, `write_gate.rs` gana un
contador `judge_spawn_count()` incrementado en el `spawn`. Es una propiedad de la
**invocación**, no del veredicto: el test no necesita que el juez responda nada.

Test: `handlers::tests::anonymous_remember_reaches_layer2_judge` — hace un
`tylluan_remember` **sin** `agent_id` y comprueba que el contador sube.

### Residual (fuera de alcance, no lo toco)

El camino de **DCR** (near-duplicate ≥0.87) hace `return` antes de resolver
`node_id`: refuerza el nodo existente y no escribe, así que tampoco pasa por el
juez. Es el hallazgo 6 de B-4, sigue abierto y no es H2. Consecuencia concreta:
una inyección que sea casi-duplicado de algo ya en memoria no se juzga nunca (y
sube el peso del nodo existente). Decisión de producto, no la tomo aquí.

## 2. H5 — el resumen de agente citaba memorias en cuarentena

### Plan corto (pedido por la cola, antes de tocar código)

| Pregunta | Respuesta implementada |
|----------|------------------------|
| ¿Qué entra en el resumen? | Los mismos candidatos que antes (15 de menor peso/más antiguos de `agent_memory`), **menos** los que están en cuarentena. |
| ¿El resumen pasa por el juez? | **No.** La opción simple: el resumen **hereda** la decisión ya tomada sobre sus fuentes. Juzgar el resumen sería otra llamada LLM por consolidación, con su propia latencia y su propio modo de fallo (ver D-10), y no añade garantía sobre el texto que ya se decidió excluir. |
| ¿Cómo se sabe que un nodo está marcado? | Por la columna `quarantined = 1` — la misma que escribe el juez (capa 2) y el rechazo duro (capa 1 no llega a escribir). No por el texto ni por metadatos. |
| ¿Qué pasa si todos los candidatos están en cuarentena? | No se crea resumen (ni un "Resumen de 0 memorias"). Se registra un `info!` y se sale. |
| ¿Se siguen decayendo las memorias? | Sí, las que se resumieron. Las en cuarentena **no** se decaen aquí: su ciclo lo gobierna el mantenimiento, que ya excluye `quarantined = 0`. |

### Qué pasaba

`consolidate_if_needed` tomaba 15 memorias y metía 80 caracteres **verbatim** de
cada una en un nodo `agent_summary`. Ese resumen lo lee `get_summary` y acaba en
el contexto de sesión que `tylluan_recall` antepone a cada llamada con
`agent_id`: el texto hostil que el juez había apartado volvía por la puerta de
atrás, ya sin ninguna marca de sospecha y en todas las sesiones siguientes.

### Qué se hizo

1. Nuevo `SilvaDB::quarantined_subset(&[String]) -> Vec<String>`: un solo
   `SELECT id FROM nodes WHERE quarantined = 1 AND id IN (…)` para los 15
   candidatos, en vez de 15 round-trips bajo el mismo mutex.
2. `consolidate_if_needed` filtra por ese conjunto antes de construir el texto, y
   sale sin crear resumen si no queda nada.

Test: `agent_memory::tests::consolidation_never_quotes_quarantined_memories` —
5 memorias (4 benignas + 1 con `HOSTILE_MARKER`), la hostil marcada
`quarantined = 1`; tras consolidar, el resumen existe, **no contiene** el marcador
y **sí** contiene las benignas.

## 3. Verificación ejecutada

```
cargo test -p tylluan-kernel --lib consolidation_never_quotes      → 1 passed
cargo test -p tylluan-kernel --lib anonymous_remember_reaches      → 1 passed
cargo test -p tylluan-kernel --lib                                 → 1110 passed; 0 failed; 4 ignored
cargo clippy -p tylluan-kernel --all-targets -- -D warnings        → EXIT 0
```

(Los 1110 incluyen los 1108 de la base + estos 2 tests nuevos.)

## 4. B-4 sobre el corpus: no medido, y por qué

El criterio pide repetir B-4 sobre `benchmarks/injection_corpus_v1.json` y
reportar cuántos nodos sospechosos acaban en cuarentena. **No lo he medido y no
doy una cifra.** Lo que impide hacerlo desde aquí:

- B-4 no es un test unitario: es un **drill con kernel de prueba vivo**
  (`test-run-gate/`, puerto y `TYLLUAN_DATA_DIR` propios) más el **juez LLM real**
  (`llama-server`), insertando los 80 nodos del corpus en SQLite **antes** del
  arranque y observando las cuarentenas resultantes.
- Mi entorno no puede arrancar procesos (regla del workspace: nunca lanzar
  procesos vía Bash; el AV bloquea el spawn en Windows). Y un test unitario no
  sirve: sin backend de razonamiento, `call_reasoning_backend_with_grammar`
  devuelve error y el juez no cuarentena nada — el resultado sería un 0 falso.

Procedimiento exacto para quien pueda lanzarlo (30 min, sin tocar producción):

```bash
# 1) kernel de prueba, puerto y datos propios
TYLLUAN_DATA_DIR=./test-run-d9 cargo run -p tylluan-cli -- start --config test-run-d9/tylluan.toml
#    (puerto distinto de 47004; comprobar: curl http://127.0.0.1:<puerto>/health)

# 2) para cada una de las 40 muestras de benchmarks/injection_corpus_v1.json,
#    un tylluan_remember con su texto (mismo método que B-4: MCP o REST con token),
#    esperando el juez fire-and-forget entre escrituras.

# 3) contar cuarentenas
curl -s -H "Authorization: Bearer $TOKEN" \
  "http://127.0.0.1:<puerto>/api/v1/memory/quarantine?limit=200" | python -c \
  "import sys,json; print(json.load(sys.stdin)['total'])"
```

Nota: el paso 3 usa el endpoint de **D-12**, que es el siguiente ítem de esta
misma cola. Si D-12 se integra primero, la medición es un `total`; si no, se
cuenta con `sqlite3 … "SELECT COUNT(*) FROM nodes WHERE quarantined=1"`.

Lo que **sí** puedo afirmar sin kernel: con el corte de `max_tokens` ya corregido
(`JUDGE_MAX_TOKENS=16`, en `main`) y con H2 cerrado, la ruta que antes era
inalcanzable (veredicto `SUSPICIOUS` → `UPDATE … quarantined = 1`) ya no tiene
ningún condicional de `agent_id` de por medio. Lo que no puedo afirmar es el
**porcentaje** resultante: depende del juez y de la tasa de fallo del 36 % que
investiga D-10.

## Impact

- **¿Depreda a otro agente?** No: toca `handler_remember` (mío por asignación de
  cola), `write_gate`, `agent_memory` y una función nueva en `silva/nodes.rs`.
  Sin cambios de contrato público de tools ni de esquema.
- **¿Depreda a un usuario?** Al contrario: cierra una vía por la que texto en
  cuarentena reaparecía en el contexto de sesión. Efecto secundario: los resúmenes
  de agentes con memorias en cuarentena serán algo más cortos (`source_count`
  menor). No se borra ni se decae nada que no se decaía antes.
- **¿Coste?** Una consulta SQL por consolidación (background, no camino caliente)
  y un `fetch_add` por escritura.
- **¿Rompe algo?** El único cambio de comportamiento observable es que el juez se
  dispara ahora también sin `agent_id`: más llamadas al backend de razonamiento
  (y ver D-10: es justo el backend que falla el 36 %). Si el TL prefiere limitar
  carga antes de arreglar D-10, este es el punto que hay que vigilar.
- **¿Se puede verificar sin mí?** Sí: los cuatro comandos de §3, más el drill de
  §4 cuando haya kernel de prueba.
