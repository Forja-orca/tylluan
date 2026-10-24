# IMPL_H-30 — 15 afirmaciones numéricas más expuestas de ROADMAP.md y STATUS.md, con fuente

Solo lectura sobre `da2ab9e` (reset hecho: divergentes de H-29 guardados en `E:\tylluan-patches\hermes\_saved\20261024_*`, worktree limpio). Push140 del TL en curso al hacer el trabajo; cero escrituras en `E:\tylluan` (checkout detached en MI worktree solo). Método igual que H-26: cada cita de código es literal grep-F contra este árbol.

## Tabla (reclamo → fuente → veredicto)

| # | Afirmación (doc:línea) | Fuente en el árbol | Veredicto |
|---|---|---|---|
| 1 | HEAD citada `0820581` (STATUS:4) | `git log`: padre de `da2ab9e` ✓; gate check_head_sync.sh | OK (dentro de 1) |
| 2 | v0.17.1 en Cargo.toml (STATUS:8) | `version = "0.17.1"` Cargo.toml ✓ (guardián §1 lo compara) | OK |
| 3 | tag v0.17.1 "apunta a ccdd509 (4 ficheros: CHANGELOG, VERSION, Cargo.toml, Cargo.lock)" (STATUS:8) | tag→`ccdd509` ✓ PERO el commit tocó **2 ficheros** (`.github/workflows/release.yml`, `install.ps1`) y no existe fichero `VERSION` en la raíz | 🔴 **inexacto: ni el conteo ni los ficheros** |
| 4 | "GLIBC_2.34 como máximo" (STATUS:8) | 0 hits de GLIBC/glibc en `.github/workflows/` | 🟡 **sin fuente en el árbol** (evidencia de artefactos de release; Needs-Human-Verify) |
| 5 | kernel corriendo `5222644` "leído 2026-10-14" (STATUS:9) | sha existe, pero STATUS:52 dice `5e49bf8` (2026-10-21) y ROADMAP:329 `955179f` (2026-10-22) → **3 valores distintos del mismo estado runtime** en dos docs; el vivo según ticks del 2026-10-23 es 955179f | 🔴 **deriva interna entre docs; estado runtime sin fuente** |
| 6 | 1329 tests = 1242+75+12 (STATUS:10,50) | AGENTS.md (guardián §2) + check_test_count.sh en CI | OK |
| 7 | 46 módulos FastMCP, 45 enrutables, visión excluida (STATUS:11; ROADMAP:5 "46 ficheros de guild, 45 enrutados") | regla R2 del guardián (45==45 ✓); `const EXCLUDED_GUILDS: &[&str] = &["vision_moondream"];` catalog.rs:98 | OK |
| 8 | "50 total en el kernel de Jose (5 externas)" (STATUS:11) | configuración runtime del host, no versionada en el árbol | 🟡 **sin fuente en el árbol** (auditoría D-34 cubre host de Jose) |
| 9 | "unwrap/expect producción 31+27=58 total… transporte 13, link 8, embeddings 2" (STATUS:29) | la herramienta del árbol `tools/count_production_unwraps.py --summary` da hoy **TOTAL 41/116/157** | 🔴 **no reproduce con la fuente declarada** (cifras envejecidas vs código actual) |
| 10 | "30 tests en security_audit.rs" (ROADMAP:27; AGENTS "30 security tests") | `grep -cE '#\[(tokio::)?test\]' crates/tylluan-kernel/tests/security_audit.rs` = **40** | 🟡 cifra envejecida (fuente existe, número no) |
| 11 | "reindexa solo cada 10 minutos main.rs (línea 1235)" (ROADMAP:102) | el valor sí existe: `Duration::from_secs(600)` + comentario "10 minutes for toaster-friendly" en **kernel** main.rs ~1919; pero `main.rs (línea 1235)` apunta hoy al texto de PID rancio del binario **cli** | 🟡 valor OK, **ancla fichero:línea driftada** |
| 12 | "34 entradas description_override()" (ROADMAP:113) | recuento actual en catalog.rs = **47** | 🟡 cifra histórica (M3) sin actualizar |
| 13 | "269/155/85 commits" entre tags (ROADMAP:242,252,265) | `git rev-list --count`: 275 / 157 / 88 (y "170" de ROADMAP:281 sí es exacto ✓) | 🟡 los 3 driftan (+6/+2/+3) |
| 14 | README.zh.md "~520 líneas" (ROADMAP:289) | `wc -l README.zh.md` = **609** | 🟡 drift |
| 15 | HNSW ">=12k umbral" (ROADMAP:143) + decay default 336h (ROADMAP:116) + cortocircuitos "3 fallos" (ROADMAP:172) | `HNSW_THRESHOLD`; `default_decay_half_life_hours() -> u64 { 336 }`; "Default 3" breaker_failures | OK los 3 (cooldown "60s" también: "Default 60." breaker_cooldown_s) |

Bonus OK verificados gratis: `mcp.rs:407` exacto (`"protocolVersion": client_protocol`) y `SUPPORTED_PROTOCOL_VERSIONS` con 4 versiones 2024-11-05→2026-07-28 (mcp.rs:33); `Cargo.toml:37` panic="abort" exacto; `ci.yml:110` sin `|| echo` ✓; dual_retrieval.rs:30/69 son hoy penalizaciones log2 (la forma `*0.1` evolucionó — la línea citada sigue siendo la zona de penalización).

## Sin fuente en el árbol (resumen explícito que pide la spec)

- GLIBC_2.34 (STATUS:8) — artefactos de release, no grepeable.
- Estado del kernel corriendo: 5222644 (STATUS:9) vs 5e49bf8 (STATUS:52) vs 955179f (ROADMAP:329) y "50 total con 5 externas" (STATUS:11) — runtime del host; el host de Jose lo cubre la auditoría D-34.
- "Discussion #2 publicada" (ROADMAP:23) y "topics del repositorio" (ROADMAP:290) — estado de GitHub, no del árbol.
- Roadmap abierto: tareas sin código (correcto por naturaleza, no son drift).

## Impact

Cero escrituras en `E:\tylluan` (push140 en curso respetado); MI worktree pasó a detached `da2ab9e` limpio; los 2 informes H-29/H-28 divergentes guardados antes en `_saved`. Sugiero al TL extender el guardián con **R4: anclas fichero:línea de ROADMAP/STATUS verificadas** (3 de 15 filas driftan solo en número/línea) y **R5: cifras que declara la propia herramienta** (unwrap 58 vs 157) — candidate, no implementado.

## No verificado

- Ejecución real de `check_test_count.sh`/CI sobre el draft: no corrí cargo.
- Cooldown 60s del breaker: "Default 60." documentado en el comentario del campo; no confirmé el valor de la función default asociada.
