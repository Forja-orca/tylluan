# IMPL S-DEEP-1 — superficie de red, cadena de suministro y modelo de amenazas del pilar Recordar

**Agente:** Deep (OpenCode)
**Fecha:** 2026-10-26
**Base:** `origin/main d79990a` (worktree `E:\tylluan-wt-deep`).
**Tipo:** **solo lectura**. Sin instalar paquetes (nada de `npx`/`uvx`/`pip install`/`npm install`), sin arrancar kernels, sin cambios de código.

---

## 1. Superficie de red (bind/listen)

| Fichero, línea | Qué escucha | Dirección por defecto | Configurable | ¿Puede quedar accesible en LAN? |
|---|---|---|---|---|
| `crates/tylluan-kernel/src/transport/http/mod.rs`, líneas 359-368 y 479 | HTTP/SSE/MCP/REST del kernel (`TcpListener::bind((host, port))`) | `127.0.0.1:47004` (`config.rs` línea 2056, `tylluan.example.toml` líneas 5-6) | Sí: `[nexus] host/port`, `--host`/`--port`, `TYLLUAN_HOST`/`TYLLUAN_PORT` | Sí, solo si se pone `host = "0.0.0.0"` (o `::`). Con `dev_mode = true` el guard de `main.rs` líneas 423-458 **rechaza** la combinación; con `dev_mode = false` exige Bearer token, pero la superficie queda en LAN. |
| `crates/tylluan-kernel/src/transport/http/mod.rs`, línea 1149 | Listener P2P Noise XK (dispatch de guilds) | `0.0.0.0`, puerto `9123` cuando `[p2p] enabled = true`; **default `false`** (`config.rs` líneas 522-529) | Sí: `[p2p] enabled/listen_port` | Sí, si se habilita: escucha en todas las interfaces. Exige peer aprobado (X25519) y el conjunto aprobado vacío es fail-closed, pero un peer aprobado puede llamar a cualquier guild (sin allow-list por herramienta). |
| `crates/tylluan-kernel/src/transport/mdns.rs`, líneas 17, 38 y 58 | mDNS advertiser/discovery (`_mcp-gateway._tcp.local.`) | Multicast mDNS del sistema; `ServiceInfo` anuncia `0.0.0.0` con el puerto del kernel | Sí: `[mdns] advertise/discover`, **default `false`** (`config.rs` líneas 486-495) | Sí si se activa: anuncia el gateway en la LAN; discovery registra peers descubiertos (requieren aprobación antes de sync). No autentica mDNS. |
| `crates/tylluan-link/src/nat.rs`, línea 89 | Socket UDP efímero para STUN (`0.0.0.0`/`[::]` con puerto efímero) | Efímero, saliente hacia los STUN configurados | Sí: `[nat] stun_servers/timeout/retries` (`config.rs` líneas 389-405) | No queda escuchando: se conecta al servidor STUN y se cierra. El puerto que devuelve no es el del kernel (limitación conocida de M12). |
| `crates/tylluan-kernel/src/tunnel/mod.rs`, líneas 73, 82, 98 y 191 | `netsh interface portproxy` de Windows (`listenaddress=0.0.0.0`) para el puente WSL | `0.0.0.0` en el puerto `wsl_bridge_port` (default 3031; `tylluan.toml` local usa 4002) → `127.0.0.1:47004` | Sí: `[tunnel] enabled`, `wsl_bridge_port` (`config.rs` líneas 1344-1352) | Sí si el túnel está activo: abre el puerto del bridge en todas las interfaces y lo reenvía al kernel local. La auth del kernel sigue aplicando, pero la superficie de red aumenta. |
| `dashboard/vite.config.ts`, líneas 73 y 108 | Vite dev/preview del dashboard | `127.0.0.1` en el puerto `5173` (dev) / `5174` (preview) | Sí: config de Vite | No por defecto; solo si se cambia `host`. Es una herramienta de desarrollo, no el binario publicado (el release no lo embebe). |
| `guilds/core/browser.py`, línea 77 | Socket de sondeo para elegir puerto de Chrome DevTools | `127.0.0.1` con puerto efímero | No | No: loopback y efímero; solo se usa para encontrar un puerto libre. |
| `guilds/core/llama_backend.py`, líneas 518, 538 y 614 | Comprobación de puerto y `llama-server` | `127.0.0.1:<puerto>`; el server se lanza con `--host 127.0.0.1` | Sí: `LLAMA_PORT`/backend config | No por defecto: solo loopback. |
| `tylluan-link/src/lib.rs`, línea 156 | `start_p2p_listener` legacy (`0.0.0.0:{port}`) | `0.0.0.0` | No en la ruta actual | **No está cableado** en el kernel actual (el listener real es `p2p.rs` línea 178 desde `http/mod.rs` línea 1166). Lo dejo anotado como código muerto a retirar, no como superficie viva. |
| `guilds/` (FastMCP) | Guilds stdio gestionadas por el kernel | Sin bind: stdio/child process | N/A | No escuchan por sí mismas; los watchers son clientes HTTP salientes. |

