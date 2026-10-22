# Candidatos de modelo para juez de inyección (ES/EN) — 2026-10-22

**Autor:** Hermes (coder). **Encargo H-4** (TL T1535). **Formato:** tabla modelo → licencia → parámetros → idiomas (incluye ES) → métricas del autor (cita literal + URL) → cabe en CPU 8 GB. Máx 80 líneas.

---

## Modelos candidatos (1.5 B–4 B o clasificadores pequeños)

| Modelo | Licencia | Parámetros | Idiomas (incluye ES) | Métricas declaradas por el autor (cita literal + URL) | CPU 8 GB |
|--------|----------|------------|----------------------|--------------------------------------------------------|----------|
| **Llama Prompt Guard 2 22M** | Llama 4 Community | 22 M (DeBERTa-xsmall) | EN, FR, DE, HI, IT, PT, **ES**, TH (86M); **22M es English-only** | «Llama Prompt Guard 2 22M reduces latency and compute costs by 75%, with minimal performance trade-offs.» / «Both Prompt Guard 2 models have been evaluated for attack detection in English, French, German, Hindi, Italian, Portuguese, Spanish, and Thai.» | ✅ Sí (~70 MB FP32) |
| **Llama Prompt Guard 2 86M** | Llama 4 Community | 86 M (mDeBERTa-base) | EN, FR, DE, HI, IT, PT, **ES**, TH | «Llama Prompt Guard 2 86M uses a multilingual base model and is trained to detect both English and non-English injections and jailbreaks.» / AUC English **0.998**, Recall@1% FPR **97.5%**; AUC Multilingual **0.995** | ✅ Sí (~350 MB FP32) |
| **Qwen3Guard-Gen-0.6B** | Apache-2.0 | 0.6 B (752 M) | **119 idiomas y dialectos** (incluye ES, EN, FR, DE, PT, RU, ZH, JA, KO, AR, HI, etc.) | «Qwen3Guard-Gen supports 119 languages and dialects, ensuring robust performance in global and cross-lingual applications.» / «Achieves state-of-the-art performance on various safety benchmarks, excelling in both prompt and response classification across English, Chinese, and multilingual tasks.» | ✅ Sí (~1.5 GB BF16 / ~0.5 GB Q4_K) |
| **Qwen3Guard-Stream-0.6B** | Apache-2.0 | 0.6 B | **119 idiomas y dialectos** (incluye ES) | «The model is multilingual (the Qwen3Guard series covers up to 119 languages) and released under Apache-2.0.» / Token-level classification head for real-time safety monitoring during incremental text generation. | ✅ Sí (~1.5 GB BF16 / ~0.5 GB Q4_K) |
| **Qwen3Guard-Gen-4B / Stream-4B** | Apache-2.0 | 4 B | **119 idiomas** (incluye ES) | «Both variants are available in three sizes (0.6B, 4B, and 8B parameters) and support up to 119 languages and dialects.» / SOTA on multilingual safety benchmarks. | ⚠️ Límite (~4 GB Q4_K + KV) |
| **ProtectAI deberta-v3-small-prompt-injection-v2** | Apache-2.0 | ~44 M (DeBERTa-v3-small) | **English only** (card: «Language(s) (NLP): English») | «This model is a fine-tuned version of microsoft/deberta-v3-small specifically developed to detect and classify prompt injection attacks.» / Archivado (no mantenido activamente). | ✅ Sí (~150 MB) |
| **ProtectAI deberta-v3-base-prompt-injection-v2** | Apache-2.0 | ~184 M (DeBERTa-v3-base) | **English only** | «Fine-tuned version of the microsoft/deberta-v3-base on multiple dataset of prompt injections and normal prompts to classify text.» / Usado como default en llm-guard y Arthur.ai. | ✅ Sí (~700 MB) |
| **WildGuard (7B)** | Apache-2.0 | 7 B (base Mistral-7B-v0.3) | **English only** (card: «Language(s): en») | «WildGuard outperforms the strongest existing open-source baselines (including Llama-Guard2 and Aegis-Guard) on F1 scores across all three tasks (by up to 25.3% on refusal detection), matches GPT-4 across tasks, and surpasses GPT-4 by up to 4.8% on adversarial prompt harmfulness.» | ❌ No (7B BF16 ≈ 14 GB; Q4_K ≈ 4.5 GB + KV > 8 GB) |
| **Llama Guard 3 1B** | Llama 3.2 Community | 1 B (Llama-3.2-1B base) | EN, FR, DE, HI, IT, PT, **ES**, TH (8 idiomas) | «Llama Guard 3-1B supports content safety for the following languages: English, French, German, Hindi, Italian, Portuguese, Spanish, Thai.» / Aligned to MLCommons standardized hazards taxonomy. | ✅ Sí (~2 GB BF16 / ~0.7 GB Q4_K) |
| **ShieldGemma 2B** | Gemma Terms of Use | 2.6 B (Gemma-2-2B base) | **English only** (card & docs: «available in English») | «ShieldGemma is a series of safety content moderation models built upon Gemma 2 that target four harm categories (sexually explicit, dangerous content, hate, and harassment).» / Benchmark: SG Prompt 0.825/0.887, ToxicChat 0.704/0.778. | ✅ Sí (~1.7 GB Q4_K) |
| **prompt-guard-1.5b (Prismor)** | No declarada en card (base Qwen2.5-1.5B-Instruct = Apache-2.0) | 1.5 B (2 B params en HF) | **English only** (card: «Limitations: English-only. Both training datasets are English. Performance on non-English injections is unknown.») | «Our model leads on F1 (0.726) and AUC (0.757) — best overall balance. Highest recall (0.893) — catches 89% of attacks, at cost of higher FPR (0.379).» (rogue-security benchmark, 5k samples) | ✅ Sí (~3-4 GB BF16 / ~1.2 GB Q4_K) |
| **BERT Multilingual Prompt Injection Detector (devndeploy)** | MIT | 110 M (bert-base-multilingual-cased) | EN, DE, **ES**, FR, ZH, VI, JA, KO, AR, RU, PT (11+) | «Accuracy 96.47%, Precision 99.20%, Recall 93.94%, F1 Score 96.50%» (7,282 samples, rikka-snow/prompt-injection-multilingual). | ✅ Sí (~400 MB) |
| **Nemotron Safety Guard Multilingual 8B** | NVIDIA Open Model License (enterprise-friendly) | 8 B (Llama-3.1-8B base + PEFT) | EN, **ES**, ZH, DE, FR, HI, JA, AR, TH (9 idiomas) | «The LLM model detects harmful content in user messages or bot responses in the following languages: Arabic, Chinese, English, French, German, Hindi, Japanese, Spanish, Thai.» / 23 safety categories, 84.2% harmful content classification accuracy. | ❌ No (8B BF16 ≈ 16 GB; Q4_K ≈ 5 GB + KV > 8 GB) |
| **AegisGuard (Timios74, fine-tune Llama Guard 3 1B)** | MIT | 1 B (base Llama Guard 3 1B) | EN, HI, Hinglish (code-mixed) — **ES no declarado** | «AegisGuard is a fine-tuned version of Llama Guard 3 1B for multi-class toxic content detection on social media platforms. It classifies text into 5 categories and recommends a moderation action (allow / mute / block).» / QLoRA 4-bit NF4, trainable params ~4M/1.1B. | ✅ Sí (~2 GB BF16 / ~0.7 GB Q4_K) |

