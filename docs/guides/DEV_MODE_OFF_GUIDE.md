# Apagar `dev_mode`: guía práctica para conectar los clientes con token

Esta guía explica qué cambia cuando el kernel deja de estar en `dev_mode = true`
y **dónde se pega el token** en cada cliente. Está escrita para hacerse sin
tocar código: cambiar el ajuste, repartir el token y comprobar cada cliente.

> **Regla de oro:** el token maestro **no se escribe en ningún documento, chat
> ni fichero versionado**. En esta guía siempre aparece como `<TU_TOKEN>`.
> Quien lo tenga puede hacer todo lo que hace un operador.

---

## 0. Qué cambia (y qué no)

| Con `dev_mode = true` (hoy) | Con `dev_mode = false` |
|---|---|
| Todo lo que llega desde esta máquina se acepta sin credencial. | Toda llamada a la API (MCP, REST, dashboard, rutas de operador) exige el token maestro. |
| `/health`, el dashboard (interfaz) y los ficheros estáticos cargan sin token. | **Igual**: siguen cargando sin token (solo la API pide credencial). |
| `/api/v1/bash` funciona (solo existe en desarrollo). | **Desaparece**: devuelve 403 aunque lleves token. Hay que usar `tylluan_do`. |
| Un `?token=` en la URL no hace falta. | Sirve para `/messages` y para llamadas de agente; **no** sirve para las rutas de operador (dan 403). |
| Las guilds internas y los watchers usan la credencial que les inyecta el kernel. | **Igual**: no hay que tocar nada en las guilds internas. |

Cómo saber en qué modo está el kernel: en el arranque, con `dev_mode = true`
aparece la línea `[!] dev_mode=true — authentication disabled`; si no aparece,
la autenticación está activa.

---

## 1. El token maestro: qué es y dónde vive

- Es **una sola cadena larga** que el kernel usa como llave maestra.
- Vive en el fichero **`.tylluan-token`**, en la raíz del proyecto de Tylluan
  (junto a `tylluan.toml`).
- También se puede dar al kernel con la variable de entorno `TYLLUAN_TOKEN`;
  si existe, tiene prioridad sobre el fichero.
- Si el kernel arranca con `dev_mode = false` y **no encuentra** token, genera
  uno aleatorio y lo guarda en `.tylluan-token`. Ese fichero está ignorado por
  git: nunca se sube al repositorio.
- **Cada kernel tiene el suyo**: el token de un kernel no vale para otro
  (si arrancas otra instancia, usa el `.tylluan-token` de esa instancia).

Para copiarlo (Windows, sin tecnicismos):

1. Abre la carpeta del proyecto de Tylluan.
2. Busca `.tylluan-token` (si no lo ves, activa "ver archivos ocultos").
3. Ábrelo con el Bloc de notas, copia **todo** el contenido y ciérralo sin
   cambiar nada.

No lo pegues en Coloquio, ni en un chat, ni en una captura de pantalla.

---

## 2. Claude Code (`~/.claude.json`)

Hoy la entrada de Tylluan en Claude Code es una URL sin credencial:

```json
"tylluan": {
  "type": "http",
  "url": "http://127.0.0.1:<puerto>/messages"
}
```

Con `dev_mode = false` eso devuelve 401. Añade la cabecera `headers`:

```json
"tylluan": {
  "type": "http",
  "url": "http://127.0.0.1:<puerto>/messages",
  "headers": {
    "Authorization": "Bearer <TU_TOKEN>"
  }
}
```

Pasos:

1. Abre `~/.claude.json` (en Windows: `C:\Users\<tu_usuario>\.claude.json`).
2. Localiza la entrada `"tylluan"` dentro de `mcpServers`.
3. Añade el bloque `headers` con tu token (sustituye `<TU_TOKEN>`).
4. Guarda y **reinicia Claude Code**.
5. Comprueba pidiendo una acción de Tylluan (por ejemplo, una consulta de
   memoria). Si responde, el token llegó bien.

Alternativa: el lanzador `tylluan-mcp.bat` imprime la orden
`/mcp add tylluan sse http://127.0.0.1:<puerto>/sse`; si prefieres SSE, añade
`?token=<TU_TOKEN>` a esa URL. Sirve para las herramientas de agente, **no**
para las rutas de operador.

---

## 3. Dashboard (interfaz web)

La interfaz carga sin token (es estática), pero las llamadas a la API no.

1. Abre el dashboard en el navegador (`http://127.0.0.1:<puerto>/dashboard`).
2. Al primer dato que no cargue, aparece el diálogo **"Autenticación
   Requerida"**.
