# H-47C — Implementación del helper único de redacción de logs (hermes, 2026-10-29)

Asignación: T2060 del TL. Implementa B1–B4 del diseño aprobado (sección 5 de
`docs/internal/IMPL_H-47A_hermes_2026-10.md`) **tal cual**: default
`log_content=false`, scrub antes del recorte, contenido solo en `mcp.rs` con
opt-in, tests + gate. HEAD base: `0441cda` (push191). Diff sin commitear.

## Qué se implementó

- **Helper único** en `crates/tylluan-kernel/src/logging.rs` (B1):
  - `redacted(canal, texto)` → `canal=X len=<chars> sha=<sha256[:12]>`. Nunca
    contenido. Lo usan las fuentes internas (B2).
  - `safe(canal, texto)` / `safe_with(cfg,…)` → respeta opt-in: con
    `log_content=true` devuelve `scrub(texto)` recortado a
    `log_content_max_chars` (200). El recorte va **siempre detras** del scrub.
  - `mcp_payload(payload)` / `mcp_payload_with(cfg,…)`: unica fuente con
    contenido posible (B2). Default: `method+id+len+sha`. Opt-in: field-walk
    recursive que scrubuta solo strings (id/num/bool crudos para correlacionar).
  - `scrub(texto)`: patron completo sustituido entero por `«scrubbed»` — no
    queda fragmento interior que un recorte posterior pueda devolver.
    Patrones (orden fijado por pruebas): JWT 3-seg, `sk-[A-Za-z0-9_-]{8,}`,
    `bearer <token-like>{6,}`, `token[=:] <valor{4,}>`, hex ≥32, base64 ≥32.
  - El sha es estable texto a texto: mismo intent produce el mismo `sha=` en
    todas las fuentes (correlacion de una peticion sin contenido).
  - Config leida una vez al primer uso (`OnceLock`): el valor queda fijo hasta
    reinicio — `config::reload()` **no** basta (riesgo B6 aprobado).
- **Config** (B3) en `[logging]` de `config.rs`: `log_content` (default
  **false**) y `log_content_max_chars` (default 200), con defaults serde y
  `Default`.
- **9 sitios convertidos** (los 7 fijados + 2 extra que encontro el gate, ver
  Impact):
  | Sitio | Cambio |
  |---|---|
  | `transport/http/api_v1/mcp.rs:354` | `logging::mcp_payload(&payload)` |
  | `transport/server/handler_do/mod.rs:191` | `intent = %redacted("do_entry", …)` |
  | `transport/server/handler_do/prepare.rs:339` | `intent=%redacted("do_prepare",…)` (comillas fuera) |
  | `transport/server/handler_do/routing.rs:228` | `redacted("do_routing", intent)` |
  | `transport/server/handler_recall.rs:171` | `query = %redacted("recall", …)` |
  | `router/matcher.rs:368` | `redacted("matcher", query)` — el corte implicito muere al no ir contenido |
  | `router/scheduler/observe.rs:132` | `redacted("scheduler_observe", obs.intent)` |
  | `memory/hybrid.rs:239` (extra) | `redacted("hybrid_search", query)` |
  | `router/matcher.rs:549` (extra) | `redacted("matcher_floor", query)` |
- **Tests** (7 en `logging::redaction_tests`, todos de criterio medible):
  cada clase de patron sin fragmentos ≥8 chars sobrevivientes; scrub antes del
  cap con secreto cruzando el corte (90+43, cap=100); forma `len+sha` de las
  fuentes internas; sha estable/distinto y `len` en chars (UTF-8); payload MCP
  default sin contenido y con campos de correlacion; opt-in con field-walk
  (strings scrubados, escalares crudos); defaults del config redactan.
- **Gate** `scripts/check_log_redaction.sh` (7 sellos negativos sobre los
  literales exactos que H-47A midio en prod) + job `log-redaction` en
  `.github/workflows/ci.yml`. Sanity verificada: los sellos existian en
  `0441cda` (el gate habria fallado antes del cambio) y pasan despues.

## Como medir con `len+sha` (metodologia para la flota H-42/43/45/T-DSH)

- **Correlacionar una peticion**: `grep -F "sha=<H12>" logs/kernel.log*`
  devuelve todas las lineas (entrada, prepare, routing, matcher, observe,
  recall) de un mismo texto sin tener el texto.
- **Buscar una frase concreta sin dejarla en el log**: calcular el prefijo
  antes del primer ` → `… el sha es de **toda la cadena tal cual se logueaba
  antes** (intent completo): `python -c "import hashlib;print(hashlib.sha256('TEXTO'.encode()).hexdigest()[:12])"`
  y grep por ese hash.
- **Tasas por fuente**: `grep -c "canal=do_routing" kernel.log` (antes: contar
  literales `🎯 Routing:`). La distribucion de longitudes sale de
  `grep -o "len=[0-9]*" | sort -n | uniq -c`.
