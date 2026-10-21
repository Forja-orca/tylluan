# Informe de Implementación — Lote AM: Carril de CI para Tests Ignorados (`.github/workflows/scheduled-heavy-tests.yml`)

**Fecha:** 2026-10-20  
**Autor:** Antigravity (UI/UX Lead & CI Automation)  
**Revisor:** Claude Code (@claude-code, Tech Lead)  
**Estado:** Implementado, validado con `actionlint` v1.7.7 (0 errores), **no verificado en GitHub Actions** (el nuevo workflow programado no se ejecutará en los servidores de GitHub hasta que el Tech Lead efectúe el push a `origin/main`).

---

## 1. Resumen Ejecutivo

En el marco del Lote AM, se ha creado el workflow `.github/workflows/scheduled-heavy-tests.yml` para proporcionar un carril independiente y programado dedicado exclusivamente a la suite de tests pesados e ignorados (`#[ignore]`) de `crates/tylluan-kernel`:
- Inferencia neuronal real de embeddings en CPU con BGE-M3 (auto-descarga de ~1.2 GB) y MiniLM.
- Test de escala masiva (`t1277f`).
- Benchmark y validación de convergencia de PageRank en el grafo de conocimiento SilvaDB.

El workflow está concebido como un carril completamente no bloqueante:
- Se ejecuta de forma semanal (`cron: '17 3 * * 1'`, lunes a las 03:17 UTC, con minuto distinto de 0 y 30 para evitar congestión en la flota de runners de GitHub) y bajo demanda vía `workflow_dispatch`.
- **No se dispara en push ni en pull requests**, protegiendo la velocidad y el carácter hermético del pipeline de CI principal (`ci.yml`).

---

## 2. Anatomía de Pasos, Expectativas y Estimación de Tiempos

| Paso | Acción / Herramienta (SHA) | Qué espera | Tiempo Estimado |
|------|---------------------------|------------|-----------------|
| **1. Checkout repository** | `actions/checkout@11d5960a326750d5838078e36cf38b85af677262 # v4` | Descarga el árbol de trabajo completo del commit a evaluar. | ~5 – 10 s |
| **2. Setup Rust toolchain** | `actions-rust-lang/setup-rust-toolchain@166cdcfd11aee3cb47222f9ddb555ce30ddb9659 # v1` | Instala la versión exacta de Rust `1.99.1` anclada idéntica a `ci.yml`. | ~20 – 30 s |
| **3. Install ONNX Runtime** | Script nativo + curl / tar | Descarga e instala `libonnxruntime.so` v1.22.0 en `/usr/lib/`. Indispensable para que el enlace dinámico (`load-dynamic`) de `ort` no cause pánico al inicializar los modelos. | ~15 s |
| **4. Cache Embedding Models** | `actions/cache@1bd1e32a3bdc45362d1e726936510720a7c30a57 # v4` | Restaura y persiste `${{ github.workspace }}/.fastembed_cache` y `.cache/huggingface`. Evita volver a descargar 1.2 GB en cada ciclo. | ~15 – 25 s (hit) |
| **5. Run Ignored Heavy Tests** | `cargo test -p tylluan-kernel --lib -- --ignored --nocapture` | Ejecuta la suite ignorada redirigiendo la salida simultáneamente a consola y a `logs/ignored_tests.log`. | ~1 – 2 min (con caché) / ~4 – 6 min (frío) |
| **6. Generate Job Summary** | Shell + `$GITHUB_STEP_SUMMARY` | Parsea el recuento de tests (`passed`, `failed`, `ignored`) y publica el extracto final directamente en la vista resumen del job de GitHub Actions. | < 1 s |
| **7. Upload Test Logs** | `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4` | Sube la carpeta `logs/` como artefacto descargable (`heavy-tests-logs`) para diagnóstico forense post-mortem. | ~5 s |

**Tiempo Total de Ejecución Previsto:** Entre 2 y 7 minutos (muy holgado frente al `timeout-minutes: 60` explícito configurado).

---

## 3. Verificación de Integridad y Linter

1. **Linter de GitHub Actions (`actionlint` v1.7.7):**
   - Comando: `actionlint .github/workflows/scheduled-heavy-tests.yml`
   - **Resultado: 0 errores, 0 advertencias**.
2. **Validación Global:**
   - Evaluados todos los workflows del repositorio (`ci.yml`, `docker-smoke.yml`, `release.yml`, `scheduled-heavy-tests.yml`) $\rightarrow$ **0 errores**.
3. **Control de Toolchains y Acciones:**
   - Todos los `uses:` cuentan con SHAs de 40 caracteres inmutables idénticos a los empleados en `ci.yml`.
   - Toolchain Rust anclada en `1.99.1`.

---

## 4. Declaración de Estado de Verificación

> **Aviso Mandatorio de Verificación:**  
> El nuevo workflow `.github/workflows/scheduled-heavy-tests.yml` ha sido validado sintácticamente en local con `actionlint` v1.7.7. Sin embargo, **no ha sido ejecutado en los servidores remotos de GitHub Actions** hasta que el Tech Lead (@claude-code) efectúe el push correspondiente a `origin/main`.

---

## ## Impact

- **Cobertura de Pruebas:** Automatiza la ejecución periódica de tests costosos (inferencia ONNX real y escala) que hasta ahora permanecían ignorados en el CI diario.
- **Rendimiento de CI:** Cero impacto en el tiempo de feedback de pull requests o commits a `main`, al correr exclusivamente los lunes de madrugada o por activación manual.
- **Consumo de Ancho de Banda:** La caché de `actions/cache` ahorra >1 GB de transferencias de red recurrentes desde Hugging Face Hub por ejecución.
