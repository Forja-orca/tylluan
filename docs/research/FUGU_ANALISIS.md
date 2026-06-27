# Fugu by Sakana AI — Análisis Completo

> Fecha: 2026-06-23
> Repositorio: https://github.com/SakanaAI/fugu
> Producto: Sakana Fugu — Multi-Agent System as a Model

---

## 1. ¿Qué es Fugu?

**Fugu es un sistema multi-agente disfrazado de API única.** No es un modelo monolítico ni un simple router — es una capa de orquestación dinámica que:

- Coordina un pool diverso de modelos frontera (incluyendo GPT, Gemini, Claude, modelos open-source)
- Aprende a asignar roles y estrategias de colaboración por tarea (no usa workflows diseñados a mano)
- Se ofrece como una API compatible con OpenAI — el cliente ve un solo endpoint, pero detrás hay un equipo de modelos trabajando en paralelo

**Dos tiers:**

| Modelo | Enfoque | Caso de uso |
|--------|---------|-------------|
| **Fugu** | Balance rendimiento/latencia | Coding diario, code review, chatbots |
| **Fugu Ultra** | Máxima calidad | Kaggle, papers, ciberseguridad, patentes |

Contexto: **1M tokens** en ambos.

---

## 2. El Repositorio de GitHub (SakanaAI/fugu)

El repo NO contiene el motor de orquestación de Fugu (eso es server-side en Sakana). Es un **config bundle para Codex CLI** que:

### Estructura del repositorio

```
configs/
  bundle.sh              — Manifiesto del bundle (versión, archivos, inyectables)
  files/fugu.json        — Perfiles de modelo para Codex (fugu, fugu-ultra)
  injects/               — Provider config (Sakana API)
scripts/
  install.sh             — Instalador: pinnea Codex + despliega config + guarda API key
  codex-fugu             — Launcher: lanza Codex con perfil fugu, chequea updates cada hora
docs/
  commands_details.md    — Documentación de instalación, flags, backup/restore
notes/
  0001-api-key-in-codex-env.md
  0002-session-index-and-version-switches.md
Fugu_technical_report.pdf  — Technical report del sistema Fugu
```

### ¿Qué hace realmente?
1. `install.sh` → Descarga/pinnea Codex CLI, despliega la config de Fugu, guarda API key en `~/.codex/.env` (0600), respalda config anterior
2. `codex-fugu` → Lanza `codex -p fugu`, verifica updates del bundle cada hora, maneja version mismatch, muestra notices
3. El perfil `fugu.json` → Define modelo con 1M contexto, `reasoning_levels: high/xhigh`, tools estándar de Codex

### Nada de esto es el motor de Fugu
El repo es solo **la puerta de entrada** para usar Fugu desde Codex CLI. La magia (orquestación multi-modelo) ocurre en los servidores de Sakana API.

---

## 3. Papers Fundacionales (ambos ICLR 2026)

### Paper 1: TRINITY — An Evolved LLM Coordinator
**arXiv:** 2512.04695 | **Web:** https://sakana.ai/trinity

#### Idea central
En vez de construir un modelo gigante, entrenar un **coordinador evolutivo** que orquesta un equipo de modelos diversos.

#### Arquitectura
- **Coordinador:** SLM de 0.6B params + head lineal de ~10K params = **<20K params entrenables**
- **Método de optimización:** sep-CMA-ES (Covariance Matrix Adaptation Evolution Strategy) — supera a RL, imitation learning y random search
- **Pipeline multi-turn:** En cada turno el coordinador asigna un rol a un LLM del pool

#### Tres roles
| Rol | Función |
|-----|---------|
| **Thinker** | Reflexiona, planea, descompone el problema |
| **Worker** | Ejecuta, genera código, produce output |
| **Verifier** | Valida, detecta errores, sugiere correcciones |

#### Resultados clave
- **LiveCodeBench: 86.2% pass@1** — SOTA al momento de publicación (superó a GPT-5 con 83.8% y Gemini 2.5-Pro con 67.2%)
- Mean relative error reduction del 21.9% frente a baselines single-model y multi-agent
- El coordinador supera a CADA modelo individual en su pool

#### Por qué funciona (análisis teórico)
1. Las representaciones de estado oculto del coordinador contextualizan mejor los inputs
2. Bajo alta dimensionalidad y presupuesto estricto, sep-CMA-ES tiene ventajas sobre RL por su separabilidad por bloques ε

---

### Paper 2: The Conductor — Learning to Orchestrate Agents in Natural Language
**arXiv:** 2512.04388 | **Web:** https://sakana.ai/learning-to-orchestrate

#### Idea central
Entrenar un modelo de 7B con **RL** para que actúe como **manager** que delega tareas a un equipo de AIs, escribiendo prompts efectivos para cada uno.

#### Diferencias clave con TRINITY
| Aspecto | TRINITY | Conductor |
|---------|---------|-----------|
| Tamaño del coordinador | 0.6B + head 10K | 7B completo |
| Optimización | sep-CMA-ES (evolutivo) | Reinforcement Learning |
| Output del coordinador | Selección de rol (Thinker/Worker/Verifier) | Lenguaje natural (prompts, topologías) |
| Flexibilidad | Roles fijos | Topologías de comunicación dinámicas |

