# ADR-030: Escudo MVP-01 F0 — detección pasiva con canary tokens

**Status:** Borrador F0, 2026-11-12 (pendiente: revisión de pareja Hermes y del TL).
**Autor:** Deep. **Decide:** José (`DEC-20261112-09`, «sí»). **Clase de riesgo:** A.
**Ficha:** `ESCUDO-MVP-01` (`D:\tylluan-loops\briefs\ESCUDO-MVP-01_deteccion_pasiva.md`). Nada de esta decisión mueve
un LLM dentro del bucle de defensa: el componente es determinista y los LLM solo leen su telemetría.

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
| **Módulo del kernel (elegida)** | El evento tiene que llegar al operario (dashboard, SSE) y a los agentes (API) en segundos; ambas superficies ya viven en el kernel con su token/ACL. Vigilancia por **polling de `std::fs::metadata`** cada 1 s sobre una lista corta (≤16 tokens): determinista y **sin dependencias nuevas** (no hace falta `notify`). |
| Guild Python (descartada) | Añade un proceso por vigilante (más el lanzador del venv, ver `BUF-D9`), exige `always_on` para sobrevivir, y su telemetría sería una tool de guild: los agentes tendrían que pasar por `tylluan_do`. Su ventaja teórica (aislamiento) ya la cubre el criterio 5, porque el vigilante es una tarea independiente y su muerte no tumba el HTTP. |

## Decisión 2 — formato de evento (mínimo, sin payload)

```json
{"ts":"2026-11-12T23:40:00.123Z","token_id":"canary://fs/ssh_key","kind":"fs_touch",
 "origin":{"pid":null,"image":null,"ip":null,"port":null},"detail":"mtime cambiado"}
```

- `kind`: `fs_touch` | `fs_read` | `cred_use` | `url_hit`.
- `origin` se rellena **solo si el SO lo entrega**. En `fs_touch` vía `metadata()` **no hay PID**: se deja `null` y se
  documenta; **no se adivina** con `sysinfo`. `url_hit` sí trae `ip`/`port`. `cred_use` trae `pid`+`image` si el
  consumidor lo declara.
- **Nunca** se guarda el contenido del fichero, la clave ni el cuerpo de la petición.

## Decisión 3 — almacenamiento y retención

- SQLite `data/canary.db`: `canary_events(id, ts, token_id, kind, pid, image, ip, port, detail)` con índice por `ts`.
- Retención **≤7 días** configurable (`[canary] retention_days = 7`, con techo duro 7). Purga al arrancar y cada 6 h.
  Test de purga: un evento con `ts` de hace 8 días no sobrevive a la purga.

## Decisión 4 — API de telemetría (solo lectura)

- `GET /api/v1/canary/status` → `{enabled, tokens, last_event_ts, watcher_alive}`.
- `GET /api/v1/canary/events?since=<rfc3339>&limit=100` → eventos (máximo 500 por llamada).
- Auth: el bearer del kernel y su ACL; el rol `guest` **no** los ve (son metadatos del entorno del operador). El
  dashboard consume `status` y pinta el aviso. **No hay endpoint de escritura.**

## Decisión 5 — falla cerrado y opt-in

- `[canary] enabled = false` por defecto. Desactivado: no se crean tokens, no se vigila, y `status` responde
  `{enabled:false}` con la lista de eventos vacía (200, no error).
- El vigilante es una `tokio::spawn` independiente: si muere o entra en pánico, `status.watcher_alive=false` y **el
  kernel y los agentes siguen funcionando** (criterio 5). Test: abortar la tarea, `/health` sigue 200.
- Alerta: un evento `canary_alert` en la telemetría del kernel y su panel; **ninguna acción automática**.

## Consecuencias

- (+) Sin dependencias nuevas, opt-in trivial, telemetría con la auth existente y el fallo contenido en una tarea.
- (−) **`fs_touch` no atribuye proceso** sin auditoría del SO (Windows SACL/admin, Linux `fanotify`/admin). Es un
  límite del MVP y F1 debe medir si basta para «detectar a tiempo».
- (−) Polling de 1 s pone el techo de detección en ~2 s (criterio 1).

## Abierto (lo decide el TL / José)

1. ¿Tokens solo dentro de `DATA_DIR` o también fuera (p. ej. `~/.ssh/`)? Fuera detecta mejor y eleva el riesgo legal.
2. Retención final y si el `ts` debe guardarse hasheado.
3. ¿`canary/events` solo para el operador, o también para agentes con rol `viewer`?

## No verificado

- Recursos (≤50 MB, <1 % CPU) y falsos positivos: **sin medir**; F1 en el portátil (Hermes-Linux).
- Atribución de proceso en Windows/Linux: sin probar (F1).
- Dictamen jurídico (AEPD/LOPDGDD): no existe; de ahí el opt-in.
