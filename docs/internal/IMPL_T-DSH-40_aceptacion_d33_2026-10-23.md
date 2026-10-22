# T-DSH-40 — Aceptación ejecutable de D-33 (el juez de la capa 2 deja de morir en silencio)

- **Agente:** dsh
- **Fecha:** 2026-10-22/23
- **Base:** `E:\tylluan-wt-dsh` en `--detach origin/main` (`97ba01e`, push 112: T-DSH-39 integrada); limpio antes de empezar.
- **Encargo (turno 1569):** un test nuevo, sin tocar `src`; aceptación de D-33 (leer su sección en `docs/internal/COLA_deep_2026-10-23.md` y `src/security/write_gate.rs`). Backend OpenAI-compatible FALSO levantado en el propio test (`TcpListener` en `127.0.0.1:0`, sin red externa) que devuelva 400 si `/v1/chat/completions` no trae `model` y 200 con respuesta válida si lo trae, y GET `/v1/models` con dos modelos: (a) `#[ignore = "D-33"]` con el juez pasando por ese backend y lo DESEADO (petición con `model` y veredicto); (b) `#[ignore = "D-33"]` con backend 400 siempre y el fallo VISIBLE (contador/log/estado); (c) test vivo que fije el comportamiento de hoy. Si la función del juez no es alcanzable desde tests, decirlo y pedir la visibilidad mínima. Sin puertos fijos; clippy 0; sin `cfg`; informe con `## Impact` y no verificado.

## Qué contiene el fichero

`crates/tylluan-kernel/tests/cola_d33_acceptance.rs` (nuevo; `src/` intacto). Levanta un **backend OpenAI-compatible falso** en un hilo con `TcpListener` en `127.0.0.1:0` (puerto efímero, sin red externa):

- `GET /v1/models` → 200 con `fake-model-A` y `fake-model-B`;
- `POST /v1/chat/completions` → **400** si el cuerpo no trae `model`, **200** con `choices[0].message.content = "SAFE"` si lo trae;
- variante `Always400` para el test de visibilidad del fallo.

Arranca el **guild `llama_backend` REAL** (Python MCP stdio, vía `GuildProcess` launcher `External`) apuntándolo al falso con la detección legacy `OPENAI_BASE_URL`, y llama a su herramienta `query_model` **como lo hace el juez** (sin `model`).

| test | tipo | qué hace / afirma |
|---|---|---|
| `d33_current_judge_request_lacks_model_and_fails` | **vivo** | la petición a `/v1/chat/completions` **no lleva `model`**, el falso responde 400 y la llamada vuelve como error: el caso en que la capa 2 muere en silencio |
| `d33_desired_judge_request_carries_model_and_gets_verdict` | **`#[ignore = "D-33"]`** | la petición lleva `model` (uno de los dos de `/v1/models`) y llega el veredicto `SAFE` |
| `d33_desired_judge_failure_is_visible` | **`#[ignore = "D-33"]`** | con el falso devolviendo 400 siempre, `TYLLUAN_KERNEL_URL` apunta al falso y `write_gate::spawn_write_gate_judge` corre: el fallo debe incrementar `judge_telemetry().to_json()["by_kind"]["backend_error"]` (lo que se ve en `GET /health?verbose=true` → `components.judge.by_kind`) |

## Alcanzabilidad desde tests (sin tocar `src`)

La función interna del juez (`call_reasoning_backend_with_grammar`/`call_judge_inner`) es `pub(crate)`, no invocable desde un test de integración. **No hace falta cambiarla**: para el test de visibilidad se usa la puerta pública `security::write_gate::spawn_write_gate_judge` (que es quien llama al juez) más `security::judge_telemetry::judge_telemetry()` (pública, la misma que expone `/health`). Para (a) se usa el guild `llama_backend` real por MCP, que es exactamente el componente que construye la petición sin `model`. **No se pide ninguna visibilidad nueva a `src`.**

## Salidas

Corrida normal (vivo; los deseados quedan fuera):

```text
test d33_desired_judge_failure_is_visible ... ignored, D-33
test d33_desired_judge_request_carries_model_and_gets_verdict ... ignored, D-33
test d33_current_judge_request_lacks_model_and_fails ... ok

test result: ok. 1 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 4.18s
```

Corrida `-- --ignored` (los dos deseados fallan **por la razón correcta**):

