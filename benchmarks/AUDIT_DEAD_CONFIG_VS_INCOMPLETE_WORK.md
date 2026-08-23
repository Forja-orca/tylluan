# Auditoría: Dead Config vs Trabajo Dejado Abierto

**Fecha:** 2026-08-24
**Ejecutado por:** Buffy (verificación propia, no confiando en reportes de otros agentes)
**Scripts ejecutados:** `check_dead_config.sh`, `check_dead_code_tests.sh`

---

## RESUMEN EJECUTIVO

| Categoría | Hallazgos | Impacto |
|-----------|-----------|---------|
| Dead config real (campo declarado, nunca leído) | 1 confirmado | Understated risk |
| Dead config semántico (parámetro aceptado, ignorado) | 1 confirmado | Zombie parameter |
| Dead references (guild excluido pero referenciado) | 4 ubicaciones | Benchmark pollution |
| Trabajo dejado abierto | 2 items | Tests sin assertions |
| Falsos positivos del script | 4 módulos "dead" | Heuristic limitation |

---

## 1. DEAD CONFIG REAL

### 1a. `InferenceProvider.capability` — **CONFIRMADO DEAD**

**Archivo:** `crates/tylluan-kernel/src/config.rs:721`
```rust
pub struct InferenceProvider {
    pub name: String,
    pub mcp_server: String,
    pub model_id: String,
    pub capability: Vec<String>, // ["chat", "vision", "thinking"]
}
```

**Evidencia:** `.capability` never appears read anywhere outside config.rs. Grep across entire `src/` returns 0 hits for `.capability` usage. The field is serialized from TOML but the values are never consumed by routing, filtering, or tool selection.

**Categoría:** Dead config. El campo existe, se parsea, pero nadie lo lee.

---

### 1b. `decay_half_life_hours` — **CONFIRMADO DEAD (zombie parameter)**

**Archivo:** `crates/tylluan-kernel/src/config.rs:919-920`
```rust
pub decay_half_life_hours: u64,  // default: 336 (14 días)
```

**Evidencia:**
- `decay.rs:45`: `let _hl = half_life_hours as f64;` — recibido, cast a `_hl` (prefijo underscore = intencionalmente no usado), nunca referenciado.
- `main.rs:1119-1125`: Tiene un warning explícito: *"config `silva.decay_half_life_hours` = {} is IGNORED: memory decay is FSRS-driven"*
- A pesar del warning, el campo sigue threaded through 7+ archivos: `main.rs`, `dream_cycle.rs`, `lifecycle.rs`, `sse.rs`, `api_admin.rs`, `http/mod.rs`
- El decay real usa `FsrsItem.retrievability()` con per-node stability, no half-life global.

**Categoría:** Dead config semántico. El parámetro se acepta y pasa, pero la función lo descarta. Es un zombie que sobrevivió a la migración FSRS.

---

## 2. DEAD REFERENCES (guild excluido pero referenciado)

### 2a. `vision_moondream` en timeout categorization

**Archivo:** `crates/tylluan-kernel/src/registry/guild_process.rs:876`
```rust
const HEAVY: &[&str] = &[
    "docker", "database", "pdf", "vision", "vision_moondream",
    ...
];
```

**Problema:** vision_moondream está en `EXCLUDED_GUILDS` (catalog.rs:98) y nunca puede ser enrutable, pero sigue en la lista de timeouts HEAVY. Si algún código consulta `guild_timeout_category("vision_moondream")`, devolverá Heavy — pero nunca debería llegar ahí porque el guild no existe en runtime.

**Categoría:** Dead reference. No causa bug funcional (el guild no arranca), pero ensucia la lógica de timeouts.

---

### 2b. `vision_moondream` en dataset de benchmark

**Archivo:** `benchmarks/dataset_i7_routing_curated.json:1012,1039`
```json
{"intent": "...", "target_guild": "vision_moondream", ...}
```

**Problema:** 2 de los 73 items del dataset targetean un guild que no puede ser enrutable. Esto infla artificialmente las métricas de "unknown" y distorsiona el benchmark.

**Categoría:** Trabajo dejado abierto. El dataset no fue actualizado tras la exclusión de vision_moondream.

---

### 2c. `vision_moondream` en evaluador y generador

**Archivos:**
- `benchmarks/benchmark_i7_j13_eval.py:63,111` — en GUILD_DESCRIPTIONS y KEYWORD_RULES
- `scripts/build_i7_dataset.py:59,139,140` — en el generador de dataset

**Problema:** El evaluador y generador siguen tratando vision_moondream como guild válido. Si alguien regenera el dataset, volverá a incluir items imposibles.

**Categoría:** Trabajo dejado abierto. Pendiente de actualizar tras la decisión de exclusión.

---

## 3. TRABAJO DEJADO ABIERTO

### 3a. Test `dump_catalog` sin assertions

