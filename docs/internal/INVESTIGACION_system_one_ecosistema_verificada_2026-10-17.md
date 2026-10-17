# Ecosistema System One / Jev: investigación verificada (2026-10-17)

**Autor:** Claude Code (TL). **Método:** cada afirmación se contrastó leyendo la fuente primaria (tarjeta del modelo, README, PR, documentación del proveedor). Lo que sólo consta en prensa o agregadores se marca como *secundario*. Sustituye, para las decisiones de Tylluan, a `INVESTIGACION_decima_clef_hermes` de Antigravity, que contenía errores (ver §6).

## 1. Qué es un modelo System One

Un modelo que recibe un `state` (texto, JSON o imágenes) y preguntas tipadas (`noul` verdadero/falso, `choice`, `score`) y devuelve **una probabilidad por opción en una sola pasada, sin generar texto**. Lo lanzó TypeSafe AI con **Jev** (anunciado el 15 de septiembre de 2026; CEO Diogo Almeida, antes en OpenAI). El nombre viene de *Thinking, Fast and Slow* (Kahneman) y el de "Jev" de William Stanley Jevons, según el propio blog de TypeSafe.

- Jev es **API cerrada**. Los pesos no son abiertos y la técnica **no está publicada**: en la entrevista de Latent Space, ante la pregunta por un paper, Almeida responde "No, not yet".
- Su método declarado es RLCD (*Reinforcement Learning for Calibrated Decisions*), sin detalles de entrenamiento. Su tesis: la calibración "se envenena" bajo RLHF.
- El dialecto de API se llama `/v1/systemone` y lo han copiado todos los clones. OpenAI publicó su propio dialecto, `POST /v1/decisions` (modelo `gpt-6-luna`, **beta pública**, con tipos `predicate`/`choice`/`score`).

## 2. Linaje de investigación (las fuentes de las que beben)

| Pieza | Fuente | Estado |
|---|---|---|
| Nombre y marco System 1/2 | Kahneman, *Thinking, Fast and Slow* | citado por TypeSafe |
| Entrenar con RL y preferencias | Christiano et al. 2017; Stiennon et al. 2020; Ouyang et al. 2022 (InstructGPT) | citados por Almeida en entrevista |
| Destilación con distribuciones del maestro | Hinton, Vinyals, Dean 2015 | citado en el informe de Decima |
| Calibración (ECE) | Guo, Pleiss, Sun, Weinberger, ICML 2017 | citado en el informe de Decima |
| Interacción tardía opción-pregunta | Khattab y Zaharia, ColBERT, SIGIR 2020 | citado en Decima |
| Codificador base | Wang et al. 2024, Multilingual E5; mmBERT (Marone et al. 2025); ModernBERT (Laya) | citados |
| Regresión ordinal para `score` | McCullagh 1980 | citado en Decima |
| Evaluación zero-shot | Aarab, BTZSC, ICLR 2026, arXiv 2603.11991 (22 datasets) | verificado |
| Maestro de Decima | Gemma 4 Technical Report (Google DeepMind 2026) | citado |
| Reglas de puntuación estrictamente propias | Laya las usa como señal de RL; la referencia clásica es Gneiting y Raftery 2007 | referencia clásica de mi conocimiento, **no verificada en esta sesión** |
| Arquitectura de Jev reconstruida | Archer Hume, *Jev's Architecture Unmasked*, base declarada de Kev | **no encontrada por buscador; no verificada** |

La conclusión honesta: **no existe un paper original de Jev**. Todo el ecosistema reproduce la idea desde la API pública y desde literatura anterior (calibración, destilación, interacción tardía, NLI).

## 3. El ecosistema de clones y alternativas

