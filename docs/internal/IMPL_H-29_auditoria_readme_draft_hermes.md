# IMPL_H-29 — Auditoría del README borrador (rama `readme-draft`, a98d409) con fuente por afirmación

Solo lectura. Worktree desechable en `…/cache/scratch/wt-tmp-h29` (detach a `a98d409`, ya retirado al entregar); mi worktree intacto (git status igual: M H-28 + ?? IMPLs). El guardian actual (con R1+R2) corrió contra el árbol del draft.

## Contexto git del draft (clave para leer los hallazgos)

`readme-draft` = base `origin/main` (7b24da2, lo que hay en GitHub) + un commit de README nuevo (a98d409 "nueva primera impresion como stack local portable"). **No lleva la cadena local del TL** (ca1162f→…→4cb3d26), por eso: su AGENTS.md dice 1214+75+12=**1301** (el README EN del draft afirma 1301 ✓ consistente con su propia base) y su README.es.md conserva el **1125 viejo** (la fix 1125→1329 vive en la cadena local aún sin pushear).

## Gates del guardian sobre el draft (salida literal, exit 1)

```
DRIFT tests: README.es.md claims 1125 tests but AGENTS.md total is 1301
DRIFT bilingual: README.md claims {value set: 13,45,50,1301,} but README.es.md claims {13,45,50,1125,} — one language is stale
RESULT: 2 DRIFT line(s)
```
17 líneas OK (tools/version/badges/links/R2 guilds 45==45 en la base del draft/R3 benchmarks).

## Tabla afirmación → fuente (solo lo NUEVO/cambiado del draft; lo invariable ya auditado en H-26 sobre el mismo árbol base)

| Afirmación del borrador | Fuente en el árbol del draft | Estado |
|---|---|---|
| "one Rust binary that every MCP client talks to" | Cargo workspace binarios; `/mcp` + `/api/v1/mcp` transport/http/mod.rs (línea 1262)-1263 | OK |
| "six tools (…tylluan_decide)" sección agente | tools.rs soberanas + REST twin api_decide.rs:1 | OK |
| Gateway (Axum) MCP SSE/Streamable + REST | axum en Cargo; rutas mod.rs (línea 1262)+ | OK |
| Inference ONNX in-process, CPU default, GPU optional | config.rs:926 default Cpu; DirectML auto config.rs:922-era (mismo árbol) | OK |
| Memory: keyword+vector+graph fusion + lifecycle kept/quieted/archived | maintenance.rs:568 `["active","quiet","consolidated","archived"]` | OK |
| Actions (guilds): child processes + **dry-run mode** + **audit log** | config.rs:704-707 flag `dry_run` (salida marcada `[DRY-RUN]`); `guild_audit_log` hash-chained federation/permissions.rs:207, kernel main.rs (línea 1236) | OK (hallazgo: ambas existían como claims sin anclar en H-26; ahora ancladas) |
| Safety: Coherence Gate + "runtime drift detection" | Coherence Gate ✓; el drift **commit-cargado-vs-disco** del Trust Console: screenshot existe (assets/screenshots/trust_console.png) pero `loaded_commit|git_sha` → 0 hits en crates/ del draft; solo hay Drift Guard de nodos (contenido) api_monitor.rs:527 | **[sin fuente verificada en código]** — o vive en el dashboard TS (no grepeado) o el caption sobre-promete; Needs-Human-Verify, no lo declaro deriva |
| Federation encrypted, local networks | Noise + mDNS opt-in (tylluan-link nat.rs/mdns; H-26 anchors) | OK |
| "from a Raspberry Pi with no downloads (`portable`)" | main.rs:17 "BM25-only, no model downloads. Zero dependencies, runs on a potato"; main.rs:641; ARM64 compila en CI ci.yml:142 | **Parcial**: perfil y target ARM verificados; NUNCA se corrió en un RPi real (CI solo compila; el propio README pide hardware reports) — claim de experiencia sin fuente en el árbol |
| "brings no model of its own: you bring the model" | mxbai-embed-large/bge-small SÍ se descargan en perfiles server/clinic (main.rs (línea 1086)-1087) | **Matiz de posicionamiento** (cierto para LLM/chat y para portable BM25; falso-estricto para embeddings): decisión de estilo de José, no error factual |
| "1301 tests across…" | AGENTS.md draft:64 1214+75+12=1301 | OK internamente; la cifra viva la valida check_test_count.sh en CI |
| README.es "1125 tests" (L225, sin tocar en el draft) | AGENTS draft=1301; fix real (1329) en cadena local sin push | 🔴 **DRIFT conocido: el draft heredó el es desactualizado**; al rebasear sobre la cadena del TL quedará 1125 vs 1329 → R1+sección2 lo cazan igual |
| Hero "Any MCP client, any model, federated by choice" | — | Posicionamiento (excluido de auditoría por regla H-24) |

## Impact

Cero escrituras fuera de este informe en mi worktree; worktree desechable removido tras correr todo (`git worktree remove`); ningún fichero del draft tocado; cero procesos/kernel tocado.

## No verificado

- Drift-detection del Trust Console en el código del dashboard TypeScript (grep solo cubrió crates Rust).
- Que `check_test_count.sh` valide 1301 en CI del draft: no corrí cargo (prohibido el kernel/cargas pesadas sin aviso).
