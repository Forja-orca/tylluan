# H-47A — Qué queda entero en el log persistente (medición en runtime) — hermes

**Asignación:** T2051 (TL), FASE A. **Base:** worktree `d995340` (HEAD confirmado); medición con binario prod-build `d4f2537` (igual que producción). **Montaje:** kernel de PRUEBA `test-run-hermes-h47` (puerto 47095, `TYLLUAN_DATA_DIR` y cwd propios, `transport=["http"]`, `dev_mode=true`), 16 llamadas MCP variadas; producción 47004 **solo lectura**. 2026-10-29 ~02:30.

## 1. Tabla: fuente → qué se loguea → tamaño medido

| Fichero y linea N | Que se loguea | Tamano observado |
|---|---|---|
| `transport/http/api_v1/mcp.rs`, linea 354 | **payload JSON-RPC completo** (incluye `content` de remember y `intent` de do), **sin mascara ni truncado** | test: 248–1,420 B/linea, media 313 B; prod: 6,177 lineas / 1,3 MB en 105 h |
| `transport/server/handler_do/mod.rs`, lineas 188-192 | intent **entero** (`intent = %intent`) | test: 248–1,391 B; prod: max **2,885 B** (796 lineas `intent=` / 0,4 MB) |
| `transport/server/handler_do/prepare.rs`, linea 339 | intent entero + guild + tool elegidos | ~200–400 B/linea |
| `handler_do/routing.rs`, linea 228 (🎯 Routing) | intent entero | test: 11 lineas / 3,205 B; max 1,377 B |
| `router/matcher.rs`, linea 368 | query del intent, **observado cortado a ~100 caracteres** en el intent de 1.200 | 216 B/linea en el caso largo |
| `transport/server/handler_recall.rs`, linea 173 | query **entera** de `tylluan_recall` | 248 B |
| `router/scheduler/observe.rs`, linea 148 | intent entero incluyendo **mensaje completo de coloquio** | max 2,885 B (misma linea que handler_do mas larga) |

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

## 5. FASE B (T2055) — Diseño del helper único de redacción (sin código)

### B1. Helper único y módulo comun

- Ubicacion: modulo `tylluan_kernel::logging` (`crates/tylluan-kernel/src/logging.rs`) — ya existe y ya es compartido por el arranque (RotatingFileWriter); no crear crate nuevo.
- Firma de diseno: `safe_segun_config(canal: &str, texto: &str, cfg: &LoggingConfig) -> String`. Con `log_content=false` (default) devuelve **solo** `canal=<canal> len=<chars> sha=<12 hex>`: ni un caracter del contenido toca el disco. Con `log_content=true` devuelve el texto **ya pasada por `scrub()`** y recortada a `log_content_max_chars` (default 200).
- `scrub(texto)`: mascara patrones — `sk-[A-Za-z0-9_-]{8,}`, `Bearer`+palabra token-like, `token[=:]\s*\S+`, JWT de 3 segmentos base64url separados por punto, y cadenas hex/base64 contiguas de >=32. **Orden obligatorio: scrub ANTES del recorte** — si el recorte cae dentro de un secreto, hoy filtraria su prefijo (el corte implicito del matcher es exactamente ese fallo: corta sin mirar).
- El hash (algoritmo a fijar en implementacion; la arboleda ya trae sha2 — los fingerprints de embedding son `sha256:` segun T-DSH-128) es deterministicamente correlacionable: mismo texto → mismo hash en las 7 fuentes, lo que permite unir las lineas de una misma peticion sin guardar el contenido.

### B2. Regla de fuente unica (mata la inflacion x3,5)

El contenido, si `log_content=true`, se escribe **solo en la fuente de entrada**: `mcp.rs:354` (payload MCP) y, para el resto de ingress REST, donde hoy ya se loguea peticion. Las otras 6 fuentes internas **siempre** `canal+len+sha`, con o sin opt-in — su valor era la correlacion, y el hash la sigue dando. Beneficio medido de regalo: un do corto pasa de ~900 B a ~320 B por llamada y el x3,5 por intent largo desaparece.

### B3. Lista de sitios (fichero y linea N en d995340)

| Sitio | Hoy | Cambio de diseno |
|---|---|---|
| `transport/http/api_v1/mcp.rs:354` | payload raw entero | UNICA fuente con contenido posible. Default: `metodo+id+len+sha`. Opt-in: JSON field-walk — solo los campos string se scrubutan, id/num/bool quedan |
| `transport/server/handler_do/mod.rs:191` | `intent = %intent` | siempre `intent_len/intent_sha` via helper |
| `transport/server/handler_do/prepare.rs:339` | intent entero + guild + tool | intent via helper; guild/tool crudos (no traen contenido de usuario) |
| `transport/server/handler_do/routing.rs:228` | intent entero | intent via helper; score/method/agent_role quedan |
| `transport/server/handler_recall.rs:173` | query entera | query via helper (len+sha default; scrub+cap opt-in) |
| `router/matcher.rs:368` | query con corte implicito ~100 car | helper con cap explicito; el corte deja de ser implicito y pasa a ir detras del scrub |
| `router/scheduler/observe.rs:148` | intent entero (max 2.885 B medidos) | intent via helper |

