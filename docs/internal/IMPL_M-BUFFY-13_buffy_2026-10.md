# IMPL M-BUFFY-13 — cabeceras veraces, 6 rojos clasificados, supervisor y hueco de CI

**Autora:** Buffy · **Fecha:** 2026-10-27 · **Base:** `origin/main` = `HEAD` = `d79990a` (verificado con `git rev-parse`, sin rebase pendiente).
**Alcance:** cierre de los hallazgos de M-BUFFY-12 (aceptado). Solo comentarios de test, motivos de `#[ignore]`, un informe y **propuestas** sin aplicar. **Cero código de producción.**
**Estado del diff:** sin commitear, como manda la cola (solo el TL comitea). 12 ficheros, +110/−74 líneas reales.

---

## 1. Cabeceras y motivos: dicen la verdad de hoy

Regla aplicada: donde el fichero hablaba de un `#[ignore]` ya retirado, o de un recuento que caducó, se sustituye por el estado verificado el 2026-10-27 con el commit que cerró el defecto. **Ningún atributo `#[ignore]` se ha añadido ni retirado en este ítem**; solo texto.

| Fichero | Decía | Dice ahora | Evidencia |
|---|---|---|---|
| `tests/cola_d29_d30_acceptance.rs:15-19` | «un test VIVO del comportamiento actual … y un test `#[ignore="D-29"\|"D-30"]` que hoy falla» | D-29/D-30 **CERRADOS en `396af2b`**; los 4 tests corren en la suite normal (0 ignorados): 2 de aceptación + 2 guardas | `norm_cola_d29_d30_acceptance.log`: `ok. 4 passed; 0 ignored` |
| `tests/cola_d29_d30_acceptance.rs:105` | «Hoy falla: no hay copia y el corrupto se pierde» | «Cumplido desde `396af2b`» | idem |
| `tests/cola_d29_d30_acceptance.rs:193-194` | «hoy `store_plan` solo escribe en memoria» | persiste en la base del kernel y `init_plan_store` recarga pendientes | `grants.rs` + D-30 verde |
| `tests/cola_d29_d30_acceptance.rs:28` y `cola_d32:17` y `cola_d35:26` | línea `Run:` con `-- --ignored` | nota: sin tests ignorados, `--ignored` no ejecuta nada | `-- --ignored` devuelve «0 tests (N filtrados)» en los tres |
| `tests/cola_d35_acceptance.rs:16-26` | «**vivo**: fija que HOY el catálogo da `None`; Deep debe invertirlo» + dos `#[ignore="D-35"]` | D-35 **CERRADO en `4fdfb2e`**; 3 tests en suite normal y en verde; el «vivo» es hoy pin de regresión en sentido contrario | `norm_cola_d35_acceptance.log`: `ok. 3 passed; 0 ignored` |
| `tests/cola_d35_acceptance.rs:74,94` | «**DESEADO**… tras arreglar el parser», «Hoy falla ya en el primer paso» | «**Aceptación**… cumplido desde `4fdfb2e`» | idem |
| `tests/decide_adversarial.rs:5-30` | sección «ESTADO DE COMPILACIÓN» con tabla de 8 filas «`#[ignore]` hasta endpoint» | endpoint y `api_types` aterrizaron (`8f16a97`/`5a4a806` + turno 1219); **0 atributos `#[ignore]`, 13 tests en verde**; tabla conservada como historia con columna «Estado hoy» | `grep -n '^#\[ignore'` → ninguno; `norm_decide_adversarial.log`: `13 passed` |
| `tests/oauth_f2_consent_adversarial.rs:14-16` | «los que fallan HOY llevan `#[ignore]`» | 7 vectores pasan, **ninguno lleva `#[ignore]`** | `norm_oauth_f2_consent_adversarial.log`: `7 passed; 0 ignored` |
| `tests/regression_d55_d56_outside_crate.rs:159-165` | «IGNORADO hasta que se cierre el deadlock … quitar `#[ignore]` cuando el fix aterrice» | **ACTIVO desde D-58 (`d1a375c`)**; corre en suite normal y pasa | `norm_regression_d55_d56_outside_crate.log`: `2 passed` |
| `tests/regression_origin_host_outside_crate.rs:16` | «solo corren con `--ignored`» | 8 casos corren siempre desde `346cf65` | `norm_regression_origin_host_outside_crate.log`: `8 passed` |

**Ya decían la verdad (verificado, sin tocar):** `tests/oauth_f2_adversarial_spec.rs:10-23` («los 8 casos corren en la corrida normal»; re-verificado `8 passed; 0 ignored`) y `tests/d32_vias_escritura_regresion.rs:4` («ya sin `#[ignore]`»; `4 passed`).

