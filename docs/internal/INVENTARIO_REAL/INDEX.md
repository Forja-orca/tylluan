# INVENTARIO_REAL — índice

**Estado:** en curso (tandas 5-7 hechas, 2026-10-28). **17 items** en `inventario.json`
(REAL 5 · A_MEDIAS 7 · MAL_PLANTEADO 3 · SIN_VERIFICAR 2); áreas 6 y 7 cerradas.
**Base:** `a564880`. **Autora:** Buffy. **Bitácora:** [PROGRESO.md](PROGRESO.md).

## Cómo se clasifica cada fila

| Estado | Significado |
|---|---|
| **REAL** | existe, está cableado y se ha comprobado con ejecución o lectura de código concluyente |
| **A_MEDIAS** | existe pero incompleto, sin cablear, sin tests, solo en rama o detrás de un flag apagado |
| **MAL_PLANTEADO** | existe pero su diseño no sirve al objetivo del producto (se dice por qué y la alternativa) |
| **FALTA** | debería existir según README/ROADMAP/ADR/SPEC o por coherencia, y no existe |
| **SIN_VERIFICAR** | hay indicio pero no evidencia suficiente (comando o `fichero:línea`) |

Cada fila lleva además **gravedad** (CRITICA/ALTA/MEDIA/BAJA), **esfuerzo** (S/M/L), **evidencia** (comando y salida, o `fichero:línea`), **verificado** (sí/no) y **quien**.

## Áreas (una tanda cada una)

| Tanda | Área | Fichero previsto | Estado |
|---|---|---|---|
| 6 | Kernel Rust: módulos y cableado | [AREA_06_kernel.md](AREA_06_kernel.md) | ✅ 24 módulos, 8 filas |
| 7 | Transporte: 6 herramientas MCP + rutas REST/A2A | [AREA_07_transporte.md](AREA_07_transporte.md) | ✅ 249 rutas vivas, 9 filas |
| 8 | Memoria: Silva, grafo, recall/remember, FSRS, consolidación, embeddings | `AREA_memoria.md` | pendiente |
| 9 | Seguridad: CoherenceGate, write gate, auth, trust, cuarentena, federación, P2P, grants, sandbox | `AREA_seguridad.md` | pendiente |
| 10 | Guilds: 5 representativos con llamada real en kernel de prueba | `AREA_guilds.md` | pendiente |
| 11 | Inferencia: llama_backend, ONNX, servicio de decisión | `AREA_inferencia.md` | pendiente |
| 12 | CLI e instaladores | `AREA_cli_instaladores.md` | pendiente |
| 13 | Dashboard y UI: pantalla → endpoint → REAL/CALCULADO/MOCK | `AREA_dashboard_ui.md` | pendiente |
| 14 | CI/CD y release | `AREA_ci_release.md` | pendiente |
| 15 | Tests: recuento, `#[ignore]`, huecos | `AREA_tests.md` | pendiente |
| 16 | Docs y promesas (README/STATUS/ROADMAP/SPEC vs código) | `AREA_docs.md` | pendiente |
| 17 | Federación y malla | `AREA_federacion.md` | pendiente |
| 18 | Empaquetado (aur/, scoop/, packages/, Docker, desktop) | `AREA_empaquetado.md` | pendiente |
| 19 | Benchmarks y evals | `AREA_benchmarks.md` | pendiente |
| 20 | Proceso de flota (worktrees, colas, IMPL pendientes) | `AREA_flota.md` | pendiente |
| 21 | Decisiones de José (mapa_bloques.json, J-1..J-7) | `AREA_decisiones.md` | pendiente |
| 22 | Cierre: 10 hallazgos, 10 victorias, autoauditoría, dashboard | `INDEX.md` final | pendiente |

## Evidencia ya consolidada (entra en las áreas 9, 10, 14, 15 sin re-medir)

- **Tests:** 1301 lib tests → 1297 passed / 4 ignored (base `d79990a`); **24** atributos `#[ignore]` en `crates/`; **6** rojos verificados con reproducción (detalle en `IMPL_M-BUFFY-13` §3).
- **Seguridad / gate HITL:** D-13 y X1 abiertos (CRÍTICOS, misma causa: sin binding token→agent_id); T-DSH-10 B2 (ALTA).
- **Guilds:** `guilds/core/knowledge.py` **no existe**; el guild está en `guilds/scholars/plugins/knowledge.py`.
- **CI:** `knowledge_guild_test` invisible (feature apagada) y roto con la feature (3× E0599).
- **ADR-024:** 5 controles con test rojo ya escrito (`adr024_containment_pending.rs`), listos para activar si José aprueba.