**Comando de reproducción:** `rg -n "TcpListener::bind|UdpSocket::bind|listen_port|0\.0\.0\.0|start_p2p_listener" crates/tylluan-kernel/src crates/tylluan-link/src` y `rg -n "host:|server:" dashboard/vite.config.ts`.

**Conclusión de red:** por defecto el kernel es loopback; las tres formas de ampliar la superficie son explícitas: `[nexus] host`, `[p2p] enabled`, `[mdns] advertise/discover` y `[tunnel]`. La regla “nunca `0.0.0.0` + `dev_mode=true`” está codificada en `main.rs` líneas 423-458 (rechazo) y `config.rs` líneas 2383-2396 (normalización). El riesgo residual es `dev_mode=false` + `0.0.0.0` con un token débil o filtrado, y el bridge WSL.

---

## 2. Cadena de suministro (dependencias directas de ejecución)

| Ecosistema | Dependencia | Versión fijada | Lockfile/fuente | Toca descompresión o parseo de datos externos |
|---|---|---|---|---|
| Rust | `axum` | 0.8.9 | `Cargo.lock` | HTTP/WebSocket, JSON/body parsing |
| Rust | `tokio` | 1.54.1 | `Cargo.lock` | Runtime de red |
| Rust | `reqwest` | 0.12.28 | `Cargo.lock` | HTTP saliente (TLS, JSON, stream) |
| Rust | `rusqlite` | 0.33.0 | `Cargo.lock` | SQLite (DB local, no red) |
| Rust | `rmcp` | 0.1.5 | `Cargo.lock` | JSON-RPC MCP |
| Rust | `fastembed` | 5.8.0 | `Cargo.lock` | Descarga/carga de modelos ONNX |
| Rust | `ort` / `ort-sys` | `=2.0.0-rc.10` | `Cargo.lock` + pin exacto | Carga ONNX; `load-dynamic` |
| Rust | `tar` | 0.4.46 | `Cargo.lock` | **Descompresión/parseo de archivos tar** (updater/instalador) |
| Rust | `flate2` | 1.1.9 | `Cargo.lock` | **Descompresión gzip** de archivos de release |
| Rust | `tower-http` | 0.6.11 | `Cargo.lock` | `compression-br`/`compression-gzip` (respuestas; no descomprime entrada) |
| Rust | `image` | 0.25.10 | `Cargo.lock` | **Parseo de imágenes** externas |
| Rust | `scraper` | 0.22.0 | `Cargo.lock` | **Parseo HTML** de resultados web |
| Rust | `mdns-sd` | 0.13.11 | `Cargo.lock` | **Parseo de paquetes mDNS de la LAN** |
| Rust | `ed25519-dalek` / `snow` | 2.2.0 / 0.10.0 | `Cargo.lock` | Firmas Noise/Ed25519 (no parseo de ficheros) |
| Python | `mcp` / `fastmcp` | `>=1.27.0,<2` / `>=3.2.4` | `guilds/requirements.txt` (rango, **sin lock ni hashes**) | JSON-RPC MCP |
| Python | `psutil` | `>=5.9.0` | `guilds/requirements.txt` (rango) | Sistema local |
| Python | `certifi` | `>=2026.10.19` | `guilds/requirements.txt` (rango) | CAs TLS |
| Python (opcional) | `pypdf`, `pdfplumber`, `Pillow` | `>=4.0.0`, `>=0.11.0`, `>=10.0.0` | `guilds/requirements-optional.txt` (rangos) | **Parseo de PDF/imágenes externas** |
| Node (solo dashboard, no binario) | `react` / `react-dom` | 19.2.7 / 19.2.7 | `dashboard/pnpm-lock.yaml` | Render de UI |
| Node (solo dashboard, no binario) | `vite` | 5.4.21 | `dashboard/pnpm-lock.yaml` | Build/dev server |
| Node (solo dashboard, no binario) | `@tldraw/tldraw` | `^5.2.4` (lock) | `dashboard/pnpm-lock.yaml` | Canvas/UI |

