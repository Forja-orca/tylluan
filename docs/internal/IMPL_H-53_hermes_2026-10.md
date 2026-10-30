# IMPL H-53 — Validación de `coherence_semantic_scope`

**Asignación:** T2105 (TL claude-code) como C-3 → H-53. **Base:** `fc074b1` (origin/main). **Diff sin commitear** en `agent/hermes-base`. Spec del turno: «C-3 (validar `coherence_semantic_scope`) → @hermes como H-53».

## Cadena de cableado, verificada en código (no de memoria)

| Eslabón | Fichero:línea | Qué hace |
|---|---|---|
| Clave de config | `crates/tylluan-kernel/src/config.rs:1787-1788` | `#[serde(default = "default_coherence_semantic_scope")] pub coherence_semantic_scope: String` |
| Default | `config.rs:1885` + impl `Default` en `config.rs:1899` | `"all"` en ambos (coinciden) |
| Lectura al arrancar | `crates/tylluan-kernel/src/main.rs:1375` | `server.coherence_gate_semantic_scope = config.security.coherence_semantic_scope.clone()` |
| Campo del server | `crates/tylluan-kernel/src/transport/server/mod.rs:84` (struct) y `:176` (init `"all"`) | snapshot del proceso; **no** hay forma de cambiarlo en runtime |
| Consumo | `crates/tylluan-kernel/src/transport/server/handler_recall.rs:640` y `:877` | pasa el scope al gate en las dos rutas de recall |
| Decisión | `crates/tylluan-kernel/src/security/coherence_gate.rs:905` → `:790` `semantic_scope_penalizes` | rama que aplica o no la penalización semántica |

Conclusión del cableado: **el valor de la config sí llega al punto de decisión**, y queda fijado en el arranque (cambiar el fichero sin reiniciar no se aplica). Eso es correcto y coincide con lo que hace el resto de la config del gate.

## Hallazgo de la validación (el importante)

`coherence_gate.rs:790-798`:

```rust
match scope {
    SEMANTIC_SCOPE_CONFLICTING_TOP => score >= median_score,
    _ => true,
}
```

**Un valor desconocido, mal escrito o vacío no da error ni warning: se comporta como `"all"`, que es el modo MÁS agresivo** (castiga a cualquier candidato con coseno < 0.85, sea fuerte o débil). Consecuencia práctica: un operador que escriba `coherence_semantic_scope = "conflicting_ttop"` o `"Conflicting_Top"` cree que está suavizando el filtro y en realidad sigue con el comportamiento duro, sin ninguna señal. No hay validación en el load de config ni aviso en el log. Queda registrado como **laguna de UX/seguridad**, con `[needs-human-verify]` sobre si el TL prefiere arreglarlo con (a) validación dura en `SecurityConfig::validate` (rechazar valor inválido) o (b) `tracing::warn!` en el arranque. No lo aplico: cambiar la semántica de validación de una clave de seguridad es política del producto y la decide el TL/Jose. Lo que sí hice es hacerlo **visible en vivo** (abajo) para que sea detectable sin leer el código.

## Evidencia A/B ya medida (no la repetí; la citan los informes del equipo)

- `docs/reference/research/2026-10-28_T-DSH-128_ab_gate_all_vs_conflicting_top.md`: dos kernels con config idéntica salvo scope (A `all` en 47091, B `conflicting_top` en 47092), 40 preguntas.
- `STATUS.md:12`: **`conflicting_top` NO recomendado** — Recall@5 cayó de **27,5 % a 17,5 %** aunque penalice la mitad de los candidatos.
- Postura del TL ya registrada (citada en `docs/internal/IMPL_H-40_diagnostico_mapa_mem_hermes.md:40`): «recomiendo SI; decisión para Jose» sobre pasar a `conflicting_top`. **El dato medido apunta a lo contrario**: mi validación refuerza dejar el default en `"all"` y no tocar producción. Lo dejo explícito porque contradice una recomendación anterior del TL: el A/B (T-DSH-128) es posterior a esa postura, y es el que tiene números.
- `tylluan.toml` de producción: la clave `[security] coherence_semantic_scope` **no existe** en el repo (el fichero de producción no está trackeado; `tylluan.example.toml` tampoco la menciona) → producción corre `"all"` por default. Laguna secundaria de descubribilidad: ningún `.toml` de ejemplo documenta la opción.

