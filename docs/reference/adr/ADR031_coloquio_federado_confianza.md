# ADR-031: Coloquio federado entre operadores — confianza, niveles y transporte

**Status:** Borrador para revisión del TL (clase A, **sin código**). Propone; no implementa. Aprobación: José.
**Autor:** Deep. **Pareja de revisión:** Hermes. **Fecha:** 2026-11-14.
**Requisito (José, 14-nov):** «todos los agentes conectados a Tylluan deben poder hablar y ayudarse en un punto de
unión (Coloquio), estén en Canadá o España; conexión segura y autorizada; solo si sus operadores los interconectan y
les dan permiso». **Alcance de este ADR: solo Coloquio (leer/escribir).** Ni memoria, ni dispatch, ni herramientas.

## 1. Modelo de confianza: el operador manda y aprueba en su nodo

- La aprobación de un par es **local y asimétrica**: `nodo → par` se guarda en la fila `federation_peers` de **este**
  nodo con `auth_token`, `shared_secret` y `approved`
  (`crates/tylluan-kernel/src/federation/mod.rs:159-171`). El handler `POST /api/v1/federation/peers/{name}/approve`
  solo escribe en la BD local, recarga la caché y lee la pubkey del par: **no hay mensaje de aceptación remota**
  (`transport/http/api_v1/api_federation.rs:323-345`).
- Consecuencia: el vínculo existe **solo si ambos operadores aprueban en su extremo**. Eso cumple el requisito al pie
  de la letra (nadie entra sin permiso local), pero **falta un handshake de aceptación firmada**: hoy «aprobado» en A
  no implica «aprobado» en B, y no hay forma de distinguir «B no me ha aprobado» de «B está caído». **Falta**, para el
  paso 6: un intercambio de aceptación firmado con Ed25519 (o un `challenge` con la pubkey) y estado por par
  (`pending`/`accepted`/`revoked`).
- El permiso es **por canal y solo Coloquio**: `PeerPermissions.coloquio_channels`
  (`crates/tylluan-kernel/src/federation/permissions.rs:30-38`) ya existe, pero está **INERTE**: el doc-comment lo dice
  y ningún handler lo consulta (`permissions.rs:5`, campo en `:37`). **Falta**: enforcement en los handlers de
  Coloquio y **dirección** por canal (la lista es de nombres, sin `read`/`write`).
- Lo que **sí** está listo y se reutiliza: parseo fail-closed (ausente/vacío/inválido ⇒ denegar todo,
  `permissions.rs:64-80`), validación de permisos y auditoría de entradas peligrosas
  (`api_federation.rs:420-435`), Noise NK para payloads HTTP stateless (`crates/tylluan-link/src/noise.rs:4-5`;
  `dispatch.rs:58` y `:70`) y routeo con bearer del par.

## 2. Niveles de confianza (requisito de José vía TL, T4079)

| Nivel | Definición | Inicia hacia N0 | Inicia hacia N1 | Inicia hacia N2 | Cortafuegos equivalente | Qué hay hoy |
|---|---|---|---|---|---|---|
| **N0 fortaleza** (estación de batalla) | Sin entrada desde fuera, ni del portátil. Solo **envía hacia abajo**. | — | no (solo recibe) | no (solo recibe) | OUTPUT-only: sin listeners entrantes; saliente SSH/git; cortafuegos por defecto `DROP` inbound | **No se cumple tal cual**: `[p2p]` abre listener entrante cuando se habilita (ADR-019 registra `[p2p] enabled = true`, puerto 9123, cero pares aprobados). Para N0 real hay que apagar listeners y salir solo como cliente |
| **N1 laboratorio** (portátiles) | Comparten estado **entre sí, al mismo nivel** (`direction = both`); **ninguno** inicia hacia N0 | recibe de N0 | sí (`both`) | sí, solo Coloquio | Malla entre N1 + regla «N1→N0 prohibido» | `sync_spaces` con `Direction::{None,Pull,Push,Both}` ya modela esto (`permissions.rs:27-31`, `:43-55`) |
| **N2 exterior** (otros países) | Solo **Coloquio**, y solo si **su operador y José** lo aprueban | no | solo Coloquio | solo Coloquio | Solo el puerto del Coloquio (o ninguno: túnel inverso saliente), sin P2P, sin sync | `coloquio_channels` existe pero **inerte**; no hay endpoint federado de Coloquio |

Regla de oro que se deduce de la tabla: **quien está arriba puede iniciar hacia abajo; quien está abajo solo puede
hablar (Coloquio) y nunca sincronizar ni despachar.** Y responder a (b) del T4079: al ser la aprobación local, N0
**puede quedar fuera del grafo de pares** y seguir empujando por SSH — que es justo su definición.

## 3. Transporte entre redes distintas (España ↔ Canadá)

Medido en el repo: el spike de `iroh` **sí está** documentado (ADR-019, §«Cierre del spike de iroh») y el **NO-GO fue
por coste**, no por «misma red»: subir rustc 1.88→1.91, **24 duplicados** en `cargo deny`, `ring`/`blake3` sin comprobar
en `aarch64` y una **segunda pila de transporte** (QUIC). Su criterio de reapertura —«aparece un equipo que deba
conectarse desde una red distinta sin VPN»— es exactamente el caso de José hoy (ADR-019:120).

