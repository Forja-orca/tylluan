# Dossier de investigación — dolores actuales del equipo y ciencia verificada que los ataca

**Autor:** Buffy (verificación de campo + research). **Fecha:** 2026-09-23.
**Método:** cada dolor fue extraído de incidentes reales de esta flota (no opinión), cada fuente fue verificada en web al escribir este doc (título, autor, año, DOI/venue), y cada mapeo declara su madurez honesta: **[VALIDADO]** (evidencia publicada, revisada por pares o en producción en otros sistemas), **[PRÁCTICA]** (patrón usado en producción por otros, adaptación a Tylluan pendiente), o **[PROPOSAL]** (honestidad: es nuestra hipótesis, la ciencia la sugiere, nadie la ha demostrado).

Principio rector (filosofía Tylluan): **bajo consumo, local, sin jerarquía central** — la investigación se eligió con ese filtro. Los sistemas biológicos llevan ~4.000 millones de años optimizando exactamente eso.

---

## Dolor 1 — Los procesos se cuelgan y los exit codes mienten

**Lo que nos costó ya:** zombie `opencode` 34 min bloqueando la cadencia vía anti-solapamiento (T705); watchdog matando procesos vivos por `WaitForExit` espurio de PS 5.1 mientras el filesystem registraba 14 min de trabajo real; `Executed` enmascarando `spawn FAILED` en el dispatcher (T688); exit 1 invisible sin log (T699). Tres veces en 24 horas la misma clase de defecto: **el supervisor mide al launcher, no al trabajo**.

**La ciencia:** *Intermittent computing* — sistemas que asumen que pueden morir en cualquier instante (alimentados por energía cosechada: solar, RF, térmica).
- Islam et al. 2023, "Amalgamated Intermittent Computing Systems", **IoTDI '23** (ACM/IEEE Conf. on Internet of Things Design and Implementation) — https://dl.acm.org/doi/fullHtml/10.1145/3576842.3582388 (403 a bots; metadata verificada vía Crossref, DOI 10.1145/3576842.3582388)
- Delgado et al. 2022, "Optimal Energy-Aware Task Scheduling for Batteryless IoT Devices" (wake-up radios + scheduling), **IEEE Transactions on Emerging Topics in Computing** — https://www.computer.org/csdl/journal/ec/2022/03/09446584

**El principio de diseño invertido:** estos sistemas NO confían en "morir limpio al final" — hacen **checkpoint del estado ANTES de consumir energía**, de modo que una muerte en cualquier punto es segura y la resurrección es trivial. Nuestros loops hacen lo contrario: asumen que vivirán hasta el final y tratan el kill como excepción. Si el loop escribiera *intención + estado* a disco **antes** de invocar el proceso externo (qué voy a hacer, con qué inputs, en qué paso voy), entonces: (a) matar es siempre seguro y por diseño, no por excepción; (b) el exit code deja de ser la fuente de verdad — **el checkpoint es la unidad de verdad**, y el observador verifica el checkpoint, no el código de salida del launcher. Un "testamento" de una línea antes de cada spawn habría detectado los tres incidentes sin grepear 871 MB de kernel.log.

**Análogo biológico:** torpor/hibernación — el animal baja su metabolismo a estado de dormancia *planificada* con reservas verificables, no como fallo. La dormancia es un estado de primera clase en la naturaleza; para nosotros debería serlo también (un loop que duerme no es un loop roto — y un loop colgado debería poder ser segado sin miedo porque su estado ya está en disco).

**Acción concreta:** en cada runner de loop (mi `coloquio_loop_buffy.py`, `agent_loop_deep.ps1` de Deep): escribir `data/loops/<loop_id>.state` (JSON: paso, intent, timestamp) antes de cada operación externa. El FleetHealthPanel de Antigravity ya puede leerlo y mostrar "en qué paso está cada loop" en vivo — observabilidad real sin tocar el kernel. **[PRÁCTICA]**

---

## Dolor 2 — 68% de recall perdido bajo concurrencia; KV-cache destruido por memoria dinámica

**Lo que nos costó ya:** el hallazgo P1 de Deep (recall cae bajo concurrencia, telemetría per-stage ya instrumentada en `1316e41`); el rediseño IoC de Antigravity (bwc-c31eee3d) nació de esto: inyectar memoria dinámica dentro del anchor destruye el prefijo determinista que la cache necesita.

**La ciencia:** el sector ya formalizó exactamente esta restricción:
- **KVFlow**, NeurIPS 2025 (poster 119883) — "workflow-aware KV cache management" para flujos agénticos: abstrae la ejecución del agente como grafo de pasos con **prefijos estables por etapa** y gestiona la cache alrededor de ellos — https://neurips.cc/virtual/2025/poster/119883
- "Recency Is Near-Optimal for LLM Prefix Caches", 2026 — teoría de eviction para prefix caches — https://www.alphaxiv.org/abs/2608.llm-prefix-cache-recency-optimal
- Multi-Segment Attention, arXiv 2606.02964 (jun 2026) — reutilización de KV entre segmentos — https://arxiv.org/html/2606.02964v1
- Confirmación operativa (vLLM): la reutilización es **solo por prefijo de tokens idéntico, nunca por conversation ID** — https://discuss.vllm.ai/t/how-should-i-set-kv-cache-in-vllm/1947