```text
running 2 tests
test d33_desired_judge_request_carries_model_and_gets_verdict ... FAILED
test d33_desired_judge_failure_is_visible ... FAILED

thread 'd33_desired_judge_request_carries_model_and_gets_verdict' panicked:
D-33 pendiente: la petición sigue sin 'model' (el backend responde 400 y el juez muere);
cuerpo: {"messages": [...], "max_tokens": 32, "temperature": 0.7, "top_p": 0.95,
"top_k": 64, "repeat_penalty": 1.1, "stream": false}

thread 'd33_desired_judge_failure_is_visible' panicked:
D-33 pendiente: components.judge.by_kind no tiene la clave 'backend_error';
un 400 del backend del juez queda solo en debug!.
by_kind actual: {"busy_skip":0,"http":1,"parse":0,"timeout":0,"transport":0}

test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1 filtered out; finished in 5.10s
```

La evidencia de (b) es fina: el `http:1` demuestra que el juez **sí llegó** al backend y su 400 se contó como `http`, no como `backend_error` (que aún no existe) — el defecto es de clasificación/visibilidad, no de transporte.

Cuando Deep cierre D-33: quitar los dos `#[ignore]` e **invertir** el test vivo.

## Verificación

| criterio | resultado |
|---|---|
| `cargo test -p tylluan-kernel --test cola_d33_acceptance` | **1 passed / 0 failed / 2 ignored** (4,18 s) |
| `cargo test --test cola_d33_acceptance -- --ignored` | **2 failed**, ambos por la razón correcta (salida pegada) |
| `cargo clippy -p tylluan-kernel --all-targets -j 3 -- -D warnings` | **exit 0** |
| `git status` | **solo** el test nuevo; `src/` intacto |
| `check_test_count.sh` | exit 0 (1272; test de integración) |
| `check_doc_links.py` / `check_docs_reality.sh` | links 0; reality solo el hallazgo preexistente de `ROADMAP_O3.md` (artefacto WSL) |

Sin `cfg(unix)`/`cfg(windows)` en ninguna función; sin puertos fijos (todo `127.0.0.1:0`); sin red externa.

## Impact

- **Solo test, fichero nuevo**: cero cambios en `src/`, guilds, configuración o corpus. El test arranca un hilo HTTP y un guild Python real en un puerto efímero y los mata al terminar; no toca el kernel `:47004` ni `E:\tylluan`.
- **Para D-33**: la aceptación queda en un comando, con las dos mitades del defecto separadas: (a) el `model` que falta en la petición y (b) la visibilidad del fallo en `components.judge.by_kind`. Además demuestra que la puerta pública del juez (`spawn_write_gate_judge` + telemetría) **es suficiente** para aceptar D-33 sin abrir nuevas funciones.
- Coste: ~4-5 s (incluye arranque del guild Python y handshake MCP); sin BD compartida (SilvaDB en memoria).

## No verificado

- **Con D-33 implementado**: no existe todavía (la cola está pendiente), así que no se comprobó que los 2 deseados pasen; solo que hoy fallan por la razón correcta.
- **LM Studio real**: el falso emula el contrato OpenAI (`/v1/models`, 400 sin `model`, 200 con completion), no el comportamiento exacto de LM Studio/Ollama; el defecto medido por Buffy es el mismo contrato, pero no se probó contra un backend real (la instrucción prohíbe depender de red/servicios).
- **Selección de `model`**: el test acepta cualquiera de los dos modelos de `/v1/models`; la clave de configuración `judge_model` que propone D-33 no existe aún y no se aserta (el test no la exige).
- **Visibilidad (b)**: se mide en `judge_telemetry()` en proceso (el mismo objeto que alimenta `/health?verbose=true`), no en el endpoint HTTP real; la telemetría es global del binario de test y podría compartirse con otros tests (hoy solo este fichero la usa en este binario).
- **El guild `llama_backend` real**: se arranca por la ruta legacy `OPENAI_BASE_URL`; la ruta de `[[external_providers]]` de `tylluan.toml` no se probó (no hay `tylluan.toml` en el worktree).
- **El test vivo**: fija el comportamiento actual para que Deep lo invierta; si D-33 se implementa sin tocar el guild (p. ej. poniendo `model` solo en el camino del juez), este test seguirá viendo el guild sin `model` y habrá que revisar el alcance de (a).
- **CI**: medido en mi Windows; el arranque del guild Python depende del PATH y de FastMCP (funcionó en 4,18 s), en un runner sin Python/guilds el test fallaría en el spawn (no está ignorado).
