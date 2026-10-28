# Plan hacia `dev_mode = false` (decisión J-2 de José)

> Estado: propuesta del TL, 2026-10-28. **La decisión de apagar `dev_mode` y de cambiar `default_role` en el `tylluan.toml` de producción es de José.** Este plan fija qué debe estar cerrado antes y cómo se comprueba, con la evidencia de la ronda de seguridad de Buffy (`docs/internal/IMPL_seguridad_ronda2_buffy_2026-10.md`) y de la auditoría de Codex (`docs/reference/research/2026-10-28_verificacion_auditoria_integral_codex_tl.md`).

## Por qué ahora

Producción corre con `dev_mode = true` y `default_role = "admin"`. Con `dev_mode = true` no hay autenticación, así que hoy los dos hallazgos de abajo no se pueden explotar; con `dev_mode = false` y sin los arreglos, el primero deja entrar a cualquier token OAuth a todos los guilds.

## Orden y criterio de cierre

| Paso | Qué | Responsable | Criterio de cierre (comando y salida esperada) |
|---|---|---|---|
| 1 | Subida de ficheros de Vision con el adaptador autenticado | Antigravity, AG-13 | `pnpm test` verde con un caso 401 y uno 200; Buffy repite B-3 y la ruta de subida da 200 con token |
| 2 | ACL: `default_role = guest`, token maestro como clase operador, gate de guild siempre evaluado | Deep, D-68 | JWT OAuth sin mapeo contra un guild restringido → 403; maestro → 200; sin `[security.acl.roles]` → deniega |
| 3 | BWC: el `agent_id` del payload debe coincidir con la identidad ligada al bearer | Deep, D-69 (test de regresión de Buffy) | entregar y votar como otro agente → 403; el test `#[ignore]` de Buffy pasa sin `#[ignore]` |
| 4 | Endurecer `tylluan update` | Deep, D-70 | tar con symlink o directorio como entrada → rechazado; cota de descompresión probada |
| 5 | Estado vacío explicado en el panel: sin token se ve "falta token", no un panel en blanco | Antigravity | captura del dashboard con `dev_mode=false` y sin token que muestre el aviso |
| 6 | Prueba conjunta en un kernel de prueba con `dev_mode=false` | Buffy | tabla de rutas del dashboard: 0 rutas rechazan un token válido (hoy: 66 de 87 dan 401 sin token y 200 con token, 0 rechazadas) |
| 7 | Decisión de José: `dev_mode = false`, `default_role`, y quién es operador | José | `curl` sin token a `/api/v1/...` → 401; el dashboard pide el token una vez y funciona |

## Si algo falla al activar

`tylluan.toml.bak-pre-restart-20261028` y los cambios se revierten con una línea (`dev_mode = true`) y un reinicio. Antes de activar: copia de `silva.db` y de la configuración.

## No verificado

- Cuántos consumidores reales usan BWC multiagente (de eso depende la severidad práctica del paso 3).
- El comportamiento con `dev_mode = false` de los agentes de la flota (Hermes, DSH, Codex): necesitan su token; habría que repartirlos antes del paso 7.