**La validación externa de nuestra decisión de diseño** (fuente secundaria, citada como pista no como prueba): un análisis de 2026 recoge dos estudios de 2025 sobre riesgos de residencia de datos en el prompt caching de proveedores cloud — https://medium.com/@michael.hannecke/the-hidden-data-residency-problem-in-prompt-caching-f99e6207451e (403 a bots). No resolví los papers primarios que cita; el argumento de fondo no depende de ellos: cachear prefijos localmente, donde la única tenencia es nuestra, elimina la clase de fuga por construcción. Nuestra postura local-first es el diseño correcto por seguridad además de por coste.

**Acción concreta:** el rediseño IoC ya aprobado va en la dirección exacta que la literatura consagra: anclaje estático del sistema (prefijo byte-idle determinista) + memoria dinámica inyectada **después** del anclaje, nunca dentro. KVFlow sugiere el siguiente paso natural: modelar nuestros flujos de agente (recall→think→do) como etapas con prefijos estables cada una, para que cada etapa herede la cache de la anterior. **[VALIDADO]** — la técnica está publicada y en producción en servidores de inferencia; nuestra adaptación específica es nuestra.

---

## Dolor 3 — Coordinar sin jerarquía (la estigmergia como mecanismo formal, no como metáfora)

**Lo que nos costó ya:** el TL asignando tarea a mano como cuello de botella; free-riding detectado (trabajo reclamado sin hacer); la misma verificación redundada porque nadie sabía quién estaba mirando qué.

**La ciencia:**
- **Parunak 1997, "Go to the Ant": Engineering Principles from Natural Multi-Agent Systems**, Annals of Operations Research 75:69-101 (238 citas según Crossref; otros agregadores cuentan más — el recuento depende de la base, se cita la fuente) — el paper canónico de feromonas digitales: agentes como "comportamientos empaquetados que se encuentran unos con otros interactuando a través del entorno", con las cuatro operaciones mecánicas: depósito, agregación, difusión, evaporación — https://jmvidal.cse.sc.edu/library/parunak97b.pdf
- Parunak 2001, "Adaptive Control of Distributed Agents Through Pheromones" — https://apps.dtic.mil/sti/tr/pdf/ADA398009.pdf (403 a bots; accesible en navegador)
- **Tero et al. 2010, "Rules for Biologically Inspired Adaptive Network Design", Science** (DOI 10.1126/science.1177894, 920 citas según Semantic Scholar a 2026-09-23) — Physarum polycephalum reconstruyó la red ferroviaria de Tokio con reglas puramente locales, logrando **eficiencia, tolerancia a fallos y costo comparables** a la red humana — https://pdodds.w3.uvm.edu/files/papers/others/2010/tero2010a.pdf

**El puente exacto con nuestro código (re-verificado con grep a 2026-09-23):** `touch_node()` deposita (34 call sites contando tests, repartidos entre los handlers de los 5 tools soberanos), `apply_decay` difunde y evapora, `get_stigmergy_heat()` lee — **ya tenemos las cuatro operaciones de Parunak sobre nodos de memoria**. La pieza nueva del ADR-015 (calor sobre zonas de trabajo) tiene respaldo directo: Tero demuestra que la regla local "engrosa lo usado, poda lo no usado" produce redes de calidad de ingeniería sin ningún planificador central — aplicado a procesos: los archivos/módulos que resuelven trabajo acumulan calor, los muertos se evaporan, y el loop de cada agente siente el calor afín a su rol. Sin lista de tareas centralizada para la rutina; el @mencion directo queda para lo urgente.

**El freno (no negociable):** sistemas sin coordinación central amplifican errores ~17.2x frente a un agente solo (zylos.ai/research/2026-05-23-swarm-intelligence-multi-agent-coordination-patterns, verificado). Consecuencia de diseño que ya practica la flota: **la estigmergia selecciona atención, nunca ejecuta sola** — el calor decide qué mirar primero; la verificación cruzada y el HITL deciden qué se acepta. **[VALIDADO]** el fenómeno, **[PROPOSAL]** la extensión a zonas de trabajo (nadie ha publicado esa pieza para agentes LLM — es nuestra investigación real).

---

## Dolor 4 — "Irrompible": cripto-agilidad para lo que hoy Noise/ChaCha20/Ed25519

**Lo que nos costaría no hacerlo:** nuestro cifrado federado (Noise NK/XK) y firmas (Ed25519) son correctos **hoy**; el riesgo no es que se rompan, es que la migración sea un rewrite si no preparamos la junta. El adversario relevante ya está documentado: *harvest now, decrypt later* (guardar hoy, descifrar cuando exista la máquina cuántica) — todo lo que capture hoy de tráfico federado de larga vida es vulnerable más adelante.

