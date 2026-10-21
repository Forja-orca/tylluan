# B-11 — Supervivencia del JWT a un reinicio (kernel de prueba, end to end)

**Fecha:** 2026-10-22 · **Agente:** Buffy · **HEAD:** `5e49bf8` (= `origin/main`)

## Alcance y exclusiones

Cola `COLA_buffy_2026-10-22.md`, item B-11. Todo se ejecutó contra un **kernel de
prueba** en `test-run-jwt/b11/` con puerto propio (`56811`) y `TYLLUAN_DATA_DIR`
propios. **No se tocó producción**: no había kernel en `127.0.0.1:47004` durante
toda la ejecución (comprobado antes y después), y no se modificó ni `.tylluan-token`
ni el token maestro del workspace. **No se modificó `src/`.**

## Metodología (y por qué hay dos configs)

El flujo OAuth completo es `GET /oauth/authorize` → `307` con `code` →
`POST /oauth/token` (PKCE S256) → JWT HS256 con `scope=mcp`.

Hay un problema de diseño que obliga a arrancar en dos fases:

| Fase | Config | Por qué |
|------|--------|---------|
| Emisión | `issue.toml` → `dev_mode = true` | `/oauth/authorize` auto-aprueba **solo** con `dev_mode` (`oauth.rs:410`); con `dev_mode=false` devuelve 403 |
| Verificación | `check.toml` → `dev_mode = false` + `--token` | con `dev_mode=true` el middleware corta en `auth.rs:472` (rama 2 → `DevBypass`) **antes** de llamar a `oauth.validate_bearer()`. En `dev_mode=true` **nada valida ningún JWT** y no habría discriminador |

**Discriminador** sobre la ruta operator-class `GET /api/v1/memory/quarantine`
(`routes.rs:313`, gate en `auth.rs:374`):

- `403` + `insufficient_scope` → el servidor **reconoce** el JWT (firma, `exp` y
  `aud` correctos) pero `scope=mcp` no entra (D-3) → **JWT VÁLIDO**
- `401` → firma / `aud` / `exp` no cuadran → **JWT INVÁLIDO**

Controles en todas las corridas: JWT basura → `401`, sin credencial → `401`,
token maestro → `200`.

## Resultado de los tres casos

### CASO 1 — mismo `DATA_DIR` (`d1`) tras reinicio → **PASA**

Emitido en el primer arranque de `d1`, verificado en el segundo arranque de `d1`.

```
########## CASO 1 — mismo DATA_DIR (d1) tras reinicio ##########
--- A) JWT emitido en el arranque previo de d1 ---
PROBE http://127.0.0.1 <puerto-de-prueba>/api/v1/memory/quarantine
HTTP 403
BODY {"error":"insufficient_scope","message":"This endpoint requires the operator class. The OAuth token presented has scope=mcp only.","scope":"operator"}
--- B) CONTROL NEGATIVO: JWT basura ---
PROBE http://127.0.0.1 <puerto-de-prueba>/api/v1/memory/quarantine
HTTP 401
BODY {"error":"unauthorized","message":"Valid Bearer token required. Check your .tylluan-token file."}
```

Controles adicionales de esta misma corrida: sin credencial → `HTTP 401`;
token maestro → `HTTP 200`.

### CASO 2 — `DATA_DIR` distinto (`d2`) → **PASA** (el JWT deja de valer)

Los dos directorios tienen secretos distintos, lo que demuestra que el secreto
está realmente ligado al `DATA_DIR`:

```
7625fc35d219da146aaacab670ef3e577176135a65899e821b384d6c9ec7a28b *d1/oauth_jwt_secret
610551f91678e2809c699f52ee46227f72bba8f7a3b513e7ab83ec15705ea151 *d2/oauth_jwt_secret

########## CASO 2 — DATA_DIR distinto (d2), mismo JWT ##########
PROBE http://127.0.0.1 <puerto-de-prueba>/api/v1/memory/quarantine
HTTP 401
BODY {"error":"unauthorized","message":"Valid Bearer token required. Check your .tylluan-token file."}
```