| Modelo | Autor | Base y tamaño | Licencia | Notas verificadas |
|---|---|---|---|---|
| **Jev** | TypeSafe AI | no publicado | API cerrada | referencia; sin pesos |
| **Clef / Clef-Flash** | Cloudflare | Clef-Flash 9B sobre Qwen3.5-9B | Apache-2.0 | tarjeta oficial; sin papers citados; Workers AI y `POST /v1/systemone` |
| **Decima** (small 122M, base 321M, agent) | amyrmahdy | mE5-small (small), mmBERT-base (base y agent) | Apache-2.0 en pesos, **con datos NC** | ONNX int8 de 127 MB (small); ver §4 |
| **Kev** | Jared Palmer | LoRA rango 16 sobre Qwen (0.8B/4B/9B); 27B con ajuste completo | Apache-2.0 | declara sus límites; ONNX comunitario (kev-4b) |
| **Laya** | Convai Innovations | ModernBERT-large 421M; multilingüe 322M | Apache-2.0 | RL con reglas propias; procedencia de datos no declarada; aviso de ">20 opciones" |
| **Jeff** | mstrasser (independiente) | Qwen3.5 0.8B/2B, Gemma 4 E2B | MIT código, Apache-2.0 pesos | cifras autoinformadas; usuarios reportan menos que Jev |
| pplx-decider-v1-27b | Perplexity AI | Qwen3.8-27B | Apache-2.0 | ajuste completo |
| Surogate Rune, Decider, Cygnet, Winnow, Liquid d1, otros | varios | 12B a 31B | mayoría Apache-2.0 | ver clasificación |

**Clasificaciones** (resumen del post de discuss.pytorch.kr, que cita las ejecuciones de terceros): en el *Jev Decision Index* 0.2.1 Jev saca 57.91 y los mejores abiertos rondan 57.4; Laya 6.04; Kev-9B 38.48. En JevBench v1.5.7 el primero es Cygnet (73.7). La mayoría de los modelos que lideran son de 12B a 31B; los pequeños que podrían correr en CPU puntúan mucho peor (Kev-9B 38.48, Laya 6.04, LFM2.5-2.6B 6.76). Un resumen de Arize afirma que Liquid d1 superó a Jev (58.9 frente a 57.9); no lo verifiqué contra la fuente.

Tres reservas sobre los benchmarks: casi todos los números de modelo son **autoinformados**; en el Decision Index las **negativas y las preguntas sin respuesta cuentan como errores**; y en JevBench 67 de 74 pares adyacentes son empates estadísticos.

## 4. Hallazgos que afectan a Tylluan