#### Capacidades del Conductor
- **Diseña topologías de comunicación** entre agentes (no roles fijos — decide qué modelo habla con cuál)
- **Prompt-engineering automático:** Escribe instrucciones personalizadas para cada worker según sus fortalezas
- **Topologías recursivas:** Puede seleccionarse a sí mismo como worker → forma cadenas de corrección ("lee mi output anterior, si falló, lanza un workflow correctivo")
- **Adaptación a pools arbitrarios:** Entrenado con pools randomizados de agentes → generaliza a cualquier combinación en inferencia

#### Resultados clave
- **LiveCodeBench: 83.9%** (SOTA al momento)
- **GPQA-Diamond: 87.5%** (SOTA al momento)
- Supera a CADA modelo individual en su pool (GPT-5, Gemini, Claude, open-source)
- Las ganancias (~3% sobre el mejor worker individual) son comparables a una generación entera de mejora de modelo frontera — pero vienen de **coordinación**, no de pretraining

---

## 4. Cómo Fugu Combina Ambos Papers

Fugu es el **producto comercial** que unifica ambas líneas de investigación:

```
                    Sakana Fugu API
                    ┌──────────────────────┐
                    │  Orchestration Layer  │
                    │  (TRINITY + Conductor)│
                    └──────┬───────────────┘
                           │
            ┌──────────────┼──────────────┐
            ▼              ▼              ▼
         GPT-5         Gemini 3.1      Claude Opus 4
       (Worker)       (Worker)         (Worker)
            │              │              │
            └──────────────┼──────────────┘
                           ▼
                   Respuesta final
```

- **De TRINITY:** La asignación de roles (Thinker/Worker/Verifier) y el pipeline multi-turn con coordinación evolutiva
- **Del Conductor:** El prompt-engineering automático en lenguaje natural, las topologías de comunicación dinámicas, y la capacidad de corrección recursiva
- **Innovación de Fugu:** No es solo un router ni solo un manager — es un sistema que **aprende a orquestar** sin reglas escritas a mano

---

## 5. Benchmarks

| Benchmark | Fugu | Fugu Ultra | Opus 4.8 | Gemini 3.1 Pro | GPT 5.5 |
|-----------|------|------------|----------|-----------------|---------|
| **SWE-Bench Pro** | 59.0 | **73.7** | 69.2 | 54.2 | 58.6 |
| **TerminalBench 2.1** | 80.2 | **82.1** | 74.6 | 70.3 | 78.2 |
| **LiveCodeBench** | 92.9 | **93.2** | 87.8 | 88.5 | 85.3 |
| **LiveCodeBench Pro** | 87.8 | **90.8** | 84.8 | 82.9 | 88.4 |
| **Humanity's Last Exam** | 47.2 | **50.0** | 49.8 | 44.4 | 41.4 |
| **CharXiv Reasoning** | 85.1 | **86.6** | 84.2 | 83.3 | 84.1 |
| **GPQA-D** | 95.5 | **95.5** | 92.0 | 94.3 | 93.6 |
| **SciCode** | **60.1** | 58.7 | 53.5 | 58.9 | 56.1 |
| **Long Context Reasoning** | 74.7 | 73.3 | 67.7 | 72.7 | **74.3** |

Fugu Ultra lidera en 8/11 benchmarks. Fugu (base) es competitivo con los frontera individuales.

---

## 6. Implicaciones para Tylluan

### ¿Qué podemos aprender de Fugu?

1. **Multi-model orchestration > single model:** Un coordinador ligero que orquesta modelos diversos consistentemente supera al mejor modelo individual. Esto valida nuestra arquitectura de guilds en Tylluan.

2. **Coordinación aprendida > workflows escritos a mano:** TRINITY y Conductor muestran que la orquestación debe ser aprendida, no diseñada. Nuestro `GuildMatcher` con scoring semántico es un primer paso — podríamos evolucionarlo con RL.

3. **Roles especializados:** Thinker/Worker/Verifier es un patrón que podríamos adaptar a nuestros guilds (ej: un guild "thinker" que descompone problemas, un "verifier" que revisa outputs).

4. **1M contexto vs nuestra aproximación:** Fugu usa 1M tokens de contexto — nosotros usamos SilvaDB con RAG. Son enfoques complementarios: contexto largo vs memoria estructurada.

### ¿Qué NO podemos copiar?

- El motor de orquestación de Fugu es server-side y propietario
- Depende de APIs de modelos frontera (caro, no soberano)
- La licencia MIT de Tylluan nos permite reutilizar patrones sin restricciones

### ¿Qué SÍ podemos implementar?

- Un coordinator guild en Tylluan que implemente el patrón Thinker/Worker/Verifier usando nuestros guilds existentes
- Mejora del `GuildMatcher` con aprendizaje por refuerzo (como Conductor)
- Topologías de comunicación dinámicas entre guilds (como las topologías recursivas de Conductor)

---

## 7. Referencias

| Recurso | URL |
|---------|-----|
| Repo Fugu (Codex bundle) | https://github.com/SakanaAI/fugu |
| Página oficial Fugu | https://sakana.ai/fugu/ |
| Paper TRINITY (arXiv) | https://arxiv.org/abs/2512.04695 |
| Página TRINITY | https://sakana.ai/trinity/ |
| Paper Conductor (arXiv) | https://arxiv.org/abs/2512.04388 |
| Página Conductor | https://sakana.ai/learning-to-orchestrate/ |
| API Sakana | https://api.sakana.ai/v1 |
| Consola Sakana | https://console.sakana.ai |