**La ciencia / el estándar:**
- NIST FIPS 203 (ML-KEM), 204 (ML-DSA), 205 (SLH-DSA) — estándares finales publicados — https://csrc.nist.gov/projects/post-quantum-cryptography
- NIST NCCoE, "Migration to Post-Quantum Cryptography" — https://pages.nist.gov/nccoe-migration-post-quantum-cryptography/
- Costa et al. 2026, "Migrating Legacy Systems to NIST PQC Standards" — https://eprint.iacr.org/2026/1467.pdf

**El principio de la literatura (crypto-agility):** el algoritmo debe ser una decisión **configurable detrás de una frontera de abstracción**, no un dato pegado al código. Para Tylluan: `tylluan-link` ya vive tras tipos de transporte particionables (`PartitionableTransport<T>`) — el mismo patrón aplicado a la capa cripto (trait `KeyExchange`/`Aead` con implementación actual Noise + futura Noise-PQ híbrida) hace de la migración un cambio de configuración. Análogo biológico: el exoesqueleto que muda — el organismo sobrevive el cambio porque el esqueleto es reemplazable por diseño, nunca fusionado con el cuerpo. **Acción mínima honesta: solo el trait y un test que compile una segunda implementación dummy** — nada de integrar PQ todavía. **[PRÁCTICA]**

---

## Dolor 5 — La memoria debe olvidar bien (el decay actual es una normalización de ventana; la biología hace un proceso de dos fases)

**Lo que nos costaría no hacerlo:** el calor estigmérgico de hoy es `count/(horas×10)` con cap 2.0 — lineal-por-ventana, documentado en T712. Funciona, pero no olvida como los sistemas que sí olvidan bien.

**La ciencia:**
- Born & Wilhelm, "System consolidation of memory during sleep", **Psychological Research** (online 2011, número 2012; 595 citas según Crossref) — la consolidación es un **proceso offline de dos fases**: etiquetado durante la experiencia, reorganización selectiva durante el sueño — https://link.springer.com/article/10.1007/s00426-011-0335-6
- Langille & Brown 2019, "Remembering to Forget: A Dual Role for Sleep Oscillations", Front. Cell. Neurosci. — el sueño **consolida Y borra** con la misma maquinaria — https://www.frontiersin.org/journals/cellular-neuroscience/articles/10.3389/fncel.2019.00071/full
- Lindsey 2024, eLife — la memoria a corto plazo como **señal de gating** que decide qué pasa a largo plazo — https://elifesciences.org/articles/90793

**El mapeo a Tylluan (mejor correspondencia 1:1 de todo el dossier):** los bucles de la flota **son literalmente el ciclo vigilia/sueño** de este sistema. Propuesta: un job offline (cron-able, como nuestros loops) que periódicamente: (1) **consolida** — lo accedido frecuentemente por recall real sube de tier (a esto sirve la señal de gating de Lindsey); (2) **olvida por no-retrieval** — lo no accedido en su ventana decae, ya lo hace `apply_decay`; (3) **rescribe** — compacta lo difuso. La fase offline es la pieza que la biología da por sentada y nosotros no tenemos. **[PROPOSAL]** — la neurociencia valida el mecanismo de fondo; su traducción a nuestro esquema concreto es investigación nuestra, con un experimento medible (recall@k antes/después de la fase offline).

---

## Resumen ejecutivo (para decidir, no para admirar)

| Dolor | Fuente canónica verificada | Acción mínima | Madurez |
|---|---|---|---|
| 1. Procesos colgados / exit codes falsos | Islam 2023 (IoTDI '23) | Checkpoint de intención+estado **antes** de cada spawn; el checkpoint es la verdad, no el exit code | PRÁCTICA |
| 2. Recall bajo concurrencia / KV-cache | KVFlow (NeurIPS 2025) | Rediseño IoC ya aprobado + prefijos estables por etapa de flujo | VALIDADO |
| 3. Coordinación sin jerarquía | Parunak 1997; Tero 2010 (Science) | ADR-015: calor de zonas con la física Tero (engrosar/podar); estigmergia selecciona atención, jamás ejecuta | VALIDADO fenómeno / PROPOSAL extensión |
| 4. Cripto-agilidad | NIST FIPS 203/204/205 + NCCoE | Solo trait `KeyExchange`/`Aead` + test de segunda impl; PQ integration NO ahora | PRÁCTICA |
| 5. Olvido bien hecho | Born 2012; Langille 2019 | Job offline de consolidación/poda (la fase "sueño" que nos falta) | PROPOSAL con experimento medible |

**El patrón de fondo (lo que pidió José que viéramos):** en los cinco dolores, la naturaleza/la ciencia converge en el mismo par de movimientos que ya definimos en el proyecto — **(a) la verdad vive en el estado persistido, nunca en el proceso vivo** (checkpoint > exit code; calor estigmérgico > mensaje directo; anclaje determinista > contexto reconstruido), y **(b) la coordinación barata gana a la comunicación cara** (feromona sobre el entorno vs. mensajería entre agentes; fases offline vs. reconfiguración en caliente). No necesitamos importar frameworks ajenos: necesitamos llevar nuestra propia arquitectura hasta donde la ciencia dice que está la solución — y en los cinco casos, la ciencia confirma que el camino correcto es el que ya empezamos.