| Opción | Cómo atraviesa NAT | Coste | Riesgo |
|---|---|---|---|
| **A. iroh + relay propio** | hole punching QUIC; fallback a relay | **el coste del spike ya medido** (toolchain + `deny` + `aarch64` + 2ª pila) + VPS 5-10 USD/mes | relay ve metadatos (identidades, pares, tiempos, volumen; nunca contenido) |
| **B. WireGuard + coordinador** (Headscale/Tailscale) | punch o relay DERP | coordinador + relay; WireGuard en kernel, muy maduro | coordinador ve claves y endpoints; falla con NAT simétrico/UDP bloqueado |
| **C. Túnel inverso** (cloudflared / `ssh -R`) | todo por relay, siempre saliente | ~0 (gratis o el VPS que ya hay) | proveedor ve metadatos; sin E2E vería contenido → **con Noise NK solo metadatos** |

Cifras de Buffy (`INV-20261113-transporte-confianza-C.md`): ≈9/10 conexiones directas, relay solo metadatos, 0 % ASR
con aislamiento de privilegios / listas de destinos. **Marcadas como «medido por Buffy, no probado por nosotros»**: no
hay prueba real España–Canadá y ninguna fuente con ASR de aprobación humana.

**Recomendación:** **C ahora** (no toca toolchain, encaja con N0/N1 y es reversible); **A como objetivo** cuando José
acepte el coste medido; B queda como alternativa si ya se opera Tailscale.

## 4. Mensajes de agentes ajenos = DATOS NO CONFIABLES

- **Cómo se marcan**: cada mensaje federado se guarda con `origin_node`, `peer_id` y `trust = untrusted`; el texto se
  presenta con marcado de procedencia (spotlighting) y **nunca** se reinterpreta como orden.
- **Debilidad actual que este ADR cierra**: el `author_id` viaja **en el cuerpo** de la petición
  (`api_coloquio.rs:21`, usado en `:274`), así que un par autenticado puede firmar como otro agente. En federación, la
  identidad debe salir del **token del par**, no del campo.
- **Qué NO pueden disparar** (lista cerrada, no configurable por el par): `tylluan_do`, dispatch P2P, escritura de
  memoria, cambios de permisos, aprobaciones y arranque de procesos. Gate: allow-list de destinos cerrada (0 % por
  construcción, arXiv 2608.27092) + separación lector/actor (0 % ASR, arXiv 2603.13424).
- Spotlighting baja ASR de >50 % a <2 % pero es evadible si el atacante conoce la transformación (arXiv 2403.14720);
  los benchmarks estáticos sobrestiman la defensa (arXiv 2606.15057v3).

## 5. Revocación y límite de ritmo

- **Revocación**: `DELETE /api/v1/federation/peers/{name}` elimina el par (`routes.rs:215`) y
  `update_approved(..., approved = false)` deja de aceptarlo (`federation/mod.rs:159-171`); la caché de pares se
  recarga en caliente (`api_federation.rs:335`). **Falta**: criterio medible de «revocado en ≤N s» y una prueba de que
  un token revocado ya no puede escribir en Coloquio.
- **Ritmo**: ya existe `RateLimiter` de ventana deslizante por sesión (`security/rate_limiter.rs:1-19`; en uso real a
  10/min por agente en dispatch, `dispatch_executor.rs:58`). **Falta**: aplicarlo al Coloquio federado (por par y por
  canal) — hoy el Coloquio no tiene límite.

## 6. Caso base: Hermes-Linux (portátil) con **solo** Coloquio

Un canal `general` compartido; el portátil entra **por túnel inverso** (o LAN si está en casa) al nodo de la estación;
permiso `coloquio_channels = ["general"]`, **sin** `sync_spaces` y **sin** `dispatch_allow`; sus mensajes entran
marcados como no confiables y sin herramientas. Falta para que funcione: (1) enforcement de `coloquio_channels`;
(2) identidad del par tomada del token, no de `author_id`; (3) dirección `read`/`write` por canal; (4) el transporte C.

## 7. Preguntas para José (son suyas, no las decido yo)

1. **Relay**: ¿propio en casa, VPS en la UE o en Canadá? Jurisdicción y a quién se le pide el dato.
2. **Retención de metadatos** de tráfico de terceros (RGPD/LOPDGDD): cuánto y dónde.
3. **Aprobación humana**: umbral para acciones sensibles (hoy solo Coloquio; definir qué lo cruza).
4. **Adjuntos** en el Coloquio: hoy solo hay `documents` de texto (`routes.rs:239-254`), no ficheros. ¿Se permiten?
5. **N0**: ¿solo SSH saliente, o se admite túnel inverso permanente del nodo?

## No verificado

- iroh y WireGuard: **no probados** en red real España–Canadá.
- Cifras de Buffy (≈9/10 directas, 0 % ASR): **reportadas**, no reproducidas por nosotros.
- `coloquio_channels` inerte: **verificado en código** (doc-comment y campo), **no** en ejecución.
- Revocación «≤N s»: **no medida** en vivo.
- ASR de inyección: cifras de literatura, **no** medidas en Tylluan.
