# Verificación cruzada del dossier (2026-09-23) — evidencia por fuente

**Quién:** Buffy (verificación de su propio T733/dossier, bajo la regla de la casa: lo que reporta el autor no es prueba).
**Cómo:** metadata estructurada vía APIs de registro (Semantic Scholar, Crossref) para cada paper con DOI; códigos de estado HTTP para cada URL citada; `grep` contra el código real para cada afirmación de implementación. Nada se aceptó de memoria — incluida la memoria del autor original.

## Resultado por fuente

| Fuente | Método | Resultado | Acción sobre el dossier |
|---|---|---|---|
| Tero et al. 2010, "Rules for Biologically Inspired Adaptive Network Design" | Semantic Scholar API, `DOI:10.1126/science.1177894` | ✅ Title/venue (Science)/year/autores exactos. Citas: **920**, no ~1.400 como se decía | Corregido el recuento con fecha de consulta |
| Parunak 1997, "Go to the Ant" | Crossref API, query bibliográfica | ✅ Title/venue (Annals of Operations Research)/year 1997/autor Van Dyke Parunak exactos. Citas Crossref: **238** (otros agregadores cuentan más; el recuento depende de la base) | Corregido citando la fuente del recuento |
| Parunak 2001 (DTIC) | HTTP status | 403 a bots (accesible en navegador) | Anotado en el dossier |
| Islam et al. 2023, "Amalgamated Intermittent Computing Systems" | Crossref API por título | ✅ Existe, pero venue real: **IoTDI '23 (ACM/IEEE Conf. on Internet of Things Design and Implementation)** — NO "ACM TECS" como decía el dossier. 7 citas Crossref. Página ACM: 403 a bots | Corregido venue + anotado el bloqueo |
| Delgado et al., "Optimal Energy-Aware Task Scheduling for Batteryless IoT Devices" | Crossref API | ✅ Venue real: **IEEE Transactions on Emerging Topics in Computing, 2022** — el dossier decía "2021, IEEE Trans. on Computers". 42 citas | Corregidos año y venue |
| Born & Wilhelm, "System consolidation of memory during sleep" | Crossref API, DOI directo `10.1007/s00426-011-0335-6` | ✅ Title/autores exactos. Venue real: **Psychological Research** (online 2011, número 2012) — el dossier decía "Springer ~1.000 citas". Citas Crossref: **595** | Corregidos venue, año y recuento |
| Langille & Brown 2019 (Frontiers), Lindsey 2024 (eLife) | HTTP status | ✅ 200 ambos | Sin cambios |
| KVFlow (NeurIPS 2025, poster 119883) | HTTP status + snippet en el momento de la redacción | ✅ 200; atribución (workflow-aware KV cache management, ejecución como grafo de pasos) consistente con lo publicado | Sin cambios |
| "Recency Is Near-Optimal for LLM Prefix Caches" (alphaxiv) / Multi-Segment Attention (arXiv 2606.02964) / foro vLLM | HTTP status | ✅ 200 los tres; la cita de vLLM (reutilización solo por prefijo de tokens, nunca conversation ID) proviene del snippet verificado al redactar | Sin cambios |
| Medium (residencia de datos en prompt caching) | HTTP status | 403 a bots. **Problema mayor que el 403:** el dossier atribuía "dos estudios peer-reviewed 2025 (Stanford)" sin haber resuelto los papers primarios — los conocía vía esta fuente secundaria | **Degradado a fuente secundaria explícita**, con declaración de que los primarios no fueron resueltos y de que el argumento no depende de ellos |
| zylos.ai (17.2x amplificación de errores) | HTTP status | ✅ 200. Ya estaba etiquetada como fuente secundaria en el dossier | Sin cambios |
| NIST CSRC (PQC) / NCCoE Migration / eprint 2026/1467 | HTTP status | ✅ 200 los tres. FIPS 203/204/205 como estándares finales consistente con la página de NIST | Sin cambios |

## Afirmaciones de código verificadas con grep (decay.rs, kernel 93d99b0 + working tree a 2026-09-23)

| Afirmación del dossier | Evidencia | Resultado |
|---|---|---|
| Fórmula de calor `count/(horas×10)`, cap 2.0 | `decay.rs:318-330` — `let heat = (count as f64 / (hours * 10.0)).min(2.0);` | ✅ Exacta |
| Difusión semántica coseno >0.85 → 1-3 trazas proporcionales | `decay.rs:294-301` — umbral 0.85, mapeo `(sim-0.85)/0.15*2` redondeado +1, `.min(3)` | ✅ Exacta |
| `prune_old_traces` existe | `decay.rs:177` | ✅ |
| "`touch_node()` deposita (11 call sites)" | `grep -rn "touch_node(" --include="*.rs" \| grep -v "fn touch_node" \| wc -l` → **34** (incluye tests) | Corregido: 34 call sites contando tests; la cifra original de 11 venía de un conteo anterior sobre handlers |

## Veredicto de sobre-afirmación (madurez/aplicabilidad)

- Dolor 1 [PRÁCTICA]: sostenido — la literatura existe y está verificada; la acción propuesta es adaptación local, no trasplante de paper.
- Dolor 2 [VALIDADO]: sostenido con el matiz ya escrito — la técnica (prefijos estables por etapa) está publicada y en producción en infraestructura de serving ajena; la adaptación a Tylluan es nuestra.
- Dolor 3 [VALIDADO fenómeno / PROPOSAL extensión]: sostenido; el 17.2x está correctamente atribuido a fuente secundaria.
- Dolor 4 [PRÁCTICA]: sostenido; la acción mínima (trait + test, sin integrar PQ) no sobreactúa la fuente.
- Dolor 5 [PROPOSAL]: sostenido; declarado como hipótesis nuestra con experimento pendiente.

## Correcciones totales: 6 materiales (venue×2, año×2, recuentos de citas×3, call sites×1, degradación de fuente×1 — algunas afectan a más de un ítem)

Ninguna corrección cambia el veredicto de fondo de los 5 dolores ni la acción mínima propuesta; todas afinan trazabilidad. El dossier corregido vive en `TEAM_RESEARCH_DOSSIER.md` (mismo commit que este fichero).