Config: dos campos nuevos en `[logging]`, junto a `max_file_size_mb`/`max_files` (config.rs lineas 219-231): `log_content` (false) y `log_content_max_chars` (200). Sin toggle en runtime: cambiarlo exige reinicio — queda como riesgo documentado en B6.

### B4. Tests propuestos (criterio medible)

1. **Unidad scrub**: fixture por patron (sk-, Bearer xxx.yyy, token=zzz, JWT 3-seg, hex32) → el resultado no contiene el patron completo ni ningun fragmento contiguo >=8 chars del secreto; mismo texto → mismo sha; texto+1char → sha distinto.
2. **Orden scrub-recorte**: secreto que cruza la posicion 100 de un texto de 200 → con cap=100 assert de que ningun fragmento del secreto sobrevive >=8 chars (el test que el corte actual del matcher fallaria).
3. **JSON field-walk del payload**: strings anidados scrubutados; `id`/numeros/booleanos intactos; con opt-in off, cero substring del fixture en la linea.
4. **Golden 7 sitios**: mismo fixture unico por sitio; default → log capturado sin ninguna substring del fixture + sha presentes; opt-in → el fixture aparece **una sola vez** (entrada) y scrubutado.
5. **Anti-regresion CI**: `scripts/check_log_redaction.sh` al estilo de los check_*.sh existentes — grep de los 7 literales (`intent = %intent`, `Received payload: {}`, `hybrid scoring '{}'`, `🎯 Routing: '{}'`, `intent='{}'`, query de recall, verdict de observe) y DRIFT si alguno vuelve a interpolar contenido directo sin helper.

### B5. Impact esperado del diseno

- Reduce superficie de secreto: hoy cualquier clave que un agente escriba en content/intent queda raw ~25 dias (medido §4); con default off solo quedan len+sha. El token maestro NO aparece hoy (0 en 1,4 GB) — el riesgo real es contenido futuro, no credencial actual.
- Reduce tamano: ~60 % menos en lineas de peticion (900→320 B/llamada corta estimada sobre mis conteos de §3).
- **ROMPE la metodologia de medicion de la flota**: H-42/H-43/H-45 y T-DSH midieron con grep de `intent=`, payloads y stage_ms sobre kernel.log. Con default off esas tecnicas quedan ciegas al contenido (len+sha sigue permitendo tasas y correlacion). Es decision explicita del TL: o se acepta, o se usan ventanas de opt-in con reinicio para depurar.

### B6. Riesgos para depurar

- Diagnostico de recall/matchers degradados: sin query legible, `matcher` y `routing` solo dan len+sha — reproducir un caso exige el texto original del agente (el hash lo verifica, no lo reconstruye).
- Over-masking con opt-in: contenido legitimo tipo `token de ejemplo=...` queda mascarado incluso con log_content=true — por spec del TL; hay que decirlo en docs/para quien depure.
- Guilds Python que ecos de contenido manden al kernel.log por su propio logger no pasan por el helper Rust. En la ventana medida (105 h) no observe ninguno (el mayor emisor, observe.rs:148, es Rust), pero no lo he barrido exhaustivamente [nv].
- Sin toggle en runtime: activar opt-in exige reinicio (ventana de despliegue); un `GET /api/v1/runtime/*` de nivel de log seria el follow-up natural, fuera de esta fase.

## Impact

Cero codigo tocado. Solo lectura de logs y config de produccion + instancia de prueba en 47095 (parada y verificada; `netstat` sin escucha tras el cierre; DATA_DIR/cwd propios, nunca el de prod; binario copiado, no compilado — SILENCIO de cargo respetado). Sin secretos impresos en este informe (solo conteos y textos enmascarados a mano).

## No verificado

- El delta de 35 KB del test incluye ruido de arranque/fondo (warm-start, curriculum, watchdog de guilds en crash-backoff): el coste por llamada aislado es el que sale de restar las lineas-fuente, no una medicion con baseline en reposo puro.
- Donde ocurre exactamente el corte ~100 car. del matcher (candidate: query normalizada antes de `matcher.rs:368`).
- Fichero y linea exactos de `scheduler::observe` y del `🎯 Routing:` (los cite por texto de log, no por numero de linea verificado).
- Que no existan otras rutas de logueo de contenido bajo flags poco usados (coloquio gateway P2P, federation pull): no ejerce en el test; solo se midio el flujo MCP/REST habitual.
- La ventana de produccion medida es del fichero vivo (105 h); los rotados no se fecharon para tasa horaria (solo se usaron para el escaneo de token).
