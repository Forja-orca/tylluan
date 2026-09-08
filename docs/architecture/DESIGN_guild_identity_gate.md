# G6 — Gate de identidad de guilds: diseño de contrato canónico

**Estado:** APROBADO por José (2026-09-08) para implementación — corresponde directamente a la fase "Capability Contracts" del roadmap v1.0 (ver `EXTERNAL_AUDIT_2026-09-08_v1_roadmap.md`, fase P0 #2). Asignado a Buffy.
**Fecha:** 2026-09-07
**Origen:** auditoría G6 de Buffy (T298), verificación cruzada con código real.

## ⚠️ Problema real y evidencia

El repositorio tiene múltiples "catálogos canónicos" de guilds que ya **no coinciden entre sí**. Esto no es un riesgo teórico — produce comportamiento observable:

| Fuente | Conteo | Divergencia |
|--------|--------|-------------|
| `catalog.rs` — `KNOWN_GUILDS` | 45 | Usa `scrapling` (normalizado via `name_override()`) |
| `guild_list.rs` — `LAZY_GUILDS` | 21 | Lista paralela, no derivada |
| `scripts/build_i7_dataset.py` — `GUILD_CATALOG` | 43 | Usa `scrapling_web` (sin normalizar) |
| `benchmark_i7_j13_eval.py` | 44 | Conserva `council` y `whats_new` (no-routable), omite `scheduler` y `seed_tools` |
| `tylluan.toml` — `always_on` | 13 | Configuración runtime |
| `tylluan.example.toml` — `always_on` | 3 | Ejemplo desfasado |
| `main.rs` — registro V2 | 4 listas | Documenta "exactamente dos" vías, pero tiene tres |

### Divergencias concretas verificadas

1. **`scrapling_web` vs `scrapling`**: I-7 (`build_i7_dataset.py:19-72`) usa `scrapling_web`; el runtime y el evaluator usan `scrapling` por el alias `name_override()` en `catalog.rs`. El dataset puede generar targets que el evaluator no puede resolver.
2. **Evaluator desfasado**: `benchmark_i7_j13_eval.py:30-75` conserva `council` y `whats_new` (marcados no-routable en `:217-225`), omite `scheduler` y `seed_tools`.
3. **`ALWAYS_ON_GUILDS` hardcodeado**: `catalog.rs` tests mantienen una lista manual que no se deriva de TOML. Si se añade un guild a `always_on` en TOML pero no a la lista de tests, la consistencia se rompe silenciosamente.
4. **Documentación incorrecta**: `main.rs:843-846` dice que existen "exactamente dos" vías de registro; el código implementa también la vía V2 (`:841-871`).
5. **Excepciones dispersas**: `vision_moondream` está excluido en producción por `EXCLUDED_GUILDS`; `sandbox` se auto-descubre pero se permite mediante excepciones de tests. El comportamiento está explicado pero no modelado en una única política.

---

## Contrato canónico: `GuildId`

Un solo identificador público por guild, con un solo registro canónico que lo define:

### Identidad

- **`GuildId`**: `snake_case`, estable, independiente del nombre de archivo Python y del nombre `FastMCP("tylluan-*")`.
- Ejemplo: `scrapling_web.py` → `FastMCP("tylluan-scrapling-web")` → **`GuildId = scrapling`**.
- `scrapling_web` es un alias legacy aceptado solo en boundaries de migración de dataset, no en routing ni evaluación.

### Registro canónico único

Cada guild tiene **un** registro que contiene:

```toml
[[guilds]]
id = "scrapling"                    # GuildId canónico
module = "guilds/scrapling_web.py"  # Path de implementación (descubrimiento por filesystem)
aliases = ["scrapling_web"]         # Aliases aceptados en input legacy
status = "routable"                 # routable | experimental | excluded
registration = "lazy"               # lazy | always_on | v2
```

### Estados mutuamente excluyentes

| Estado | Significado | Ejemplo |
|--------|-------------|---------|
| `routable` | Existe en el catálogo runtime, puede ser evaluado | `bash`, `filesystem`, `scrapling` |
| `experimental` | Se auto-descubre para desarrollo, no es enrutable ni se evalúa | `sandbox` |
| `excluded` | Ausente intencionalmente del catálogo routable | `vision_moondream` |

Los estados `routable`, `experimental` y `excluded` son **mutuamente excluyentes** y se representan una sola vez, no a través de `EXCLUDED_GUILDS`, `NOT_GUILDS` y excepciones de tests separadas.

### Política de registro

- El catálogo canónico owns qué guilds existen y son routables.
- `LAZY_GUILDS` se convierte en **metadata de política runtime**, no en una segunda lista de identidad.
- TOML controla la política de activación (`always_on`, warm pool, V2), no define si un guild existe.
- Nombres desconocidos en `always_on`, `warm_pool` o declaraciones V2 **deben fallar un check de consistencia**, no crear registros fantasma silenciosamente.

### Benchmarks

- I-7 y el evaluator consumen el set canónico de `GuildId`.
- **No mantienen diccionarios de nombres escritos a mano independientes.**
- `council` y `whats_new` son herramientas internas no-routable y **no son targets de benchmark**.
- `scheduler` y `seed_tools` son routables y **deben estar incluidos**.

---

## Resolución de divergencias concretas

| Divergencia | Resolución |
|-------------|------------|
| I-7 usa `scrapling_web` | Normalizar a `scrapling` con alias `scrapling_web` en migración de dataset |
| Evaluator conserva `council`, `whats_new` | Eliminar del set de targets benchmark |
| Evaluator omite `scheduler`, `seed_tools` | Añadir al set de targets benchmark |
| `ALWAYS_ON_GUILDS` hardcodeado en tests | Derivar del registro canónico; test de consistencia contra TOML |
| `main.rs` documenta "dos vías" | Corregir documentación a "tres vías" (lazy, always_on, V2) |
| `EXCLUDED_GUILDS` / `NOT_GUILDS` separados | Consolidar en campo `status` del registro canónico |

---

## Diseño del gate CI

### Opción recomendada: snapshot generado + live comparison

**Enfoque:** un script Python en `tools/` genera un snapshot determinista desde el registro canónico (`tylluan.toml` o equivalente) y lo compara contra las fuentes de consumo (runtime, I-7, evaluator).

```
┌─────────────────────────┐
│  Registro Canónico       │  ← tylluan.toml [[guilds]]
│  (fuente única)          │
└──────────┬──────────────┘
           │ genera
           ▼
┌─────────────────────────┐
│  Snapshot JSON           │  ← tools/guild_catalog_snapshot.py
│  (GuildId + status +     │     generado en CI
│   aliases + registration)│
└──────────┬──────────────┘
           │ compara contra
           ├──────────────────► catalog.rs KNOWN_GUILDS
           ├──────────────────► build_i7_dataset.py GUILD_CATALOG
           ├──────────────────► benchmark_i7_j13_eval.py
           ├──────────────────► tylluan.toml always_on / v2
           └──────────────────► filesystem guilds/*.py
```

### Qué verifica el gate

1. **Unicidad**: cada `GuildId` aparece exactamente una vez en el registro canónico.
2. **Alias coherente**: si un guild tiene alias, el alias aparece en la lista de aliases, no como un GuildId separado.
3. **Archivo existe**: el `module` path apunta a un `.py` existente en `guilds/`.
4. **Nombre FastMCP consistente**: el stem del archivo produce un `GuildId` que coincide con el registro (post-`name_override`).
5. **Benchmarks consumen el set canónico**: I-7 y evaluator contienen exactamente los GuildIds con `status = "routable"`.
6. **TOML coherente**: nombres en `always_on` / V2 existen en el registro canónico y son `routable`.
7. **Sin nombres fantasma**: ningún GuildId aparece en consumers pero no en el registro canónico.

### Qué NO verifica el gate (non-goals)

- **Comportamiento runtime**: el gate es estático (nombres, estados, paths), no invoca routing ni ejecuta benchmarks.
- **Calidad de los guilds**: no evalúa si un guild funciona correctamente, solo si existe y es identificable.
- **Compatibilidad con MCP server names**: los nombres `tylluan-*` de FastMCP son metadata; el gate no los valida contra `GuildId`.
- **Migración automática**: el gate detecta drift, no lo corrige. La corrección es manual (o en un futuro, un `cargo make` alias-sync).

---

## Orden de implementación

1. **Definir el registro canónico**: mover las listas actuales dispersas a un único `[[guilds]]` en `tylluan.toml` (o `guilds/catalog.toml` si se prefiere separación).
2. **Escribir `tools/guild_catalog_snapshot.py`**: script que lee el registro canónico y genera un snapshot JSON determinista.
3. **Escribir `tools/check_guild_consistency.py`**: compara el snapshot contra las fuentes de consumo y falla si hay divergencias.
4. **Integrar en CI**: el check corre en cada PR que toque `guilds/`, `tylluan.toml`, benchmarks o `catalog.rs`.
5. **Corregir las divergencias conocidas**: normalizar `scrapling_web→scrapling`, añadir `scheduler`/`seed_tools` al evaluator, eliminar `council`/`whats_new`.
6. **Eliminar listas redundantes**: `EXCLUDED_GUILDS`, `NOT_GUILDS`, la lista manual de `ALWAYS_ON_GUILDS` en tests.

---

## Formato del snapshot generado

```json
{
  "guilds": {
    "scrapling": {
      "module": "guilds/scrapling_web.py",
      "aliases": ["scrapling_web"],
      "status": "routable",
      "registration": "lazy"
    },
    "sandbox": {
      "module": "guilds/sandbox.py",
      "aliases": [],
      "status": "experimental",
      "registration": "lazy"
    },
    "vision_moondream": {
      "module": "guilds/vision_moondream.py",
      "aliases": [],
      "status": "excluded",
      "registration": "lazy"
    }
  },
  "metadata": {
    "generated_at": "2026-09-07T12:00:00Z",
    "source": "tylluan.toml",
    "total_routable": 43,
    "total_experimental": 1,
    "total_excluded": 1
  }
}
```

---

## Criterio de aprobación

El gate CI está listo cuando:
- `cargo make check-guild-consistency` pasa en el estado actual del repo (después de corregir las divergencias conocidas).
- Un cambio intencional (añadir un guild, cambiar su status) produce un fallo claro y accionable, no un error críptico.
- El snapshot generado puede consumirse por I-7 y el evaluator como fuente de verdad, eliminando las listas manuales.

## Siguiente paso

1. **Aprobación de José** del diseño.
2. **Definir el registro canónico** en `tylluan.toml` (o `guilds/catalog.toml`).
3. **Escribir los dos scripts** (`snapshot.py` + `check_consistency.py`).
4. **Corregir las 5 divergencias conocidas** en un commit separado.
5. **Integrar en CI** y cerrar G6.
