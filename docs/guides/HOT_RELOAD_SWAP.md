# Zero-Downtime Hot-Reload Swap — ARCHIVED DESIGN

> **⚠️ PARTIALLY IMPLEMENTED — the proxy half was never built.**
> **What exists** (kernel side, read from `crates/tylluan-kernel/src/transport/http/mod.rs`, `start_http_server`; not exercised in this audit): if the configured port is already taken, the kernel binds the next free port in `port+1..=port+100`; it writes the bound port to `data/active_port.json` (relative to its working directory); and it sends `POST /api/v1/admin/shutdown` to the previous kernel when that kernel's port is recorded in that file and lies in `port..=port+100`.
> **What does not exist:** the `tylluan-proxy` binary/crate meant to keep `:47004` stable and forward to the active port. Without it, clients keep using whatever port they were configured with. Do not start a second kernel in the same working directory as a running one expecting a seamless swap: it takes the next free port and tells the running kernel to shut down, so it replaces it and clients must reconnect to the new port.
> The rest of this file is the original design, kept for context.
---

Este documento contiene la especificación y diseño del sistema de **recarga en caliente sin caídas (Zero-Downtime Hot-Reload)** — un diseño del que solo se construyó la parte del kernel (ver el aviso de arriba).

---

## 🏗️ Arquitectura: Local Blue-Green Proxy Swap

La arquitectura de recarga en caliente permite actualizar el núcleo Rust de Tylluan (`tylluan.exe`) sin desconectar a los clientes activos (WebSockets de Canvas, streams SSE del Dashboard o clientes MCP de escritorio).

```
                      +-------------------+
                      |   Client / UI     |
                      +-------------------+
                                |
                   (Stable Port 47004 over WS/SSE)
                                v
                      +-------------------+
                      |    tylluan-proxy    | <----+ Checks data/active_port.json
                      +-------------------+      |
                                |                |
                  +-------------+-------------+  |
                  |                           |  |
            (Port 3031)                 (Port 3032)
                  v                           v
         +-----------------+         +-----------------+
         |  Old Kernel V3  |         |  New Kernel V3  |
         | (Terminating)   |         |    (Active)     |
         +-----------------+         +-----------------+
```

### 1. El Proxy Inverso (`tylluan-proxy`)
* **Ubicación:** `crates/tylluan-proxy` (never created)
* **Puerto Estable:** Escucha en `127.0.0.1:47004`. Todos los clientes de la red y frontend se conectan aquí.
* **Mapeo Dinámico:** Lee el puerto activo desde `data/active_port.json` (por ejemplo, `{"port": 3031}`).
* **Watcher Pasivo:** Vigila el archivo cada 250ms. Al cambiar el puerto, desvía el nuevo tráfico HTTP y hereda la gestión del túnel de WebSocket de forma transparente.

### 2. Arranque del Kernel con Puertos Dinámicos
* **Fallback de Enlace:** En `crates/tylluan-kernel/src/transport/http/mod.rs` (the kernel does not do this fallback today), si el puerto solicitado (`47004`) está ocupado (lo cual ocurre siempre que el proxy esté arriba), el kernel busca el primer puerto libre en el rango `3031..=3130`.
* **Registro de Puerto:** Una vez que el kernel está listo e inicializado, escribe su puerto activo en `data/active_port.json`.

### 3. Graceful Shutdown & Protección SQLite
* **Evitar Downtime de Inicialización:** El nuevo kernel NO apaga al viejo al arrancar (lo cual provocaría unos 15 segundos de caída mientras se cargan los modelos de IA y embeddings).
* **SQLite en Modo WAL:** Al estar las bases de datos en modo WAL, ambas instancias del kernel pueden tener abiertas las bases de datos simultáneamente.
* **Apagado Diferido:** El nuevo kernel carga la memoria e inicia el servidor HTTP en su puerto dinámico. Una vez enlazado el puerto y escrito en `data/active_port.json`, envía un `POST /api/v1/admin/shutdown` al puerto del kernel anterior en segundo plano. Esto asegura una transición instantánea sin pérdida de disponibilidad.

---

## 🛠️ Archivos del sistema (del diseño; no existen)

* **Proxy Binary:** `crates/tylluan-proxy/src/main.rs` (never created)
* **Proxy Configuration:** `crates/tylluan-proxy/Cargo.toml` (never created)
* **Kernel Dynamic Bind & Broadcast & Shutdown:** `crates/tylluan-kernel/src/transport/http/mod.rs` (the dynamic-port bind described here was not built)
* **Kernel Entry point:** `crates/tylluan-kernel/src/main.rs`

---

## 🧪 Instrucciones del diseño (no ejecutables: el proxy y el kernel con puerto dinámico no existen)

Para iniciar el sistema de recarga en caliente de forma limpia:

1. **Compilar todo el Workspace:**
   ```powershell
   cargo build --release
   ```

2. **Ejecutar el Proxy:**
   ```powershell
   # En una terminal dedicada
   ./target/release/tylluan-proxy.exe
   ```

3. **Ejecutar el Kernel:**
   ```powershell
   # En otra terminal. Detectará que 47004 está ocupado por el proxy y se moverá a 3031.
   ./target/release/tylluan.exe
   ```

4. **Realizar un Hot Swap:**
   Cuando modifiques el kernel y quieras desplegarlo:
   * Vuelve a compilar el kernel.
   * Ejecuta el nuevo kernel. Éste enviará el comando de apagado al kernel en `3031`, se enlazará a `3032`, y actualizará el archivo json.
   * El proxy en `47004` redirigirá el flujo de inmediato sin que el usuario ni las conexiones WebSockets activas pierdan conectividad permanente.
