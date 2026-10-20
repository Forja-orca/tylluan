# PLAN — Página de consentimiento OAuth (F2 fase B) (Deep, 2026-10-19)

**Estado: plan, no implementado.** Es la pieza de cara al humano; la revisa el TL
antes de escribir una línea de código. Nada de lo aquí descrito está en el árbol
de trabajo.

Contexto: hoy `dev_mode=false` hace que `/oauth/authorize` responda **403**
(`oauth.rs`, cierre de T1). Con F2-B esa respuesta pasa a ser una página de
consentimiento que exige la **credencial del operador** (el token maestro
tecleado por el humano en el navegador). `dev_mode=true` mantiene la
auto-aprobación actual sin cambios (R12).

Fuentes: `DESIGN_oauth_operator_2026-10.md` §65-69 y R10 ·
`docs/reference/research/2026-10-19_mcp_autorizacion_requisitos_para_oauth_f2.md`
(consentimiento por cliente, "MUST clearly display the redirect URI hostname",
DCR fuera por deprecado → registro previo). Lo ya entregado en D-2/D-3
descubrimiento, `aud`, `iss` y desafíos de scope.

---

## 1. Pantallas

Una sola página HTML autocontenida servida por el kernel (sin build del
dashboard: headless-first, funciona aunque `dashboard/dist` no exista). Sin
wizards, sin pasos intermedios, sin scroll a 1280×720.

### P1 — Consentimiento (única pantalla con interacción)

```
┌──────────────────────────────────────────────────────────┐
│  Tylluan · autorización de acceso                        │
│                                                          │
│  Un cliente quiere acceder a tu kernel.                  │
│                                                          │
│  Cliente      mcp-remote (id: d4f1…a9c2)                 │
│  Te devolverá a   127.0.0.1:<puerto>     ← HOSTNAME, no  │
│                                            la URL entera │
│  Alcance      mcp — usar las 6 herramientas soberanas    │
│               con el rol de agente configurado           │
│  Servidor     http://192.168.1.20:47004                  │
│  Duración     30 días (revocable en cualquier momento)   │
│                                                          │
│  Credencial de operador                                  │
│  [ •••••••••••••••• ]            (password, autocomplete │
│                                    off, nunca autofill)  │
│                                                          │
│  [ Cancelar ]                            [ Autorizar ]   │
└──────────────────────────────────────────────────────────┘
```

Reglas de la pantalla:

- **Host del `redirect_uri` en grande** y separado de la ruta: el vector real de
  phishing es que el host cambie, no el path. Se muestra el host tal cual
  (`127.0.0.1:<puerto>`), nunca embebido en una frase.
- **Nombre del cliente** viene del registro previo (`client_name`), escapado,
  recortado a 64 caracteres y sin caracteres de control. Si no hay nombre
  registrado, se muestra el `client_id` — nunca "Cliente desconocido" a secas,
  porque oculta que el `client_id` es estable y verificable.
- **Alcance en lenguaje llano**, no `scope=mcp`: "usar las 6 herramientas
  soberanas con el rol de agente configurado". Si en el futuro hubiera más
  scopes, se listan todos, uno por línea, con su frase llana.
- **Cancelar** siempre visible sin scroll, misma jerarquía tipográfica que
  Autorizar, a la izquierda (orden de lectura); Autorizar es el primario.
- Campo de credencial: `type=password`, `autocomplete="off"`, `spellcheck=false`,
  nunca `value` pre-rellenado, nunca `autofocus` (evita que un Enter accidental
  envíe); el foco inicial va al encabezado para lectores de pantalla.
- Sin logos de terceros, sin casillas premarcadas, sin "recomendado", sin
  cuentas atrás que metan prisa.
- Cero JavaScript. Si hace falta JS para algo, es que el diseño está mal.
- Accesibilidad: `lang="es"`, un `h1`, `label` real por campo, `aria-describedby`
  para el error, contraste ≥ 4.5:1, foco visible, `role="alert"` en el error.

### P2 — Autorizado (sin interacción)

302 directo al cliente con `code`, `state` e `iss`. No hay página intermedia. Si
el navegador no sigue el redirect (caso raro), se muestra: "Autorizado. Vuelve a
tu cliente." con un enlace al `redirect_uri`. Nada más.

### P3 — Denegado

302 al cliente con `error=access_denied` y `state`. La página de respaldo dice
solo "Has cancelado. No se ha concedido acceso." Sin detalles del cliente
(no tiene sentido filtrar nada en una pantalla que puede ver cualquiera).

### E1 — Credencial incorrecta

