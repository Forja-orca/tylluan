# ADR-019: Redes por equipo — emparejamiento por invitación, permisos por par y por espacio

- **Estado:** ✅ **APROBADA por José el 2026-10-14** (las tres decisiones de la sección 7, abajo). **No implementada**: nada de lo que describe la sección 3 existe todavía en el código, salvo lo marcado como existente.
- **Fecha:** 2026-10-14
- **Origen:** decisión de José (2026-10-13): la malla global queda fuera; Tylluan se conecta en redes **por equipo**, con un emparejamiento simple y muy seguro (LAN o enlace de invitación). Antes de borrar nada, se analiza.
- **Sustituye en la práctica a:** `PROPOSAL_distributed_inference_credit_mesh.md` (mercado de créditos) y a la ambición de descubrimiento global por DHT mainline. No se borran; se marcan como registro.
- **Nota de numeración:** ADR-014 ya está ocupado (`ADR014_capability_provider_fabric.md`); este es el 019.

---

## 1. Qué decidió José y qué no

Decidido por José: sin malla global; redes de equipo; emparejamiento simple y ultraseguro.

**No decidido todavía** (lo que este ADR propone y José debe aprobar o cambiar): el modelo de permisos, el uso de `iroh`, la federación de Coloquio. Ninguno de esos tres puntos es una decisión tomada.

## 2. Estado real del código (verificado 2026-10-14)

| Pieza | Estado |
|---|---|
| Pares con aprobación manual | **Existe.** `federation_peers.approved INTEGER NOT NULL DEFAULT 0` (`federation/mod.rs:64`), con `auth_token`, `shared_secret` y `ed25519_pubkey` por par. |
| Cifrado de sincronización | **Existe.** Noise NK cuando se conoce la clave pública del destino; ChaCha20 como alternativa (`federation/mod.rs:206-220`). |
| Despacho P2P entre pares | **Existe.** Noise XK en `tylluan-link` (`noise.rs:18`). |
| Descubrimiento en LAN | **Existe, apagado por defecto.** mDNS (`config.rs`, `MdnsConfig`). |
| Descubrimiento global | **Apagado en todas las releases.** DHT mainline (`mesh.mainline_dht_enabled = false`, sin feature compilada). |
| Atravesar NAT | **No funciona.** El cliente STUN de `nat.rs` no se usa y no hay relay. |
| Emparejamiento por invitación | **No existe.** Hoy se añade un par a mano con URL y secreto compartido. |
| Permisos por par y por espacio | **No existe.** Un par aprobado sincroniza según la configuración global de federación. |
| Lista de comandos permitidos para despacho remoto | **No existe (verificado 2026-10-14, paso 1).** El handler P2P (`transport/http/mod.rs:1005-1037`) pasa `req.guild` y `req.tool` directo a `registry.call_tool`, sin allow-list ni ACL; un par aprobado puede invocar cualquier gremio, incluido `bash`. El propio código lo admite (`config.rs:435-438`). El listener P2P está apagado por defecto (`config.rs:445`). |
| Federación de Coloquio | **No existe.** |

## 3. Propuesta

### 3.1 Emparejamiento por invitación
- El nodo A genera una invitación de **un solo uso** y con caducidad corta (propuesta: 10 minutos) que contiene su clave pública Ed25519, su dirección y un secreto aleatorio.
- La invitación viaja por un canal que José controla (QR en LAN, enlace pegado). El nodo B la canjea; ambos muestran una huella corta (6 palabras) para confirmación humana en los dos lados.
- Tras el canje, la invitación se destruye. Un segundo uso falla.
- La clave de cada par queda fijada (pinned). Si cambia, el par pasa a no aprobado.

### 3.2 Permisos por par y por espacio
Cada par aprobado tiene una lista explícita de **lo que puede hacer**, vacía por defecto:
- `sync_spaces`: qué espacios de memoria se comparten con él (por defecto ninguno).
- `direction`: `none` | `pull` | `push` | `both` por espacio.
- `dispatch`: lista de **herramientas permitidas** para despacho remoto. **Por defecto, denegar todo** (deny-by-default). Una herramienta que no está en la lista no se ejecuta, aunque el par esté aprobado.
- `coloquio`: canales compartidos con ese par (por defecto ninguno).

### 3.3 Revocación
- Revocar un par lo marca no aprobado, borra `shared_secret` y cierra sus sesiones abiertas de inmediato.
- Los datos que ya recibió no se pueden recuperar; el ADR no promete lo contrario.

### 3.4 Coloquio federado
Opt-in por canal y por par, con `federation_source` para evitar eco. Un mensaje recibido de un par no puede invocar herramientas ni despertar agentes locales; es texto inerte, igual que un recuerdo federado hoy pasa por el Coherence Gate.

### 3.5 Transporte: dos caminos, uno se elige por medición
- **Camino base (sin dependencias nuevas):** redes que el equipo ya controla (LAN, VPN propia, Tailscale/WireGuard del usuario) + emparejamiento de 3.1. Cubre el caso «mismo equipo, misma red».
- **Camino iroh (opcional, solo si el spike pasa):** conexión directa con salto a relay cuando hay NAT.

## 4. Spike de `iroh` (asignado a Deep; no toca código de `main`)

Se adopta solo si pasan **S1 y S5** y se cumple S2 y S4. Si no, camino base.

