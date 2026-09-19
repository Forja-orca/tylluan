# Addendum: ¿Coloquio también, y un único motor de KV-cache/runtime? — investigación

> **Autor:** Claude Code (Tech Lead)
> **Contexto:** José preguntó (1) si Coloquio debería aprovechar también el
> trabajo de KV-cache/runtime, y (2) si construir un único motor
> pluggable de KV-cache/runtime — añadible y modificable donde haga
> falta, ahora y en el futuro — es buena idea.
> **Precede a:** `kv_cache_shared_prefix_research.md` (Deep),
> `multi_agent_kv_cache_runtime_research.md` (Antigravity), ambos
> cerrados bajo `bwc-8bfc9aca`. Este documento no reabre su veredicto —
> lo extiende con dos preguntas nuevas, investigadas con la misma
> disciplina.
> **Fecha:** 2026-09-19
> **Estado:** Investigación — sin contrato de implementación todavía,
> pendiente de decisión de alcance con José.

---

## 1. ¿Coloquio también? — Sí, pero no es un problema nuevo

Coloquio no es solo un log de chat — dispara llamadas LLM reales: la
clasificación híbrida de `CoherenceGate` (Layer 4, `coherence_gate_hybrid_enabled`),
cualquier síntesis/coordinador, y en el futuro el juicio de aprobación
del dispatcher si algún día se automatiza. Todas pasan por
`llama_backend.py`, el mismo punto que Deep auditó (`kv_cache_shared_prefix_research.md`
§1): **hoy cada llamada manda un único mensaje `user`, sin prefijo
estructurado**.

**Conclusión verificada, no nueva investigación**: el veredicto GO ya
cerrado (DPC — Deterministic Prefix Canonicalization + `n_keep`/`--cache-reuse`)
se aplica igual a las llamadas de Coloquio que a cualquier otra — porque
todas comparten el mismo cuello de botella (`llama_backend.py`, sin
prefijo). No hace falta una investigación aparte para Coloquio; hace
falta que la implementación del GO (todavía sin contrato asignado) cubra
**todos** los call sites reales, no solo uno, y que quien lo implemente
audite cuántos hay antes de tocar el primero — riesgo real si cada
caller construye su propio prompt a mano: el prefijo dejaría de ser
literalmente idéntico entre agentes, y `--cache-reuse` no encuentra
coincidencia de prefijo exacto si dos callers ordenan sus bloques
distinto.

**Recomendación**: cuando se asigne el contrato de implementación del GO
(DPC), su alcance explícito debe ser "todos los call sites de
`llama_backend.py`", con Coloquio como uno de ellos, no como una tarea
separada.

---

## 2. ¿Un único motor pluggable de KV-cache/runtime? — Investigado, patrón real y validado, con una condición de escala importante

### 2.1 El patrón que propones ya existe en producción, en varios laboratorios