**Archivo:** `crates/tylluan-kernel/tests/dump_catalog_test.rs`
```rust
#[test]
fn dump_catalog() {
    let catalog = tylluan_kernel::router::catalog::builtin_catalog();
    println!("CATALOG_COUNT: {}", catalog.len());
    for g in &catalog {
        println!("GUILD: {} cat={:?} mod={}", g.name, g.category, g.module_path);
    }
}
```

**Problema:** No tiene ninguna aserción. El test siempre pasa sin verificar nada. Es un dump de consola, no una prueba.

**Categoría:** Trabajo dejado abierto. Alguien creó el test para inspeccionar el catálogo pero nunca añadió validaciones.

---

### 3b. `check_dead_config.sh` es report-only

**Archivo:** `scripts/check_dead_config.sh`

**Problema:** El script detecta dead config correctamente (encontró `capability` y `encrypt_at_rest`), pero siempre sale con exit 0 salvo `--strict`. No bloquea CI. La documentación dice *"promote it to a blocking CI gate only once the suspect list has been triaged"* — ese triage nunca se hizo.

**Categoría:** Trabajo dejado abierto. El script funciona pero no protege.

---

## 4. FALSOS POSITIVOS DEL SCRIPT (NO SON DEAD)

Los 4 módulos que `check_dead_code_tests.sh` reportó como "dead" son **falsos positivos** del heurístico grep:

| Módulo | Por qué parece dead | Por qué está vivo |
|--------|---------------------|-------------------|
| `maintenance` | `.maintenance` no aparece fuera de config.rs | Se usa vía `tylluan_kernel::maintenance::` (import path, no method) |
| `guard` | `.guard` no aparece fuera de config.rs | Se usa vía `use tylluan_kernel::guard::GuardedTask` (5 usos en main.rs) |
| `tunnel` | `.tunnel` no aparece fuera de config.rs | Se usa vía `tylluan_kernel::tunnel::TunnelManager` (main.rs:365) |
| `metrics_exporter` | `.metrics_exporter` no aparece fuera de config.rs | Se usa vía `crate::metrics_exporter::metrics_handler` (routes.rs) |

**Conclusión:** El script detecta `.field_name` patterns pero no `use crate::module::Type` imports. Es una limitación conocida del heurístico grep. Los 4 módulos están vivos y funcionales.

---

### 4a. `encrypt_at_rest` — **FALSO POSITIVO confirmado**

**Archivo:** `crates/tylluan-kernel/src/config.rs:1056-1057`

El script lo marca como suspect, pero `encrypt_at_rest` se lee dentro de `open_db()` — una función free (no un método `&self`), así que el check de accessor pattern no lo detecta. Confirmado vivo: usado en `curriculum.rs`, `federation`, `agent_memory.rs`, `agent_profile.rs`.

---

## 5. TABLA RESUMEN

| # | Hallazgo | Archivo:Línea | Tipo | ¿Causa bug? | Acción |
|---|----------|---------------|------|-------------|--------|
| 1 | `InferenceProvider.capability` nunca leído | config.rs:721 | Dead config | No | Eliminar campo o documentar uso futuro |
| 2 | `decay_half_life_hours` zombie parameter | config.rs:919 | Dead config semántico | No | Eliminar parámetro de `apply_decay()` signature |
| 3 | `vision_moondream` en HEAVY timeouts | guild_process.rs:876 | Dead reference | No | Eliminar de la lista |
| 4 | `vision_moondream` en dataset benchmark | dataset:1012,1039 | Work-left-open | Sí (infla métricas) | Eliminar 2 items del dataset |
| 5 | `vision_moondream` en eval.py | eval.py:63,111 | Work-left-open | Sí (evalúa guild muerto) | Eliminar de GUILD_DESCRIPTIONS |
| 6 | `vision_moondream` en build_i7_dataset.py | build_i7_dataset.py:59,139,140 | Work-left-open | Sí (regenera items inválidos) | Eliminar del generador |
| 7 | `dump_catalog` test sin assertions | dump_catalog_test.rs | Work-left-open | No (test no-op) | Añadir assertions o eliminar |
| 8 | `check_dead_config.sh` no bloquea CI | check_dead_config.sh | Work-left-open | No | Triaged y promover a --strict |

---

## 6. LO QUE NO ES NI DEAD NI ABIERTO

- **`encrypt_at_rest`**: Vivo, leído vía `open_db()` free function. Falso positivo del script.
- **`maintenance`, `guard`, `tunnel`, `metrics_exporter`**: Módulos vivos, usados vía import paths. Falsos positivos del heurístico grep.
- **`dry_run`**: Vivo, leído vía accessor `guilds_dry_run()`. Correctamente detectado por el script.
- **`capabilities_enforce`**: Vivo, leído vía accessor. Correctamente detectado.