1. **No hay una forma documentada de "negativa" en el dialecto `/v1/systemone`.** Ni TypeSafe, ni Cloudflare, ni Kev, ni Decima describen que una pregunta pueda omitirse. El esquema de salida de Cloudflare no exige claves en `answers`, así que la omisión es técnicamente posible pero **sin documentar**. Decima añade un indicador `abstain` sólo si el llamante pasa `min_confidence`. El dialecto de OpenAI sí tiene un tipo `refusal`. **Conclusión:** la omisión de clave que anotamos en el spike de Deep es un comportamiento observado, no un contrato. El tratamiento defensivo de la fase 2 (pregunta ausente = sin respuesta) es el correcto.
2. **Decima declara que no sirve de puerta de seguridad.** Su informe y su tarjeta excluyen comprobar comandos de shell o llamadas a herramientas: un comando destructivo recibió 0.693 de "no destructivo". La tarjeta de decima-agent añade que envió dos comandos destructivos a "ask" en lugar de "deny". **Ningún modelo de esta familia debe sustituir las reglas deterministas** de `dispatch_allow` o del Coherence Gate. Es coherente con el invariante v8 que ya probamos.
3. **Decima flaquea justo donde más se podría usar.** Su informe admite que en decisiones largas con varios hechos (typed-decisions) puntúa 0.399, **por debajo de la línea base de la clase mayoritaria (0.461)**; su puntuación oficial en JDI es 12.8, **por debajo del azar (27.1)**; y su ECE medio empaquetado es bueno (0.056), pero el propio informe señala que, tras reajustar la temperatura, su ECE en typed-decisions (0.075) es el peor de los seis sistemas comparados. Sirve para preguntas acotadas y estados cortos, como `scheduler_routing`, y habría que medirlo con nuestro harness pareado.
4. **Licencias de entrenamiento.** Decima-small se entrenó con ANLI y XNLI (CC BY-NC 4.0), AG News y SST-5 (sin licencia formal) y multilingual-NLI-26lang (hereda NC). Su propio `LICENSING.md` dice que **no está resuelto** si el NC alcanza a los pesos, y recomienda reentrenar sin esos datos si hay uso comercial. **decima-base** también usó particiones de XNLI/MNLI; **decima-agent** no aclara si incluye datos NC. Los pesos son Apache-2.0, pero el riesgo residual existe.
5. **El dataset limpio no es Apache-2.0.** `decima-system-one-tasks` es **CC BY-4.0** (138k decisiones sintéticas, escritas y etiquetadas por Gemma 4 26B-A4B; 1.4 % de `s2` lo escribió Qwen3-Coder-Next; "no verificado por humanos"). Atribución obligatoria, sin trabas de uso comercial.
6. **Servir en local ya es viable.** llama.cpp fusionó el soporte de Clef (sólo texto) el 3 de octubre de 2026 (PR 29831, commit 99b9548), con limitación de una secuencia por lote. vLLM fusionó `/v1/systemone` el 8 de octubre (PR 59299), sólo para preguntas `choice`. `decima serve` expone `/v1/systemone` en el puerto 11436. Esto valida el cliente HTTP de la fase 2.
7. **Clef-Flash pesa 9B de parámetros** (corrige mi nota previa de "9 GB"). La cifra de 27B que circula corresponde a Clef sin "Flash" y sólo la da una fuente secundaria.

## 5. Recomendaciones

- **Fase 3 sigue bloqueada para "empaquetar Decima".** Mi propuesta: **Tylluan no distribuye pesos.** El proveedor de la fase 2 es un cliente HTTP a un modelo que el usuario instala. La documentación lista modelos por licencia y tamaño y deja la elección al usuario. Así el riesgo NC no recae en el proyecto.
- Candidatos para probar con el harness pareado (todos Apache-2.0, ninguno con datos limpios garantizados): Decima-small (más ligero, 127 MB), Kev-0.8B (límites declarados con honestidad) y Laya (por su calibración, con reservas por los datos).
- **Nunca** conectar un modelo System One como puerta de seguridad. Mantener v8.
- Dejar `/v1/decisions` (dialecto OpenAI) como diseño, tal como se acordó: sigue en beta.
- Congelar con hash los pesos que se evalúen antes de cualquier adopción (pendiente de la fase 3).

## 6. Correcciones al informe de Antigravity

- Afirmó que el dataset limpio es Apache-2.0: **es CC BY-4.0**.
- Afirmó (y yo lo había anotado) que la negativa del modelo es una clave ausente en `answers`: **no está documentado en ninguna fuente primaria revisada**.
- Citó `STATE_TRUNCATED` en `decima/systemone.py`: **ese código no aparece en ese módulo**, que trocea los estados largos en lugar de truncarlos; no revisé `serve.py`, donde también lo situaba.
- Acertó en: licencias NC de Decima (tabla de `LICENSING.md`), Clef-Flash Apache-2.0 con `/v1/systemone`, llama.cpp y vLLM fusionados, y la existencia de `POST /v1/decisions` de OpenAI.

## 7. Qué no verifiqué

El Hugging Face Space del Decision Index (sólo vi resúmenes de terceros), la entrada original del blog de Cloudflare, el post de Archer Hume, el endpoint de Workers AI, `serve.py` de Decima, el aviso de ">20 opciones" de Laya y la cifra de Liquid d1.