Se re-renderiza P1 con el error inline bajo el campo (`role="alert"`):
"Credencial de operador incorrecta." El cliente, el host y el alcance se
mantienen; la credencial **nunca** se rellena ni se refleja. Tras 5 fallos en
5 minutos por IP (y un límite global más alto), la página pasa a 429 con
"Demasiados intentos. Espera unos minutos." y se registra el intento sin el
valor.

### E2 — Solicitud inválida

`redirect_uri` no loopback, cliente no registrado, PKCE ausente o `resource`
ajeno: **página de error sin redirect** (RFC 6749 §4.1.2.1: jamás redirigir a un
`redirect_uri` no validado). Un párrafo, sin formulario, sin enlace al cliente.

---

## 2. Estados (máquina explícita)

```
GET /oauth/authorize (dev_mode=false)
        │ valida client_id + redirect_uri loopback + PKCE (+ resource/aud)
        ▼
   PENDING ──(5 min sin decisión)──────────────► EXPIRED  (400, reintentar desde el cliente)
        │
        ├──(POST + CSRF ok + credencial ok + Autorizar)──► APPROVED ──(code canjeado 1 vez)──► CONSUMED
        │                                                        └──(60 s sin canje)──────────► EXPIRED
        ├──(POST + Cancelar)────────────────────────────► DENIED   (302 error=access_denied)
        ├──(POST + credencial mala)×5 en 5 min──────────► LOCKED   (429, ventana deslizante)
        └──(POST sin CSRF / CSRF reusado)───────────────► rechazo 400, estado intacto
```

- El `code` sigue siendo de un solo uso y de vida corta como hoy; el
  consentimiento **no** alarga la vida del `code`.
- `PENDING` se guarda en memoria del proceso (igual que `PendingCode` hoy): un
  reinicio del kernel invalida solicitudes a medias, que es lo correcto.
- `APPROVED`/`DENIED`/`LOCKED` se registran en el audit log existente
  (append-only), una línea por decisión.

---

## 3. Qué se recuerda y dónde

### 3.1 Consentimiento por cliente — sí se recuerda

Fichero `consents.json` en el directorio de datos del kernel (mismo criterio que
el resto de estado pequeño; si el TL prefiere tabla en `silva.db`, cambia el
soporte, no el contenido):

```json
{
  "version": 1,
  "consents": [
    {
      "client_id": "d4f1…a9c2",
      "client_name": "mcp-remote",
      "redirect_uri": "http://127.0.0.1:<puerto>/callback",
      "scopes": ["mcp"],
      "granted_at": "2026-10-19T21:40:12Z",
      "last_used_at": "2026-10-19T21:40:12Z",
      "expires_at": "2026-11-18T21:40:12Z",
      "revoked_at": null
    }
  ]
}
```

- **Clave**: `client_id`. Un consentimiento no cubre a otro cliente, aunque el
  `redirect_uri` coincida.
- **Se guarda también el `redirect_uri` completo**, no solo el host: si el mismo
  `client_id` aparece con otro `redirect_uri`, se exige consentimiento nuevo. Es
  el vector de phishing que la spec señala (el host se muestra, pero el path
  puede cambiar el destino).
- **Se guardan los scopes concedidos**: si mañana un cliente pide más, se
  re-consiente; si pide menos, se sirve lo concedido.
- **`expires_at`**: 30 días deslizante (se renueva al usarse). Pasado el plazo,
  vuelve P1 con la credencial (no con una casilla "recuérdame").
- **Revocación**: `revoked_at` no borra la fila (auditoría); el cliente
  revocado vuelve a P1 en el siguiente authorize.
- **Nunca se guarda**: la credencial de operador (en ninguna forma, ni hash, ni
  sesión de navegador), la IP del humano, el contenido de memoria, ni la
  relación entre la persona y un agente concreto.

### 3.2 Revocación — decisión pendiente del TL

Propuesta: `POST /api/v1/oauth/consents/:client_id/revoke` y
`POST /api/v1/oauth/consents/revoke-all`, ambos clase operador (token maestro
por cabecera). Es un endpoint **nuevo**, por eso no lo doy por hecho: si el TL
prefiere, la v1 del consentimiento se limita a editar `consents.json` a mano y
el endpoint llega en otra fase.

### 3.3 Cookies

Una sola cookie de sesión del formulario, `tylluan_consent`, `HttpOnly`,
`SameSite=Lax`, `Secure` cuando el origen sea https, host-only, vida = la del
`PENDING` (5 min). Contiene solo el `consent_id` y el nonce CSRF ligado; no
contiene credencial ni scopes. Se borra al decidir.

---

## 4. Presupuesto de fricción

