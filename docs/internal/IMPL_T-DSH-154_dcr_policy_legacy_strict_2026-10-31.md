# T-DSH-154 — Política DCR configurable (`[silva] dcr_policy = legacy | strict`)

- Agente: dsh
- Fecha: 2026-10-31
- Base: `origin/main = b9f1331` (push217), HEAD confirmado. **Encima del diff pendiente de T-DSH-153** (misma base; T-DSH-153 sigue sin integrar, el TL la está revisando).
- Encargo (T2250): política DCR detrás de `[silva] dcr_policy = 'legacy' | 'strict'`, **defecto `legacy` hasta medir**; `strict` = fusionar solo si es el MISMO `agent_id`, el MISMO tipo de nodo y similitud >= 0,95; si no, crear nodo propio **enlazado** al parecido (sin perder el texto). Tests de ambos modos.

## Diff

```
 crates/tylluan-kernel/src/config.rs                     |  16 +++
 crates/tylluan-kernel/src/main.rs                       |   1 +
 crates/tylluan-kernel/src/transport/server/mod.rs       |   5 +
 crates/tylluan-kernel/src/transport/server/handler_remember.rs | 157 +++++++++++++++++++--
 (+ diff pendiente de T-DSH-153: nodes.rs, fts.rs y tests/remember_long_text_recall.rs)
```

Sin tocar `Cargo.toml`, `ci.yml`, dashboard ni producción.

## Implementación

1. **Config** (`config.rs`): nuevo campo `SilvaConfig::dcr_policy: String` con `#[serde(default = "default_dcr_policy")]` → **`"legacy"` por defecto**; entrada en `SilvaConfig::default()`. Cualquier valor desconocido se comporta como `legacy` (fail-safe).
2. **Inyección** (`main.rs` + `server/mod.rs`): `TylluanServer` gana el campo `pub dcr_policy: String` (default `"legacy"` en el constructor) y `main` lo rellena desde `config.silva.dcr_policy` — mismo patrón que `coherence_gate_semantic_scope`.
3. **Decisión pura y testeable** (`handler_remember.rs`):
   - `enum DcrAction { Merge, Link, Skip }`;
   - `dcr_action(policy, sim, same_agent, same_type)`: `legacy` → Merge con sim >= 0,87 (como siempre); `strict` → Merge solo con sim >= 0,95 **y** mismo agente **y** mismo tipo; con 0,87 <= sim < 0,95 o contexto distinto → Link; por debajo de 0,87 → Skip;
   - `dcr_same_agent(metadata, aid)`: lee `metadata.agent_id`; sin la clave solo casa con `anonymous`.
4. **Handler DCR**: el bloque ahora lee la política del server, calcula `same_agent`/`same_type` (tipo nuevo: `task` si es tarea pendiente, `agent_memory` si hay agente, `memory` si anónimo) y hace `match`:
   - `Merge` → camino histórico de fusión (append + embedding + reinforce + juez), sin cambios;
   - `Link` → **no fusiona**, guarda el id del parecido y, tras crear el nodo propio, añade arista `add_edge(&node_id, &target, "near_duplicate", 0.9, "{}")` (el texto entrante se conserva íntegro);
   - `Skip` → flujo normal.

## Tests

| test | qué cubre |
|---|---|
| `dcr_policy_legacy_merges_similar_regardless_of_agent_and_type` | legacy fusiona con 0,87 sin mirar agente/tipo; <0,87 Skip; valor desconocido = legacy |
| `dcr_policy_strict_merges_only_exact_context` | strict: Merge solo con 0,95 + mismo agente + mismo tipo; 0,94 o agente/tipo distinto → Link; <0,87 → Skip |
| `dcr_same_agent_reads_metadata` | `metadata.agent_id` manda; sin clave solo casa `anonymous`; JSON inválido no casa |
| `dcr_policy_defaults_to_legacy_and_parses_strict` | default `legacy`; TOML vacío → legacy; `[silva] dcr_policy="strict"` parsea |

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` | **1447 passed, 0 failed, 4 ignored** (1443 previos + 4 nuevos) |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (tras corregir 2 lints: doc list y `if` colapsable) |
| `cargo test -p tylluan-kernel --tests` (pasada intermedia) | solo rojo **preexistente** `corpus_es_shape`; sin rojos nuevos. No se repitió tras los dos arreglos de lint (solo doc/let-chain, sin cambio de comportamiento) |

Entorno: `CARGO_PROFILE_DEV_DEBUG=line-tables-only`, `CARGO_INCREMENTAL=0`, `CARGO_TARGET_DIR=D:\cargo-target\shared-dev`, `-j 8`.

## Impact

- **Default `legacy` = cero cambio de comportamiento** en producción (no cambia recuperación; no requiere A/B por sí mismo). El interruptor `strict` queda disponible para el A/B de no-inferioridad que hará Hermes (H-58) antes de plantearlo como default.
- `strict` cumple la decisión del TL: mismo agente + mismo tipo + >= 0,95 para fusionar; el resto de parecidos >= 0,87 no se pierden: nodo propio + arista `near_duplicate`.
- Tests de ambos modos añadidos; clippy limpio.

## No verificado

- **No hay test de integración del handler completo** en modo strict (no se ejecutó `tylluan_remember` real con dos escrituras y motor para comprobar la arista `near_duplicate`); solo la decisión pura, `same_agent` y config. El camino `Link` (crear nodo + arista) está implementado pero no medido end-to-end.
- El umbral strict 0,95 y el tipo de arista `near_duplicate` no se han medido con datos reales; el A/B legacy vs strict es de Hermes (H-58).
- `--tests` no se repitió tras los dos arreglos de lint (solo doc/`if` colapsable, sin cambio de comportamiento); la pasada intermedia dio solo `corpus_es_shape` (rojo ajeno preexistente).
- El diff va **encima del pendiente de T-DSH-153** (nodes.rs/fts.rs/test) en el mismo worktree; si el TL integra T-DSH-153 por separado, este diff debe aplicarse después.
- No se probó con `dcr_policy` inválida en un TOML real (solo el fallback por código: cualquier valor ≠ "strict" = legacy).
- No se tocó producción ni el kernel (`:47004`); sin cambios en `Cargo.toml`/`ci.yml`/dashboard.