Control token maestro en la misma corrida → `HTTP 200` (nadie se queda sin
acceso por el cambio de `DATA_DIR`).

### CASO 3 — borrado de `oauth_jwt_secret` → **PASA** (arranca igual y el JWT viejo no vale)

```
########## CASO 3 — borrado oauth_jwt_secret de d1 ##########
--- secreto ANTES: 7625fc35d219da146aaacab670ef3e577176135a65899e821b384d6c9ec7a28b
--- secreto DESPUES: 8ee0345502f1a7d60dc5ce17a1a692e8b46e9dfd9b9a8cbc268b92d339579130
--- arranque con secreto borrado:
{"status":"ok","version":"0.17.1","commit":"043eb71"}
--- JWT viejo:
PROBE http://127.0.0.1 <puerto-de-prueba>/api/v1/memory/quarantine
HTTP 401
BODY {"error":"unauthorized","message":"Valid Bearer token required. Check your .tylluan-token file."}
--- CONTROL token maestro:
HTTP 200
--- banner del arranque (no debe decir dev_mode=true):
🔐 Auth: Master Token is active (see .tylluan-token for the full key)
```

El arranque **no falla** con el secreto borrado (coherente con
`with_persisted_secret`, `oauth.rs:90`: nunca hace fallar el arranque), regenera
un secreto nuevo distinto, y el token maestro sigue funcionando.

**Los tres casos cumplen el criterio de la cola.**

---

## Hallazgo lateral — `--config` con ruta relativa se pierde (divergencia silenciosa)

Descubierto al montar el banco de pruebas, no era el propósito del B-11 pero
bloqueaba la ejecución, así que lo reproduje con salida literal.

### Causa raíz

`main.rs` cambia de directorio **antes** de leer `--config`:

```rust
let workspace_root = setup::find_workspace_root();
// SOVEREIGN FIX: Always operate from workspace root
std::env::set_current_dir(&workspace_root);          // main.rs:469-473
...
// --config se registra TAL CUAL vino en la CLI (puede ser relativo)
TylluanConfig::set_config_path_override(PathBuf::from(config_path_str));  // main.rs:493
...
if let Ok(content) = std::fs::read_to_string(config_path_str) {  // main.rs:527 → relativo al workspace root
    config = toml::from_str(&content)?;
} else {
    warn!("⚠️ Config file from CLI not found or invalid: {}", config_path_str);  // solo WARN
}
```

`find_workspace_root` (`setup.rs:3`) sube hasta 5 niveles buscando `tylluan.toml`
/ `.venv` / `guilds`. Un `--config` relativo se resuelve contra esa raíz, **no
contra el cwd desde el que arrancó el operador**.

Evidencia (arranque desde `test-run-jwt/b11` con `--config failopen.toml`):

```
INFO tylluan_nexus: 📄 Config path override from CLI: failopen.toml
WARN tylluan_nexus: ⚠️ Config file from CLI not found or invalid: failopen.toml
```

El fichero existía en el cwd real del operador; se buscó en la raíz del workspace.

### Impacto (matizado — NO lo etiqueto "fail-open" sin más)

Los defaults son seguros, y eso limita la gravedad:

- `default_host() = "127.0.0.1"` (`config.rs:1918`, con test de auditoría
  Audit Finding #2 en `config.rs:2366`)
- `dev_mode: false` por defecto (`config.rs:633`)
- `intent_filter = true` por defecto (`config.rs:1413`)

Por tanto el fallo **no** desactiva la autenticación por sí mismo. Lo que sí ocurre:

1. **Divergencia silenciosa.** La config del operador se descarta por completo y
   el kernel arranca con defaults; la única señal es un `WARN` dentro de
   `logs/kernel.log`, no en stderr ni en el banner. El banner incluso miente:
   imprime `Config  : tylluan.toml` escrito a mano (`main.rs`, banner de arranque)
   sin reflejar el fichero realmente cargado.
2. **El token maestro no es el que eligió el operador.** Al caer a defaults,
   `ensure_auth_token()` lee `.tylluan-token` de la raíz del workspace, así que
   un operador que fijó su token en la config autentica contra otro distinto.
3. **Puerto: colisión con producción.** Perdido el `--config`, el puerto pasa a
   ser `47004`. Con `--config relativo` y sin `--port`, el kernel de prueba se
   une al puerto de producción. Esto ya está documentado como causa de la
   incidencia del **2026-10-11** en el propio comentario de `resolve_pid_data_dir`
   (`main.rs`): *"`--config test.toml` + no `--port` shared prod's `./data`"*. Es
   la misma clase de fallo, hoy todavía sin cerrar en su origen.
4. **Dirección del fallo dependiente de cada default.** No es sistemáticamente
   permisiva ni restrictiva: para cada key depende de si el default es más o menos
   estricto que lo pedido. Esa imprevisibilidad es justo lo que ADR-020 (fail-closed)
   busca evitar.

### Reproducción

```bash
cd test-run-jwt/b11
tylluan-nexus --config failopen.toml    # relativo → WARN + defaults
tylluan-nexus --config "$PWD/failopen.toml"   # absoluto → carga bien
```

### Arreglo propuesto (no aplicado — `src/` está fuera del alcance de esta cola)

Cualquiera de estos dos, en orden de preferencia:

1. **Fail-closed:** si `--config` se pasa explícitamente y el fichero no se puede
   leer, **abortar** con error en stderr (no `warn!` + defaults). Un flag explícito
   que se ignora es un engaño.
2. **Resolver antes del `chdir`:** convertir `config_path_str` a ruta absoluta
   (`std::fs::canonicalize`) **antes** de `set_current_dir`, en el mismo punto en
   el que ya se registra el override.

La opción 1 además haría visible la incidencia 2026-10-11 en el momento exacto en
que ocurre, en vez de a posteriori en el log.

## Verificación

- Los tres casos se ejecutaron contra el binario `target/debug/tylluan-nexus.exe`
  compilado en este worktree (`commit 043eb71` reportado por `/health`).
- `git status` sin tocar nada fuera de `test-run-jwt/` y `docs/internal/`.
- Tras la ejecución: sin procesos `tylluan-nexus` vivos, puertos 56811-56814
  libres, `47004` sin listener (igual que al empezar).

## Limitaciones

- **No verificado en producción.** El item B-10 (aceptación post-reinicio contra
  producción) sigue **bloqueado**: `127.0.0.1:47004` no responde, el kernel de
  producción no está arrancado en este momento.
- El discriminador `403`/`403` exige `dev_mode=false` en la fase de verificación
  (necesario, ver Metodología). No se valida por tanto el JWT *dentro* de un
  kernel en `dev_mode=true`, donde el middleware lo salta — eso es
  comportamiento esperado (`auth.rs:472`), no una anomalía, pero queda explícito.
- TTL del JWT emitido por el flujo: 3600 s (`oauth.rs:587`). Las tres corridas se
  hicieron dentro de esa ventana; **no se probó la caducidad por tiempo**, que no
  la pide la cola.

## Impact

- **`src/` intacto:** ningún fichero de producción modificado. Artefactos nuevos
  solo en `test-run-jwt/b11/` (configs, `probe.py`, salidas) y este informe.
- **Kernels de prueba:** solo en puerto 56811-56814 con `TYLLUAN_DATA_DIR`
  propio bajo `test-run-jwt/b11/`. El de producción (47004) ni se arrancó ni se
  contactó; `.tylluan-token` y el token maestro no se modificaron.
- **Hallazgo para Deep/tech lead:** `--config` relativo se descarta en silencio
  (`main.rs:469-473` + `main.rs:527`), con impacto real de colisión de puerto
  sobre producción (incidencia 2026-10-11, misma causa, aún abierta en origen).
  Requiere decisión de arreglo; queda fuera de esta cola por la exclusión de `src/`.

**Estado: no verificado en producción (B-10 pendiente de arranque del kernel de
José). Los 3 casos del B-11 están verificados en kernel de prueba con salida
literal.**