**Observaciones de cadena de suministro:**

- **Rust:** versiones resueltas y fijadas en `Cargo.lock`; `ort`/`ort-sys` tienen pin exacto. El riesgo principal de descompresión está en `tar` + `flate2` (updater/instalador) y en la carga de modelos ONNX de `fastembed`/`ort`.
- **Python:** `guilds/requirements.txt` usa **rangos sin hashes**; no hay lockfile reproducible ni `--require-hashes`. Un `pip install` en el momento del despliegue puede resolver versiones distintas. Es el eslabón más débil de la cadena.
- **Node:** el dashboard no se embebe en los binarios publicados (release compila sin `bundled-dashboard`); sus dependencias afectan al build/dev, no al kernel distribuido. Aun así, `pnpm-lock.yaml` fija versiones para CI.
- **Brotli:** `tower-http` incluye `compression-br` (compresión de respuestas) y `brotli 8.0.4` aparece en el lock; no encontré un punto de **descompresión de Brotli de entrada externa** en el kernel. El instalador usa gzip/tar, no Brotli.
- **CVE:** **no he abierto ninguna CVE en NVD ni en el aviso del proveedor**. No afirmo que una versión sea vulnerable o segura; todo CVE queda **no verificado** en esta pasada. La ola de octubre citada por el TL no se usa como evidencia.

**Comandos reproducibles:** `rg -n '^name = "(axum|tokio|reqwest|rusqlite|rmcp|fastembed|ort|tar|flate2|tower-http|image|scraper|mdns-sd)"' Cargo.lock`; `cargo tree -p tylluan-kernel --depth 1` (solo lectura); `sed -n '1,80p' guilds/requirements.txt`; `rg -n 'react@|react-dom@|vite@|@tldraw' dashboard/pnpm-lock.yaml`.

---

## 3. Modelo de amenazas del pilar Recordar (inyección de recuerdos)

Una política que inyecta recuerdos en el contexto del agente es, por definición, un **canal de entrada al prompt**. InjecMEM y la clase de ataques de memoria envenenada demuestran que una sola interacción puede dirigir respuestas posteriores. El diseño debe asumir que **todo recuerdo es dato no confiable hasta que su envoltorio demuestre lo contrario**.

### Qué debe llevar cada recuerdo inyectado

| Campo | Contenido | Por qué |
|---|---|---|
| `node_id` / `source_id` | Identificador estable del nodo y del productor | Trazabilidad y capacidad de retirar/aislar |
| `origin` | `user_direct`, `agent_generated`, `guild_output`, `federation_peer`, `upload`, `a2a` | El origen decide la confianza; no se deduce del texto |
| `provenance_chain` | Agente/guild/peer que lo escribió y quién lo aprobó | Un recuerdo puede haber pasado por varias manos |
| `untrusted` / `layer1_flag` | Booleano de D-32 y motivo canónico | Un nodo marcado no se inyecta como instrucción |
| `confidence` / `score` | Score de retrieval + calibración; `abstain` si no hay evidencia | Evita inyectar por relleno |
| `cued_by` | Qué pista del turno disparó el recuerdo (query, keyword, entidad) | Explica “por qué ahora” y permite auditar falsos positivos |
| `timestamp` / `valid_until` | Fecha y validez | Evita inyectar recuerdos caducados |
| `integrity` | Hash del contenido + firma/HMAC del envoltorio con la identidad del kernel | Detecta manipulación entre almacén e inyección |
| `scope` | Agente/proyecto/espacio al que pertenece | Evita contaminación cruzada entre agentes |
| `injection_id` | UUID por inyección | Correlaciona el recuerdo con la respuesta y el audit log |

