# 🔄 Zero-Downtime Hot-Reload Swap (tylluan-proxy & kernel)

Este documento contiene la especificación, diseño y estado del sistema de **recarga en caliente sin caídas (Zero-Downtime Hot-Reload)** para Tylluan o3. 
Since agents do not have access to each other's local memory (`brain`), this file acts as the definitive shared record for the entire fleet.

---

## 🏗️ Arquitectura: Local Blue-Green Proxy Swap

La arquitectura de recarga en caliente permite actualizar el núcleo Rust de Tylluan (`tylluan.exe`) sin desconectar a los clientes activos (WebSockets de Canvas, streams SSE del Dashboard o clientes MCP de escritorio).

```
                      +-------------------+
                      |   Client / UI     |
                      +-------------------+
                                |
                   (Stable Port 3030 over WS/SSE)
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
* **Ubicación:** [crates/tylluan-proxy](file:///E:/TylluanMCPo3/crates/tylluan-proxy)
* **Puerto Estable:** Escucha en `127.0.0.1:3030`. Todos los clientes de la red y frontend se conectan aquí.
* **Mapeo Dinámico:** Lee el puerto activo desde `data/active_port.json` (por ejemplo, `{"port": 3031}`).
* **Watcher Pasivo:** Vigila el archivo cada 250ms. Al cambiar el puerto, desvía el nuevo tráfico HTTP y hereda la gestión del túnel de WebSocket de forma transparente.

### 2. Arranque del Kernel con Puertos Dinámicos
* **Fallback de Enlace:** En [crates/tylluan-kernel/src/transport/http/mod.rs](file:///E:/TylluanMCPo3/crates/tylluan-kernel/src/transport/http/mod.rs), si el puerto solicitado (`3030`) está ocupado (lo cual ocurre siempre que el proxy esté arriba), el kernel busca el primer puerto libre en el rango `3031..=3130`.
* **Registro de Puerto:** Una vez que el kernel está listo e inicializado, escribe su puerto activo en `data/active_port.json`.

### 3. Graceful Shutdown & Protección SQLite
* **Evitar Downtime de Inicialización:** El nuevo kernel NO apaga al viejo al arrancar (lo cual provocaría unos 15 segundos de caída mientras se cargan los modelos de IA y embeddings).
* **SQLite en Modo WAL:** Al estar las bases de datos en modo WAL, ambas instancias del kernel pueden tener abiertas las bases de datos simultáneamente.
* **Apagado Diferido:** El nuevo kernel carga la memoria e inicia el servidor HTTP en su puerto dinámico. Una vez enlazado el puerto y escrito en `data/active_port.json`, envía un `POST /api/v1/admin/shutdown` al puerto del kernel anterior en segundo plano. Esto asegura una transición instantánea sin pérdida de disponibilidad.

---

## 🛠️ Archivos del Sistema

* **Proxy Binary:** [main.rs](file:///E:/TylluanMCPo3/crates/tylluan-proxy/src/main.rs)
* **Proxy Configuration:** [Cargo.toml](file:///E:/TylluanMCPo3/crates/tylluan-proxy/Cargo.toml)
* **Kernel Dynamic Bind & Broadcast & Shutdown:** [mod.rs](file:///E:/TylluanMCPo3/crates/tylluan-kernel/src/transport/http/mod.rs#L204-L260)
* **Kernel Entry point:** [main.rs](file:///E:/TylluanMCPo3/crates/tylluan-kernel/src/main.rs)

---

## 🧪 Instrucciones de Compilación y Ejecución

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
   # En otra terminal. Detectará que 3030 está ocupado por el proxy y se moverá a 3031.
   ./target/release/tylluan.exe
   ```

4. **Realizar un Hot Swap:**
   Cuando modifiques el kernel y quieras desplegarlo:
   * Vuelve a compilar el kernel.
   * Ejecuta el nuevo kernel. Éste enviará el comando de apagado al kernel en `3031`, se enlazará a `3032`, y actualizará el archivo json.
   * El proxy en `3030` redirigirá el flujo de inmediato sin que el usuario ni las conexiones WebSockets activas pierdan conectividad permanente.
