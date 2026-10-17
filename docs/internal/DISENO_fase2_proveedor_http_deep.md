# DISEÑO — Fase 2 del servicio de decisión: proveedor HTTP `/v1/systemone` por el Inference Gateway

- **Estado: PROPUESTA — solo diseño, cero código. Espera el OK del TL antes de implementar** (encargo directo, Coloquio T1227).
- **Autor:** Deep. **Fecha:** 2026-10-16. **Base verificada:** `origin/main` `8c9287b`.
- **Alcance:** lo que ADR-020 §6 llama *"Fase 2 (Decisión y Gobernanza): Adaptador `/v1/systemone`, perfiles de configuración por función, integración con `InferenceBudget` y auditoría"*, acotado al **camino de decisión**. Nota de numeración: ADR-021 §9 llama "Fase 1" a este mismo entregable (primer proveedor de modelo local, contratos en `shadow`). Este documento usa el número del encargo.

---

## 0. Resumen ejecutivo

| Pieza | Propuesta | §  |
|---|---|---|
| Config | `[decision.provider]` como tabla serde con `kind`; la forma string actual (`provider = "rules"`) queda byte-idéntica | §2 |
| Transporte | `reqwest` ya presente (sin dependencias nuevas); cliente aislado en `router/decision/` ahora, absorbido por el harness de ADR-020 Fase 1 cuando aterrice | §3 |
| Timeout | Dos carriles: consultas libres **síncronas** con el deadline del wire; contratos en sombra **asíncronos** (30 s por defecto) — la recolección de pares no puede morir en el presupuesto interactivo de 2 s | §4 |
| Degradación | Todo fallo cae a reglas / abstención explícita; **el modelo nunca revierte un rechazo determinista** (validación, opciones cerradas, abstención de reglas) | §5 |
| Privacidad | Fase 2 solo loopback (`ProviderLocality::Local`): clínico jamás sale de la máquina. Endpoint no-loopback se **rechaza en config** (no existe aún la lista LAN de ADR-020). `/v1/decisions` en nube: clínico **nunca**, entrega posterior y deshabilitado | §6 |
| `/v1/decisions` | Adaptador diseñado (mapeo de dialecto), **no implementado** en esta fase; entrega condicionada a la maquinaria de sensibilidad | §7 |
| Auditoría | Escritura en dos tiempos sobre las 11 columnas de v28 (`INSERT` reglas → `UPDATE` modelo); propuesta de migración v29 con `model_identity` + `model_latency_ms` (decisión del TL, alternativa sin migración documentada) | §8 |

---

## 1. Estado real verificado (HEAD `8c9287b`)

**VERIFICADO en el árbol:**