---

## Resumen de viabilidad CPU 8 GB + español

| Modelo | Params | ES | CPU 8 GB | Licencia permisiva |
|--------|--------|----|----------|-------------------|
| Llama Prompt Guard 2 86M | 86 M | ✅ | ✅ | Llama 4 Community (uso comercial con atribución) |
| Qwen3Guard-Gen-0.6B | 0.6 B | ✅ | ✅ | **Apache-2.0** (OSI-approved) |
| Qwen3Guard-Stream-0.6B | 0.6 B | ✅ | ✅ | **Apache-2.0** |
| Llama Guard 3 1B | 1 B | ✅ | ✅ | Llama 3.2 Community |
| BERT Multilingual (devndeploy) | 110 M | ✅ | ✅ | **MIT** |
| ShieldGemma 2B | 2.6 B | ❌ | ✅ | Gemma Terms (no OSI) |
| ProtectAI small/base | 44/184 M | ❌ | ✅ | **Apache-2.0** |
| WildGuard 7B | 7 B | ❌ | ❌ | Apache-2.0 |
| Nemotron 8B | 8 B | ✅ | ❌ | NVIDIA Open Model License |
| prompt-guard-1.5b | 1.5 B | ❌ | ✅ | No declarada (base Apache-2.0) |
| AegisGuard 1B | 1 B | ❌ | ✅ | MIT |

---

