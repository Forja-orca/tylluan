# ADR-030: Escudo MVP-01 F0 — detección pasiva con canary tokens

**Status:** Aprobado por José el 2026-11-13 (`DEC-20261113-03`). **v2 (2026-11-13, noche):** enmienda la Decisión 1 con
la evidencia medida, cierra las tres preguntas que quedaban abiertas y responde a la revisión adversarial de Hermes
(informe de las 22:26). Sigue opt-in, desactivado por defecto, hasta el dictamen jurídico.
**Autor:** Deep. **Decide:** José (`DEC-20261112-09`, «sí»); el TL cerró las tres preguntas el 2026-11-13.
**Clase de riesgo:** A. **Ficha:** `ESCUDO-MVP-01` (`D:\tylluan-loops\briefs\ESCUDO-MVP-01_deteccion_pasiva.md`). Nada de
esta decisión mueve un LLM dentro del bucle de defensa: el componente es determinista y los LLM solo leen su telemetría.

## Contexto

Primer componente del escudo: (a) colocar **canary tokens** (señuelos de fichero, credencial o URL local que nadie
legítimo toca), (b) registrar **solo metadatos** cuando alguien los toca, (c) avisar al operario y exponer telemetría
estructurada. Criterios observables en la ficha §4: detección ≤2 s, 0 falsos positivos en 24 h, retención ≤7 días,
alerta ≤5 s, falla cerrado, ≤50 MB y <1 % CPU en reposo, opt-in, `sec_tests` verde.

Restricciones del repo que condicionan la decisión: binario único con perfil `portable`, soporte ARM64, sin
dependencias nuevas si se pueden evitar, y **nada en `C:`** (regla 15 del protocolo nocturno).

## Decisión 1 — vive en el kernel: `crates/tylluan-kernel/src/security/canary.rs`

| Opción | Por qué |
|---|---|
| **Módulo del kernel (elegida)** | El evento tiene que llegar al operario (dashboard, SSE) y a los agentes (API) en segundos; ambas superficies ya viven en el kernel con su token/ACL. Motor de vigilancia por plataforma (§1.1). |
| Guild Python (descartada) | Añade un proceso por vigilante (más el lanzador del venv, ver `BUF-D9`), exige `always_on` para sobrevivir, y su telemetría sería una tool de guild: los agentes tendrían que pasar por `tylluan_do`. Su ventaja teórica (aislamiento) ya la cubre el criterio 5, porque el vigilante es una tarea independiente y su muerte no tumba el HTTP. |

### 1.1 Motor de vigilancia por plataforma (enmienda v2)

El borrador v1 elegía **polling de `std::fs::metadata` cada 1 s** y lo justificaba con «sin dependencias nuevas (no hace
falta `notify`)». Esa premisa es falsa, y la medición la tumba:

- **`notify` ya es dependencia directa del kernel y está sin usar.** `crates/tylluan-kernel/Cargo.toml:67` →
  `notify = "7"`; `Cargo.lock` lo resuelve como `notify 7.0.0`; y no hay una sola llamada a su API en el código:
  `git grep -n -E "RecursiveMode|recommended_watcher|PollWatcher|EventKind" -- crates/` → **rc=1 (0 usos)**. El coste
  del watcher ya está pagado en el binario.
- **Linux/macOS: el backend de inotify entrega accesos, no solo escrituras.** En el crate vendorizado
  (`~/.cargo/registry/src/index.crates.io-*/notify-7.0.0/src/inotify.rs`) hay `EventKind::Access(AccessKind::Close(_))`
  en las líneas 332 y 340, y `Access(Open(_))` en la 356. Por tanto **`fs_read` sí es observable en Linux**.
