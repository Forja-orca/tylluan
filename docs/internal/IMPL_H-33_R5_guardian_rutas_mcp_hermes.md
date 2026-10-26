# IMPL H-33 — R5 del guardian: toda ruta MCP/SSE pasa por validate_mcp_origin_host (hermes)

Spec (T1766, TL): «H-33 (R5 del guardian) ya se puede hacer porque T-DSH-86 está aceptado: R5 = toda ruta MCP/SSE del router pasa por validate_mcp_origin_host». Diff sin commitear sobre `5d21cc7`, sección 10 nueva en `scripts/check_readme_claims.sh` (+56 líneas; secciones 1–9 intactas).

## Diseño

Check estático de solo lectura, sin cargo, espejo bash del inventario de rutas que M-BUFFY-10 puso en Rust (`mcp_origin_guard_tests`). Tres comprobaciones:

1. **Rutas → guard:** toda `.route("<path>", …mcp_handler|sse_handler…)` registrada en `crates/tylluan-kernel/src/transport/http/` debe estar en `MCP_ORIGIN_GUARD_PATHS` (auth.rs:672). Ruta nueva sin extender el guard = agujero DNS-rebinding silencioso (el hallazgo exacto de T-DSH-85), CI verde.
2. **Guard → rutas:** entrada del guard sin ruta registrada = ancla obsoleta (renombrado oculto). Comparación bidireccional.
3. **Cableado:** `validate_mcp_origin_host_middleware` debe seguir unido a `middleware::from_fn*` en mod.rs.

Extracción vacía es DRIFT, nunca OK (lección del fixture de deriva de H-31: un candidato vacío no puede conceder OK). Inyectables para fixtures: `CHECK_README_R5_ROUTE_FILES`, `CHECK_README_R5_GUARD_FILE`, `CHECK_README_R5_MOD_FILE`.

## Evidencia (ambos casos, exigidos por la escuela H-31)

- **Limpio** (ficheros reales, `5d21cc7`): `OK r5: 5 registered MCP/SSE routes all covered by MCP_ORIGIN_GUARD_PATHS ( /api/v1/mcp /mcp /messages /sse /sse/download ); middleware wired` — script completo exit 0.
- **Deriva** (fixtures en scratch): tres mutaciones simultáneas — ruta nueva `.route("/mcp/admin", any(api_v1::mcp_handler))` sin guardar, `"/sse/download"` fuera del const, y el `.layer(from_fn_with_state(...))` descableado → **3 DRIFT r5, exit 1**, una línea por defecto.
- **Fantasma:** `"/mcp/ghost"` añadido al const sin ruta → `DRIFT r5: guarded path /mcp/ghost has no registered MCP/SSE route`, exit 1.

## Impact

Guardián solo — cero cambio de runtime. Si el TL lo cablea en `verify.sh` (decisión suya, como con R4/H-25), una ruta MCP/SSE nueva sin guard rompe CI antes de mergear. El fixture de deriva demuestra que no es decorativo: cazó las tres mutaciones.

## No verificado

- El comportamiento runtime del guard (403/415) no lo prueba R5 — eso lo cubren `mcp_origin_guard_tests` y la suite de regresión fuera del crate; Hermes verificó solo el 403 en vivo contra `/messages`.
- T-DSH-87 (e6e19c5) amplió el guard a `/api/v1/*` mutante con test de método; R5 sigue anclado al const MCP/SSE como pide la spec. Si el TL quiere vigilar también esa extensión, propongo una R6 opcional (comprobar que el bloque `let guarded` sigue presente en auth.rs:761) — fuera del alcance de H-33, no la implementé.
- R5 no está cableado en `verify.sh` ni en CI.
