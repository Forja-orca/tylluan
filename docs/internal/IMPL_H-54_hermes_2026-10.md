# IMPL H-54 — Capa de auditoría local (logging) opt-in

**Asignación:** T2112 (TL claude-code). **Base:** `fc074b1` (origin/main post-H-51). **Cero compromiso:** diff sin commitear, en `agent/hermes-base`. Gate de docs: referencias de línea sueltas en forma `línea NNNN`.

## 1. Problema y diseño (T2112 → código)

T2112 pedía:
1. `[logging]` mantiene producción redactada (ya hecho en H-47C: `log_content=false` → solo `canal=len+sha`).
2. Nueva capa auditoría local `data/audit/` (gitignored) con **contenido completo**, apagada por defecto (`audit_layer=false`), rotación por tamaño + limpieza por edad (`audit_retention_days`), permisos, endpoint admin de lectura, **test que demuestre que la capa de producción nunca contiene texto**.

**Arquitectura (código):**
- La producción **no cambia**: `redacted()`/`safe_with()`/`mcp_payload_with()` devuelven exactamente lo que hoy (H-47C). El scrub ocurre ANTES del recorte; las 6 fuentes internas **nunca** emiten contenido.
- La capa de auditoría se engancha **en los helpers** (donde existe el texto crudo): `redacted()`, `safe_with()`, `mcp_payload_with()` llaman a `audit_write(canal, texto_crudo)` si `audit_layer=true`.
- El writer de auditoría es un `RotatingFileWriter` paralelo al vivo (`audit.log` en `audit_dir`), mismo esquema de rotación por tamaño (`audit_max_mb`/`audit_max_files`), más limpieza por edad (`audit_retention_days` al abrir y al rotar).
- `data/` ya está en `.gitignore` (línea 12): el fichero de auditoría **nunca** llega al repo público.
- Permisos: en Unix `dir 0700` + `file 0600`; en Windows hereda ACL del perfil de usuario (documentado como límite).
- Admin endpoints: `GET /api/v1/admin/audit` (lista) + `GET /api/v1/admin/audit/file?name=...` (contenido) — automáticamente protegidos por el middleware admin (`auth.rs:261`).
- Test obligatorio: `audit_layer_writes_raw_audit_prod_stays_redacted` → la auditoría tiene secreto crudo, la producción solo `len+sha`.

## 2. Mapa del diff (8 ficheros, +417/−18)

| Fichero | Líneas | Qué hace |
|---|---|---|
| `crates/tylluan-kernel/src/config.rs` | 247-271, 273-312, 342-360 | 5 campos nuevos en `LoggingConfig` + serde defaults + `Default impl` + `audit_max_bytes()` |
| `crates/tylluan-kernel/src/logging.rs` | +276 líneas: bloque de auditoría (writer, sink, hooks, edad/nombre/listing/lectura) y 5 tests | `audit_writer`/`prune_audit_by_age`/`audit_sink`/`audit_write`/`prune_audit_by_age`/`is_expired`/`valid_audit_file_name`/`audit_listing`/`read_audit_file`; hooks en `redacted`/`safe_with`/`mcp_payload_with` |
| `crates/tylluan-kernel/src/transport/http/api_v1/api_audit.rs` | +71: `admin_audit_list` (línea 309) y `admin_audit_file` (línea 334), declarados antes de `mod tests` |
| `crates/tylluan-kernel/src/transport/http/api_v1/routes.rs` | 186-187 | Registro `GET /api/v1/admin/audit` + `GET /api/v1/admin/audit/file` |
| `crates/tylluan-kernel/src/transport/http/api_v1/api_audit.rs` | 472-526 | Tests H-54 dentro de `redaction_tests` (5 tests) + tests RotatingFileWriter heredados |
| `AGENTS.md` | 78 | Bump tests lib 1424→1430, total 1511→1517 |
| `README.md` | 176 | Bump tests total 1511→1517 |
| `docs/internal/IMPL_H-54_hermes_2026-10.md` | — | Este documento (nuevo, untracked) |

## 3. Tests entregados (`crates/tylluan-kernel/src/logging.rs`, módulo `redaction_tests`)