- El trait de decisión es el simple de ADR-018: `DecisionProvider::decide(&DecisionRequest) -> HashMap<String, DecisionAnswer>` + `provider_id()` — `crates/tylluan-kernel/src/router/decision/mod.rs:78`. `DecisionRequest{state: Value, questions: HashMap<String, DecisionQuestion>}`; `DecisionQuestion::{Choice{options}, Score{levels}, Noul}` (L22-38).
- El servicio **no tiene proveedor de modelo**: `DecisionService::new` instala siempre `RulesDecisionProvider`; `[decision] provider` solo admite `"rules"` y cualquier otro valor avisa y cae a reglas — `router/decision/service.rs:73-76` y `:104-108`.
- Preguntas libres (sin contrato) hoy se abstienen: `abstained=true`, `abstain_reason="no_model_provider"`, `status=degraded` — `service.rs:127-146`.
- El log de sombra ya existe con las 11 columnas canónicas de v28 y escribe `model_value`/`model_probs_json`/`agreed` como `NULL` — `service.rs:230-259`; lectura `read_shadow_log` L262-293. Constantes del wire (deadline 2000 ms por defecto, techo 10000) — `router/decision/api_types.rs:38-39`; validación L193-266.
- Config actual: `DecisionConfig{enabled, default_mode, contracts, provider: String, max_questions, max_state_bytes, shadow_log}` — `crates/tylluan-kernel/src/config.rs:1874-1934` (`provider` L1892-1893).
- **La tabla es la fuente de verdad para la evaluación pareada**: `crates/tylluan-evals/src/paired_eval.rs` lee `decision_shadow_log` en solo-lectura **por nombre de columna** (L79-84), une por `request_hash` con una verdad externa `{hash: valor}`, y trata `model_value NULL` como **fallo del modelo** (abstención conservadora, L330-334). Su fixture fija `agreed` como `1/0/NULL` y `provider` admisible como `'rules+model'` (L824, L831).
- **El cliente HTTP de referencia ya existe y está probado contra servidor real**: `SystemOneDecisionProvider` en `crates/tylluan-kernel/examples/decision_fabric_spike.rs:1475-1597` — `POST {endpoint}/v1/systemone` con cuerpo `{model, state, questions{name:{type,instructions,criteria}}}` (L1528), respuesta `{answers:{name:{probabilities, noul?}}}` (L1461-1471). Mapeo puro de preguntas L1611-1661; distribución con `noul` tipado preferente y clamp `[0,1]` L1647-1661. Timeout del arnés: 120 s (L1487). Lecciones T1004 incorporadas: `instructions` obligatorio (el servidor devuelve HTTP 400 sin él), una pregunta inválida **no** debe tumbar el resto del lote, y el servidor puede omitir preguntas concretas sin invalidar las demás (L1505-1526, L1559-1590).
- El presupuesto de inferencia existe: `InferenceBudgetConfig{permits=1, max_queue_wait_secs=0}` (`crates/tylluan-kernel/src/memory/inference_budget.rs:66-101`), colgado de `[inference.budget]` (`config.rs:754-760`), con semáforo real y métricas (L141-151). DEFAULT = legacy (espera ilimitada; el rechazo es opt-in).
- `reqwest 0.12` ya está en el árbol (`tylluan-kernel/Cargo.toml`, `rustls-tls`, `json`, sin OpenSSL) — reutilizado por el spike.
- **No existe** `[[inference.allowed_lan_endpoints]]` en código (0 apariciones en `crates/`): la lista LAN de ADR-020 §3.6 es diseño, no realidad.

**MEDIDO (Fase 0, datos propios):**

- Variante B (SystemOne HTTP + clef-flash, `llama-server -hf ggml-org/Clef-Flash-GGUF --port 8080`, release b11507) — cerrada **NO-GO con dato real el 2026-10-08**: pasa accuracy/calibración pero **falla latencia por 27×** el presupuesto de 500 ms de la banda System-One (`ROADMAP.md:287`, `docs/roadmap/ROADMAP_O3.md:41`). José descartó clef-flash por peso (9 GB GGUF) y Decima-small quedó **GO-PROVISIONAL** (+5 netos de 245, sin test pareado todavía — el test pareado es justo lo que esta fase habilita).

**INFORME (nota de investigación del TL, `docs/reference/research/2026-10-16_decision_model_clones_analysis_and_plan.md`):**