El envoltorio debe ser **estructurado** (JSON con metadatos + texto delimitado), no una concatenación de texto plano. El agente debe ver la procedencia y la marca de desconfianza, no solo el contenido.

### Qué no se debe inyectar nunca

- Contenido con `untrusted_content = true` o `quarantined = 1` **sin sanear y sin banner**.
- Contenido de origen externo (federación, A2A, subida de ficheros, web, correo de agente) que no haya pasado por capa 1 + provenance; “lo dijo un peer” no es confianza.
- Recuerdos con firma/HMAC inválida, hash que no coincide o `source_id` inexistente.
- Recuerdos caducados, borrados, en conflicto no resuelto o con `confidence` por debajo del umbral.
- Texto que el propio agente no pidió y que no tiene una pista explícita: la inyección proactiva sin cue es contexto basura y superficie de ataque.
- Secretos/tokens/datos personales aunque el nodo sea “trusted”: la inyección no es el canal para exfiltración.
- Cualquier recuerdo cuando el presupuesto de contexto o la política de privacidad del agente no lo permiten: **fail-closed**, no “inyectar de todos modos”.

### Política mínima propuesta

1. **Selección:** solo inyectar si hay una pista (`cue`) y el score supera el umbral; si no, no inyectar y registrar el `abstain`.
2. **Sanitización:** capa 1 + provenance antes de inyectar; si el nodo está marcado, se inyecta como **dato citado con banner**, nunca como instrucción.
3. **Firma:** el kernel firma el envoltorio completo (Ed25519/HMAC) y el receptor verifica; sin firma válida, no se inyecta.
4. **Límite:** máximo N recuerdos y M tokens por turno; el exceso se descarta y se cuenta.
5. **Auditoría:** `injection_id`, `source_id`, `cued_by`, score y decisión (inyectado/descartado) en el audit log; el agente puede citar la procedencia.
6. **Control:** interruptor por agente/proyecto y “no proactivo” por defecto; la inyección siempre es explícita y reversible.

---

## NO verificado

- No ejecuté `npx`/`uvx`/`pip install`/`npm install` ni arranqué kernels; la superficie se leyó del código.
- No probé en vivo `host=0.0.0.0`, P2P, mDNS ni el bridge WSL; no verifiqué qué interfaces expone cada uno en esta máquina.
- No abrí NVD/avisos de proveedor: **ningún CVE está verificado**; las versiones de la tabla son del lockfile, no un veredicto de seguridad.
- No verifiqué si `p2p`/`mdns` están activos en la configuración de producción (`tylluan.toml` local sí tiene `[p2p]`, pero no leí su `enabled` para no asumir).
- No medí el impacto real de una inyección de memoria ni probé InjecMEM; el modelo de amenazas es diseño, no una reproducción.

## Impact

- **Superficie:** auditoría de red, cadena de suministro y política de inyección. Sin cambios de código.
- **Red:** el kernel es loopback por defecto; P2P/mDNS/WSL son opt-in y amplían la superficie de forma explícita. El guard `0.0.0.0 + dev_mode=true` está implementado; el riesgo residual es `dev_mode=false` con token débil/filtrado y el bridge WSL.
- **Cadena de suministro:** Rust está lockeado; Python no tiene lock ni hashes (riesgo principal); el dashboard no entra en el binario publicado.
- **Recordar:** una inyección proactiva debe ser un sobre firmado, con procedencia, confianza, cue y auditoría; nunca texto externo sin sanear. Sin esas condiciones, el pilar amplía el vector de memoria envenenada.
- **No depredación:** S-DEEP-1 estaba asignado a Deep; no se tocó el checkout compartido.