1. `audit_defaults_off_no_dir_created` (línea 528): `audit_layer=false` por defecto; writer devuelve `None`; no crea directorio.
2. `is_expired_pure_logic` (línea 543): 31d→expirado con retención 30; 29d→no; `days=0`→nunca; mtime futuro (clock-skew)→false.
3. `valid_audit_file_name_validation` — el nombre real del test es `valid_audit_name_validation` (línea 559): `audit.log`/`audit.log.N` válidos; traversal/otros rechazados.
4. `audit_layer_writes_raw_audit_prod_stays_redacted` (línea 571): con `audit_layer=true` → `audit.log` contiene secreto crudo; `redacted`/`safe_with`/`mcp_payload` default devuelven `len+sha` sin contenido → **test obligatorio cumplido**.
5. `audit_listing_and_read_end_to_end` (línea 614): listing ordena por mtime desc; read devuelve contenido; nombre inválido→error; inexistente→None.

## 4. Verificación medida (cargo en `CARGO_TARGET_DIR=D:/cargo-target/hermes`, branch propio)

Cifras medidas sobre la base final `a85526b` (origin/main del 2026-10-30, tras el ff-merge), con `CARGO_TARGET_DIR=D:/cargo-target/hermes`:

- `cargo test -p tylluan-kernel --lib`: **1443 passed; 0 failed; 4 ignored** en 54,96 s.
- `cargo test -p tylluan-kernel --test fingerprint_counters_endpoint_test` (H-52): **1 passed; 0 failed** en 2,72 s.
- `cargo clippy -p tylluan-kernel --all-targets -- -D warnings`: **CLIPPY_EXIT=0** (47,76 s en el run de `fc074b1`; 3 m 19 s en el de `a85526b`).

Tests nuevos aportados por este diff: **9 en la suite lib** = 5 de H-54 en `logging.rs` + 4 de H-53 en `coherence_gate.rs`, más 1 test de integración H-52 en binario aparte. Corrección honesta: en una medición intermedia (base `fc074b1`, antes de los 4 tests de H-53) la suite dio 1436; la cifra autoritativa es la de arriba, 1443.

Dos arreglos que fueron necesarios y van aquí, no en un anexo:
- `safe_with` auditaba dos veces por petición (llamaba a `audit_write` y luego a `redacted`, que también lo llama). Corregido en `logging.rs:311-321`: un `audit_write` por ruta, con el texto crudo.
- Clippy (edition 2024) exigió let-chain en `prune_audit_by_age` (`logging.rs:167`) y rechazó ítems declarados después de `mod tests`: los handlers de H-54 quedaron **antes** del módulo de tests en `api_audit.rs` (líneas 309 y 334).
- Contaminación CRLF: **normalizada contra blob del base** antes de medir.

## Impact

- **Critical path:** `logging.rs` (helpers + sink), `config.rs` (schema), `api_audit.rs` (endpoints admin). No toca `transport/` (rutas, no lógica de negocio), `guilds/`, `integrations/`, `tylluan*.toml`.
- **Prod sin cambios:** capa de producción (tracing) sigue exactamente igual (H-47C). La capa de auditoría es paralela y opcional.
- **Seguridad:** por defecto APAGADA (`audit_layer=false`); sin opt-in no se escribe nada fiel a disco. Gitignored + permisos restrictivos. Admin endpoints protegidos por middleware admin existente (`auth.rs:261` → `path.starts_with("/api/v1/admin/")`).
- **Limpieza dual:** rotación por tamaño (techo duro disco) + retención por edad (limpieza silenciosa al abrir/rotar).

## No verificado

- [no verificado] Permisos 0600 en Windows: en Unix se aplica vía `std::os::unix::fs::PermissionsExt`; en Windows se documenta que hereda ACL del perfil (no hay `chmod` equivalente portable sin crate extra).
- [no verificado] `audit_retention_days` limpieza por edad al rotar: probada en test puro `is_expired`; la invocación real en `prune_audit_by_age` al abrir/rotar no se ha ejercido en vivo con ficheros de días distintos (test de integración pendiente).
- [no verificado] Rotación de auditoría con `audit_max_mb=0` (solo por edad): código permite 0→sin rotación por tamaño; no testado combinado.
- [no verificado] Endpoint `/api/v1/admin/audit/file` con fichero > memoria disponible: stream no implementado (lee entero a RAM); para ficheros de auditoría típicos (< 20 MB) es aceptable, pero no verificado a escala.
- [no verificado] No se midió overhead de CPU/IO del hook `audit_write` en los 7 call sites bajo carga real (solo test unitario). Se espera despreciable (Mutex + formateo JSON) pero no medido.