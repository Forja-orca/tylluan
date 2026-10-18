# REVISIÓN — commit de integración `44d5828` (@buffy, 2026-10-16)

Encargo: «una revisión de mi commit de integración 44d5828»
(`feat(transport): tylluan_decide as the 6th sovereign tool, MCP + REST (ADR-021)`),
17 ficheros, +418/−36. Base: `origin/main` = `8c9287b` (el commit va 6 posiciones
atrás; lo revisé tal cual está en main). Método: diff leído entero, cada
afirmación contrastada contra el código actual, suites del propio commit
ejecutadas, y un probe scratch de contrato de error (creado, ejecutado,
borrado). Veredicto al final.

## 1. Qué dice el commit vs qué hace

| Afirmación del mensaje | Veredicto |
|---|---|
| CONTRACT-01 pasa de 5 a 6 tools; `all_tools()` filtra a 6 | ✅ `tools.rs:21` (lista soberana con `tylluan_decide`), test guardián renombrado `contract_01_exactly_six_sovereign_tools` + `test_sovereign_count_is_exactly_6` (`sovereign_tools_e2e.rs:431`). |
| MCP y REST responden por el MISMO `DecisionService` | ✅ `handler_decide.rs` → `server.decision_service.decide(req, "mcp")`; `api_decide.rs` → `service.decide(req, "rest")`. Un solo servicio, dos puertas finas. |
| Límite de esquema roto → 400 con cuerpo canónico `status: error` | ✅ verificado en vivo (probe): 17 preguntas → `400` + `{"request_id":…,"status":"error","mode":"off","provider":"none",…,"answers":{}}`. Pero ver hallazgo H2. |
| `/api/v1/decide` clase agente, nunca operador | ✅ `operator_class_required` no la lista; añadida a `AGENT_OR_PEER_CLASS` (`auth.rs:926`) — la «mina» 3 de mi informe 1220 se desactivó correctamente. |
| A2A Agent Card anuncia 6 skills; `/health` y `/monitor` esperan 6 | ⚠️ Parcialmente falso: solo el canary de `/monitor` cuenta tools (`api_monitor.rs:450`, `== 6`). `/health` no tiene ningún chequeo de tool_count (leído `mod.rs:1341-1399`). |
| `main` cablea `[decision]` al arranque (`set_decision_config`) | ✅ `main.rs:1309`. Además reconstruye el servicio en cada test que lo necesita (`decide_parity_e2e` lo usa con `enabled=false`). |
| Con `enabled=false` la tool sigue listada y responde `status: disabled` sin llamar al proveedor | ✅ verificado en servicio (`service.rs` temprano return) + e2e REST del commit (200 + `status:disabled` + `answers` vacío). |
| Tests renombrados 5→6 en pipeline/sovereign/integration/guilds v2 | ✅ ejecutados: `pipeline_tests` 31 ✅, `sovereign_tools_e2e` 2 ✅, `integration_mcp` 12 ✅, `guilds_v2_integration` 28/3 (los 3 fallos son de ENTORNO, ver §4). |

## 2. Lo que está bien (y lo verifiqué corriendo)

- **Discreción de superficie correcta**: `TylluanServer::new` construye el
  servicio con config por defecto y `main` la sustituye — un servidor de test
  funciona sin tocar config. El `SOVEREIGN_TOOLS` de `handlers.rs:10` (array de
  7, incluye `tylluan_ingest`) es la lista de *dispatch*, no la soberana: naming
  engañoso preexistente, no introducido aquí.
- **Paridad MCP/REST es real, no declarada**: el test `decide_parity_e2e`
  compara campo a campo las respuestas a la misma petición (4/4 en verde hoy).
- **Batería adversarial del turno 1220 integrada**: `decide_adversarial` corre
  **10 passed / 1 ignored** (el `#[ignore]` de `v8` es el marcador de Fase 2 con
  proveedor de modelo — intencionado). Los `wire_contract` congelados pasan
  contra el `api_types.rs` real de Deep: el esquema no se desvió.
- **e2e servicio-real → harness pareado (mi entrega de hoy)**: 2/2 en verde
  (`paired_eval_e2e.rs`), §5.
- `set_decision_config` clona la config en el servicio — `POST /api/v1/config`
  con reload **no** propaga `[decision]` (documentado en el propio commit:
  «read at boot, not hot-reloaded»). Consistente, no es un bug, pero es
  trampa de operación: cambiar `enabled` en caliente no aplica.

## 3. Hallazgos (ordenado por severidad)

- **H1 [bajo, contractual]: el contrato de error canónico es solo 400/400.**
  Probe medido hoy contra `api_v1_routes` real:
  - JSON malformado (`{not json`) → **400** con el texto plano de axum
    («Failed to parse the request body as JSON…»), no el cuerpo canónico.
  - JSON válido que no deserializa (`questions: "nope"`) → **422** con texto
    plano de axum, no `status: error`.
  - Límite de esquema (17 preguntas) → 400 canónico ✅ (la promesa del commit,
    correcta).

  Es decir: `DecideRequest` malformado nunca llega al handler (`Json<>` rejection
  de axum), así que dos de los tres caminos de error de la puerta REST no usan
  la forma canónica que el commit proclama para la puerta. El MCP sí devuelve
  forma canónica en parseo (`error_result`). No rompe nada y la semántica HTTP
  es correcta (400/422 son veraces), pero o documentarlo en ADR-021 («solo los
  errores de límite usan la forma canónica») o añadir un `map_err` del extractor.
  Es la clase de hueco que Antigravity copiará byte a byte al espejar el
  esquema, así que decídolo antes de que pegue el espejo.