- **Windows: `notify` no puede ver lecturas.** El backend de `ReadDirectoryChangesW` pide
  `FILE_NOTIFY_CHANGE_FILE_NAME | DIR_NAME | ATTRIBUTES | SIZE | LAST_WRITE | CREATION | SECURITY`
  (`notify-7.0.0/src/windows.rs:265-271`) y **nunca** `FILE_NOTIFY_CHANGE_LAST_ACCESS` (`LAST_ACCESS` no aparece en
  ningún `.rs` del crate). Además, `fs::metadata()` tampoco entrega accesos: solo mtime/atime/size/created, y el atime
  de Windows suele estar desactivado por volumen.

Decisión: **`notify` (watcher por eventos) en Linux/macOS; polling de `metadata()` cada 1 s en Windows**. El polling no
desaparece: se queda como implementación de `fs_touch` en la plataforma donde no hay alternativa barata.

Consecuencia honesta: el techo de detección ≤2 s solo lo garantiza el watcher; en Windows lo pone el polling y un
cambio puede tardar hasta ~1 s en verse.

## Decisión 2 — formato de evento (mínimo, sin payload)

```json
{"ts":"2026-11-12T23:40:00.123Z","token_id":"canary://fs/ssh_key","kind":"fs_touch",
 "origin":{"pid":null,"image":null,"ip":null,"port":null},"detail":"mtime cambiado"}
```

- `origin` se rellena **solo si el SO lo entrega**. En `fs_touch` por polling **no hay PID**: se deja `null` y se
  documenta; **no se adivina** con `sysinfo`. `url_hit` sí trae `ip`/`port`. `cred_use` trae `pid`+`image` si el
  consumidor lo declara.
- **Nunca** se guarda el contenido del fichero, la clave ni el cuerpo de la petición.

### 2.1 `kind` con disponibilidad real por plataforma (enmienda v2)

| `kind` | Linux/macOS | Windows | Mecanismo | `origin` que entrega |
|---|---|---|---|---|
| `fs_touch` | sí | sí | `notify` (write/create/remove) · polling `metadata()` en Windows | ninguno (límite declarado) |
| `fs_read` | sí | **no se emite** | `notify` → `Access(Open/Close(Read))` (inotify/FSEvents) | ninguno |
| `cred_use` | sí | sí | consumo de la credencial señuelo, declarado por su único consumidor | `pid`+`image` |
| `url_hit` | sí | sí | ruta local inventada en el HTTP del kernel | `ip`+`port` |

`fs_read` deja de prometerse en Windows: `GET /canary/status` publica `kinds_available` para que el operador sepa qué
ve de verdad y no lea «no hay lecturas» como «no ha pasado nada».

### 2.2 Timestomp: mtime, tamaño y hash del señuelo

Un token se crea con contenido conocido; al registrarlo se guardan `mtime`, `size` y `sha256` del contenido
(`sha2 = "0.10"` ya es dependencia del kernel). El vigilante alerta si:

1. el `mtime` **retrocede** respecto del último valor observado (`touch -r`, `SetFileTime`), o
2. el `size` cambia, o
3. el `sha256` del contenido ya no coincide con el esperado.

Test F1: retroceder el mtime sin tocar el contenido ⇒ evento con `detail:"mtime_backwards"`; sobrescribir con otro
contenido ⇒ `detail:"content_hash_mismatch"`.

### 2.3 Allow-list del propio kernel (falsos positivos)

Con tokens dentro de `DATA_DIR`, el propio kernel escribe ahí (WAL `-wal`/`-shm`, `backups/`, `VACUUM INTO`,
métricas). Los tokens viven **solo** bajo `data/canary/tokens/`, y el vigilante excluye explícitamente `*.db-wal`,
`*.db-shm`, `data/canary.db` y `backups/`. Test F1: «0 eventos originados por el propio PID en 24 h» (Linux: se
descarta el evento cuyo PID sea el del kernel; Windows: ningún token cae en una ruta que el kernel escriba).

### 2.4 Qué miden los criterios §4

- «detección ≤2 s» y «0 falsos positivos en 24 h» se miden **sobre `fs_touch`**, en las dos plataformas.
- `fs_read` (solo Linux) se mide aparte y **no** cuenta para el criterio hasta que F1 lo valide con números.