**Motivos con recuentos viejos (los 3 de la cola, más 1 que caducó con D-30):**

| Fichero:línea | Motivo viejo | Motivo nuevo |
|---|---|---|
| `approve_action_intent_adversarial.rs:143` | «permite al autor aprobar su plan … F1,F3 ya corregidos» | «`agent_id` declarativo permite consumir el plan de **otro** autor (spoof) … **15/16 filas conformes**» (el fallo real es X1, no el autor) |
| `trust_banners_adversarial.rs:138` | «vectores abiertos… (banner ausente en think/@inbox legacy, localidad auto-creada, herencia no retroactiva)» | «**1 vector abierto de 12** (B2); think/@inbox legacy y herencia retroactiva **ya cerrados**» |
| `tylluan-evals/src/tests.rs:563,616` | «takes minutes on CPU» | «**~49 min** (medido 2026-10-26, M-BUFFY-12: 2932 s)» |
| `continuity_probes.rs:362-371` | «los planes pendientes NO persisten entre procesos» | premisa **caducada por D-30**; el rojo mide una divergencia real entre `get_plan` y `list_pending_actions` (§3) |

---

## 2. `knowledge_guild_test`: ¿a CI? **NO, y con dato**

Tres hechos medidos, cada uno suficiente:

1. **Invisible sin feature.** `cargo test -p tylluan-kernel --test knowledge_guild_test` compila el target con **0 tests** (`ok. 0 passed; 0 filtered out`) porque todo el módulo está bajo `#[cfg(feature = "guild_knowledge_test")]`. CI nunca lo ejecuta y no avisa.
2. **No compila con la feature.** `--features guild_knowledge_test` → `error[E0599]: no method named write_all` ×3 (líneas 47, 106, 107): falta `use std::io::Write;` dentro del módulo gateado.
3. **El objetivo no existe.** Invoca `python -m guilds.core.knowledge` y su motivo dice «requires … `guilds/core/knowledge.py`». **Ese fichero no existe** (`ls` → No such file); el guild real es `guilds/scholars/plugins/knowledge.py`. Aun con `.venv` y con el import arreglado, el proceso muere con `ModuleNotFoundError: No module named 'guilds.core.knowledge'`.
4. **Y su aserción no puede pasar.** El primer test hace `child.kill()` y luego exige `status.success() == true`: un hijo **matado** nunca termina con éxito en Windows, así que el test está en rojo garantizado incluso con un guild perfecto.

**Decisión propuesta:** **no** añadirlo a CI. Es deuda muerta que da falsa cobertura (nombre de integración, cero aserciones útiles). Dos salidas, en orden de preferencia:

- **A (recomendada, S):** borrar `knowledge_guild_test.rs`. El guild `knowledge` ya tiene cobertura estructural (`test_every_guild_file_is_in_catalog`, registro lazy/always-on) y de contenido en `asi06_layer1_ingest_memory_write` vía el camino real.
- **B (si se quiere integración viva, M):** reescribirlo contra `guilds.scholars.plugins.knowledge`, con `use std::io::Write;`, lectura de stdout por líneas JSON-RPC (sin `kill()` antes de leer) y job de CI **no bloqueante** con `.venv` sembrado. Diff exacto del mínimo viable:

```rust
 // crates/tylluan-kernel/tests/knowledge_guild_test.rs
 #[cfg(feature = "guild_knowledge_test")]
 mod knowledge_guild_tests {
+    use std::io::Write;
     use std::process::{Command, Stdio};
@@
-            .args(&["-m", "guilds.core.knowledge"])
+            .args(&["-m", "guilds.scholars.plugins.knowledge"])
```

```yaml
 # .github/workflows/ci.yml (solo con la salida B; hoy NO se propone aplicar)
       - name: Knowledge guild handshake (opt-in, necesita .venv)
         run: cargo test -p tylluan-kernel --features guild_knowledge_test \
              --test knowledge_guild_test -- --ignored --nocapture
```

No he modificado el workflow (la cola lo prohíbe si hace falta tocar nada más; aquí hacen falta las dos cosas).

---

## 3. Los 6 `#[ignore]` que fallan de verdad: gravedad verificada