3. Pega el token en el campo **Bearer Token** (hay un botón **"Pegar
   portapapeles"**; el campo dice `Pega aquí el contenido de .tylluan-token...`).
4. Pulsa guardar. El token queda en el `localStorage` del navegador para no
   tener que repetirlo en cada visita.
5. Si el diálogo no salta solo, usa el botón de la llave en la cabecera
   (**"Configurar Bearer Token"**).

Los paneles de operador (mantenimiento, sandbox, cuarentena, MCP externos,
dispatches, peers de federación) funcionan con ese mismo token maestro pegado
en el diálogo.

---

## 4. Claude Desktop y `mcp-remote`

Hay dos caminos; con `dev_mode = false` **solo uno funciona hoy**:

- ✅ **Cabecera Bearer (recomendado):** en la configuración de Claude Desktop
  (`%APPDATA%\Claude\claude_desktop_config.json`), añade la cabecera al puente:

```json
{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": [
        "mcp-remote@latest",
        "http://127.0.0.1:<puerto>/messages",
        "--allow-http",
        "--header",
        "Authorization:${TYLLUAN_TOKEN}"
      ],
      "env": {
        "TYLLUAN_TOKEN": "Bearer <TU_TOKEN>"
      }
    }
  }
}
```

- ❌ **OAuth (pantalla de consentimiento):** con `dev_mode = false`, al abrir
  `/oauth/authorize` aparece la pantalla de consentimiento del operador, pero
  el flujo OAuth de los clientes de escritorio **no está terminado** (falta la
  fase de registro de clientes/permisos). Resultado práctico: se queda en la
  pantalla de consentimiento y no conecta. Usa la cabecera Bearer.

El token por URL (`...?token=<TU_TOKEN>`) vale para `/messages`, pero **no**
para las rutas de operador (403). Para clientes de escritorio, usa la cabecera.

---

## 5. Scripts y CLI

### Scripts (Python y similares)

Exporta el token en la sesión antes de ejecutarlos (PowerShell):

```powershell
$env:TYLLUAN_TOKEN = "<TU_TOKEN>"
```

O deja el fichero `.tylluan-token` en la raíz del repo: los scripts de Coloquio
del repositorio (`scripts/coloquio_loop_buffy.py`, `scripts/coloquio_summarizer.py`)
lo leen solos (primero la variable, luego el fichero) y, si no hay token,
siguen enviando las peticiones sin credencial como hasta ahora.

### CLI (`tylluan-cli`)

```powershell
tylluan sandbox --token <TU_TOKEN> status
tylluan sandbox --token <TU_TOKEN> set balanced
tylluan stop --token <TU_TOKEN>
```

`tylluan sandbox` también acepta la variable de entorno `TYLLUAN_TOKEN` en vez
de `--token`. `tylluan stop` necesita `--token` (no lee la variable).

Consejo: evitar dejar el token escrito en el historial del shell; si se usa
`--token`, limpiar el historial después.

---

## 6. Agentes del equipo

Los agentes hablan con el kernel por REST (Coloquio) o por MCP:

- **Por REST (Coloquio):** cada petición necesita
  `Authorization: Bearer <TU_TOKEN>`. Los scripts del repo ya lo hacen si
  encuentran la variable o el fichero (paso 5). Cualquier script propio de un
  agente debe añadir la cabecera o exportar `TYLLUAN_TOKEN`.
- **Por MCP (Claude Code y similares):** la configuración del cliente es la del
  paso 2.
- Un agente **no** debe pegar el token en Coloquio ni reenviarlo: si lo
  necesita, que lo lea del fichero o del entorno de su propio proceso.

---

## 7. Síntoma → causa → arreglo

| Síntoma | Causa probable | Arreglo |
|---|---|---|
| Todo devuelve **401** (MCP, REST, dashboard, scripts) | No se está enviando token, o es el de otra instancia | Copiar el `.tylluan-token` **de esa** instancia y pegarlo en el cliente correspondiente (pasos 2-5). Reiniciar el cliente. |
| El dashboard carga pero todos los paneles dan **401** | El diálogo de token no se ha rellenado | Botón de la llave → "Configurar Bearer Token" → pegar y guardar (paso 3). |
| **403** en una ruta de operador (configuración, mantenimiento, sandbox, cuarentena, dispatches, peers) usando `?token=` | El token por URL solo vale para agente, no para operador | Usar cabecera `Authorization: Bearer <TU_TOKEN>` (diálogo del dashboard, `--token` del CLI o `headers` del cliente MCP). |
| **403** en `/api/v1/bash` incluso con token | `/api/v1/bash` está deshabilitado fuera de desarrollo | No hay que arreglarlo: usar `tylluan_do` / una guild (paso 0). |
| Claude Desktop / `mcp-remote` se queda en la **pantalla de consentimiento** y no conecta | El flujo OAuth de escritorio no está terminado | Configurar la cabecera Bearer con `--header` + `env` (paso 4). |
| `tylluan stop` responde "requires a bearer token" | Falta el token en el comando | `tylluan stop --token <TU_TOKEN>` (paso 5). |
| Un script da **401** pero el dashboard funciona | El script no ve la variable ni el fichero (se ejecuta desde otra carpeta) | Exportar `TYLLUAN_TOKEN` en su entorno o ejecutarlo desde la raíz del repo (allí está `.tylluan-token`). |
| Todo funciona en un kernel y falla al arrancar otro | El token es por instancia | Usar el `.tylluan-token` de la instancia nueva (paso 1). |

---

## 8. Comprobación rápida tras el cambio

1. `GET /health` → debe responder **sin** token (si no, algo más se rompió).
2. Una llamada REST con `Authorization: Bearer <TU_TOKEN>` → **200**.
3. Una ruta de operador (por ejemplo, la configuración) con cabecera → **200**;
   la misma con `?token=` → **403** (es lo esperado, no es un fallo).
4. `/messages` con `?token=` → funciona; `/api/v1/bash` con cabecera → **403**
   (esperado).
5. Dashboard: pegar token en el diálogo y comprobar que los paneles cargan.

Si algo no encaja, la tabla del paso 7 cubre los tres errores que aparecen
siempre (401, 403 por `?token=` y 403 de `/api/v1/bash`).

---

## 9. Referencias

- Configuración de clientes MCP (Claude Desktop, Codex, Qwen, Cursor, VS Code):
  [CLIENT_INTEGRATION_GUIDE.md](CLIENT_INTEGRATION_GUIDE.md).
- Inventario técnico de lo que cambia al apagar `dev_mode` (por fichero y
  cliente): informes internos de dsh T-DSH-82 y T-DSH-83 (en `docs/internal/`,
  cuando se integren en `main`).
- Preparación de scripts y CLI con token: informe interno de dsh T-DSH-83.
