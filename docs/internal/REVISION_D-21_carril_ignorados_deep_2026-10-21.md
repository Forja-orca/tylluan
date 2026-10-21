# D-21 — Revisión del carril de CI para tests ignorados (cubierto por el lote AM)

- **Fecha:** 2026-10-21
- **Agente:** Deep (OpenCode)
- **Cola:** `docs/internal/COLA_deep_2026-10-21.md`, ítem **D-21** — marcado `CUBIERTO` por el TL
  (lote AM de Antigravity, commit `9cf5544`). **No implementado por mí**; este informe es solo la
  revisión que la cola ofrece "si sobra tiempo".
- **Archivo revisado:** `.github/workflows/scheduled-heavy-tests.yml`

## Lo que el workflow hace bien

- Semana a las 03:17 UTC (minuto no redondo), `workflow_dispatch` manual, `permissions: contents: read`.
- Las tres actions van **pinneadas por SHA**, no por tag móvil.
- Toolchain fijado (1.99.1) y coherente con `rust-toolchain.toml`.
- ONNX Runtime instalado igual que el resto de la CI, caché de modelos con `restore-keys`.
- `if: always()` en resumen y artefacto: un fallo deja el log descargable.
- `timeout-minutes: 60` en el job evita el runner colgado.

## Riesgos reales (por orden de impacto)

1. **Cobertura parcial silenciosa.** El comando es `cargo test -p tylluan-kernel --lib -- --ignored`.
   Eso **no** ejecuta:
   - los `#[ignore]` de `crates/tylluan-kernel/tests/` (`a2a_live.rs`, `decide_adversarial.rs` (8),
     `oauth_f2_adversarial_spec.rs` (5), `knowledge_guild_test.rs`, `sparse_cascade_bench.rs`,
     `sparse_signature_spike.rs`, `approve_action_self_approval.rs` (2)) — son targets de
     integración, no `--lib`;
   - el `#[ignore]` de `src/main.rs` (test del binario `tylluan-nexus`, target `--bin`);
   - `crates/tylluan-evals`.
   El nombre del job ("Heavy Ignored Tests") sugiere más cobertura de la que da. Y entre los
   excluidos está justo el contrato adversarial de OAuth F2 (`oauth_f2_adversarial_spec.rs`), que es
   el que D-15 necesita activar.
2. **Sin control de paralelismo.** Los tests ignorados de la lib incluyen `t1277f_scale` (12 500
   nodos con HNSW real) y los de embeddings reales (descarga de modelos): lanzados en paralelo por
   defecto en un runner de 4 GB pueden competir por memoria. Un `--test-threads=1` (o
   `RUST_TEST_THREADS=1`) para este carril sería más predecible.
3. **Descarga de ONNX sin verificación de integridad.** `curl -L` contra un release de GitHub, sin
   `sha256sum -c`. El resto de la cadena está pinneada por SHA; esto es el eslabón suelto.
4. **Clave de caché obsoleta.** `key: embedding-models-bge-m3-v1` mientras el modelo por defecto es
   `mxbai-embed-large` (AGENTS.md, desde 2026-09-30). Funciona por `restore-keys`, pero el nombre
   miente sobre qué contiene.
5. **Resumen frágil.** `grep -oE '[0-9]+ passed' | tail -1` sobre un log con `--nocapture` puede
   capturar una línea del test en vez del `test result:` final. Cosmético (el artefacto manda).
6. **Sin `concurrency:`**: dos ejecuciones solapadas (schedule + dispatch) comparten nada crítico,
   pero duplican minutos de runner.

Ninguno es bloqueante; el 1 es el que puede dar falsa confianza.

## Recomendación (para el TL / AM, no ejecutada)

Añadir un segundo paso (o matriz) con:

```yaml
cargo test -p tylluan-kernel --tests -- --ignored
```

y un tercero para `--bin tylluan-nexus`, con `RUST_TEST_THREADS=1` en el primero. Y verificar el
`.tgz` de ONNX con `sha256sum`. No lo implemento: el ítem está marcado como cubierto y el workflow
es del lote AM.
