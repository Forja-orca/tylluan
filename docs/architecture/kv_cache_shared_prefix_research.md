# KV-Cache compartido para Tylluan: KVCOMM + CacheScout — veredicto GO/NO-GO

> **Autor:** Deep (OpenCode)
> **Target:** José, Claude Code (Tech Lead), Antigravity (benchmarks)
> **Contrato:** bwc-8bfc9aca-c734-46c8-8569-a60a64c00ec8
> **Status:** Diseño con veredicto — sin código todavía
> **Fecha:** 2026-09-18
> **Pregunta del contrato:** ¿el prefijo compartido de Tylluan (system prompt + contexto de memoria recuperado) es cacheable con lo que llama-server ya expone (`--cache-reuse`, `n_keep`), o hace falta algo nuevo?

---

## 1. Hallazgo local previo (verificado en código, no teoría)

`guilds/core/llama_backend.py` llama a `/v1/chat/completions` con un **único mensaje `user`** (líneas 253-255: `"messages": [{"role": "user", "content": prompt}]`). No hay system prompt, no hay contexto de memoria compartido, no hay prefijo fijo alguno en la forma de llamada actual.

**Consecuencia directa:** hoy NO existe nada que cachear. El "anchor" de agente (system+tools, que CacheScout mide como 53-62% de todos los tokens de prompt en workloads multi-agente) no se envía. La pregunta del contrato tiene una precondición que no se cumple — el prefijo hay que CREARLO primero, después se evalúa su cacheabilidad.

## 2. Lo que llama-server (llama.cpp) expone hoy

| Mecanismo | Qué hace | Alcance |
|---|---|---|
| **Slot KV caching nativo** (server) | Cada slot conserva su KV entre requests; si el prompt siguiente comparte el prefijo EXACTO (mismos tokens), el prefill se salta | Prefijo exacto, por slot |
| `n_keep` / `--keep` | Mantiene los primeros N tokens en el cache del slot (p.ej. el system prompt) entre turns | Prefijo exacto |
| `--cache-reuse` | Reutiliza el prefijo cacheado del slot cuando el prompt coincide | Prefijo exacto |
| `--parallel N` | Múltiples slots (una agente por slot = aislamiento de contexto + reuso estable) | 1 slot/agente |

**Ninguno de los mecanismos expuestos por llama-server hace traducción de offsets (KVCOMM) ni gestión de evicción semántica (CacheScout).** Esa capacidad vive en el motor (nivel KV-array / block manager), no en la superficie HTTP.

## 3. La literatura, aplicada a nuestro caso

### KVCOMM (NeurIPS'25, arXiv:2510.12872)
- Problema que resuelve: contextos multi-agente con texto compartido pero **prefijos divergentes** (cada agente extiende distinto). El caching de prefijo exacto falla por *offset variance* (las posiciones cambian).
- Mecanismo: *anchor pool* — pool de desviaciones de cache observadas bajo prefijos variados; en cada reuse, estima el offset del prefijo nuevo y **traduce el KV del texto compartido**, arrancando el decode sin prefill.
- Resultados: >70% reuse, **7.8× prefill speedup** (1K tokens, 512 prefijo, 5 agentes), TTFT 430→55ms.
- Requisito implícito: **acceso a nivel KV-array** (leer/insertar/desplazar caches por capa) — un runner de framework (PyTorch/vLLM-class), NO expuesto por llama-server HTTP.

### CacheScout (arXiv:2608.14624)
- Problema que resuelve: la gestión de cache reactiva (LRU) **evicta anchors reusables** antes de su próxima invocación.
- Mecanismo: capa runtime agent-aware sobre vLLM — *transition learner* (Markov de primer orden sobre la ejecución de agentes, online) → evicción por supervivencia + **prefetch en background** del anchor del próximo agente.
- Datos clave: el contexto fijo recurrente = **53-62% de todos los tokens** en 4 workloads multi-agente. Hit rate +10-18pp, TTFT −18-45%, latencia por turno −29-38%, throughput +57%. Estado del runtime <25KB.
- Requisito: control a nivel block manager del motor de serving (implementado sobre vLLM).

## 4. Veredicto

### GO — nivel config (barato, medible, sin infraestructura nueva)
**Crear el prefijo compartido + activar el caching nativo de llama-server:**

1. **Cambio de forma de llamada en `llama_backend.py`**: enviar un system prompt fijo de Tylluan + el anchor de memoria recuperado como prefijo estructurado (`messages: [system, {memoria}, user]`) — este es el paso que crea el anchor; hoy no existe.
2. **llama-server**: `--parallel` con slot estable por agente (un slot por agente → el prefijo del slot se reutiliza entre invocaciones del mismo agente), `n_keep` = longitud del prefijo fijo (system + memoria), `--cache-reuse` habilitado.
3. **Métrica**: TTFT de llamadas LLM (CoherenceGate, synthesis, coordinator) antes/después + tasa de cache-hit del slot. En hardware CPU modesto (RPi4-class), eliminar el prefill del prefijo (53-62% de los tokens según CacheScout) es el mayor ahorro disponible sin tocar el motor.

**Este GO captura la mayor parte de la ganancia del anchor de CacheScout** (el prefijo exacto recurrente por agente) con configuración, no con investigación.

### NO-GO — a nivel motor (KVCOMM/CacheScout completos) por ahora
La traducción de offsets (KVCOMM) y la gestión de evicción/prefetch semántica (CacheScout) requieren acceso KV-array / block manager que **llama-server no expone por HTTP**. Implementarlos sobre el stack actual = fork de llama.cpp o cambio de motor de serving — proyecto de investigación real (semanas), no configuración. **Decisión: diferir hasta que el GO de configuración esté medido y el caso de negocio sea cuantificable.**

## 5. Separación de capas — lo que esto NO arregla

El hallazgo del 68% de pérdida en `tylluan_recall` bajo 8 agentes es el **mutex de embeddings** (`router/embeddings.rs:24`, BGE-M3), no prefill de LLM. KV-cache resuelve la redundancia de *prefill LLM*; el mutex necesita *batching de embeddings* (el spike NO-GO de esta semana con causa raíz documentada). Ambas son eficiencia multi-agente pero en capas distintas — el contrato de investigación las mantiene separadas deliberadamente.

## 6. Recomendación de siguiente paso (post-veredicto)

1. Antigravity: benchmark de TTFT en hardware ancla (RPi4/CPU modesta) con la forma de llamada ACTUAL (sin prefijo) — la línea base contra la que medir el GO.
2. Deep: cuando la línea base exista, implementar el cambio de forma de llamada + flags de llama-server (tarea de configuración, ~1 sesión), y medir el delta.
3. KVCOMM/CacheScout a nivel motor: reabrir solo si el delta del GO no alcanza y el caso de negocio lo justifica.

---

**Veredicto en una línea:** el prefijo NO existe hoy (hay que crearlo — GO barato y medible), y una vez creado, llama-server ya lo cachea con su slot prefix caching nativo (n_keep/--cache-reuse/--parallel); KVCOMM/CacheScout completos requieren acceso a nivel motor que el stack actual no expone — NO-GO diferido con métrica de reapertura clara.