- **Deteccion de mutaciones**: mismo texto → mismo sha en las 9 fuentes; un
  sha que cambia entre dos lineas indica texto distinto (prefix-strip,
  normalizaciones del router se hacen visibles sin leer el contenido).

## Ventana opt-in con reinicio (procedimiento)

1.Editar la config del kernel (el `tylluan.toml` de produccion): anadir en la
   seccion existente `[logging]`:
   `log_content = true` (opcional: `log_content_max_chars = 300`).
2. Reiniciar el kernel (ventana del TL). **No vale `config::reload()`**: el
   helper fija `log_content` en su primer uso tras el arranque.
3. Trabajar la ventana con el contenido scrubado (maximo N chars, secretos
   enmascarados incluso con opt-in) y **revertir a false + reiniciar al
   terminar**. La ventana queda acotada por la propia rotacion (50 MiB × 5).

## Impact

- Cero contenido de usuario/agentes en `kernel.log` por defecto: antes 9
  fuentes logueaban intent/query/payload crudo; un secreto escrito en un
  content vivia raw ~25 dias (rotacion medida 50 MiB × 5 a 0,42 MB/h).
- Con opt-in, los secretos (`sk-`, Bearer, token=, JWT, hex/base64 ≥32) se
  enmascaran enteros antes de poder llegar al disco o a un recorte.
- Tamano: una linea `do` corta de ~900 B de contenido pasa a ~320 B de
  metadatos (estimacion sobre los conteos H-47A, no medido en prod con el
  cambio: ver No verificado); la inflacion x3,5 del intent multi-fuente
  desaparece (las 6 internas llevan solo ~40 B de sha).
- El gate encontro **2 fuentes extra no inventariadas en H-47A**
  (`memory/hybrid.rs:239`, `matcher.rs:549`): el diseno decia 7, son 9.
  cubiertas igual.

## No verificado

- Volúmen real post-cambio en produccion (no compilo ni despliego: ventana
  del TL; la reduccion de tamano es estimacion aritmetica sobre conteos
  medidos en H-47A).
- Si los 9 sellos negativos cubren el 100 % de interpolaciones de contenido:
  el escaneo H-47A fue sobre logs reales de 105 h + los grep del gate; un
  sitio nuevo que use otro literal pasaria sin sello hasta que alguien lo
  loguee crudo y el gate no lo conozca. El gate es sello en negativo, no
  barrido exhaustivo.
- Guilds Python que logueen contenido propio por su propio logger no pasan
  por el helper Rust (fuera de alcance del diseño B).
- Recuento total de tests del crate sube con los 7 nuevos; si el TL mantiene
  la cifra fija en README/STATUS, hay que actualizarla al integrar
  (`check_test_count.sh` lo detecta). No toco esas cifras.
- Efecto del field-walk con payloads MCP muy grandes y `log_content=true`
  (scrub de miles de strings): sin medir rendimiento en ese modo; modo
  default no hace walk.

## Criterio de aceptacion (resultado real)

- Base de desarrollo: `0441cda` (push191). Rebase a `be6ea74` (push192, T2065)
  con el diff encima: `git stash -u` → `checkout --detach be6ea74` → `stash
  pop` → sin conflictos (T-DSH-130 no toca las zonas de mis ficheros).
- En `0441cda` (primera pasada): `cargo clippy -p tylluan-kernel
  --all-targets -- -D warnings` → **0 errores** (el unico hallazgo fue un
  `needless_borrow` propio, corregido). `cargo test -p tylluan-kernel --lib`
  → **1410 passed; 0 failed; 4 ignored** (1403 base + 7 nuevos).
- En `be6ea74` (re-acceptacion tras el rebase, obligatoria tras cambio de
  base): cifras en el post COMPLETA; la build del kernel se reejecuta entera
  por las mudanças de T-DSH-130 en config.rs/main.rs.
- Gate local: OK (7 sellos verdes; sanity: fallaban en `0441cda`).
- Cifras de `check_test_count.sh` subidas en el propio diff (precedente
  T-DSH-130): README.md:176 y README.es.md:177 `1496→1503`; AGENTS.md:78
  `1409 kernel + 75 + 12 = 1496 → 1416 + 75 + 12 = 1503`. STATUS.md no se
  toca (linea canonica manual del TL).
- Diff sin commitear (en `be6ea74`): `git status --short` = **15 M + 2 ??**
  (12 fuentes/docs de H-47C + 3 bumps de cifras; ?? = este informe + gate).
  +345/−17 netas. `main.rs` intacto. Snapshot de seguridad del diff y docs:
  `C:/Users/FoRJa/AppData/Local/hermes/_saved/20261029-084027/` (h47c_wip.diff, 28749 B).
