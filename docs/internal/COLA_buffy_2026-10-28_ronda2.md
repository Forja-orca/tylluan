# COLA Buffy, ronda 2 (2026-10-28) — la lanza José con un mensaje corto

Quién: Buffy (seguridad). Dónde: tu worktree `E:\tylluan-wt-buffy`. Antes de nada: `git fetch origin`, rebasa a `origin/main`, y lee por este orden tu fichero de entrada, `STATUS.md` y `REVIEW.md` (el tercero; su regla sobre hallazgos de autenticación exige petición real).
Reglas: solo informe, más un test si el hallazgo lo pide, como diff sin commitear; `docs/internal/IMPL_<tarea>_buffy_2026-10.md` con `## Impact` y `## No verificado`. Solo el TL commitea y empuja. Nunca toques el kernel de producción ni `E:\tylluan`.
Kernel de prueba (obligatorio): copia `E:\cargo-target\prod-build\release\tylluan-nexus.exe`, puerto propio (47093), `DATA_DIR` propio vacío, `dev_mode = false`, nunca el cwd de producción. Dos identidades: crea dos tokens de agente distintos en tu `tylluan.toml` de prueba.

## Orden

### B-1 — LEAD-01 de la auditoría: BWC no liga el `agent_id` del payload al bearer
- Qué: `crates/tylluan-kernel/src/transport/http/api_v1/api_contracts.rs` toma `agent_id` del JSON; falta probar si el servidor lo compara con la identidad del token (`current_bound_agent_id()` en `auth.rs`).
- Hacer: con dos tokens (A y B) y un contrato desechable, intenta entregar y votar como B usando el token de A. Tabla `petición -> estado HTTP -> efecto en el contrato`.
- Aceptación: evidencia literal de request y response; veredicto "confirmado" o "no reproducible".

### B-2 — LEAD-02: token OAuth sin mapeo hereda `default_role = admin` en `/api/v1/guilds/{guild}/call/{tool}`
- Hacer: en el kernel de prueba pon `default_role = "admin"` (como producción) y luego `"guest"`; llama a un guild restringido con un bearer OAuth válido no mapeado en `security.acl.tokens`. Tabla `default_role -> estado -> efecto`.
- Aceptación: captura del estado HTTP y de si el handler ejecutó; propuesta de valor mínimo seguro para `default_role`.

### B-3 — Lista de preparación para `dev_mode = false`
- Contexto: José cierra `dev_mode` y quita `default_role = admin` más adelante (decisión J-2). Antes hay que saber qué se rompe.
- Hacer: con `dev_mode = false` y un token válido, recorre las rutas que usa el dashboard (`dashboard/src` y `packages/tylluan-ui-core/src`) y marca cuáles dan 401 o 403 sin motivo. Conocido ya: `VisionTab.tsx`, línea 119, sube ficheros sin bearer (tarea AG-13 de Antigravity). Busca el resto.
- Aceptación: tabla `ruta -> usa el adaptador autenticado (sí/no) -> estado con token -> estado sin token`.

### B-4 (si queda tiempo) — Ataque a `tylluan update` (D-64, ya en `main`)
- Hacer: pruebas adversariales sobre `crates/tylluan-cli/src/main.rs`: tar con ruta `..`, enlace simbólico, checksum alterado, versión menor, binario cambiado de nombre. Tabla `caso -> resultado esperado -> obtenido`.

## Si te atascas
Escribe en el informe qué paso falla, el comando exacto y su salida, y para.

## Cómo informar
Mensaje `[COLA buffy]` en Coloquio con comandos, cifras, ficheros tocados y qué no verificaste.