- **LMCache** ([lmcache/lmcache](https://github.com/lmcache/lmcache)):
  demonio independiente que gestiona el KV-cache **desacoplado del motor
  de inferencia**, con backends de almacenamiento/transporte pluggables
  — permite tiering persistente entre RAM de CPU, disco local, y
  backends remotos, sin que el motor de serving (vLLM, etc.) sepa nada
  de esos detalles.
- **NIXL** (NVIDIA Inference Xfer Library): API unificada para
  transferencia de KV-cache, abstrayendo las librerías de transporte
  reales (UCX/RDMA, libfabric/EFA) — mismo principio: una interfaz
  estable, implementaciones intercambiables por debajo.
- **AI-Dynamo**: se posiciona como plano de control para *mixed-engine
  serving*, con una representación de request unificada entre motores
  distintos — exactamente "un único punto que podemos añadir/modificar
  donde haga falta" aplicado a orquestación, no solo a caché.
- **ONNX Runtime**: expandió su API de *execution providers* pluggables
  — mismo principio otra vez, aplicado a proveedores de cómputo en vez
  de caché.

**Veredicto de la pregunta en abstracto: SÍ, es un patrón arquitectónico
real, no una intuición sin respaldo — el consenso de la industria en
2026 es "capas de inferencia pluggables y composables en vez de stacks
monolíticos"**, confirmado por múltiples laboratorios independientes.

### 2.2 La condición que hay que aplicar a Tylluan, no ignorar

LMCache/NIXL/AI-Dynamo están diseñados para servir GPUs de datacenter con
motores tipo vLLM/SGLang — resuelven un problema de escala (miles de
requests/seg, múltiples GPUs, tiering entre HBM/RAM/disco/red). El ancla
de diseño de Tylluan es **CPU modesta / RPi4 / móvil, un solo
`llama-server` local**. Copiar LMCache tal cual sería la misma clase de
error que ya se descartó en el Desván de la biblia para el `Credit Mesh`
y el reparto RPC de `llama.cpp`: una solución de escala equivocada para
el hardware que de verdad tenemos que soportar.

**Lo que sí es directamente aplicable, investigado y con código real**:

- **Leyline** (arXiv:2606.01065) — directivas explícitas `(span,
  replacement)` sobre el KV-cache para editar una conversación agéntica
  sin re-prefill completo (reintentos de tool calls, pivotes de
  trayectoria). Relevante para el dispatcher: cuando un agente reintenta
  tras un fallo, hoy se re-prefija todo desde cero.
- **"Agent Memory Below the Prompt"** (arXiv:2603.04428, código real en
  [GitHub](https://github.com/yshk-mxim/agent-memory)) — persiste el
  KV-cache de cada agente a disco en Q4 (4-bit) y lo recarga directo en
  la capa de atención, evitando el re-prefill O(n) completo. Medido:
  15.7s → 577ms (disco) / 719ms (memoria) a 4K de contexto. **Esto es
  exactamente el problema de "no cabe el contexto de 8 agentes a la vez
  en un RPi4"** que el propio documento de Antigravity ya modeló
  matemáticamente (§3.2 de `multi_agent_kv_cache_runtime_research.md`)
  — aquí hay una solución con código abierto ya funcionando, no solo
  matemática teórica.

### 2.3 Veredicto concreto para Tylluan

**No** construir un LMCache propio de alcance general — sería
sobre-ingeniería para el hardware ancla, y el propio `llama-server` ya
resuelve el 65-82% de la ganancia teórica con configuración (GO ya
cerrado). **Sí** hay valor real en un módulo delgado, propio,
Tylluan-shaped, con un objetivo mucho más modesto que "motor universal":

```
crates/tylluan-kernel/src/router/inference_runtime.rs   (nombre propuesto)

- Punto único de construcción de prompt (DPC): todo caller de
  llama_backend pasa por aquí, no construye su propio system+contexto a
  mano. Mismo principio que config::open_db() para SQLite: un choke
  point, no una convención que cada caller puede olvidar.
- Punto único de decisión de persistencia de contexto por agente
  (inspirado en Agent Memory Below the Prompt): si el RPi4-class no
  puede mantener N agentes residentes, decide qué agente se persiste a
  disco Q4 y cuál se re-prefija, en vez de dejarlo a la política
  implícita de eviction del slot de llama-server.
- Trait/interfaz mínima para que, si en el futuro Tylluan soporta un
  motor distinto a llama-server (otro backend, otro hardware tier),
  el resto del kernel no tenga que cambiar — la lección de NIXL/AI-Dynamo
  (interfaz estable, implementación intercambiable) aplicada a la escala
  real de Tylluan, no a la escala de datacenter.
```

Esto **no es un contrato nuevo urgente** — depende de que el GO de DPC ya
esté implementado primero (si no existe ni el prefijo estructurado, no
hay nada que gestionar con este módulo). Se propone como el **siguiente
paso natural después de que el contrato de implementación del GO
(pendiente de asignar) esté cerrado**, no en paralelo.

---

## 3. Resumen para decisión

| Pregunta | Veredicto | Siguiente paso |
|---|---|---|
| ¿Coloquio necesita su propia investigación de KV-cache? | No — mismo problema, mismo GO ya cerrado | Al implementar el GO (DPC), su alcance debe cubrir explícitamente todos los call sites de `llama_backend.py`, Coloquio incluido |
| ¿Motor único pluggable de KV-cache/runtime? | Patrón real y validado (LMCache/NIXL/AI-Dynamo), pero a escala equivocada para nuestro hardware ancla si se copia tal cual | Módulo delgado propio (`inference_runtime.rs`), con DPC + persistencia por agente estilo *Agent Memory Below the Prompt* — **después** de que el GO de configuración esté implementado, no en paralelo |

---

## 4. Citas y referencias verificadas

1. **LMCache**: [github.com/lmcache/lmcache](https://github.com/lmcache/lmcache) — demonio de KV-cache desacoplado del motor, backends pluggables.
2. **Leyline**: Ma, B., Eitzinger, J., Koestler, H. (2026). *Leyline: KV Cache Directives for Agentic Inference*. [arXiv:2606.01065](https://arxiv.org/abs/2606.01065).
3. **Agent Memory Below the Prompt**: Shkolnikov, Y. P. (2026). *Persistent Q4 KV Cache for Multi-Agent LLM Inference on Edge Devices*. [arXiv:2603.04428](https://arxiv.org/abs/2603.04428), código: [github.com/yshk-mxim/agent-memory](https://github.com/yshk-mxim/agent-memory).
4. **RadixAttention vs prefix caching (SGLang/vLLM)** — comparativa técnica 2026 verificada vía múltiples fuentes independientes coincidentes (TTFT p50 310ms→195ms a 80% de prefijo compartido, umbral de rentabilidad ~60% de prefijo compartido).
5. **NIXL / AI-Dynamo** — confirmado vía cobertura técnica de infraestructura de inferencia 2026, no fuente primaria de NVIDIA con acceso directo; marcado como corroborado, no cita primaria.
