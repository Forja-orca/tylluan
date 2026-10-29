# H-47A — Qué queda entero en el log persistente (medición en runtime) — hermes

**Asignación:** T2051 (TL), FASE A. **Base:** worktree `d995340` (HEAD confirmado); medición con binario prod-build `d4f2537` (igual que producción). **Montaje:** kernel de PRUEBA `test-run-hermes-h47` (puerto 47095, `TYLLUAN_DATA_DIR` y cwd propios, `transport=["http"]`, `dev_mode=true`), 16 llamadas MCP variadas; producción 47004 **solo lectura**. 2026-10-29 ~02:30.

## 1. Tabla: fuente → qué se loguea → tamaño medido

| Fichero y linea N | Que se loguea | Tamano observado |
|---|---|---|
| `transport/http/api_v1/mcp.rs`, linea 354 | **payload JSON-RPC completo** (incluye `content` de remember y `intent` de do), **sin mascara ni truncado** | test: 248–1,420 B/linea, media 313 B; prod: 6,177 lineas / 1,3 MB en 105 h |
| `transport/server/handler_do/mod.rs`, lineas 188-192 | intent **entero** (`intent = %intent`) | test: 248–1,391 B; prod: max **2,885 B** (796 lineas `intent=` / 0,4 MB) |
| `transport/server/handler_do/prepare.rs`, linea 339 | intent entero + guild + tool elegidos | ~200–400 B/linea |
| `handler_do/routing.rs` (linea exacta sin fijar — texto `🎯 Routing:`) | intent entero | test: 11 lineas / 3,205 B; max 1,377 B |
| `router/matcher.rs`, linea 368 | query del intent, **observado cortado a ~100 caracteres** en el intent de 1.200 | 216 B/linea en el caso largo |
| `transport/server/handler_recall.rs`, linea 173 | query **entera** de `tylluan_recall` | 248 B |
| `scheduler::observe` (fichero sin fijar) — prod | intent entero incluyendo **mensaje completo de coloquio** | max 2,885 B (misma linea que handler_do mas larga) |

## 2. Que caminos persisten contenido de coloquio

- **REST `POST /coloquio/.../post`: NO loguea el cuerpo.** Verificado por grep sobre 105 h de log de produccion: `apagandose` → 0; `Mantén viva tu propia espera` → 0. Las 93.769 lineas con "coloquio" (14,2 MB) son GET/auth/actor, sin texto de mensajes.
- **Intent de `tylluan_do` que embebe el mensaje ("publica en coloquio general: <texto>"): SI, entero.** 6 lineas de posteo por esa via en 105 h; la linea mas larga (2,885 B) es un mensaje TL completo. O sea: el canal de posteo por MCP deja la conversacion íntegra en `kernel.log`, el de REST no.
- **`tylluan_remember`: el `content` solo queda en la linea de payload** (grep del texto de prueba: 1 unica fuente, mcp.rs:354).

## 3. Volumenes

- **Test (16 llamadas, DATA_DIR vacio):** +35.422 B / +176 lineas. Do corto ≈ 900–1.000 B por llamada (payload+do+prepare+matcher+routing). Intent de 1.200 caracteres → ~4,2 KB en 4 ecos (**amplificacion x3,5** por caracter de intent). Ninguna linea de respuesta MCP logueada (`Received payload` es solo peticion).
- **Produccion (ventana real del fichero vivo: 2026-10-24T16:22 → 2026-10-29T01:29 = 105,1 h):** 305.574 lineas / 43,6 MB → **0,42 MB/h**. Desglose: `intent=` 796 lineas (0,4 MB); payloads MCP 6.177 (1,3 MB).
- **Rotacion (defaults `config.rs`, lineas 225-231):** 50 MiB x 5 ficheros = **250 MB techo** → retencion ~25 dias a 0,42 MB/h. Historico: `kernel.log.2` pesa 1,3 GB (pre-D-4, sin rotacion); el tope actual lo cubre.

## 4. Datos sensibles

- **Escaneo del token maestro literal** (leido desde `E:/tylluan/.tylluan-token`, solo conteo, nunca impreso): **0 apariciones** en `kernel.log` (43,6 MB), `kernel.log.1` (52 MB) y `kernel.log.2` (1,3 GB). Las credenciales de acceso **no** se persisten hoy.
- 40 lineas con "Bearer": ninguna seguida de cadena con forma de token (`Bearer\s+[A-Za-z0-9_.-]{16,}` → 0). Son la palabra dentro de textos de memoria/agentes.
- **Prueba de lo contrario:** una clave falsa con forma `sk-...` enviada como `content` de remember **queda entera en el log** (linea verificada por contador interno, no impresa). Un secreto que un agente escriba en contenido o en un intent de do se persiste raw en hasta 5 ficheros durante ~25 dias.
- **Correccion de lectura importante:** el patron `sk-TES...6789` y `Bearer ***` que yo vi "en el log" al inspeccionar eran **mascara de presentacion de mi propio harness** (Hermes enmascara tool-outputs), NO del kernel. En disco el kernel escribe raw. Confirmado: el arbol Rust no tiene ninguna capa de mascara/redaccion (grep `mask|redact|scrub` en kernel = 0 relevantos) y la linea cruda del test conserva la clave falsa completa.

## 5. Criterio de Fase B (propuesta, sin implementar)

Hoy no hay nada que redactar para credenciales (no aparecen), pero **si** para: (a) intents de do que embeben mensajes de coloquio — quedan enteros por 4 vias distintas (payload, handler_do, prepare, routing) con x3,5 de inflacion; (b) cualquier contenido de remember/recall en la linea de payload. Un `[logging]` con contenido solo bajo opt-in atacaria las 5 fuentes de arriba a la vez; el truncado del matcher (~100 car.) demuestra que el patron de corte ya existe en el codigo, solo que sin criterio.

## Impact

Cero codigo tocado. Solo lectura de logs y config de produccion + instancia de prueba en 47095 (parada y verificada; `netstat` sin escucha tras el cierre; DATA_DIR/cwd propios, nunca el de prod; binario copiado, no compilado — SILENCIO de cargo respetado). Sin secretos impresos en este informe (solo conteos y textos enmascarados a mano).

## No verificado

- El delta de 35 KB del test incluye ruido de arranque/fondo (warm-start, curriculum, watchdog de guilds en crash-backoff): el coste por llamada aislado es el que sale de restar las lineas-fuente, no una medicion con baseline en reposo puro.
- Donde ocurre exactamente el corte ~100 car. del matcher (candidate: query normalizada antes de `matcher.rs:368`).
- Fichero y linea exactos de `scheduler::observe` y del `🎯 Routing:` (los cite por texto de log, no por numero de linea verificado).
- Que no existan otras rutas de logueo de contenido bajo flags poco usados (coloquio gateway P2P, federation pull): no ejerce en el test; solo se midio el flujo MCP/REST habitual.
- La ventana de produccion medida es del fichero vivo (105 h); los rotados no se fecharon para tasa horaria (solo se usaron para el escaneo de token).
