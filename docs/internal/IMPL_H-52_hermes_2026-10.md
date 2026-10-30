# IMPL H-52 — Contadores del filtro de huella de embeddings visibles en REST

**Asignación:** T2105 (TL claude-code) como C-2 → H-52. **Base:** `fc074b1` (origin/main post-H-51). **Diff sin commitear** en `agent/hermes-base`. Spec leída verbatim del turno (REST `/api/v1/coloquio/channels/general/turn/2105`) y de `docs/internal/COLA_codex_2026-10-28.md`, sección C-2.

## Qué pedía la spec (C-2, AUD-08)

> `SilvaDB::fingerprint_observation()` cuenta cuántos candidatos excluiría el filtro (`embedding_fingerprint_filter`, apagado por defecto), pero ninguna ruta lo expone: hoy no se puede medir antes de activarlo. Añadir los tres contadores (por modelo, por dimensión, excluidos) a un endpoint REST de solo lectura que ya exista, **sin ruta nueva**. Test del handler. Decidir y justificar si va en MCP. Aceptación: el endpoint devuelve los contadores y el resto del JSON queda intacto. Fuera de alcance: activar el filtro.

## Qué se hizo

**Endpoint elegido: `GET /api/v1/system/status`** (`crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs:793`). Razón de la elección: es la "ruta de señales del sistema" que la propia spec cita como ejemplo, ya es de solo lectura y ya mezcla salud + métricas del proceso (`silva_healthy`, `uptime_secs`, bloque `system`). Alternativas descartadas: `/api/v1/silva/stats` (mezcla recuentos de grafos, sin contexto de salud) y `/api/v1/health/golden-signals` (SLOs, semántica distinta).

- Helper `fingerprint_status_json(db)` en `api_admin.rs:778`: lee `SilvaDB::fingerprint_observation()` (`memory/silva/mod.rs:414`) + `fingerprint_filter_enabled()` (`memory/silva/mod.rs:406`) y fija las cuatro claves del JSON.
- Bloque nuevo en el JSON del handler (`api_admin.rs:800`):
  ```json
  "embedding_fingerprint": {
    "filter_enabled": false,
    "would_exclude_model": 0,
    "would_exclude_dim": 0,
    "excluded": 0
  }
  ```
  Semántica: con `filter_enabled=false`, los dos `would_exclude_*` son **observación** de lo que el filtro excluiría (exactamente lo que AUD-08 pedía poder medir antes de activarlo). Los contadores son globales desde el arranque (átomos `Relaxed`), no por petición — igual que la fuente.
- **Ruta nueva: ninguna** (la spec lo prohíbe explícitamente).
- **JSON preexistente intacto**: solo se añade una clave de primer nivel; las seis claves anteriores y el bloque `system` no se tocan (el test lo comprueba).

## MCP: decisión y justificación (pedida por la spec)

**No va en MCP.** La norma de José dice "toda capacidad en REST y MCP", pero `CONTRACT-01` (`AGENTS.md`, invariantes de arquitectura) fija exactamente 6 herramientas soberanas (`tylluan_do/remember/recall/think/graph/decide`) y prohíbe añadir tools sin enmienda formal aprobada por José. Estos contadores son **telemetría de operador** (saber si conviene activar un filtro), no una capacidad que un agente deba invocar para actuar: el agente no filtra embeddings ni decide el scope, y exponerlo como tool sería dar visibilidad de knobs de configuración sin ningún poder asociado. Precedente: ningún endpoint de stats del sistema (`/api/v1/system/status`, `/health`, `/metrics`) tiene contrapartida MCP hoy. Si el TL quiere exponerlos igualmente, la vía correcta es un campo dentro de la respuesta de `tylluan_recall` (la tool que consume el filtro), no una tool nueva — queda como propuesta, no lo implemento porque cambiar el payload de una soberana también es decisión de contrato.

## Tests entregados

`crates/tylluan-kernel/tests/fingerprint_counters_endpoint_test.rs` (nuevo, binario de test de integración, harness compartido `tests/support/mod.rs` con `test_state` + `build_test_app`, mismo patrón que `audit_latency_endpoint_test.rs`):
1. `GET /api/v1/system/status` devuelve 200 y **conserva** las claves previas (`silva_healthy`, `mailbox_healthy`, `curriculum_entries`, `uptime_secs`, `score`, `system`) → exigencia de aceptación "el resto del JSON queda intacto".
2. El bloque `embedding_fingerprint` existe con `filter_enabled=false` y los tres contadores en 0 en un `SilvaDB` recién abierto.
3. Los valores del JSON **coinciden con la fuente real** en el mismo instante (`state.silva.fingerprint_observation()` / `fingerprint_filter_enabled()`), no con literales duros.

Cobertura de la parte que INCREMENTA los contadores: ya existe en `memory/silva/search.rs` (tests `fingerprint_observation` en líneas 1350-1863: inerte→(0,0,0), (model,dim,excluded), deltas before/after). No la duplico.

## Mapa del diff H-52

| Fichero | Cambio |
|---|---|
| `crates/tylluan-kernel/src/transport/http/api_v1/api_admin.rs` | helper `fingerprint_status_json` (línea 778) + clave `embedding_fingerprint` en el JSON del handler (línea 800) |
| `crates/tylluan-kernel/tests/fingerprint_counters_endpoint_test.rs` | test de integración nuevo (1 test, router real) |

## Verificación medida

(ver números en el mensaje de entrega; `cargo test -p tylluan-kernel --test fingerprint_counters_endpoint_test` y `cargo clippy -p tylluan-kernel --lib --tests -- -D warnings` con `CARGO_TARGET_DIR=D:/cargo-target/hermes`)

## Impact

- **Critical path:** sí, pero read-only. `api_admin.rs` (transport/) → exige sección `## Impact` en el commit (ADR-013) y toca la zona vigilada por la puerta anti-depredación; no toco `guilds/`, `integrations/` ni `tylluan*.toml`.
- **Comportamiento:** ningún default cambia. El filtro sigue apagado por defecto y no se puede activar con este cambio (fuera de alcance por spec).
- **Rendimiento:** una lectura de 3 átomos `Relaxed` + 1 bool por llamada a `/api/v1/system/status` (endpoint de bajo tráfico, no está en el hot path de recall).
- **Coste:** cero gasto externo (todo local).

## No verificado

- [no verificado] El test de integración corre contadores en 0 (SilvaDB recién abierto, sin consultas). No se ejercita un estado con contadores ≠ 0 **a través del HTTP**, porque para subirlos hace falta un `search_hybrid` con fingerprint activo; esa parte está probada en lib tests de `search.rs` pero no end-to-end por REST.
- [no verificado] Contrato con consumidores reales del dashboard: ningún panel consume `/api/v1/system/status` hoy (grep en `dashboard/src` no lo usa), así que no hay riesgo de roto, pero tampoco hay validación visual del nuevo bloque.
- [no verificado] Comportamiento tras `config::reload()` (el filtro puede encenderse en runtime por otra vía): no medido; los contadores siguen siendo acumulativos y el flag se lee en vivo desde el átomo, así que debería reflejarlo, pero no lo probé.
