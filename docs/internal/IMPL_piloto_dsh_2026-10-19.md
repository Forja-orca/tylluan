# IMPL — Piloto DSH (verificación `verify.sh --rust` + conexión MCP) — 2026-10-19

Agente: **dsh** (DeepSeek Harness, worktree `E:\tylluan-wt-dsh`). Entrega **sin commitear**
(diff = este fichero nuevo + dos logs sin trackear como evidencia). Zero cambios en código.

## 0. Sincronización (regla obligatoria, citada con hash)

- Inicio de sesión: `git fetch` → HEAD worktree `a220fb8` == `origin/main` (0 de retraso).
- Durante el piloto `origin/main` se movió (pushes del TL): `06571c6` → `a4535ce` → `89f589d` → `50b880c`.
- Cierre: `git checkout --detach origin/main` → **HEAD = origin/main = `50b880c`**; delta con el
  primer run: 1 fichero Rust de los verificados (`crates/tylluan-link/src/identity.rs`), cubierto
  por RUN3.

## 1. Qué se ejecutó (comando → exit → resumen)

Comando en los tres casos (Git Bash explícito — ver §4): `scripts/verify.sh --rust`, job en
segundo plano, kernel intocado (`:47004` solo GET /health informativo).

| Run | HEAD | CARGO_TARGET_DIR | Exit verify.sh | Resultado |
|---|---|---|---|---|
| RUN1 | `a220fb8` | `E:\tylluan-targets\dsh` (EXTERNO) | **1** ❌ | check ✅ · kernel lib **1016 ok / 41 FAILED** / 3 ign · clippy kernel ✅ · link ✅ · clippy link ✅ · fsrs ✅ 12 |
| RUN2 | `a220fb8` | `E:\tylluan-wt-dsh\target` (in- repo, frío) | **0** ✅ | 6/6 gates ✅ — detalle abajo |
| RUN3 | `50b880c` (tip) | idem, warm | **0** ✅ | 6/6 gates ✅ — detalle abajo |

**RUN3 (el representativo del tip `50b880c`), salida real del log
`piloto_verify_rust3_2026-10-19.log` (log local de DSH, no versionado):**

- `cargo check -p tylluan-kernel` ✅ (51.84 s)
- `cargo test -p tylluan-kernel --lib` ✅ — `test result: ok. 1057 passed; 0 failed; 3 ignored` (25.17 s)
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings` ✅ (45.07 s)
- `cargo test -p tylluan-link --lib --tests` ✅ — lib **75 passed**; integración 7+5+6+4+3+3+3 ✅
- `cargo clippy -p tylluan-link --lib --tests -- -D warnings` ✅
- `cargo test -p tylluan-fsrs` ✅ — **12 passed**
- `✅ All selected checks passed` · `EXIT_CODE=0` · duración total 6 min 23 s (warm; RUN2 frío: 10 min 50 s)
- Informativo (no bloquea): live kernel `0ac9dfb` 21 commits tras HEAD — rebuild es potestad de José.

**Cifras contrastadas contra la doc del propio tip:** 1057 + 75 + 12 = **1144** == tabla de
AGENTS.md en `50b880c`. ✔ (RUN2, en `a220fb8`, dio 74 en link: el test extra de `identity.rs`
llegó con `06571c6` — coherente.)

## 2. Hallazgo principal: 41 fallos FALSOS con `CARGO_TARGET_DIR` externo (bug latente, con evidencia)

RUN1 siguió la indicación «usa tu propio CARGO_TARGET_DIR» poniéndolo **fuera del repo** y
produjo 41 fallos en `router::catalog`, `router::matcher`, `registry::capability_registry`,
`transport::server::*` — todos con el mismo síntoma: catálogo de guilds **0**
(`Expected at least 35 guilds, got 0`), cuando `guilds/` estaba intacto (73 .py, idéntico a main).

**Prueba A/B con el MISMO binario de test** (`tylluan_kernel-e2403b873cef1a76.exe`, filtro
`router::catalog`):

| CWD al ejecutar el exe | Resultado |
|---|---|
| raíz del worktree | **19 passed; 0 failed** |
| `crates\tylluan-kernel` (el que usa `cargo test`) | **10 passed; 9 failed** |

**Causa raíz leída en código** — `router/catalog.rs:582-607` (`builtin_catalog`):
1. Prueba `current_dir()/guilds`, sube **un solo** nivel; con cargo (CWD = crate root) busca en
   `crates/guilds` → no existe.
2. Fallback: 6 niveles hacia arriba desde **`current_exe()`** — presupone que el ejecutable vive
   *dentro* del repo (`repo/target/debug/deps` → encuentra `repo/guilds` a 3 niveles). Con target
   externo la subida recorre `deps→debug→dsh→tylluan-targets→E:\` y nunca lo encuentra → catálogo
   vacío → 41 tests dependientes del catálogo fallan en cascada. CI y los flotas pasan siempre
   porque su target está dentro del repo (incluso `target-test`).

**Impacto potencial:** cualquier agente siguiendo la práctica documentada de aislar builds con un
target fuera del repo obtiene 41 fallos fantasmas y podría reportarlos como deuda inexistente.
**Propuesta de fix** (código Rust es de Deep; aquí solo se reporta): anclar en el fallback con
`env!("CARGO_MANIFEST_DIR")/../../..` (patrón ya usado en las líneas 1000 y 1069 de
`catalog.rs`, dentro de tests) y/o respetar un env tipo `TYLLUAN_REPO_ROOT`. Añadir un test que
fije el catálogo con target externo.

> **Actualizado 2026-10-19 (dsh, T-DSH-1):** la propuesta está implementada y fijada con tests en
> `router/catalog.rs` — ver `IMPL_t-dsh-1_catalog_anchor_2026-10-19.md`.

## 3. Lecciones de entorno (Windows/esta máquina) — falsos negativos neutralizados

1. El `bash` del PATH es **WSL sin toolchain** (`/mnt/e/...`, sin cargo/rustup): usar
   `C:\Program Files\Git\bin\bash.exe` explícito o verify muere con «command not found».
2. El shell del harness es **Windows PowerShell 5.1**: `Tee-Object` escribe UTF-16 (log del RUN1
   ilegible para ripgrep) y romper comillas/`$(...)` al atravesar el borde de argumentos nativos
   (`bash -c "$cmd"` se destrozó). Receta estable: Git Bash con args directos, log por
   `Out-File -Encoding utf8`, sin `bash -c` con scripts embebidos.
3. El job-wrapper que termina en 0 **no** prueba que verify termine en 0: hay que citar el
   `EXIT_CODE` del log (así se detectó que RUN1 era rojo).

## 4. Conexión MCP del harness al kernel (segunda parte del piloto)

- Bundle config-only `@local/tylluan-mcp` v1.0.0 instalado en el perfil DSH
  (`plugin_manager install_bundle` → `application: applied`, sin warnings). Config:
  `transport: streamable-http`, `url: http://127.0.0.1:47004/mcp`, `failOnStartupError: true`.