## Decisión 3 — almacenamiento y retención

- SQLite `data/canary.db`: `canary_events(id, ts, token_id, kind, pid, image, ip, port, detail)` con índice por `ts`.
- Retención **7 días como techo duro** (`[canary] retention_days = 7`, se recorta si se pide más). Purga al arrancar y
  cada 6 h. Test de purga: un evento con `ts` de hace 8 días no sobrevive.

## Decisión 4 — API de telemetría (solo lectura)

- `GET /api/v1/canary/status` → `{enabled, tokens, kinds_available, last_event_ts, watcher_alive}`.
- `GET /api/v1/canary/events?since=<rfc3339>&limit=100` → eventos (máximo 500 por llamada).
- Auth: el bearer del kernel y su ACL. **`events` solo para el rol de operador**; `viewer` recibe 403. `status` sí
  puede ir a `viewer` (sin eventos). El dashboard consume `status` y pinta el aviso. **No hay endpoint de escritura.**

## Decisión 5 — falla cerrado y opt-in

- `[canary] enabled = false` por defecto. Desactivado: no se crean tokens, no se vigila, y `status` responde
  `{enabled:false}` con la lista de eventos vacía (200, no error).
- El vigilante es una `tokio::spawn` independiente: si muere o entra en pánico, `status.watcher_alive=false` y **el
  kernel y los agentes siguen funcionando**. Test: abortar la tarea, `/health` sigue 200.
- Alerta: un evento `canary_alert` en la telemetría del kernel y su panel; **ninguna acción automática**.

## Preguntas abiertas — cerradas en v2 (2026-11-13)

1. **Alcance de los tokens:** solo dentro de `DATA_DIR`; fuera, lista explícita opt-in
   (`[canary] allow_outside = []`, vacía por defecto). No se crean señuelos en `~/.ssh/` ni en `~/.config/` salvo que el
   operador nombre la ruta.
2. **Retención y `ts`:** 7 días de techo duro. El `ts` **no se hashea** — hashearlo rompería `?since=` y el orden
   temporal, que es justo lo que hace útil la telemetría. `token_id` es un identificador opaco (`canary://fs/<id>`), no
   la ruta en claro.
3. **Quién ve `canary/events`:** solo operador. El rol `viewer` no los ve, porque son metadatos del entorno del
   operador (qué ficheros señuelo existen y cuándo se tocaron).

## Consecuencias

- (+) Sin dependencias nuevas de verdad: el watcher es el crate `notify` que el binario **ya** lleva; en Linux/macOS
  además da lecturas, que es lo que hace útil el escudo frente a una exfiltración por copia.
- (+) Opt-in trivial, telemetría con la auth existente y el fallo contenido en una sola tarea.
- (−) **`fs_touch` no atribuye proceso**: sin auditoría del SO (Windows SACL/admin, Linux `fanotify`/admin) no hay PID,
  y el MVP no lo adivina. Es el límite duro del escudo, no un detalle de implementación.
- (−) **Windows no ve lecturas** con ninguna vía barata: allí el escudo detecta manipulación del señuelo, no su copia.
  Debe decirlo `status.kinds_available`, no la prosa del ADR.
- (−) El polling de 1 s pone el techo de detección en ~2 s en Windows (criterio 1).

## No verificado

- Recursos (≤50 MB, <1 % CPU) y falsos positivos en 24 h: **sin medir**; F1 en el portátil (Hermes-Linux).
- Atribución de proceso en Windows/Linux: sin probar (F1).
- Los eventos `Access` de inotify: leídos en la fuente vendorizada del crate, **no reproducidos** en esta estación
  (Windows), donde ese backend no compila.
- Dictamen jurídico (AEPD/LOPDGDD): no existe; de ahí el opt-in y que esto sea F0 y no producción.
- El «0 falsos positivos» depende de la allow-list de §2.3 y esa allow-list **no está implementada todavía**.
