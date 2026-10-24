# IMPL_H-26 — Auditoría de TODAS las afirmaciones de README.md (afirmación → fichero:línea)

Spec T1670: solo lectura; tabla de fuentes + 3 reglas nuevas para el guardián. Árbol: `7b24da2` (origin/main), reset previo cumplido (todo `??`/`M` idéntico a `b85d0bb` del TL, stash de red hecho). Método: lectura completa del README (607 líneas) y grep -n de cada ancla contra el árbol; el conteo de guilds se replicó con el **criterio exacto del kernel** (`scan_guilds_directory`, catalog.rs:536-576), no con `find | wc`.

## Tabla afirmación → fuente

| Afirmación (README:línea) | Fuente en el árbol | Estado |
|---|---|---|
| Licencia MIT (L4, L601) | LICENSE:1 «MIT License» | OK |
| v0.17.1 actual (L24, L317) | tag `v0.17.1`, fecha 2026-10-13 (git log) | OK |
| 45 guilds (L59, L392, L217) | réplica de scan_guilds_directory: core 18+builders 14+scholars 9+wardens 2+watchers 3 = 46 − excluido `vision_moondream` (catalog.rs:98) = **45** | OK exacto |
| 6 herramientas soberanas (L116-128) | `expose_guild_tools` config.rs:611; test «six tools» mcp_registry_tests.rs (línea 1137); CONTRACT-01 six (tests.rs:636) | OK |
| `tylluan_decide` nuevo en main 2026-10-16, no en tag (L132) | commit 8aae6aa 2026-10-16; tag v0.17.1 = 2026-10-13 < 10-16; `git tag --contains` = vacío | OK |
| REST twin `POST /api/v1/decide` (L132) | router/decision/api_types.rs:6 (tipos compartidos MCP/REST) | OK |
| LongMemEval R@5 82%, R@10 90%, R@1 46%, MRR 0.46 (L142-145) | benchmarks/longmemeval_v0.12.0.json (notes: «82% R@5 > 50%», fecha 2026-07-05, BGE-M3) | OK, con disclaimer honesto (L136) |
| Corpus sintético 50% R@5 (L150) | mismo JSON («synthetic … gave 50%») | OK |
| 500 preguntas NO afirmado — dice «50 human-authored» (L138) | JSON: 50 preguntas | OK (cuidado: cifra mal recordada en circulos previos; el README actual es correcto) |
| Perfiles portable/clinic/server, bge-small 384d ~67MB (L156-163) | tylluan-cli/src/main.rs:19, 1086-1087 | OK |
| install por defecto portable (L181) | main.rs InstallProfile + assert test 1593 | OK |
| Binarios Linux x86_64/aarch64, Windows x86_64, macOS arm64 (L241-244) | .github/workflows/release.yml:24-31 (aarch64-linux, aarch64-apple, x86_64-windows-msvc; 8 entradas target) | OK [falta confirmar x86_64-linux en el matrix completo] |
| Python 3.12 + FastMCP para guilds; doctor `degraded` (L217) | doctor: status «degraded» doctor/mod.rs:112 | OK la mecánica; «verified 2026-08-22» = evento externo |
| Kernel en 127.0.0.1 (línea 47004) (L248, L368) | DEFAULT_PORT=47004 main.rs:12; «Canonical port» config.rs (línea 1966); host default main.rs (línea 1123) | OK |
| `.tylluan-token`, dev_mode (L252-254, L551) | dev_mode auto-approve solo OAuth: oauth.rs:86,1059; commit 61bc97e (OAuth Phase 1) | OK |
| `data/active_port.json` / `TYLLUAN_PORT` (L575) | transport/http/mod.rs:447-448 | OK |
| A2A `/.well-known/agent-card.json` + JSON-RPC (L234, L330) | transport/http/a2a.rs:775; a2a_client.rs:14 | OK |
| Noise NK/XK, ChaCha20-Poly1305, Ed25519 (L134, L468-469, L398) | chacha20poly1305 Cargo.toml:100 | OK (dependencia;NK/XK en link) |
| STUN opt-in, no en arranque por defecto (L133) | config.rs:383-389 («must NOT run» en arranque) | OK |
| mainline DHT deshabilitado por defecto (L224) | config.rs:434 `default_mainline_dht_enabled() -> false` | OK |
| ONNX CPU + DirectML/CUDA sin GPU externa (L260) | config.rs:922 (auto DirectML Windows); kernel Cargo.toml:149 | OK |
| Signal Loop `recall_feedback` (L264, L331) | memory/night/feedback_signal_phase.rs:3 (ADR-011) | OK |
| PageRank + degree penalty (L126, L382, L190) | silva/graph.rs:931-932 (CONTRACT-01 degree penalty DIVISION); LightRAG en evals/src/tests.rs:134-180 | OK |
| HNSW >12k nodos, 1024d (L280, L382) | silva/hnsw.rs:5 `HNSW_THRESHOLD: usize = 12_000` | OK umbral |
| Lifecycle quiet/archived (L284) | night/lifecycle_phase.rs:35 (consolidated→archived) | OK |
| `[federation]` canónico, `[silva] sync_interval_ms` deprecado (L503) | config.rs:358-366 (+effective_sync_interval_ms) | OK |
| `federation_source` excluido de rebote (L521) | graph_rag.rs:199 | OK |
| Space = topic_key→metadata.space→default (L527) | federation/permissions.rs:84-86 | OK |
| deny-by-default permisos peer (L525, L348) | federation/permissions.rs:8,63 («deny-all, error never grants») | OK |
| SQLCipher tras feature `encryption` (L296) | kernel Cargo.toml:145 `encryption = ["rusqlite/bundled-sqlcipher-vendored-openssl"]` | OK |
| Embedding cache LRU (L303) | lru 0.12 Cargo.toml:122; QueryEmbeddingCache silva/mod.rs:195 | OK |
| FSRS-5 crate (L483) | crates/tylluan-fsrs/Cargo.toml:7 | OK |
| Dashboard dev  (línea 5173) (L269 implícito) | CORS http/mod.rs (línea 1319) (dev (línea 5173) prod (línea 3030)) | OK |
| Capturas screenshots/*.png (L105 etc.) | assets/screenshots/: 5 citadas, 5+ presentes | OK |
| Rust 1.88+ (L36) | rust-toolchain.toml:2 canal 1.99.1 (≥1.88) | OK |

## Afirmaciones SIN fuente en el árbol (clase aparte, no deriva)

1. «(verified 2026-08-22)» L217 y similares: eventos de verificación pasados — fuente = logs/historial, no el árbol. El guardián no puede ni debe falsearlas; proponer permitirlas con etiqueta `verified YYYY-MM-DD` y no auditarlas.
2. Números de arranque en ROADMAP/otro doc (no en README actual) — verificables solo corriendo el binario.
3. Llamadas a action («Raspberry Pi reports», Discussions URL): no son afirmaciones de estado.

## 3 reglas nuevas propuestas para check_readme_claims.sh

- **R1 — paridad bilingüe numérica**: multiset de tokens numéricos/versión de README.md debe coincidir con README.es.md (regex `v?\d+(\.\d+)+|\d[\d,.]*\s*(%|tests|guilds|questions)`). La deriva 1125/1329 (cazada en H-24) habría sido bloqueada antes, no después. Costo: ~10 líneas awk/sort. Falsos positivos: cifras idiomáticas («30 seconds») — permitir lista de exclusión.
- **R2 — conteos con criterio del kernel, no `find`**: la afirmación «N guilds» debe contrastarse replicando `scan_guilds_directory` (familia→plugins/, `.py` sin `_`, contiene `FastMCP(`, menos `EXCLUDED_GUILDS` parseado de catalog.rs:98). Un `find | wc` ingenuo da 64 o 46 y **falsea la deriva en ambas direcciones**; el criterio real dio 45 exacto hoy. Misma idea para «N tools» (ya hecho por nombre) y tests (check_test_count ya lo hace).
- **R3 — cifras de benchmark con artefacto JSON backing**: todo número en tabla de rendimiento debe existir como valor en el JSON citado en la misma línea/sección (parsear `benchmarks/*.json` y comparar 82/90/46/0.46 vs JSON v0.12.0), y el README debe declarar fecha+modelo del run (patrón disclaimer L136 ya existente → exigir su presencia). Cierra la clase «cifra redonda sin fuente» que casi reportamos como deriva este tick (86.8/92.2/500q eran fantasmas de memoria; el README actual dice 82/90/50).

## Impact

Solo lectura: cero escrituras fuera de este informe, cero ejecuciones de kernel/cargo/red. No toca scripts (reglas propuestas, no implementadas — implementación es tarea aparte con luz verde del TL).

## No verificado

- ~~Matrix de release.yml~~ **Cerrado post-entrega**: 4/4 targets afirmadas confirmadas — `x86_64-unknown-linux-gnu` en release.yml:21 (corregido en fe de T siguiente; también se corrigió mi conteo de líneas del informe: 65, no 99).
- NK/XK y Ed25519: confirmado el dependency ChaCha20-Poly1305; el handshake NK/XK vive en tylluan-link y no lo línea-a-línea (confirmado en entregas previas H-docs, no repetido aquí).
- Reglas propuestas: sin prueba de falsos positivos sobre histórico (haría falta correrlas contra los últimos 20 READMEs).
