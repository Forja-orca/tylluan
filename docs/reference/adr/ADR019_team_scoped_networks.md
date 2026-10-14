# ADR-019: Redes por equipo — emparejamiento por invitación, permisos por par y por espacio

- **Estado:** 📐 **PROPUESTA para revisión de José.** No implementada. Nada de lo que describe la sección 3 existe todavía en el código, salvo lo marcado como existente.
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
| Lista de comandos permitidos para despacho remoto | **No verificada.** Antes de implementar hay que auditar qué puede ejecutar hoy un par aprobado. |
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