## Qué cambié para que la validación sea posible sin tocar comportamiento

Endpoint **ya existente**, `GET /api/v1/security/coherence-gate/stats` (`crates/tylluan-kernel/src/transport/http/api_v1/api_security.rs:19`). Sin ruta nueva. Añade:

- `semantic_scope`: valor efectivo de `[security] coherence_semantic_scope` leído de la config.
- `semantic_scope_known`: `true` solo si es `all` o `conflicting_top` → un typo se delata en vivo.
- `semantic_scope_note`: advierte del snapshot de arranque y del fallback silencioso `_ => true`.
- Los tres contadores de capa que el struct `CumulativeGateStats` ya tenía y el endpoint **no exponía**: `total_pattern_eliminated`, `total_provenance_penalized`, `total_semantic_penalized` (`coherence_gate.rs:799-810`). Sin `total_semantic_penalized` no había manera de ver el efecto del scope; ahora sí.

Con eso, validar el scope en una máquina viva es: leer el endpoint, correr 5-10 recalls, volver a leer, y comparar `total_semantic_penalized` / `total_penalized` con `semantic_scope_known=true`.

## Tests entregados (`crates/tylluan-kernel/src/security/coherence_gate.rs`, `mod tests` línea 1128+)

1. `semantic_scope_all_penalizes_any_disagreeing_candidate` — `all` castiga fuerte y débil; en acuerdo (≥0.85) nunca.
2. `semantic_scope_conflicting_top_spares_weak_candidates` — solo penaliza score ≥ mediana; respeta el umbral de coseno (incluido el límite exacto 0.50 vs mediana 0.50).
3. `semantic_scope_unknown_value_behaves_as_all_documented_gap` — 6 valores inválidos (`""`, `"ALL"`, `"Conflicting_Top"`, `"conflicting_top "`, `"all2"`, `"ninguno"`) caen todos en `_ => true`. Este test **fija el hallazgo**: si alguien añade validación dura en el futuro, este test deberá cambiar a propósito, no por accidente.
4. `semantic_scope_default_is_all_in_config_struct` — `SecurityConfig::default()` == `SEMANTIC_SCOPE_ALL`; vigila que los tres defaults (serde, impl Default, init del server) no se desincronicen.

## Mapa del diff H-53

| Fichero | Cambio |
|---|---|
| `crates/tylluan-kernel/src/transport/http/api_v1/api_security.rs` | handler `coherence_gate_stats`: +6 claves (scope efectivo,_known, note, 3 contadores) |
| `crates/tylluan-kernel/src/security/coherence_gate.rs` | +4 tests puros del scope en `mod tests` |

## Impact

- **Critical path:** `transport/` (endpoint de stats) → requiere `## Impact` en el commit (ADR-013). No toco la lógica de decisión del gate ni sus umbrales: la función `semantic_scope_penalizes` queda byte-idéntica, solo gané tests sobre ella.
- **Defaults:** idénticos (`all` sigue siendo default en config y en server). Ninguna clave nueva de config.
- **Seguridad:** el endpoint ya estaba bajo el mismo nivel de exposición que el resto de `/api/v1/security/*/stats` (mismo middleware); solo ensancha el payload.
- **Decisiones que NO tomo:** no activo `conflicting_top`, no añado validación dura de la clave, no edito `tylluan.toml` de producción. Son políticas y van al TL/Jose.

## No verificado

- [no verificado] `semantic_scope` en el endpoint se lee de la config **en el momento de la llamada**, no del snapshot del proceso: si el fichero cambia tras el arranque, el valor mostrado puede no ser el que está en ejecución. Lo documenté en la nota del JSON; no añadí un getter del snapshot (no existe y tocar `ServerState` para sacarlo es más superficie que el beneficio).
- [no verificado] `total_semantic_penalized` en vivo: no ejecuté recalls contra un kernel real para ver el contador moverse (el kernel vivo es el de producción, y no lanzo procesos sin OK). La fuente del contador sí está cubierta por tests existentes del gate.
- [no verificado] Que el A/B de T-DSH-128 sea reproducible: cito sus cifras (`STATUS.md:12`) como están; no repetí la corrida de 40 preguntas.
- [no verificado] Que `conflicting_top` no tenga otros efectos además del Recall@5 (latencia, diversity): el informe de DSH mide recall; no busqué métricas adicionales.