## Recomendación para juez de inyección ES/EN en CPU 8 GB

1. **Primera opción**: **Qwen3Guard-Gen-0.6B** (Apache-2.0, 119 idiomas incluido ES, métricas SOTA multilingües, ~1.5 GB BF16).
2. **Segunda opción**: **Llama Prompt Guard 2 86M** (multilingüe nativo con ES, 86 M, ~350 MB, Llama 4 Community).
3. **Tercera opción (clasificador encoder)**: **BERT Multilingual Prompt Injection Detector** (MIT, 110 M, 11 idiomas con ES, F1 96.5% en benchmark multilingüe).
4. **Opción generativa 1B**: **Llama Guard 3 1B** (Llama 3.2 Community, 8 idiomas con ES, ~2 GB BF16).

> **Nota**: Los modelos >4 B (WildGuard 7B, Nemotron 8B, Qwen3Guard-4B/8B, ShieldGemma 9B/27B) exceden 8 GB RAM en CPU con KV cache. prompt-guard-1.5b y AegisGuard no cubren español según sus autores.

---

## Fuentes abiertas en esta sesión (todas verificadas salvo marcadas)

- Llama Prompt Guard 2: https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-22M, https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M, https://docs.mozilla.ai/any-guardrail/api-reference/index/prompt-injection/prompt-guard
- Qwen3Guard: https://huggingface.co/Qwen/Qwen3Guard-Gen-0.6B, https://huggingface.co/Qwen/Qwen3Guard-Stream-0.6B, https://qwenlm.github.io/blog/qwen3guard/, https://arxiv.org/abs/2510.14276
- ProtectAI: https://huggingface.co/protectai/deberta-v3-small-prompt-injection-v2, https://huggingface.co/protectai/deberta-v3-base-prompt-injection-v2, https://docs.mozilla.ai/any-guardrail/api-reference/index/prompt-injection/protectai
- WildGuard: https://huggingface.co/allenai/wildguard, https://docs.mozilla.ai/any-guardrail/api-reference/index/content-safety/wild-guard
- Llama Guard 3: https://huggingface.co/meta-llama/Llama-Guard-3-1B, https://docs.mozilla.ai/any-guardrail/api-reference/index/content-safety/llama-guard
- ShieldGemma: https://ai.google.dev/gemma/docs/shieldgemma, https://ai.google.dev/gemma/docs/shieldgemma/model_card, https://docs.mozilla.ai/any-guardrail/api-reference/index/content-safety/shield-gemma
- prompt-guard-1.5b: https://huggingface.co/prismor/prompt-guard-1.5b
- BERT Multilingual: https://huggingface.co/devndeploy/bert-prompt-injection-detector
- Nemotron: https://docs.nvidia.com/nim/llama-3-1-nemotron-safety-guard-multilingual-8b-v1/latest/index.html, https://build.nvidia.com/nvidia/llama-3_1-nemotron-safety-guard-8b-v3/modelcard
- AegisGuard: https://huggingface.co/Timios74/aegisguard-llama-guard
## Verificación del TL (2026-10-23)

- **Comprobado por el TL en la ficha oficial** (https://huggingface.co/Qwen/Qwen3Guard-Gen-0.6B): licencia Apache-2.0; «supports 119 languages and dialects»; la salida son niveles de seguridad (seguro, controvertido, no seguro) con categorías.
- **Dos matices que cambian la recomendación:** (1) la ficha **no nombra el español** entre esos idiomas: que lo cubra es una inferencia de este documento, no una cita; (2) Qwen3Guard es un modelo de **seguridad de contenido** (categorías como violencia, contenido sexual o datos personales; «Jailbreak» solo para la entrada), **no un detector de inyección indirecta** en documentos. Su idoneidad como juez de la capa 2 es una **hipótesis a medir** en M-BUFFY-3, no una conclusión.
- **Candidato mejor alineado con la tarea, según este mismo documento:** Llama Prompt Guard 2 86M, entrenado específicamente para detectar inyecciones y jailbreaks, multilingüe con español declarado. Es un clasificador pequeño (no generativo), por lo que podría ejecutarse dentro del kernel con ONNX en lugar de pasar por el guild `llama_backend`; esa vía es una idea para D-33, **no medida**. Su licencia («Llama 4 Community») requiere lectura jurídica antes de depender de ella.
- Las demás celdas (métricas de autor, tamaños, licencias) tienen un solo lector (Hermes); el TL no las ha reabierto.