| # | Prueba | Umbral para aprobar |
|---|---|---|
| S1 | Reutilizar la identidad Ed25519 (`identity.key`) como identidad de iroh | Misma clave, sin segunda identidad |
| S2 | 20 intentos de conexión entre dos redes distintas | Cada intento conecta, directo o por relay; se registra cuántos van directos |
| S3 | Relay autoalojado | El relay no ve contenido en claro (captura de tráfico) |
| S4 | Coste | Binario ≤ +10 %; compila en Linux, Windows y `aarch64`; `cargo deny` limpio |
| S5 | Clave desconocida | Se rechaza antes de entregar ningún dato de aplicación |

Los umbrales son propuestos por el TL, no medidos; José puede cambiarlos antes de lanzar el spike.

## 5. Riesgos y lo que este ADR no resuelve

- Un par aprobado y malicioso sigue pudiendo enviar memoria envenenada. El Coherence Gate la filtra al recuperar, pero no es una garantía.
- Sin sandbox por defecto (`SandboxConfig.enabled = false`), el despacho remoto de herramientas de shell es el mayor riesgo; por eso el valor por defecto es denegar.
- iroh añade un relay como pieza nueva; si falla S3, no se adopta.
- La huella de 6 palabras protege contra suplantación solo si las personas la comparan de verdad.

## 6. Orden de trabajo propuesto (cada paso se cierra antes del siguiente)

1. Auditar qué puede hacer hoy un par aprobado (despacho y sincronización). Informe, sin código.
2. Permisos por par y por espacio, con denegación por defecto y tests de rechazo.
3. Invitación de un solo uso y fijación de clave.
4. Revocación inmediata.
5. Spike `iroh` (en paralelo desde ya, sin tocar `main`).
6. Coloquio federado, solo tras 2 y 4.

## 7. Decisiones que necesita José

1. ¿Apruebas el modelo de 3.2 (todo vacío por defecto, despacho denegado)?
2. ¿Apruebas lanzar el spike de `iroh` con los umbrales de la sección 4?
3. ¿Coloquio federado entra en este ciclo o se aparca hasta cerrar 2 y 4?

### Resolución (2026-10-14, José)

1. **Modelo de permisos de 3.2: aprobado** (todo vacío por defecto, despacho denegado). El paso 1 del orden de trabajo (auditar qué puede hacer hoy un par aprobado) es lo primero, antes de cualquier código.
2. **Spike de `iroh`: se lanza**, con los umbrales de la sección 4, sin tocar `main`.
3. **Coloquio federado: aparcado** hasta cerrar los pasos 2 y 4.

Los umbrales S1-S5 siguen siendo propuestos por el TL y no medidos; el spike los mide.

### Resultados parciales (2026-10-14)

**Paso 1 (auditoría de capacidades de un par aprobado) — hecho, verificado por el TL.** Hallazgos con `fichero:línea` sobre `a6f45e3`:
- La aprobación es un booleano (`federation_peers.approved`, `federation/mod.rs:64`); no hay permisos por par.
- La exportación entrega todo nodo con `shareable = 1` (`memory/silva/anchors.rs:166-169`) y la recepción inserta en la tabla `nodes` sin partición (`api_federation.rs:365`).
- El despacho P2P entrega `req.guild` y `req.tool` a `registry.call_tool` sin lista de permitidos (`transport/http/mod.rs:1005-1037`); el código lo admite en `config.rs:435-438`. `bash` admite `python` y `node` (`guilds/builders/plugins/bash.py:77-95`) y el sandbox está apagado por defecto (`config.rs:1560-1563`).
- Estado de producción en el momento de la auditoría: `[p2p] enabled = true` (puerto 9123, escuchando) con **cero pares aprobados**, por lo que el conjunto aprobado está vacío y el listener rechaza a todos (falla cerrado). El riesgo aparece en cuanto se aprueba un par.

**Spike de `iroh` — S1, S5 y la parte de tamaño de S4 medidas; S2 y S3 pendientes.** S1 y S5 pasan; el binario crece +1,03 %. Precondiciones: iroh 1.x exige rustc 1.91 (el repo fija 1.88) y `cargo deny` falla por 24 versiones duplicadas (requiere `skip` razonados). Decisión pendiente de José. No se adopta nada hasta medir S2 (dos redes distintas) y S3 (relay sin ver contenido).

### Cierre del spike de `iroh`: NO-GO por ahora (2026-10-15, José)

José respondió a la pregunta que decidía el valor de `iroh`: el uso real es **misma red, LAN o su propia VPN**, y el camino base basta. `iroh` solo aporta el caso de equipos en redes distintas con NAT, que hoy no existe. Se cierra el spike sin medir S2 ni S3 y no se cambia la toolchain ni `deny.toml`.

Datos que quedan como registro (medidos por Deep, rama local `spike/iroh`, nada en `main`): S1 pasa, S5 pasa, binario +1,03 % con rustc 1.91.

Coste que se evita: subir rustc 1.88 a 1.91 en repo y CI, 24 versiones duplicadas en `cargo deny` (entre ellas `ed25519-dalek` 3.0 en pre-lanzamiento), código nativo nuevo (`ring`, `blake3`) sin comprobar en `aarch64`, y una segunda pila de transporte (QUIC) junto a Noise.

**Se reabre solo si** aparece un equipo que deba conectarse desde una red distinta sin VPN. Antes habría que resolver: que S5 desactive los relays y el descubrimiento públicos por defecto de iroh, medir el tamaño con el uso real del endpoint y compilar en `aarch64` en CI.

El paso 5 del orden de trabajo queda cerrado. Los pasos 2, 3 y 4 siguen adelante sobre el camino base.