| Momento | Acciones humanas | Objetivo |
|---|---|---|
| Primera conexión de un cliente | 2 (pegar credencial + clic Autorizar) | ≤ 2 acciones, ≤ 20 s |
| Conexiones siguientes (mismo cliente, mismo `redirect_uri`, mismo scope, dentro de 30 días) | **0** | auto-aprobación por consentimiento recordado |
| Consentimiento caducado / revocado | 2 | igual que la primera vez |
| Cancelar | 1 clic | cancelar nunca cuesta más que autorizar |

- Un solo campo de texto en todo el flujo.
- Cero llamadas a un LLM en el flujo: la decisión es humana y la prueba de que
  es el operador es la credencial, no un juicio de modelo. Esto también evita
  que un prompt inyectado en el `client_name` influya en nada.
- Cero dependencias de red: la página no carga fuentes ni scripts externos
  (CSP `default-src 'none'; style-src 'nonce-…'; img-src 'none'`).
- Objetivo medible en test de UI: 1 página, 1 formulario, 1 campo, 2 botones,
  0 scroll a 1280×720, 0 JS.

---

## 5. Seguridad del formulario (lista de comprobación para la implementación)

1. **CSRF**: nonce de un solo uso ligado al `consent_id`, generado al renderizar
   y verificado en el POST; reuso o ausencia → 400 sin cambiar el estado.
2. **Credencial**: comparación en tiempo constante (la misma que ya usa el
   bearer), sin eco en HTML/logs, límite de intentos por IP y global, y
   `Cache-Control: no-store` en todas las respuestas de la página.
3. **Escapado**: todo dato del cliente (`client_name`, `client_id`,
   `redirect_uri`) se escapa como texto; el host se renderiza como texto plano,
   nunca como enlace (un enlace con `javascript:` sería el fallo clásico).
4. **Clickjacking**: `X-Frame-Options: DENY` + `frame-ancestors 'none'`.
5. **Sin redirect en errores de validación** de `client_id`/`redirect_uri`/PKCE.
6. **Escalada**: la página **no** puede conceder la clase operador. Si un
   cliente pide `scope=operator`, se responde `invalid_scope` sin renderizar
   nada. Un JWT nunca debe satisfacer `operator_class_required` (ADR-019), y
   este formulario no abre esa puerta.
7. **Auditoría**: una línea por decisión (aprobado/denegado/bloqueado/caducado)
   con `client_id`, host del `redirect_uri`, scopes, decisión y hora. Sin
   secretos, sin IPs.
8. **Multi-instancia**: `consents.json` se lee y escribe con escritura atómica
   (temp + rename) y se re-lee al arrancar; si dos procesos comparten el
   fichero, gana la última escritura completa (documentado, no se pretende
   consenso).

---

## 6. Tests que escribiré cuando se apruebe (no ahora)

- Máquina de estados: PENDING→APPROVED→CONSUMED; PENDING→DENIED; PENDING→EXPIRED;
  5 fallos→LOCKED; POST sin CSRF no cambia estado; `code` de un solo uso.
- Render: el host del `redirect_uri` aparece en el HTML; `client_name` con
  `<script>` sale escapado; una petición con `scope=operator` no renderiza P1.
- Consentimiento recordado: segundo authorize del mismo cliente no pide
  credencial; cambio de `redirect_uri` con el mismo `client_id` sí la pide;
  consentimiento caducado vuelve a P1; revocado vuelve a P1.
- Credencial: mala → error inline y contador; buena → 302 con `code`+`state`+`iss`.
- Cabeceras: `Cache-Control: no-store`, `X-Frame-Options: DENY`, CSP presente.
- Fricción: el HTML no contiene `<script>` ni recursos externos.

---

## 7. Decisiones que necesito del TL antes de implementar

1. **Almacenamiento**: ¿`consents.json` (propuesto) o tabla en `silva.db`?
2. **Plazo**: ¿30 días deslizante (propuesto) o solo duración de sesión?
3. **`scope=operator`**: confirmo que este formulario **nunca** lo concede
   (rechazo con `invalid_scope`). ¿De acuerdo, o quieres un flujo separado?
4. **Revocación**: ¿entra en esta fase el endpoint de revocar, o v1 solo con el
   fichero editable?
5. **Credencial en el navegador**: no la recuerdo en ninguna forma (propuesto).
   Si quieres "recuérdame", cambia el modelo de amenaza y lo replanteamos.

## 8. Lo que este plan deliberadamente NO hace

- No implementa nada (por instrucción de la cola: para aquí).
- No añade scopes nuevos ni toca los ya emitidos (`mcp`).
- No toca el flujo `dev_mode=true`.
- No introduce DCR (deprecado; se asume registro previo / cliente conocido).
- No añade dependencias: HTML generado en Rust, cero JS, cero fuentes externas.