- `initialize` verificado en vivo: `serverInfo tylluan-nexus-sovereign 0.17.1`, protocolo
  `2025-06-18`. Sin SSE ni `mcp-remote`. `auth_required:false` (kernel local) → cero tokens en
  ficheros.
- Herramientas expuestas al agente: exactamente las 6 soberanas (`mcp__tylluan__tylluan_do /
  remember / recall / think / graph / decide`) — CONTRACT-01 intacto.
- Prueba end-to-end: `tylluan_recall` devolvió memoria real (30 resultados, CoherenceGate filtró
  28) y `read_channel(general)` por el guild coloquio funciona en solo lectura.
- **Pendiente de José:** alta de `dsh` en `.tylluan/agents.toml` (ADR-009). Sin alta, cualquier
  post mío saldría con la firma genérica `agent` (visto en general T1267/T1273) — suplantaría mi
  identidad, así que **no publico en Coloquio**; entrega por fichero + sesión.

## 5. No verificado

- Gates `--dashboard` y `--docs` de `scripts/verify.sh` (fuera del alcance del piloto `--rust`).
- Ejecución en CI real (esto es la misma toolchain estable local, no el runner).
- Comportamiento del kernel en vivo con `50b880c` (kernel de José sigue en `0ac9dfb`; no
  reconstruido — decisión suya).
- `identity.rs` (el fix `06571c6`) más allá de sus tests unitarios de link (75 ✅ en RUN3); no
  auditado en profundidad.
- macOS/ARM — soporte community-tested por definición (CLAUDE.md); no hay hardware.
- El bug de `builtin_catalog` en Linux/CI: analizado por código, no reproducido en Linux (allí el
  target suele ser in-repo, por eso nunca se ha visto).
- Publicación en Coloquio con identidad real: bloqueada hasta el alta de `dsh`.

## ## Impact

- Solo se AÑADE este informe (untracked). Ningún fichero de código, config o docs existentes
  tocados. Ningún proceso del kernel iniciado/detenido. Ningún commit ni push.
- Ficheros de evidencia en el worktree (untracked): `piloto_verify_rust_2026-10-19.log`
  (RUN1, mixto UTF-16 por el bug §4.2), `piloto_verify_rust2_2026-10-19.log` (RUN2),
  `piloto_verify_rust3_2026-10-19.log` (RUN3).
- El target `E:\tylluan-targets\dsh` externo queda como está para forense del A/B; puede
  borrarse.
