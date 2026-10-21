# H-1 — Las cuatro fuentes primarias que el documento de preguntas dejó sin verificar

**Tarea:** H-1 (asignación directa de José, 2026-10-21). **Agente:** Hermes. **Naturaleza:** investigación en fuentes primarias, solo lectura — ningún fichero del repo fue modificado; este es el único fichero nuevo, sin commitear.
**Base del worktree:** `514058e` (HEAD == origin/main verificado 2026-10-21 con `git fetch origin && git rev-parse HEAD origin/main`).
**Documento destino:** `docs/reference/research/2026-10-21_preguntas_correctas_camino_a_v1.md` — cierra los cuatro `[NO VERIFICADO]` de las líneas 104, 126, 127 y 166.
**Método:** cada cita fue extraída del texto del sitio oficial (EUR-Lex / OWASP / arXiv), no de buscadores ni resúmenes. Nivel `[FUENTE]` en todos los bloques.

---

## 1. OWASP — lista oficial de riesgos agénticos

**Fuente primaria (página oficial del recurso):** https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/ — publicada **9 de diciembre de 2025**.

> «The OWASP Top 10 for Agentic Applications 2026 is a globally peer-reviewed framework that identifies the most critical security risks facing autonomous and agentic AI systems. Developed through extensive collaboration with more than 100 industry experts, researchers, and practitioners, the list provides practical, actionable guidance to help organizations secure AI agents that plan, act, and make decisions across complex workflows.»