Reproducción común (un build a la vez, `-j 4`, target dir dedicado; logs en `E:\tylluan-patches\buffy\mbuffy13\`):

```bash
CARGO_TARGET_DIR=/e/cargo-target/buffy cargo test -p tylluan-kernel \
  --test <fichero> -- --ignored --nocapture
```

### CRÍTICOS (arriba: son el mismo defecto de fondo, el gate HITL de aprobación)

**C-1 · D-13 — cualquier agente aprueba sin credencial de operador.** `approve_action_self_approval.rs:144` (rojo en `:280`).
Reproducción mínima: `d13_self_approval_requires_operator_credential` → **4 casos sin cumplir**:
- `CASO 2`: otro agente (no operador) envía la aprobación → **HTTP 200** y el plan queda **consumido**; esperado rechazo con `operator_required`.
- `CASO 2`: el plan **fue consumido** por un agente no operador (debe seguir pendiente).
- `CASO 4`: 1 fila de audit con `human_intervention=[0]` para un intent rechazado.
- `CASO 4`: la aprobación del operador **no** generó ninguna fila con `human_intervention=1` (el camino HITL no queda registrado).

Lectura: el «humano en el bucle» de `tylluan_do(plan=true)` es hoy **decorativo**; la auditoría además registra la intervención humana al revés. Gravedad **CRÍTICA**, esfuerzo **M** (binding token→rol operador en `approve_action` + corregir la escritura de audit). Dueño: Deep.

**C-2 · X1 — spoof de `agent_id` para consumir el plan de otro autor.** `approve_action_intent_adversarial.rs:143` (rojo en `:386`).
Matriz de 16 filas: **15 conformes, 1 no**: `X1 spoof agent_id` → declarando `innocent-agent` se consumió un plan cuyo autor es `plan-author`, **HTTP 200**. Es el mismo agujero que C-1 por la puerta del lenguaje natural: sin binding `token → agent_id`, la identidad es una cadena que el llamante elige. Gravedad **CRÍTICA**, esfuerzo **M** (mismo fix que C-1; José debe decidir el binding). Dueño: Deep.

> Nota de encuadre: la cola pide «¿es un CRÍTICO?» por hueco. C-1 y C-2 lo son por separado y **comparten causa raíz**; cerrarlos por separado dejaría el otro abierto. Se recomienda un solo trabajo con dos tests de aceptación ya escritos.

### ALTA

**A-1 · T-DSH-10 B2 — un externo que se auto-registra identidad pasa por local y viaja sin banner.** `trust_banners_adversarial.rs:138` (rojo en `:524`).
Matriz: **11/12 conformes**; falla `B2 mailbox_send`: `identidad_creada=true banner=false`. Es decir: llamar a `mailbox_send` y crear la identidad del remitente **concede localidad**, y el contenido externo entra al contexto del modelo sin `[CONTENIDO NO CONFIABLE]`. Gravedad **ALTA** (degrada la defensa anti-inyección por etiqueta), esfuerzo **S/M** (la localidad debe derivarse de una identidad **previa y confiable**, no de la que el propio mensaje creó). Dueño: Deep.

### MEDIA

**M-1 · `t25_b2` — el plan persistido no aparece en `list_pending_actions` tras el reinicio.** `continuity_probes.rs:370` (rojo en `:413`).
Salida: `reinicio real: el plan pendiente no debería seguir ahí; in-process sigue por el estático: No pending actions.` Interpretación verificada: D-30 hizo que los planes **persistan** (correcto, `396af2b`), así que el assert viejo ya no aplica; pero el síntoma que destapa es real: `get_plan("t25-plan")` encuentra el plan y la herramienta `list_pending_actions` responde «No pending actions» — **dos vistas del mismo store que no coinciden**, y el operador no ve la aprobación que existe. Gravedad **MEDIA** (no ejecuta nada por sí solo; sí oculta trabajo pendiente), esfuerzo **S**. Dueño: Deep. El test debe invertirse a «el plan sobrevive Y aparece en el listado».

### BAJA / deuda de test

**B-1 · `vias3_6` — INVERTIDO: el defecto que documentaba está cerrado.** `asi06_redteam_vias3_6.rs:409` (rojo en `:460`).
Qué invierte, exactamente: el CASO B afirmaba que una cadena de 3 memorias con veneno y **ninguna marcada** producía un resumen **limpio** (el bypass de Unit 42: el veneno viajaba a memoria persistente sin banner en ningún eslabón). Hoy el resumen **sale marcado** y el test falla con «el bypass ya no existe. Actualiza el informe». Causa verificada: `AgentMemoryManager::record_memory` aplica `trust::mark_if_layer1_flagged` en la escritura (`memory/agent_memory.rs:221-229`, arreglo de D-32 V4), de modo que las fuentes ya nacen marcadas y la herencia de `consolidate_if_needed` (`agent_memory.rs:414-431`) hace su trabajo. **No hay hueco de seguridad**: el test describe el mundo anterior. Su inversión pendiente es editorial y de una línea de aserción, pero es una **victoria**, no un riesgo. Gravedad **BAJA**, esfuerzo **S**. Dueño: Buffy (siguiente pasada).

**B-2 · `a2a_live` — no es un hueco: es un harness externo ausente.** `a2a_live.rs:18` (rojo en `:25`).
`card fetch: … (os error 10061)` contra `127.0.0.1:8901`: exige el agente echo del SDK oficial de A2A corriendo en local. No se puede cumplir en CI sin instalar Node/npx (prohibido por la cola y por la regla de máquina). Gravedad **BAJA**, esfuerzo **S** (documentar el harness o moverlo a `#[ignore]` con motivo de entorno, que ya tiene). Dueño: TL decide si se queda como prueba manual.

**Resumen del ítem:** 2 CRÍTICOS (una causa raíz), 1 ALTA, 1 MEDIA, 2 BAJA (de las cuales 1 es un defecto ya cerrado y 1 es falta de entorno).

---

## 4. Supervisor: 528 avisos idénticos en 30 min

**Emisor:** `crates/tylluan-kernel/src/registry/supervisor.rs:41`.
**Mecánica:** el bucle revisa cada `check_interval_secs`; para cada guild `always_on` caído con `retries >= 3` entra en la rama y **emite `error!` en cada tick**, sin recordar que ya lo dijo. En el soak: 3 guilds (`bash`, `filesystem`, `memory`) × 176 ticks = **528 líneas idénticas**.

**Arreglo propuesto (NO aplicado), una línea lógica:**

```rust
if *retries >= 3 { if *retries == 3 { error!("🔥 [Supervisor] Guild '{}' failed {} times — marking as degraded, no further restarts", name, retries); } *retries = retries.saturating_add(1); continue; }
```

Efecto: el aviso sale **una vez por guild** (al llegar a 3) y el contador sigue por encima del umbral (4, 5… saturando), de modo que el comportamiento de «no reintentar» no cambia — solo deja de repetir el mensaje. Alternativa equivalente, más legible en tres líneas: guardar `*retries == 3` para el `error!` y hacer `*retries = (*retries).saturating_add(1)` antes del `continue`. No lo aplico: código de producción, y la cola pide propuesta.

---

## Verificación ejecutada (misma máquina, `CARGO_TARGET_DIR=E:\cargo-target\buffy`, `-j 4`)

| Gate | Comando | Resultado |
|---|---|---|
| Clippy (CI usa `-D warnings`) | `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` | **exit 0** (5m08s) |
| Suite lib, mismos números | `cargo test -p tylluan-kernel --lib` | **1301 tests → 1297 passed, 4 ignored, 0 failed** (idéntico a `lib_before`: 1297/4) |
| Enlaces de documentación | `python scripts/check_doc_links.py` | **exit 0** — 184 enlaces en 432 `.md` |
| Inventario de `#[ignore]` | `grep -rn -E '^\s*#\[ignore' crates/` | **24 atributos** (18 en kernel, 2 en evals, 4 en src) · 6 rojos verificados |

## Impact

- **Superficie de producción:** ninguna. No se toca `src/`, ni timeouts, ni workflows, ni `tylluan*.toml`. El diff son comentarios de test, cuatro cadenas de motivo de `#[ignore]`, un informe nuevo y (ya en el diff previo) la cabecera de `regression_origin_host` + el `quote()` de canal en `scripts/coloquio_loop_buffy.py` (arreglo de un `UnicodeEncodeError` real al leer `#recomendación`, verificado en vivo el 2026-10-26).
- **Comportamiento:** ninguno cambia; `#[ignore]` no se añaden ni se retiran, así que la suite normal ejecuta exactamente los mismos tests antes y después (1297/4 lo confirma).
- **ADR-013 (no depredación):** no hay borrado de trabajo ajeno ni reescritura de historia; el worktree está sobre `origin/main` exacto (`d79990a`) y el diff se entrega sin commitear para que solo el TL integre.
- **Riesgo residual:** los mensajes de pánico de `cola_d35` siguen diciendo «D-35 pendiente» (solo se ven si el test **regresiona**); cambiarlos es tocar literales de código y la cola pedía solo comentarios. Queda anotado para la siguiente pasada.

## No verificado

- **No** he re-medido los ~49 min del A/B de embeddings: es la cifra aceptada de M-BUFFY-12 (2932 s); solo la he trasladado al comentario.
- **No** he ejecutado el kernel de producción ni he abierto sockets fuera de 127.0.0.1; no hay prueba en vivo de C-1/C-2 contra el kernel real, solo contra el router en tests (es la reproducción que fija el contrato).
- **No** he verificado si `guilds/scholars/plugins/knowledge.py` responde un `initialize` correcto (no se lanzó el guild: la decisión del ítem 2 no lo necesita y la cola prohíbe instalar dependencias).
- **No** he tocado ni evaluado el arranque en frío >120 s (hallazgo (a) del soak): la cola lo deja como hecho de CPU.
- **No** hay medición de cuánto ruido de logs adicional produce el supervisor en producción (528 es del soak de 30 min).
