# IMPL T1252 — Reapertura del canal `instructions` en el proveedor (Deep, 2026-10-18)

- **Tarea**: Coloquio turno 1252 (TL): reabrir el canal de `instructions` en el proveedor
  **sin cambiar el comportamiento cuando no hay instructions**; instructions como contenido
  no confiable con límite de longitud; sin tocar las reglas deterministas ni el invariante
  v8; entrega = tests nuevos + ADR-020 con `## Impact` + clippy `-D warnings`;
  **orden explícita**: NO re-medir redacciones hasta la v2 de Buffy.
- **Base**: `9e43e2b` (worktree `E:\tylluan-wt-deep`, diff sin commitear — solo Claude commitea).

## 1. El problema (hallazgo del duelo P2, confirmado contra el código)

`QuestionSpec.instructions` ya existía en el wire (§1217: tope 1 KiB en `validate_request`,
`api_types.rs:233`), pero el canal estaba CORTADO por dentro:

1. `to_core_question` (`api_types.rs:273`) traducía `QuestionSpec` → `DecisionQuestion`
   y **descartaba `instructions`**;
2. `question_spec` del proveedor (`systemone_http.rs:287`) generaba un texto fijo propio
   (`"Answer the {name} question about this state."`).

Resultado: el wire aceptaba instructions, validaba su longitud… y jamás llegaban al modelo.
Solo `state` + nombres de opciones + nombre de pregunta subían al servidor (lo observado en
el duelo: `systemone_http.rs:287`).

## 2. Qué se hizo

| Pieza | Cambio |
|---|---|
| `decision/mod.rs` | Nuevo `CoreQuestion { kind: DecisionQuestion, instructions: Option<String> }`; `DecisionRequest.questions: HashMap<String, CoreQuestion>`. **El enum `DecisionQuestion` queda intacto.** Constructores: `CoreQuestion::new` (con canal) / `without_instructions` (canal cerrado). |
| `api_types.rs` | `to_core_question` devuelve `CoreQuestion` con las instructions crudas del wire (sin interpretar: la desconfianza vive en la frontera del proveedor). |
| `systemone_http.rs` | `question_spec(name, &CoreQuestion)` + nuevo `resolve_instructions`: ausente/vacío→**defecto byte idéntico al previo a T1252**; saneado de caracteres de control (conserva `\t\n\r`); >`MAX_PROVIDER_INSTRUCTIONS_BYTES` (1024 bytes UTF-8, tope propio independiente del wire, defensa en profundidad) → **ignorado** con `warn`, cae al defecto — nunca truncado. `distribution_for` sigue recibiendo `&q.kind`. |
| `rules.rs` | Un solo cambio: `match &question.kind` (las reglas deciden SOLO sobre el tipo tipado; `instructions` no participa). Los tests de reglas construyen igual vía `req_with`, que cierra el canal internamente — **cero cambio de lógica/umbrales**. |
| `service.rs` | Acomodos de tipo (`CoreQuestion` en el import y en la anotación de `spawn_shadow`); el resto fluye por `to_core_question`. Carril libre y sombra de contrato heredan el canal. |
| `examples/decision_fabric_spike.rs` | Acomodos mecánicos (`.kind` en matches, envoltorios en 6 inserciones, 3 anotaciones de tipo). Sin cambio de comportamiento del spike. |
| `tests/decide_systemone_http.rs` | Mock ahora conserva `last_body` (captura el body POST) + 2 tests E2E nuevos. |

**Sin tocar**: esquema wire §1217, superficie MCP/REST, reglas deterministas (umbrales y
lógica), rechazo de valores fuera de opciones, invariante v8 (la sombra jamás altera la
respuesta emitida), config, timeouts.

## 3. Tests nuevos (7 en total)

**Unidad (lib):**
1. `to_core_question_carries_wire_instructions_verbatim` — passthrough Some/None (api_types).
2. `wire_instructions_reach_the_model_spec_verbatim` — instructions → body del spec, verbatim;
   `type`/`criteria` intactos.