**La lista completa (PDF oficial, https://genai.owasp.org/download/52117/, v2026, diciembre 2025; nombres confirmados también en el anuncio oficial de OWASP, https://genai.owasp.org/2025-12-09/owasp-top-10-for-agentic-applications-the-benchmark-for-agentic-security-in-the-age-of-autonomous-ai/):**

| # | Riesgo | Incidente citado por OWASP |
|---|--------|----------------------------|
| ASI01 | Agent Goal Hijack | EchoLeak |
| ASI02 | Tool Misuse & Exploitation | Amazon Q |
| ASI03 | Identity & Privilege Abuse | — |
| ASI04 | Agentic Supply Chain Vulnerabilities | exploit de GitHub MCP |
| ASI05 | Unexpected Code Execution (RCE) | AutoGPT RCE |
| ASI06 | Memory & Context Poisoning | Gemini Memory Attack |
| ASI07 | Insecure Inter-Agent Communication | — |
| ASI08 | Cascading Failures | — |
| ASI09 | Human-Agent Trust Exploitation | — |
| ASI10 | Rogue Agents | Replit meltdown |

Del anuncio oficial, sobre ASI06 (cita literal): «Memory poisoning reshaped behaviour long after the initial interaction (ASI06 – Memory & Context Poisoning, e.g Gemini Memory Attack).»

**Encaje con Tylluan (pertinencia, no fuente):** el repo ya usa esta taxonomía — «CoherenceGate ASI06 capa 1» (Q5 del documento, T-DSH-15 verifica «B-12 (ASI06 capa 1 vs D-17)»). La designación ASI06 («Memory & Context Poisoning») es exactamente la categoría oficial que cubre la defensa `untrusted_content`/banner/cuarentena. ASI07 corresponde al hueco de escritores de federación/A2A cerrado en T-DSH-22; ASI03 es el hueco de identidad declarativa de Q6. La lista existe y es citable — el fallo de la sesión TL («la página no la mostró») fue de herramienta, no de disponibilidad.

**Comando de verificación:** `curl -s https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/ | grep -o "Top 10 for Agentic Applications 2026"` → devuelve el título. Descarga del PDF: https://genai.owasp.org/download/52117/

---

## 2. Ley de Ciberresiliencia (CRA) — régimen del software libre y la figura del «steward»

**Fuentes primarias:**
- Texto oficial DO: Reglamento (UE) 2024/2847, https://eur-lex.europa.eu/eli/reg/2024/2847/oj/eng (OJ L, 20.11.2024).
- Texto consolidado: https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX%3A02024R2847-20241120
- Página oficial de la Comisión sobre el régimen de software libre: https://digital-strategy.ec.europa.eu/en/policies/cra-open-source (actualizada **15 de octubre de 2026**).

### 2.1 Fechas — la afirmación del documento (L127) es correcta

Artículo 71 (texto consolidado, literal): «This Regulation shall enter into force on the twentieth day following that of its publication in the Official Journal of the European Union. […] This Regulation shall apply from 11 December 2027. However, Article 14 shall apply from 11 September 2026 and Chapter IV (Articles 35 to 51) shall apply from 11 June 2026.»
Publicado el 20-11-2024 → entrada en vigor 10-12-2024 ✔; notificación de vulnerabilidades explotadas desde 11-09-2026 ✔; obligaciones principales desde 11-12-2027 ✔. **Añadir dato que el documento no menciona:** los organismos notificados / evaluación de conformidad (Cap. IV) ya son aplicables desde el **11 de junio de 2026** (fecha ya vigente hoy).

### 2.2 El software libre no monetizado queda fuera del alcance

Considerando (18) (literal, DO):
> «only free and open-source software made available on the market, and therefore supplied for distribution or use in the course of a commercial activity, should fall within the scope of this Regulation. […] the provision of products with digital elements qualifying as free and open-source software that are not monetised by their manufacturers should not be considered to be a commercial activity. […] the mere presence of regular releases should not in itself lead to the conclusion that a product with digital elements is supplied in the course of a commercial activity. […] This Regulation does not apply to natural or legal persons who contribute with source code to products with digital elements qualifying as free and open-source software that are not under their responsibility.»

Confirmado por la Comisión (página oficial, literal): «only free and open-source software that is made available on the market, and therefore supplied for distribution or use in the course of a commercial activity, falls in scope of the Cyber Resilience Act. Notably, the provision of products with digital elements qualifying as free and open-source software that are not monetised by their manufacturers should not be considered to be a commercial activity.»

### 2.3 La figura del «open-source software steward»

Definición, Artículo 3(14) (literal):
> «‘open-source software steward’ means a legal person, other than a manufacturer, that has the purpose or objective of systematically providing support on a sustained basis for the development of specific products with digital elements, qualifying as free and open-source software and intended for commercial activities, and that ensures the viability of those products;»

Régimen (Considerando 19): «legal persons who provide support on a sustained basis for the development of such products which are intended for commercial activities, and who play a main role in ensuring the viability of those products (open-source software stewards), should be subject to a light-touch and tailor-made regulatory regime.» Y no pueden llevar marcado CE.

Obligaciones, Artículo 24 (resumen literal de sus 3 apartados): (1) política de ciberseguridad documentada verificablemente que fomente el desarrollo seguro y el manejo de vulnerabilidades por los desarrolladores; (2) cooperación con las autoridades de vigilancia del mercado a requerimiento; (3) las obligaciones del Art. 14(1) (notificación de vulnerabilidades activamente explotadas) les aplican «to the extent that they are involved in the development», y las del 14(3)/(8) (incidentes graves) a sus sistemas de información de desarrollo.

Régimen sancionador, Artículo 64(10)(b) (literal): las multas administrativas no se aplican a «any infringement of this Regulation by open-source software stewards.» (Confirmado por la Comisión: «In accordance with Article 64(10), open-source software stewards are not subject to administrative fines for infringements of the CRA.»)

### 2.4 Consecuencia para Tylluan (mapa para el jurista, no dictamen)

- Definición de fabricante, Art. 3(5) (literal): «markets them under its name or trademark, **whether for payment, monetisation or free of charge**». El disparo del alcance no es el cobro en la definición de fabricante, sino el «making available on the market» = suministro «in the course of a commercial activity» (Art. 2(1) + Considerando 18).
- Estado actual del proyecto: MIT, releases gratuitas, sin monetización conocida → por el Considerando 18 y la página de la Comisión, **hoy no sería actividad comercial** y no dispararían las obligaciones de fabricante (Art. 13). Riesgo concreto para la pregunta del documento («¿es José fabricante?»): se activa en el momento en que haya monetización (venta, soporte pagado, dual con edición comercial).
- La figura de steward exige **persona jurídica** distinta del fabricante; un desarrollador individual que publica su propio proyecto no encaja en ella (encaja en fabricante o fuera de alcance). El Considerando 18 excluye a quien «contribuye con código a FOSS que no está bajo su responsabilidad» — ese no es el caso de José, que es el mantenedor responsable.
- Si Tylluan se integra «in a commercial activity» de terceros, el integrador sí asume obligaciones de fabricante, incluida la debida diligencia sobre componentes FOSS (Art. 13(5)) — relevante para el caso ancla (clínica).

---

## 3. SLMJury — arXiv 2606.07810 (la cita de STATUS.md queda verificada)

**Fuente primaria:** https://arxiv.org/abs/2606.07810 — «✦ SLMJury: Can Small Language Models Judge as Well as Large Ones?», Anish Laddha, Nitesh Pradhan (LNMIIT, Jaipur) y Gaurav Srivastava (Virginia Tech). Enviado **viernes 5 de junio de 2026, 19:38:15 UTC (v1)**, cs.CL. Código y leaderboard: https://github.com/anishh15/SLMJury

**Cita literal del hallazgo (4) del abstract:**
> «Under the Reflect-Critique-Refine (RCR) debate protocol, multi-agent debate degrades accuracy across all tested configurations, whereas the top judges resist six adversarial personas with <=0.55% variance.»

**Cifras de apoyo (§4.4, literal del HTML oficial):** «The best debate (Qwen3-4B Variant B, 87.04%) falls 0.77% below Qwen3-4B's individual 87.81% at B=8,192; Phi-4-mini Variant B (86.92%) loses 0.69%; and the best cross-architecture Variant A (84.52%) loses 3.29% relative to its strongest constituent.» Y su implicación práctica (literal): «For binary correctness judging, a single strong judge (Phi-4, 89.55%) is simultaneously more accurate and cheaper, by a factor of three in forward passes, than any ensemble or debate we evaluated.»

**Veredicto sobre STATUS.md:69:** la atribución es correcta en sustancia — «el debate multi-agente degrada la precisión de modelos pequeños» está verificada. Dos matices honestos:
1. El término «compute-matched» **no aparece** en el artículo; la comparación es contra la línea base individual del miembro más fuerte con el mismo presupuesto de tokens (B=8,192), más el argumento de coste «a factor of three in forward passes». Es una paráfrasis razonable del equipo, no una cita del paper.
2. Alcance declarado del resultado (literal): «the result is specific to RCR and binary judging, and other aggregation rules may behave differently», y «This contrasts with open-ended settings where debate can help (Chan et al., 2024)». El NO-GO de la Fase 0 de Tylluan midió **generación de respuestas** (arm deliberativo A-SSA), no veredictos binarios: SLMJury corrobora la dirección (el debate no ayudó en ningún configurado probado por ellos) pero no es prueba directa para el caso abierto de Tylluan. El cierre con McNemar de Decima-small (Q9) sigue siendo lo decisivo.

**Comando de verificación:** `curl -s https://arxiv.org/abs/2606.07810 | grep -o "multi-agent debate degrades accuracy across all tested configurations"`

---

## 4. Reglamento (UE) 2017/745 — definición de «producto sanitario»

**Fuente primaria:** https://eur-lex.europa.eu/eli/reg/2017/745/art_2/oj/eng (EUR-Lex, artículo 2 del MDR en su versión consolidada).

**Artículo 2(1) (literal, definición completa):**
> «‘medical device’ means any instrument, apparatus, appliance, **software**, implant, reagent, material or other article intended by the manufacturer to be used, alone or in combination, for human beings for one or more of the following specific medical purposes:
> — diagnosis, prevention, monitoring, prediction, prognosis, treatment or alleviation of disease,
> — diagnosis, monitoring, treatment, alleviation of, or compensation for, an injury or disability,
> — investigation, replacement or modification of the anatomy or of a physiological or pathological process or state,
> — providing information by means of in vitro examination of specimens derived from the human body, including organ, blood and tissue donations,
> and which does not achieve its principal intended action by pharmacological, immunological or metabolic means, in or on the human body, but which may be assisted in its function by such means.»

Además, Artículo 2(4) (literal, frase final): «Software shall also be deemed to be an active device.»

**La bisagra jurídica es la «intended purpose» — Artículo 2(12) (literal):**
> «‘intended purpose’ means the use for which a device is intended according to the data supplied by the manufacturer on the label, in the instructions for use or in promotional or sales materials or statements and as specified by the manufacturer in the clinical evaluation;»

**Consecuencia para el caso ancla (mapa para el jurista, no dictamen):** la calificación como producto sanitario depende de la **finalidad declarada por el fabricante** (etiqueta, IFU, materiales promocionales, evaluación clínica), no del uso que un tercero haga del software. El «software» está expresamente incluido en el Art. 2(1): un disclaimer «no es producto sanitario» es necesario pero **no suficiente por sí solo** — si el propio proyecto declara o promueve finalidades médicas (diagnóstico, monitorización, predicción…), pasa a ser producto sanitario con independencia de la licencia MIT. Si el usuario final (médico) lo reorienta a una finalidad médica propia, el análisis de quién asume el rol de fabricante (Arts. 2(38)/5 del MDR) es pregunta pendiente para el jurista, no resuelta aquí. La recomendación del documento (L129: aviso «no es producto sanitario» en README y quickstart) queda respaldada, con el matiz: debe ir paired a no promover usos médicos en la documentación pública — porque la propia documentación es lo que el Art. 2(12) mira.

**Comando de verificación:** `curl -s https://eur-lex.europa.eu/eli/reg/2017/745/art_2/oj/eng | grep -o "does not achieve its principal intended action by pharmacological"`

---

## Divergencias encontradas (las tres, todas menores)

1. **STATUS.md:69 — «compute-matched»:** paráfrasis del equipo, no término del paper. El paper compara con el miembro individual más fuerte (mismo B) y añade coste en forward passes. La cita sustantiva es correcta; conviene ajustar la frase en STATUS o marcarla como paráfrasis.
2. **Documento Q9 (L104):** SLMJury es *consistente* con el NO-GO de la Fase 0, pero su resultado es específico de **juicio binario bajo RCR**; la Fase 0 midió generación deliberativa. No usarlo como prueba cerrante del caso abierto.
3. **Documento Q11 (L127):** falta una fecha — Cap. IV del CRA aplicable desde 11-06-2026 (ya vigente), además de las dos que cita.

## Qué este fichero NO hace

- No modifica el documento de preguntas ni ningún otro fichero del repo — la integración es decisión de José tras revisión.
- No es asesoría legal: son citas literales + mapa de consecuencias para el jurista (igual que el documento original).
- No tocó código, gates, STATUS.md ni docs existentes. `git status --short` en este worktree debe mostrar **solo** este fichero como untracked.

## Registro de verificación (comandos reales ejecutados hoy)

```bash
cd D:/hermes-projects/tylluan-wt-hermes
git fetch origin && git rev-parse HEAD origin/main   # ambos: 514058ef6fbde466ee083fda41a7872ee42f2e22
git status --short                                  # solo este fichero, untracked
```

Extracción de citas: texto completo de cada URL oficial capturado y buscado literalmente (OWASP resource page + announcement; eur-lex CELEX 02024R2847 consolidado + OJ `eli/reg/2024/2847/oj/eng` para considerandos; `eli/reg/2017/745/art_2` para el Art. 2 MDR; arxiv.org/abs/2606.07810). Fechas de captura: 2026-10-21.