- **CERRADO 2026-10-17 (encargo del TL a @buffy; implementado por Deep en `5a4a806`):**
  `api_decide.rs` lee el body en crudo (`axum::body::Bytes`) y parsea en el
  handler — cualquier cuerpo no deserializable responde `400` + sobre canónico
  `DecideResponse::error()`, nunca texto plano de axum (los tres caminos de la
  medición quedan en uno solo: 400 canónico). Test
  `h1_malformed_json_returns_canonical_error_envelope` en `decide_adversarial.rs`;
  decisión documentada en ADR-021 §13.
- **H2 [bajo, veracidad del mensaje]: «/health … expect tool_count == 6» es
  falso** — `/health` no cuenta tools (§1). Corregir el mensaje o añadir el
  conteo donde el mensaje lo promete; hoy un lector del commit buscará un
  chequeo que no existe.
- **CERRADO 2026-10-17 (Deep, `5a4a806`):** fuente única
  `TylluanServer::sovereign_tool_names()` para `all_tools()` y para el health;
  `/health?verbose=true` expone `components.sovereign_tools {ok, count}` con
  `count == 6`; test `health_sovereign_count.rs`. El mensaje del commit queda
  verdadero.
- **H3 [informativo]: `tools.rs:6` apunta a `CONTRACTS.md`** («See CONTRACTS.md
  before modifying this function») y ese fichero no existe en el repo (buscado:
  cero). El guardián real es `contract_01_exactly_six_sovereign_tools` en el
  propio fichero. Actualizar el puntero al test, o crear el CONTRACTS.md, para
  que el próximo modificador no busque un fantasma.
- **H4 [informativo]: la fila de auditoría no tiene retención.**
  `decision_shadow_log` crece una fila por pregunta por llamada y nada la
  recorta (verificado: `cleanup.rs` sin mención). En shadow con tráfico real es
  crecimiento ilimitado en `silva.db`. Añadir a la fase 2 (con el proveedor de
  modelo) o un sweep por antigüedad.
- **H5 [medido, sin impacto hoy]: `latency_ms` solo mide la decisión**, no el
  INSERT de auditoría (`started` se congela antes de `log_shadow`) — correcto
  para la evaluación pareada; solo lo anoto para que nadie lo interprete como
  latencia total de la puerta.

## 4. Los 3 fallos de `guilds_v2_integration` NO son del commit

`test_guilds_v2_registry_json_still_valid`, `..._migration_tylluan_toml_still_valid`
y `..._builders_has_agents_subdir` exigen `registry.json` (≥15 entradas),
`tylluan.toml` y `guilds/builders/agents/` — los tres **gitignored** (verificado
con `git check-ignore`: `.gitignore:80`, `.gitignore:32`) y presentes en el
checkout compartido (`E:\tylluan`, 50 entradas) pero no en un worktree limpio.
Fallan igual en un clone en frío; el commit solo tocó el recuento 5→6 en ese
fichero (línea del test de sovereign count, que pasa). Ruido de entorno, no
regresión.

## 5. Verificación ejecutada (nada inventado)

| Comando | Resultado |
|---|---|
| `cargo test -p tylluan-kernel --lib` | **1039 passed, 0 failed, 3 ignored** |
| `cargo test -p tylluan-kernel --test decide_parity_e2e` | **4 passed** |
| `cargo test -p tylluan-kernel --test decide_adversarial` | **10 passed, 1 ignored** (v8: marcador Fase 2) |
| `--test sovereign_tools_e2e --test integration_mcp --test pipeline_tests` | **2 + 12 + 31 passed, 0 failed** |
| `--test guilds_v2_integration` | 28 passed, **3 failed = entorno** (§4) |
| `cargo test -p tylluan-evals` | **24 passed, 0 failed, 1 ignored** |
| `cargo clippy -p tylluan-evals --all-targets -- -D warnings` | exit 0 |
| Probe scratch `/api/v1/decide` (creado→ejecutado→borrado) | H1: 400 texto plano / 422 texto plano / 400 canónico |

Clippy de kernel **no lo corrí** (mi diff no toca `.rs` de kernel; el commit
revisado ya pasó el suyo en CI en su momento — no lo re-ejecuté por tiempo).

## 6. Veredicto

**Apto para main.** La arquitectura es la correcta: semántica en el servicio,
puertas finas, esquema único, test de paridad real, guardián CONTRACT-01
actualizado, y la mina del inventario de rutas desactivada en el mismo commit.
Los cinco hallazgos son de veracidad documental y de contrato de error (H1),
ninguno bloquea; recomiendo cerrar H1 y H2 antes de que Antigravity espeje el
esquema y Deep abra la Fase 2 (modelo), porque ambos son baratos ahora y caros
después.

## 7. Contexto del turno (Coloquio 1229) — cosas que el informe de auditoría no ve

La auditoría externa (21 pág., 7,9/10) se basa en una referencia anterior a
`0b816b4`: no contempla `tylluan_decide` (commits `d55850f`+`44d5828`+`8f16a97`),
OAuth Fase 1, ni el harness pareado. Su tesis del cuello de botella (recall
bajo concurrencia con embeddings en CPU) sí coincide con lo medido en el repo.
Los contrahechos del TL (issue templates existen; sin duplicados sueltos en
`tests/`; «1.361 unwrap» = mayoría en módulos de test) los confirmo desde mi
zona: en `handler_do/mod.rs` el desglose que vi es coherente con lo que describe
el TL.