3. `absent_instructions_keep_the_pre_t1252_default` — None → defecto EXACTO en los 3 tipos.
4. `oversized_instructions_fall_back_instead_of_truncating` — 1025 B → defecto; 1024 B exactos → pasa.
5. `control_characters_are_stripped_and_blank_falls_back` — `"AB\u{0}CD\nEF\u{7}GH"` → `"ABCD\nEFGH"`;
   solo-controles / en blanco → defecto.

**E2E (carril libre contra mock con captura de body):**
6. `free_lane_forwards_wire_instructions_to_the_model` — wire → proveedor → body POST verbatim,
   respuesta de la llamada idéntica (no degrada nada).
7. `free_lane_without_instructions_keeps_the_default_body` — sin instructions → el body lleva
   exactamente el texto pre-T1252.

## 4. Verificación (ejecutada, sin apoyarse en nada previo)

| Paso | Resultado |
|---|---|
| `cargo check -p tylluan-kernel --all-targets` | **OK** (0 errores/avisos) |
| `cargo test -p tylluan-kernel` (lib + TODAS las integraciones) | **lib 1053 passed, 0 failed**; integraciones todas `ok` — incluye `decide_systemone_http` 13 (11+2 nuevas), `decide_adversarial` 4, `decide_parity_e2e` 20, y el bloque v8/rejection en verde |
| `cargo clippy -p tylluan-kernel -- -D warnings` (comando exacto del TL) | **EXIT 0** |
| `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **EXIT 0** (tests y ejemplo incluidos) |
| `scripts/check_test_count.sh` | tras `--fix`: **VERDE** — README 1139 ✅ AGENTS `kernel=1053 link=74 fsrs=12 total=1139` ✅ |
| Re-medición de redacciones | **NO HECHA** (orden del TL: esperar v2 de Buffy) |

**Recuentos honestos**: lib kernel 1048 → **1053** (+5: 4 systemone + 1 api_types);
total 1134 → **1139**. Actualizado en README.md y AGENTS.md vía `check_test_count.sh --fix`
y en la línea general de STATUS.md a mano. **Pendiente para el TL al commitear**: la línea
HEAD/fecha de STATUS.md (línea ~50) lleva `3db6890` + "1134 total" de 2026-10-15 — es una
línea atada al hash del commit (manual por diseño) y solo Claude la puede poner con el hash
real del commit que cierre esto.

## 5. Documentación

- **ADR-020** (`docs/reference/adr/ADR020_inference_gateway.md`):
  - nueva §3.9 "Canal de `instructions` por pregunta (T1252, 2026-10-18)" — estado previo
    cortado, diseño, límites no-confiables, alcance (libre + sombra) y blindaje;
  - bullet de VERIFICADO en §8 (Marcado de Veracidad) con los tests;
  - sección **`## Impact`** al final, como exigió el TL.

## Impact

- **Producción**: mismo comportamiento en todo camino existente — sin instructions el body
  del proveedor sale byte idéntico al de antes (2 tests lo afirman: unidad + E2E). Las
  reglas deterministas y el invariante v8 quedan intactos verificados por sus suites
  originales (nada de esa lógica se editó salvo el acceso `kind`).
- **Superficie externa sin cambios**: wire §1217, MCP `tylluan_decide`, REST
  `POST /api/v1/decide` — el único efecto observable para un cliente es que las instructions
  que ya enviaba **ahora sí llegan** al modelo.
- **Seguridad**: instructions tratadas como no-confiables en la frontera del proveedor
  (tope 1024 B propio + saneado de controles + fallback; nunca truncado), en paralelo al
  tope de wire de §1217 — doble barrera, cero ruta de escape (viaja como valor JSON de
  `serde_json`).
- **Recuentos**: +5 lib tests y +2 E2E → docs de recuento sincronizados (README/AGENTS vía
  el gate, STATUS línea general a mano; línea HEAD pendiente del commit, anotado arriba).
- **No tocado (orden del TL)**: ninguna medición del duelo P2 se re-ejecuta ni se re-reporta;
  la reapertura queda lista para que la v2 de Buffy (redacciones) mida sobre un canal que
  ahora funciona.