- Un **único cliente HTTP `/v1/systemone` cubre cuatro runtimes**: `llama-server` (llama.cpp PR #29818, fusionado — soporta laya/julia-1/lev/openjev/kev, **no Decima**), `Ollama >= 0.35`, **`decima serve` (puerto 11436, servidor stdlib propio)** y LiteLLM (L34-41). Consecuencia de diseño: el proveedor debe ser **agnóstico del runtime**, como ya lo es el del spike.
- Límites citados *[vía LiteLLM]*: 1–128 preguntas, hasta 255 opciones en `choice`, 2–10 niveles en `score`, sin streaming (ADR-020 §3.7). Los límites del wire de Tylluan (16 preguntas, 32 opciones, 10 niveles) son estrictamente más pequeños — no hay conflicto.
- Discrepancia detectada (para ADR-020 v8, no la resuelve este diseño): la nota afirma que el tipo `box` **no aparece en ninguna fuente primaria**; ADR-020 §3.7 lo incluye. El contrato interno de Tylluan no tiene `Box`, así que es irrelevante para esta fase.

---

## 2. Forma de `[decision.provider]` en la config

### 2.1 Restricción de compatibilidad

`provider = "rules"` (string) debe seguir funcionando **exactamente igual** (ADR-020 §1.D.1: comportamiento por defecto byte-idéntico). El servicio ya degrada con aviso a reglas ante un valor desconocido (`service.rs:104-108`) — ese comportamiento se conserva como red de seguridad, no como estrategia.

### 2.2 Propuesta: enum serde *untagged* (string actual ∨ tabla nueva)

```toml
# Forma actual (sin cambios) — por defecto y byte-idéntica:
[decision]
provider = "rules"

# Forma nueva (fase 2): tabla con `kind`
[decision.provider]
kind = "systemone_http"
endpoint = "http://127.0.0.1:11436"     # decima serve; también vale http://127.0.0.1:8080 (llama-server/clef)
model = "decima-small"                    # se envía como campo "model"
request_timeout_ms = 5000                 # techo del carril síncrono (consultas libres)
shadow_timeout_ms = 30000                 # deadline del carril asíncrono (contratos en sombra)
max_inflight = 1                          # llamadas concurrentes al servidor

# Fase 2b (diseño §7, entrega posterior y deshabilitado por defecto):
# [decision.provider]
# kind = "openai_decisions"
# model = "gpt-6-luna"
# api_key_env = "OPENAI_API_KEY"
# base_url = "https://api.openai.com"
# allow_private = false
```

Forma Rust propuesta (sketch — no se implementa sin OK):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecisionProviderConfig {
    /// Forma histórica: "rules".
    Name(String),
    /// Forma fase 2: tabla con `kind`.
    Table(DecisionProviderTable),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DecisionProviderTable {
    SystemoneHttp(SystemoneHttpConfig),
    OpenAiDecisions(OpenAiDecisionsConfig), // fase 2b
}
```

`DecisionConfig.provider: String` pasa a `provider: DecisionProviderConfig` con `#[serde(default)]` = `Name("rules")`. El servicio resuelve el proveedor efectivo según forma y `kind`.

### 2.3 Campos y defaults (systemone_http)

| Campo | Defecto | Por qué |
|---|---|---|
| `kind` | — (obligatorio) | Selector de dialecto/transporte |
| `endpoint` | — (obligatorio) | Base sin path; el cliente añade `/v1/systemone` (precedente: spike L1529) |
| `model` | — (obligatorio) | Se envía en el cuerpo; queda también en `model_identity` del registro |
| `request_timeout_ms` | `5000` | Techo duro del carril síncrono; el deadline efectivo es `min(max_latency_ms ‖ 2000, este techo)` |
| `shadow_timeout_ms` | `30000` | Carril asíncrono; cubre el p99 de clef-flash medido (~13,5 s, 27×500 ms) con margen ×2; `decima serve` está muy por debajo |
| `max_inflight` | `1` | Realidad single-slot de `llama-server`; los modelos ligeros pueden subirlo |
| `breaker_failures` / `breaker_cooldown_s` | `3` / `60` | Cortacircuitos simple (§4) |

### 2.4 Validación en arranque (fail-closed donde importa)

1. `kind` desconocido / campos faltantes / URL malformada → **aviso + fallback a reglas** (nunca un arranque roto; mismo espíritu que hoy).
2. **Privacidad**: `systemone_http` con endpoint **no-loopback** → **se rechaza la config en esta fase** (aviso + fallback a reglas). Razón: `[[inference.allowed_lan_endpoints]]` de ADR-020 §3.6 no existe aún en código (verificado, §1); aceptar LAN sin esa lista abriría el envío de estado clínico a la red sin registro explícito. La lista llega con ADR-020 Fase 1 y entonces esta validación se relaja a "exigir entrada en la lista".
3. `openai_decisions` (nube) → se acepta parsear, pero **deshabilitado de facto** en fase 2 (§7): cualquier llamada que llegue al adaptador sin la condición de entrega devuelve `unsupported` → degradación.

---

## 3. Transporte y dependencia con ADR-020

- **Sin dependencias nuevas**: `reqwest` (ya en árbol) + `tokio::time::timeout`. Política de redirects: **ninguna** (`redirect::Policy::none()`, misma exigencia anti-rebinding de ADR-020 §3.6).
- **Ubicación fase 2**: `router/decision/systemone_http.rs` implementando `DecisionProvider`, aislado del resto. El mapeo de dialecto es el del spike (probado contra servidor real) — se **porta** con sus lecciones T1004, no se reinventa.
- **Relación con ADR-020**: cuando aterrice la Fase 1 del gateway (hoy: diseño v7, cero código), el envío pasa detrás de su harness (trait `DecideProvider` de ADR-020 §3.3, dialecto `SystemOne`, `ModelIdentity` §3.4, auditoría y presupuesto del gateway). Este diseño **no duplica** esas piezas: el cliente de fase 2 es un shim pequeño y sustituible; config y semántica del registro no cambian al hacer el traslado.
- **Decisión abierta para el TL** (§12-Q3): aceptar el shim ahora (recomendado: evita serializar la fase de decisión detrás de un ADR sin código) o esperar al gateway.

---

## 4. Timeout y presupuesto (dos carriles)

**Hecho que ordena todo:** con el modelo disponible hoy (clef-flash), el p99 de la llamada HTTP es ~13,5 s (27× el presupuesto de 500 ms de la banda System-One, §1). Un modo sombra síncrono con deadline interactivo (≤2–10 s) produciría **carril entero de `model_value NULL`** — es decir, cero datos pareados. La recolección de datos no puede colgar del presupuesto de latencia del llamante.

**Carril A — consultas libres (síncrono).** La respuesta del modelo *es* el producto (`tylluan_decide` como puerta directa, ADR-021 §5.1).
- Deadline efectivo = `min(max_latency_ms del wire ‖ 2000 ms, request_timeout_ms)`.
- Timeout / error → `abstain` con motivo (`provider_timeout` / `provider_error`), `status=degraded`; **jamás** una respuesta inventada.
- Consecuencia honesta: un modelo que no quepa en la banda interactiva (clef-flash) se abstendrá sistemáticamente en este carril — es correcto; este carril es para modelos ligeros (`decima serve`, `Ollama` pequeños).
- `connect_timeout`: 1 s.

**Carril B — contratos en sombra (asíncrono).**
- La decisión de reglas se computa y se responde **como hoy** (latencia intacta); en paralelo se lanza la consulta al modelo en una tarea de fondo con deadline `shadow_timeout_ms`.
- Cuando el modelo completa: `UPDATE` de la fila (o filas) de esa decisión con `model_value`, `model_probs_json` y `agreed` (§8). Si nunca completa (timeout/fallo): las columnas quedan `NULL` — que `paired_eval` ya interpreta como fallo conservador del modelo.
- Por qué asíncrono y no un deadline largo síncrono: (1) el modo sombra no debe alterar la latencia observable del llamante (doctrina shadow de ADR-021 §3.2 y "cero cambio al camino caliente" de ADR-018); (2) la subida de modo se decide con datos, y los datos deben existir aunque el modelo sea lento; (3) el registro ya tolera `NULL` por diseño.

**Concurrencia, presupuesto y cortacircuitos:**
- Semáforo `max_inflight` (defecto 1). Si la espera de ranura excede el deadline del carril → **se omite la llamada** (NULL + traza), nunca se acumulan zombies.
- Integración con `InferenceBudget` (ADR-020 §6): la llamada al modelo adquiere permiso del budget existente (`memory/inference_budget.rs`); el tiempo de espera cuenta dentro del deadline del carril. Si el budget rechaza (solo cuando el operador activa `max_queue_wait_secs>0`) → degradación inmediata. Nota: el default legacy (espera ilimitada) es aceptable en el carril B (tarea de fondo) pero el semáforo propio acota igualmente.
- Cortacircuitos: `breaker_failures=3` fallos consecutivos → abierto `breaker_cooldown_s=60`; mientras abierto, ninguna llamada (NULL), una traza `warn` al abrir y al cerrar. Estado en memoria, por proveedor.

---

## 5. Degradación a reglas y "el modelo nunca revierte un rechazo determinista"

| Situación | ¿Llama al modelo? | Qué ve el llamante | Qué se registra |
|---|---|---|---|
| `[decision] enabled=false` | No | `status=disabled` | Cero filas (hoy ya) |
| Contrato nombrado + modo `off` | **No** (ADR-021 §3.2) | Respuesta de reglas (hoy) | Fila con reglas, columnas de modelo `NULL` |
| Contrato + `shadow`, modelo OK | Sí (carril B, asíncrono) | Respuesta de **reglas**, sin cambio | Fila con `rules_value` + `model_value` + `model_probs_json` + `agreed` |
| Contrato + `shadow`, timeout/error del modelo | Sí, falla | Respuesta de reglas, sin cambio | `model_value=NULL`; causa en `tracing` (warn) |
| Validación del wire falla (0/17 preguntas, >16 KiB…) | **No** | `status=error` | Cero filas |
| El modelo responde **fuera de las opciones declaradas** | Sí, pero se descarta | Abstención `answer_out_of_options` se mantiene | `model_value=NULL` + traza `warn` (no se apunta el valor violador: contaminaría la evaluación pareada) |
| Reglas se **abstienen/fallan** en una pregunta de contrato | **No** se rescata con el modelo | Abstención según regla | `model_value=NULL` (el rechazo determinista nunca se "mejora") |
| Pregunta libre + proveedor configurado | Sí (carril A, síncrono) | Respuesta del modelo (o abstención con motivo) | Fila con `rules_value=NULL` + `model_value` |
| Pregunta libre + sin proveedor | No | Abstención `no_model_provider` (hoy) | Fila con ambos `NULL` |
| Futuro `advisory`/`gate` (fuera de fase 2) | Sí | Regla de reserva §3.4 de ADR-021 al fallar; autoridad siempre determinista | Igual + resultado final |

**La regla de autoridad de ADR-021 §3.3 se materializa así:** validación del esquema, opciones cerradas y abstención de reglas son **terminales** en esta fase — el modelo no las revierte, no las rellena y no las mejora (la fila conserva el rechazo como decisión y el desacuerdo queda a la vista). El vector adversario `v8` de Buffy ("el modelo no revierte un rechazo determinista", hoy `#[ignore]`) se des-ignora en la implementación de esta fase contra un servidor simulado.

---

## 6. Privacidad (clase de datos)

Ejes de ADR-020 §3.2 (`ProviderLocality` × `DataSensitivity`) aplicados a esta fase:

- **`systemone_http` en loopback = `ProviderLocality::Local`.** Todo permitido, **incluido `Clinical`**: el estado clínico no sale de la máquina (el escenario del médico rural sigue siendo 100 % offline). Esto es exactamente lo que la fase 2 habilita.
- **Endpoint no-loopback → rechazado en config** (§2.4-2). La puerta LAN existirá cuando exista `[[inference.allowed_lan_endpoints]]` (ADR-020 §3.6/Fase 1) y entonces la validación pasa a "entrada exacta host+puerto con `max_sensitivity` y `allowed_spaces`".
- **`/v1/decisions` (nube): datos clínicos NUNCA.** Regla del encargo, **más estricta** que el "HITL por petición" de ADR-020 §1.A.5 — que aún no está cableado. El adaptador se entrega en fase posterior, deshabilitado por defecto, y hasta que exista la maquinaria de aprobación + sensibilidad por petición **el clínico no viaja**: fail-closed, sin excepción silenciosa. `Private` exigirá opt-in explícito (`allow_private=true`, matriz ADR-020 §3.2.C); `Public` permitido.
- **Hueco reconocido (pregunta al TL, §12-Q5):** el esquema wire (§1217) no tiene campo de sensibilidad y el servicio **no puede inferir "clínico"** de un `state` arbitrario. En fase 2 no muerde porque solo se implementa el camino local. Para `/v1/decisions` habrá que decidir: campo opcional `sensitivity` en el wire (cambio de esquema, dueño el TL) o declaración de sensibilidad máxima por contrato. No invento la solución aquí.
- **El registro es seguro por construcción:** `state` jamás se guarda en claro (solo `request_hash`, `service.rs:33-44`); `model_probs_json` contiene únicamente claves del conjunto cerrado construido por el sistema + números; `model_value` es una de esas opciones. Nunca texto libre del modelo, nunca contenido del estado.
- **Errores HTTP del servidor**: el cuerpo se recorta (300 chars, precedente spike L1546) y va **solo a `tracing` local** — puede contener eco del estado; nunca toca la BD. `tracing` no sale de la máquina.

---

## 7. Adaptador `/v1/decisions` (OpenAI, nube) — diseño; entrega posterior

Diseñado ahora (ADR-020 §3.7 ya fija el mapeo), **no implementado** en fase 2:

| Aspecto | Mapeo desde el contrato interno |
|---|---|
| Petición | `input` = texto del `state`; `questions` = **array** `{type, name, instructions}` |
| `Choice` | `choices[] = {value, description?}` (el core no lleva descripciones hoy: `description=null`) |
| `Score` | `levels[] = {label, description?}` orden menor→mayor |
| `Noul` | `predicate` |
| Respuesta | array con `type`+`name`; `choice`→`{choice, probabilities[], confidence}`; `predicate`→`{probability}`; `score`→`{score, probabilities[], confidence}` |
| Rechazo | tipo `refusal` → `Answer::abstain(reason="refusal")` en el contrato interno |
| `Box` | No existe en el contrato interno → nada que mapear (la discrepancia `box` de ADR-020 §3.7 vs nota 2026-10-16 queda para ADR-020 v8) |

- Config: `kind="openai_decisions"` con `api_key_env` (la clave **nunca** en ficheros trackeados — regla del repo).
- **Condición de entrega (recomendación):** no antes de que exista (a) el campo/declaración de sensibilidad (§6) y (b) idealmente el gateway de privacidad de ADR-020 Fase 1. Cualquier fecha antes de eso obliga a bloquear clínico "a ciegas" — posible pero frágil.
- Errores/timeout/degradación: idénticos al carril correspondiente de §4/§5.

---

## 8. Qué se registra en `decision_shadow_log` (fase 2)

**Columnas actuales (v28, congeladas §1217):** `id, ts, contract, request_hash, rules_value, model_value, model_probs_json, provider, latency_ms, agreed, mode`. Una fila **por pregunta**.

**Escritura en dos tiempos (carril B):**

1. `INSERT` como hoy: `rules_value` (argmax simple de reglas; `NULL` si abstención), `provider="rules-v0"`, `latency_ms` de la decisión de reglas, `mode` del contrato. Se conserva el `rowid` de cada fila por pregunta.
2. Al completar el modelo (tarea de fondo): `UPDATE decision_shadow_log SET model_value=?, model_probs_json=?, agreed=? WHERE id=?` — los tres campos **juntos o ninguno** (atómico por fila).
   - `model_value` = argmax del modelo **solo si está dentro del conjunto declarado**; si no, `NULL` (§5).
   - `model_probs_json` = serialización compacta `{opción: prob}` de la distribución del modelo.
   - `agreed` = `1` si `model_value == rules_value`, `0` si difieren, `NULL` si cualquiera de los dos es `NULL`. Coincide con el fixture de `paired_eval` (`agreed NULL` cuando `model_value NULL`).

**Semántica por caso** (consistente con §5): `model_value NULL` significa "el modelo no respondió de forma utilizable" (timeout, error, rechazo por opciones, o no consultado) — **indistinguible en la tabla a propósito**: `paired_eval` lo cuenta como fallo conservador del modelo (`paired_eval.rs:330-334`, `:757-771`). El diagnóstico fino (por qué) vive en `tracing`.

**`provider`** se mantiene con el proveedor de autoridad en el INSERT (`"rules-v0"`). El fixture de `paired_eval` admite `'rules+model'` (L824) pero **no lo exijo como convención**: propongo la opción A.

**Propuesta A (recomendada, decisión del TL): migración v29 — dos columnas aditivas**

- `model_identity TEXT NULL` — identidad del modelo que produjo la respuesta ("systemone-http-v1:clef-flash" o el `ModelIdentity` completo de ADR-020 §3.4 cuando exista). Lo pide ADR-021 §3.5 ("proveedor e identidad del modelo").
- `model_latency_ms INTEGER NULL` — latencia de la llamada al modelo. La necesita la cláusula 3 de la evaluación (p99 del modelo vs presupuesto) que `paired_eval` hoy no puede calcular porque `latency_ms` es la de la decisión de reglas.
- Seguridad del cambio: `paired_eval` lee **por nombre de columna** (`paired_eval.rs:79-84`) → columnas aditivas no lo rompen; su test de esquema crea su propia tabla, así que no cambia.

**Alternativa B (sin migración):** codificar la identidad en `provider` al hacer el UPDATE (`"rules-v0+systemone-http-v1:clef-flash"`) y dejar la latencia del modelo solo en `tracing`. Coste: la evaluación pareada pierde el p99 del modelo y el invariante "provider = proveedor de autoridad" se difumina. Documentada por si el TL prefiere no tocar el esquema.

**Preguntas libres:** fila con `rules_value=NULL` + `model_value` (el INSERT ocurre tras la respuesta síncrona). Ya previsto por `paired_eval` (`rules_value NULL` → nunca coincide con la verdad, L107-108).

---

## 9. Traceo y observabilidad

Solo `tracing` (sin endpoints nuevos). Nunca se traza el `state` (ni fragmentos), ni cuerpos de respuesta del modelo completos (solo recortes de error, 300 chars).

| Evento | Nivel | Contenido |
|---|---|---|
| Inicio/fin de llamada al modelo | `debug` | provider, modelo, contrato, latencia |
| Timeout / error HTTP / decode | `warn` | causa + cuerpo de error recortado |
| Respuesta fuera de opciones | `warn` | pregunta + valor violador (local) |
| Degradación aplicada (carril A) | `warn` | motivo de la abstención |
| Budget/ranura agotados → omisión | `warn` | carril + deadline |
| Cortacircuitos abierto/cerrado | `warn` | estado + fallos consecutivos |
| `UPDATE` del registro falló | `warn` | fila + error (la decisión no se rompe) |

---

## 10. Fuera de alcance (esta fase)

- Cablear módulos internos del core a los contratos (la puerta interna sigue sin consumidores reales: el estado actual no cambia).
- Modos `advisory` / `gate` y filtro de salida (ADR-021 §7).
- HITL para clínico-en-nube y cualquier uso de `/v1/decisions` en producción.
- Infraestructura de ADR-020 Fase 1 (la absorbción del shim llega con ella) y dual-space.
- Dashboard, canvas de Coloquio (lista de permitidos sin cambios).

---

## 11. Verificación propuesta (DoD de la implementación, tests primero)

1. **Compatibilidad config**: `provider="rules"` byte-idéntico (test de parseo + test de comportamiento existente intacto); tabla `systemone_http` parsea; `kind` inválido → aviso + reglas; endpoint LAN → rechazo de config.
2. **Wire**: mapeo de preguntas byte-comparable con el del spike (`systemone_question_spec`); `instructions` sintetizado (400 real del servidor); lote con una pregunta degenerada no tumba el resto (T1004); respuesta parcial (el servidor omite una pregunta) no invalida las demás.
3. **Carril A**: timeout → `abstain(provider_timeout)` + `degraded`; servidor caído → `abstain(provider_error)`.
4. **Carril B**: la respuesta al llamante sale sin esperar al modelo (test con servidor simulado que tarda); `UPDATE` escribe los tres campos juntos; timeout → fila intacta con NULL.
5. **Rechazo terminal**: `v8` des-ignorado — modelo que "revierte" un rechazo de validación/opciones no cambia respuesta ni registro.
6. **Privacidad**: `openai_decisions` con estado clínico → rechazo antes de cualquier socket (fail-closed); endpoint LAN rechazado.
7. **Registro + evaluación**: e2e `DecisionService` → filas reales → `tylluan-evals --suite paired` (join por `request_hash` contra truth de prueba) — el hueco que cierra el encargo B1 del TL a Buffy; con la opción A, `model_identity`/`model_latency_ms` poblados.
8. **Cortacircuitos y concurrencia**: 3 fallos → abierto; ranura saturada → omisión con traza.
9. `cargo clippy -p tylluan-kernel -- -D warnings` limpio + los gates de docs del repo.

---

## 12. Preguntas abiertas para el TL (necesito respuesta antes de código)

1. **Registro**: ¿migración v29 (`model_identity` + `model_latency_ms`) — opción A — o codificar en `provider` — opción B (§8)?
2. **Carril B asíncrono**: ¿se acepta la escritura en dos tiempos (INSERT→UPDATE) como semántica de la fase? Es la recomendación por el p95/p99 medido y por no tocar la latencia del llamante.
3. **Shim vs gateway**: ¿implemento ya el cliente aislado en `router/decision/` (recomendado) o espero a ADR-020 Fase 1?
4. **Rescate de abstenciones**: ¿confirmas la regla conservadora de que una pregunta de contrato donde las reglas se abstienen **no** se consulta al modelo (§5), o querrás pares también en ese caso (con `rules_value NULL`)?
5. **Sensibilidad**: ¿campo opcional `sensitivity` en el wire (§1217) o declaración por contrato, y cuándo? (Solo bloquea `/v1/decisions`.)
6. **`/v1/decisions`**: ¿entrega solo-diseño ahora y con qué condición de implementación (§7)?
7. **Ejemplo del toml**: ¿`endpoint` de ejemplo = `decima serve` (`:11436`, GO-PROVISIONAL, ligero) documento también llama-server (`:8080`)? ¿O prefieres al revés?

---

## 13. Marcado de veracidad

- **VERIFICADO (contra `8c9287b`, file:line en §1):** trait y contrato de decisión, wiring del servicio y fallback de `provider`, textos de abstención, columnas y escritura del log de sombra, config `DecisionConfig`, contrato de `paired_eval` (incluido `model_value NULL` = fallo conservador y tolerancia aditiva de columnas), cliente SystemOne del spike con sus lecciones T1004, `InferenceBudget` real, ausencia de `allowed_lan_endpoints` en código, `reqwest` en el árbol.
- **MEDIDO:** variante B NO-GO 2026-10-08 (27× de latencia, `ROADMAP.md:287`, `ROADMAP_O3.md:41`); Decima-small GO-PROVISIONAL.
- **INFORME (no verificado por mí, citado):** cobertura de runtimes por el dialecto `/v1/systemone` (`decima serve :11436`, llama.cpp PR #29818, Ollama ≥0.35.1, LiteLLM) y límites 1-128/255/2-10 — nota `2026-10-16_decision_model_clones_analysis_and_plan.md` L34-41.
- **PROPUESTA (mía, revisable):** forma de config (§2), carriles y defaults (§4), tabla de degradación (§5), opción A de registro (§8), plan de verificación (§11).
- **SIN CÓDIGO:** este documento no toca ni una línea de implementación; a la espera del OK del TL.
