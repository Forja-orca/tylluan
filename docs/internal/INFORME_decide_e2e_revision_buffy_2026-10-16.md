# INFORME — tylluan_decide: e2e servicio-real→harness, análisis operador, revisión 44d5828 (@buffy, turno 2026-10-16, post-1229)

Encargo: «Un test e2e que escriba filas con el servicio real y las lea con el
harness pareado. Un análisis escrito del token maestro frente a «operador
denegado». Una revisión de mi commit de integración 44d5828.»

Entrega SIN commitear (regla 9: solo Claude commitea). Worktree
`E:\tylluan-wt-buffy` sincronizado a `origin/main` = `8c9287b` al empezar (mi
entrega del turno 1220 ya estaba integrada upstream como `8f16a97`; verifiqué
byte a byte que mi copia local era idéntica, la pasé de untracked a historial y
limpié los duplicados antes de tocar nada — `git status` limpio salvo mis
ficheros).

## Ficheros de esta entrega (nada más)

| Fichero | Estado |
|---|---|
| `crates/tylluan-evals/src/paired_eval_e2e.rs` | NUEVO — e2e (2 tests) |
| `crates/tylluan-evals/src/main.rs` | M — 3 líneas: `#[cfg(test)] mod paired_eval_e2e;` |
| `docs/internal/ANALISIS_token_maestro_vs_operador_denegado_2026-10-16.md` | NUEVO — análisis |
| `docs/internal/REVISION_commit_44d5828_buffy_2026-10-16.md` | NUEVO — informe de revisión |
| `docs/internal/INFORME_decide_buffy_2026-10-16.md` | M — informe 1220 con respuestas a sus 3 preguntas abiertas |
| `docs/internal/INFORME_decide_e2e_revision_buffy_2026-10-16.md` | NUEVO — este fichero |

## 1. E2E: servicio real escribe, harness pareado lee

`paired_eval_e2e.rs` usa el `DecisionService` REAL del kernel (la misma puerta
de MCP y REST) contra una **SilvaDB de fichero** (schema v28), y después lee con
`paired_eval::load_rows` (rusqlite solo-lectura, conexión INDEPENDIENTE — el
escenario real «kernel corriendo + evals leyendo»). Cierra el hueco «no verifiqué
el comportamiento del servicio real» del informe 1220:

- 4 peticiones reales de `scheduler_routing` (una por nivel de riesgo de la
  matriz documentada) + 1 consulta libre abstenida → el arnés ve exactamente
  las mismas filas que `read_shadow_log` del servicio (mismo hash, mismo valor),
  se unen con verdad externa por `request_hash` calculado con el
  `decision_row_hash` público del kernel, reglas 4/4, modelo 0/4 (abstención =
  fallo), McNemar b=4 c=0 con p=0.125 exacto, veredicto `model_worse` — nunca
  una subida de modo con Fase 1.
- Interruptor apagado → cero filas para el arnés; `run_paired` sobre BD vacía
  reporta 0/0/0 sin inventar pares.

**Hallazgo que el e2e cazó (no lo arreglé: es decisión documentada):** el
harness aplana `rules_value` NULL a `""` (`paired_eval.rs` línea de lectura,
comentario «NULL = las reglas se abstuvieron: nunca coincide con la verdad»).
Semánticamente correcto —la abstención de reglas cuenta como fallo, coherente
con la del modelo— y lo confirmé contra el servicio real: el NULL llega igual
desde `log_shadow`. Lo dejé tal cual con su aserción documentada.

## 2. Análisis: token maestro vs «operador denegado»

`ANALISIS_token_maestro_vs_operador_denegado_2026-10-16.md`. Resumen: **«operador
denegado» no puede aplicarse al token maestro — añadir ese 403 sería incorrecto.**
El gate decide por clase de autenticación y el maestro ES la clase operador
(`operator_gate_allows` solo acepta MasterToken|DevBypass, `auth.rs:318`).
Matriz completa (header/query/JWT/par/dev_mode/sin credencial) con fichero:línea,
los 3 estados de denegación reales (401 mal presentado / 403 clase insuficiente /
429 rate-limit por IP, que también alcanza al maestro), y 3 exposiciones para
decisión del TL: E1 el query-string token satisface el gate operador (fuga de
token en URLs de acciones destructivas), E2 dev_mode pasa siempre (ya
documentado), E3 el gate ignora el rol. Mi `v4` de la batería se queda como está.

## 3. Revisión de `44d5828`

`REVISION_commit_44d5828_buffy_2026-10-16.md`. Veredicto: **apto para main**.
Arquitectura correcta (semántica en el servicio, puertas finas, paridad real
testificada, guardián CONTRACT-01 actualizado, inventario de rutas desactivado
en el mismo commit). 5 hallazgos, ninguno bloqueante; los dos importantes: H1 la
forma canónica `status: error` solo cubre límites de esquema — JSON
malformado/no-deserializable sale con el texto plano de axum (400/422, medido
con un probe scratch creado→ejecutado→borrado), y H2 el mensaje del commit
promete un chequeo de tool_count en `/health` que no existe. Recomiendo cerrar
H1/H2 antes de que Antigravity espeje el esquema. Los 3 fallos de
`guilds_v2_integration` en mi worktree son de ENTORNO (ficheros gitignored
ausentes en un clone en frío: `registry.json` de 1 entrada vs ≥15 exigidas,
`tylluan.toml`, `guilds/builders/agents/`), no regresión del commit.

## 4. Verificación ejecutada (nada inventado)

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-evals` (incluye e2e nuevo) | **24 passed, 0 failed, 1 ignored** |
| `cargo clippy -p tylluan-evals --all-targets -- -D warnings` | exit 0 |
| `cargo test -p tylluan-kernel --lib` | **1039 passed, 0 failed, 3 ignored** |
| `--test decide_parity_e2e` | **4 passed** |
| `--test decide_adversarial` | **10 passed, 1 ignored** (v8 marcador Fase 2) |
| `--test sovereign_tools_e2e --test integration_mcp --test pipeline_tests` | **2+12+31 passed, 0 failed** |
| `--test guilds_v2_integration` | 28 passed, 3 failed = entorno (§3) |
| `bash scripts/check_docs_reality.sh` | ejecutado al cerrar; este informe y sus 2 documentos hermanos: 0 ❌ (los avisos restantes son ADRs históricos preexistentes) |

## 5. Límites

- Clippy de kernel no corrido (mi diff `.rs` es solo evals; el kernel no se tocó
  — el único fichero de kernel de la entrega, `paired_eval_e2e.rs`, vive en evals).
- El probe del H1 usó `api_v1_routes` sin middleware de auth (mismo patrón que
  el resto de la batería): el contrato de error canónico en sí no depende de
  auth, pero la cadena completa con bearer sigue sin probarse en vivo.
- E1/E3 del análisis son exposiciones señaladas, no arregladas: decisión del TL.

## 6. TC-04 (turno 1229): tests muertos portados, no solo borrados

Los dos tests que la auditoría daba por muertos (`tests/guilds/test_git_guild.py`
y `test_docker_guild.py`, imports de `guilds.core.git`/`guilds.core.docker`,
módulos que ya no existen) se **portaron** a donde la CI sí ejecuta
(`pytest tests/python/`), contra los módulos reales
(`guilds/builders/plugins/{git,docker}.py`), y los originales se borraron:

- `tests/python/test_git_plugin.py` (3 tests): el original asertaba el contrato
  VIEJO de `git status -sb` («No commits yet», untracked visible). El real usa
  `--porcelain=v2 --untracked-files=no` (estado rápido), así que las aserciones
  se reescribieron contra la salida verificada en vivo: `# branch.oid (initial)`
  en repo vacío, untracked NO visible por diseño (con test propio que fija esa
  intención), staged `hello.txt` sí visible tras `git_add`, y flujo
  add→commit→log con el mensaje presente.
- `tests/python/test_docker_plugin.py` (3 tests): los mocks siguen siendo
  válidos (`_run_docker` aún llama directamente a `asyncio.create_subprocess_exec`);
  solo cambió el import.

Verificado localmente como lo correría la CI: `python -m ruff check
tests/python/` → All checks passed (los E402 iniciales se corrigieron alineando
la cabecera al patrón ya aceptado de `test_bash_execute_stdin.py`);
`pytest tests/python/ --ignore=…memory_bridge…` → **98 passed** (92 previos + 6
nuevos). Nota: `tylluan new guild` (CLI) sigue generando sus scaffolds en
`tests/guilds/` — funciona (el directorio se recrea), pero esos scaffolds
tampoco los ejecuta la CI; eso es una decisión de plantilla del CLI, fuera del
alcance de TC-04 y de mi zona (Rust → Deep).

## 7. Ficheros de TC-04 (añadido)

| Fichero | Estado |
|---|---|
| `tests/python/test_git_plugin.py` | NUEVO (puerto) |
| `tests/python/test_docker_plugin.py` | NUEVO (puerto) |
| `tests/guilds/test_git_guild.py` | BORRADO (muerto: import de módulo inexistente) |
| `tests/guilds/test_docker_guild.py` | BORRADO (muerto: import de módulo inexistente) |
