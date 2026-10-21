# Investigación en Fuentes Primarias: Licencia de Decima, Servido de Clef, Respuesta `refusal` y Panorama de Clones Open-Source de Jev / System One (ADR-021)

- **Fecha:** 2026-10-16
- **Contexto:** Cierre de las lagunas bloqueantes de la Fase 3 de ADR-021 (§7 de `2026-10-16_decision_model_clones_analysis_and_plan.md` y Turno 1227 de Coloquio) y selección fundamentada de repositorios y modelos abiertos para el servicio de decisión de Tylluan (`tylluan_decide`).
- **Metodología:** Verificación directa en fuentes primarias (tarjetas de modelo y dataset en Hugging Face, ficheros de código/licencia e informes técnicos en GitHub, PRs de `sglang`, `vllm`, `llama.cpp`, `litellm`, y documentación oficial de OpenAI y Cloudflare). Cada punto está marcado como `[VERIFICADO]` o `[NO ENCONTRADO]`, con su URL exacta y cita literal (`> "..."`).

---

## 1. Punto (a): Licencia de Decima y de sus datos de entrenamiento (`amyrmahdy/decima-*`) y uso comercial

### 1.1. Licencia del código, pesos y exportaciones ONNX de Decima
- **Estado:** `[VERIFICADO]` — **Apache-2.0** tanto para el código como para los pesos y las exportaciones ONNX en las tres variantes publicadas (`decima-small`, `decima-base`, `decima-agent`), sobre backbones MIT (`intfloat/multilingual-e5-small`) o Apache-2.0 (`jhu-clsp/mmBERT-base`), y teacher Apache-2.0 (`google/gemma-4-26B-A4B-it`).
- **Fuente primaria 1:** `https://huggingface.co/amyrmahdy/decima-small/raw/main/README.md`
  > `license: apache-2.0`
  > `base_model: intfloat/multilingual-e5-small`
  > `**Apache-2.0** (model weights, ONNX exports and code). The base encoder, multilingual-e5-small, is MIT; the teacher, Gemma-4-26B-A4B, is Apache-2.0.`
- **Fuente primaria 2:** `https://huggingface.co/amyrmahdy/decima-base/raw/main/README.md`
  > `license: apache-2.0`
  > `base_model: jhu-clsp/mmBERT-base`
- **Fuente primaria 3:** `https://huggingface.co/amyrmahdy/decima-agent/raw/main/README.md`
  > `license: apache-2.0`
  > `base_model: amyrmahdy/decima-agent`

### 1.2. Licencias de los datasets de entrenamiento de Decima y si permiten uso comercial
- **Estado:** `[VERIFICADO]` — El checkpoint publicado de `decima-small` (`v1i`) **fue entrenado sobre una mezcla que incluye datasets con licencia No Comercial (`CC BY-NC 4.0`) y datasets sin licencia formal (solo investigación)**, además de datasets con cláusula ShareAlike (`CC BY-SA`). Aunque el autor publica los pesos bajo Apache-2.0 acogiéndose a la práctica habitual del Hub y a la doctrina de que los pesos no reproducen el texto, **advierte explícitamente que existe riesgo NC para uso comercial** y recomienda un reentrenamiento limpio (*Opción C*) si el modelo se va a distribuir o explotar comercialmente sin riesgo.
- **Fuente primaria 1 (Aviso en la tarjeta de `decima-small`):** `https://huggingface.co/amyrmahdy/decima-small/raw/main/README.md`
  > `**Training-data disclosure.** Decima was trained on teacher-generated data and on gold labels derived from public datasets under a range of licences — attribution (CC BY: banking77, CLINC150, MASSIVE, WANLI, FarsTail), share-alike (SNLI, BoolQ, parts of MultiNLI), **non-commercial (CC BY-NC 4.0: ANLI, XNLI, multilingual-NLI-26lang)**, and datasets published without a licence for research use (AG News, SST-5). The model does not contain or reproduce these datasets, but if your use is commercial, review their terms. Full table: release/LICENSING.md.`
- **Fuente primaria 2 (Auditoría de licencias del propio autor de Decima):** `https://raw.githubusercontent.com/amyrmahdy/decima/main/release/LICENSING.md`
  - **Citas literales de `§2. Licence table`:**
    > `| **Gemma-4-26B-A4B-it** teacher (NVFP4 build) | Apache-2.0 ([HF](https://huggingface.co/google/gemma-4-26B-A4B-it), [Google](https://ai.google.dev/gemma/docs/gemma_4_license)) | outputs: **yes**, no licence restriction | none; attribution in the card is good practice |`
    > `| **intfloat/multilingual-e5-small** (base weights, redistributed as part of Decima) | MIT ([HF](https://huggingface.co/intfloat/multilingual-e5-small)) | n/a | any licence; **keep the MIT copyright + permission notice** in the model repo |`
    > `| MultiNLI nyu-mll/multi_nli (gold 66k) | mixed: OANC (permissive), fiction *Seven Swords* CC BY-SA 3.0, others CC BY 3.0 / US public domain ([HF card](https://huggingface.co/datasets/nyu-mll/multi_nli), [site](https://cims.nyu.edu/~sbowman/multinli/)) | yes, with attribution; SA applies to part | SA on data; whether SA reaches trained weights is unsettled (generally assumed not) |`
    > `| SNLI stanfordnlp/snli (gold 20k) | CC BY-SA 4.0 ([site](https://nlp.stanford.edu/projects/snli/)): "licensed under a Creative Commons Attribution-ShareAlike 4.0 International License" | yes, BY-SA | as above |`
    > `| **ANLI** facebook/anli (gold 30k) | **CC BY-NC 4.0** ([GitHub](https://github.com/facebookresearch/anli)): "ANLI is licensed under Creative Commons-Non Commercial 4.0" | non-commercial only | **NC risk** (see §3) |`
    > `| WANLI alisawuffles/WANLI (gold 25k) | CC BY 4.0 on [HF card](https://huggingface.co/datasets/alisawuffles/WANLI); the GitHub repo has no LICENSE. GPT-3-generated, crowd-revised; premises from MNLI | yes, attribution | none known. The OpenAI output terms bound the WANLI authors, not us (low risk) |`
    > `| **XNLI** facebook/xnli (gold 88k + **teacher-labelled 33.6k**) | **CC BY-NC 4.0** ([LICENSE](https://github.com/facebookresearch/XNLI/blob/main/LICENSE): "Creative Commons Attribution-NonCommercial 4.0 International Public License"). The HF card omits it | non-commercial only | **NC risk** |`
    > `| FarsTail, loaded from azarijafari/FarsTail (gold 14.5k) | Apache-2.0 ([dml-qom/FarsTail](https://github.com/dml-qom/FarsTail)); the HF mirrors have no or "unknown" licence | yes, keep notice | none |`
    > `| **multilingual-NLI-26lang-2mil7** MoritzLaurer/... (gold 138k) | no licence on the [HF card](https://huggingface.co/datasets/MoritzLaurer/multilingual-NLI-26lang-2mil7). It is a machine translation of MNLI, **ANLI**, Fever-NLI, LingNLI and WANLI and inherits their terms (strictest: **NC** from ANLI; Fever/LingNLI not verified) | **no** | **NC risk** (ANLI part) |`
    > `| BoolQ google/boolq (gold 17.8k) | CC BY-SA 3.0 ([GitHub](https://github.com/google-research-datasets/boolean-questions)): "released under the Creative Commons Share-Alike 3.0 license". Passages from Wikipedia (BY-SA) | yes, BY-SA | SA question as for SNLI |`
    > `| banking77 (mteb/banking77; gold-cls 42k + teacher-labelled 11.3k) | CC BY 4.0 ([PolyAI](https://github.com/PolyAI-LDN/task-specific-datasets)); the mteb mirror says MIT, but that relabel has no effect | yes, attribution | none |`
    > `| CLINC150 clinc/clinc_oos plus (gold-cls 42k + teacher-labelled 11.3k) | CC BY 3.0 ([LICENSE](https://github.com/clinc/oos-eval/blob/master/LICENSE)) | yes, attribution | none |`
    > `| MASSIVE (mteb/amazon_massive_intent; gold-cls 137k + teacher-labelled 44.8k) | CC BY 4.0 ([NOTICE](https://github.com/alexa/massive/blob/main/NOTICE.md): "licensed under CC BY 4.0, Copyright Amazon.com Inc. or its affiliates"); the mteb mirror says Apache-2.0 | yes, attribution + Amazon copyright line | none |`
    > `| **AG News** fancyzhx/ag_news + sh0416/ag_news titles (gold-cls 45k + teacher-labelled 11.4k) | **no formal licence**; the HF card says unknown. The author page ([Gulli](http://groups.di.unipi.it/~gulli/AG_corpus_of_news_articles.html), quoted via the HF card because the page was unreachable) says the data is "provided by the academic community for research purposes … and any other non-commercial activity". The text is third-party news | **no** | **NC / unclear risk** |`
    > `| **SST-5** SetFit/sst5 (gold-cls 32k + teacher-labelled 11.4k) | **no licence** ([SST site](https://nlp.stanford.edu/sentiment/), [SetFit/sst5](https://huggingface.co/datasets/SetFit/sst5)). The text is Rotten Tomatoes review snippets (Pang & Lee) | **no** | **unclear risk** |`
  - **Citas literales de `§3`, `§4` y `§5` (`release/LICENSING.md`):**
    > `Decima-small (v1i) was trained on ANLI, XNLI and 26lang-2mil7 (all CC BY-NC or containing it), and on AG News and SST-5 (no licence; research or non-commercial use at best). No court has settled whether a model's weights are bound by an NC licence on its training data. Training may also be covered by TDM exceptions (EU DSM Art. 3/4) or by fair use. Precedent on the Hub is permissive: MoritzLaurer/mDeBERTa-v3-base-xnli-multilingual-nli-2mil7 (trained on 26lang-2mil7, which includes ANLI) and facebook/bart-large-mnli are both MIT. That shows common practice, not legal safety. Separately, *republishing* the NC or unlicensed **text** is clearly not allowed commercially, so we don't do it.`
    > `**Option C — "Commercial-clean" retrain, then Apache-2.0.** Drop ANLI, XNLI (gold *and* teacher-labelled xnli rows), 26lang-2mil7, AG News and SST-5. Keep synthetic (184k), MultiNLI, SNLI, WANLI, FarsTail, BoolQ, banking77, CLINC150, MASSIVE (~450k rows total). Replace the missing multilingual NLI with Gemma-labelled synthetic pairs, which are clean.`
    > `**My recommendation:** if Decima will be used or sold commercially, do **C** and release under **Apache-2.0**. If v1i ships before a clean retrain, **A with full disclosure** is common practice on the Hub, but the residual risk is the owner's to accept.`
    > `1. **Model licence**: A, B or C (§4a). If C, approve one retrain plus re-eval. → **Decided: A, Apache-2.0 + disclosure.**`
- **Fuente primaria 3 (Dataset sintético limpio de Decima, publicado por separado bajo Apache-2.0):** `https://huggingface.co/datasets/amyrmahdy/decima-system-one-tasks/raw/main/README.md`
  > `license: apache-2.0`
  > `184,427 synthetic System One decision tasks generated with Gemma-4-26B-A4B-it (Apache-2.0) and used in training Decima.`
  > `Public-benchmark text that was also teacher-labelled during Decima's training (Amazon MASSIVE, Banking77, CLINC150, AG News, SST-5, XNLI) and gold-label NLI/classification rows are **excluded** from this release so that every row here has a single, clean provenance: synthetic prompt → Apache-2.0 teacher.`

### 1.3. Cuantización int8 de Decima (§8.8 / §9 del informe técnico — cierre de la laguna #4)
- **Estado:** `[VERIFICADO]` — Se leyó íntegra la sección técnica sobre cuantización int8 de Decima. La cuantización dinámica estándar de ONNX Runtime (`MatMulInteger` con una sola escala `uint8` por tensor) degrada el acuerdo top-1 frente a fp32 al **64.4%** debido a tres canales *outlier* (`15, 35, 321` con `|x| ~ 70` tras LayerNorm en las proyecciones FFN del encoder `e5-small`). La exportación `onnx/int8` publicada lo corrige usando `com.microsoft::MatMulNBits` simétrico por bloques de 32 (`block_size=32`, `accuracy_level=4`) en el encoder, manteniendo `quantize_dynamic` en el scorer.
- **Fuente primaria:** `https://raw.githubusercontent.com/amyrmahdy/decima/main/docs/TECHNICAL-REPORT.md`
  > `### 8.8 int8: why dynamic quantisation failed and block-wise 8-bit works`
  > `ONNX Runtime's [29] quantize_dynamic (MatMulInteger) quantises each activation tensor with **one** uint8 scale per call. The E5 backbone has **outlier channels**: channels 15, 35 and 321 after LayerNorm reach |x| up to about 70 at the inputs of the FFN output projections, while the median channel is about 1`
  > `With one scale covering [-70, 70], the step size is about 0.55 and all normal channels are rounded to two or three levels. Compounded over 12 layers, top-1 agreement with fp32 drops to **64.4%** and mean |Δp| reaches **0.289**.`
  > `**Fix.** Constant-weight MatMuls in the encoder are replaced by com.microsoft::MatMulNBits: - weights are stored as symmetric int8 in blocks of 32 along the reduction dimension K; - with accuracy_level = 4, activations are also quantised per block of 32, so an outlier channel spoils only its own block, one of 384/32 = 12 blocks, or 48 in the FFN output projection; - the word-embedding table (250k × 384, about 80% of the bytes) is stored int8 with one scale per row (Gather + Gather + Mul); - the scorer keeps dynamic int8 with per-channel weights, which is harmless there (agreement 1.000, mean |Δp| 2e-4) and faster than MatMulNBits for many option tokens per call.`
  > `| precision | total bytes | JevBench macro | mean |Δp| vs fp32 | top-1 agreement vs fp32 |`
  > `| fp32 | 476 MB | 69.5% | 0 | 100.0% |`
  > `| fp16 | 238 MB | 69.5% | 0.0003 | 99.8% |`
  > `| int8 dynamic (MatMulInteger, broken) | 120 MB | — | 0.289 | 64.4% |`
  > `| **int8 block-32** (MatMulNBits, shipped) | **138 MB** | **69.1%** | **0.015** | **97.5%** |`

---

## 2. Punto (b): Cómo se sirve Clef / Clef-Flash (`POST /v1/systemone` vs `/v1/chat/completions` vs `/v1/decisions`)

### 2.1. Servido en SGLang (`Cloudflare/clef` y `Cloudflare/clef-flash`)
- **Estado:** `[VERIFICADO]` — SGLang sirve Clef y Clef-Flash nativamente en **`POST /v1/systemone`** (introducido en el PR `sgl-project/sglang#42721` tras `v0.5.21`). Tanto la tarjeta oficial de Hugging Face de `Cloudflare/clef-flash` y `Cloudflare/clef` como el Cookbook oficial de SGLang muestran llamadas a `/v1/systemone`. Más aún, SGLang **rechaza con HTTP 400** las peticiones a `/v1/decisions`, chat (`/v1/chat/completions`) y generación cuando sirve Clef. *(Esto corrige la nota preliminar de `2026-10-16_decision_model_clones_analysis_and_plan.md` §3 que mencionaba `/v1/chat/completions`).*
- **Fuente primaria 1 (Model Card oficial `Cloudflare/clef-flash` y `Cloudflare/clef`):** `https://huggingface.co/Cloudflare/clef-flash/raw/main/README.md` y `https://huggingface.co/Cloudflare/clef/raw/main/README.md`
  > `### SGLang`
  > `Clef-Flash can be served with SGLang. See the [SGLang cookbook](https://docs.sglang.io/cookbook/autoregressive/Cloudflare/clef#hw=h200&variant=flash&quant=bf16&strategy=balanced&nodes=single) for launch commands on H200, B200, and B300.`
  > `The server answers Jev/SystemOne requests at /v1/systemone:`
  > `curl http://localhost:<puerto>/v1/systemone -H 'Content-Type: application/json' -d '{"model":"Cloudflare/clef-flash","state":"My Stripe integration keeps failing. Please help ASAP.","questions":{"urgency":{"type":"noul","instructions":"Does this message express urgency?"}}}'`
- **Fuente primaria 2 (SGLang Cookbook oficial):** `https://docs.sglang.io/cookbook/autoregressive/Cloudflare/clef`
  > `Clef support merged after v0.5.21 in #42721.`
  > `Route. Send requests to /v1/systemone, or set your System One client’s base URL to the server. This includes TypeSafe SDK clients. See the API reference for request and response fields. Clef refuses /v1/decisions, chat, and generation requests.`
  > `One GPU, one prefill per request. Keep TP=1 and PP=1. All questions share one prompt, so the model reads the state once. SGLang detects joint_head_config.json, loads joint_head.safetensors, and enables embedding mode. It disables radix caching, chunked prefill, and CUDA graphs.`
- **Fuente primaria 3 (PR `sgl-project/sglang#42721`):** `https://github.com/sgl-project/sglang/pull/42721`
  > `Title: [Model] Support Clef and Clef-Flash decision models on /v1/systemone`
  > `This PR serves the published checkpoints as-is on the existing /v1/systemone route, following the decision-models design.`
  > `**Refusals:** /v1/decisions, /encode, chat_template_kwargs and LoRA requests return 400, and /health_generate returns 200.`

### 2.2. Servido en `llama.cpp` (`ggml-org/Clef-Flash-GGUF` y `ggml-org/Clef-GGUF`)
- **Estado:** `[VERIFICADO]` — `llama-server` sirve Clef y Clef-Flash en **`POST /v1/systemone`** a partir del PR `ggml-org/llama.cpp#29831` (que extiende el endpoint `/v1/systemone` introducido en el PR `#29818`). Actualmente soporta solo texto (sin visión hasta que se fusione `#29622`) y una secuencia por batch.
- **Fuente primaria 1:** `https://huggingface.co/ggml-org/Clef-Flash-GGUF/raw/main/README.md`
  > `llama serve -hf ggml-org/Clef-Flash-GGUF`
  > `This is a decision model, to be used via /v1/systemone API. Requires https://github.com/ggml-org/llama.cpp/pull/29831`
- **Fuente primaria 2:** `https://github.com/ggml-org/llama.cpp/pull/29831`
  > `title: "model: add support for clef decision model (text-only)"`
  > `Follow-up https://github.com/ggml-org/llama.cpp/pull/29818`
  > `Pre-quantized models: llama serve -hf ggml-org/Clef-GGUF / llama serve -hf ggml-org/Clef-Flash-GGUF`
  > `Known limitations: - No vision support yet, requires https://github.com/ggml-org/llama.cpp/pull/29622 to be merged - Single seq per batch (ref: TAG_SPLIT_QGATE_QWEN)`

### 2.3. Servido en vLLM
- **Estado:**
  - `[VERIFICADO]` para el endpoint `POST /v1/systemone` en vLLM sobre modelos autorregresivos Qwen3/Qwen3.5 (únicamente para preguntas `choice`).
  - `[NO ENCONTRADO]` soporte nativo en vLLM para cargar `joint_head.safetensors` (`JointSchemaHead`) de `Cloudflare/clef` o `Cloudflare/clef-flash` ni para preguntas `noul`/`score`.
- **Fuente primaria 1 (PR `vllm-project/vllm#59299`):** `https://github.com/vllm-project/vllm/pull/59299`
  > `title: "[Feat] Structured decisions endpoint (/v1/systemone)"`
  > `This implements a new endpoint, with a single question type (choice), for AR models only: POST /v1/systemone`
  > `Limits: 64 questions (only choice), and options capped at the smaller of the tokenizer's supported label set (see below) and 128.`
- **Fuente primaria 2 (Documentación de LiteLLM sobre vLLM Decisions):** `https://docs.litellm.ai/docs/decisions`
  > `vLLM answers only choice questions, serves Qwen3 and Qwen3.5 models, and needs a vLLM build that includes vllm-project/vllm#59299, which landed after v0.31.0`

### 2.4. Servido en Cloudflare Workers AI
- **Estado:** `[VERIFICADO]` — En Cloudflare Workers AI se invoca por REST en `POST https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/@cf/cloudflare/clef-flash` (o `@cf/cloudflare/clef`). Recibe el mismo cuerpo JSON que `/v1/systemone` (`model`, `state`, `questions`, `images`) y devuelve la respuesta SystemOne envuelta dentro de la clave `"result"` del sobre estándar de Cloudflare (`{"result": {"model": ..., "answers": ..., "usage": ...}, "success": true}`).
- **Fuente primaria 1:** `https://developers.cloudflare.com/workers-ai/models/clef-flash/index.md`
  > `curl https://api.cloudflare.com/client/v4/accounts/$CLOUDFLARE_ACCOUNT_ID/ai/run/@cf/cloudflare/clef-flash \`
  > `  -X POST \`
  > `  -H "Authorization: Bearer $CLOUDFLARE_AUTH_TOKEN" \`
  > `  -d '{ "model": "clef-flash", "state": { "ticket_id": "T-1842", ... }, "questions": { ... } }'`
- **Fuente primaria 2:** `https://docs.litellm.ai/docs/decisions`
  > `Cloudflare model names without an @cf/ prefix are expanded to @cf/cloudflare/<model>, and the {"result": ...} envelope Cloudflare returns is unwrapped so the response has the same shape as the other providers.`
- **Fuente primaria 3:** `https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/llms/cloudflare/decisions/transformation.py`
  > `def unwrap_response(self, payload: object) -> object:`
  > `    response_mapping = _RESPONSE_MAPPING_ADAPTER.validate_python(payload)`
  > `    if "answers" in response_mapping:`
  > `        return response_mapping`
  > `    try:`
  > `        return _RESPONSE_MAPPING_ADAPTER.validate_python(response_mapping.get("result"))`

---

## 3. Punto (c): Forma exacta de la respuesta de rechazo (`refusal` / abstención)

### 3.1. En el dialecto OpenAI `POST /v1/decisions`
- **Estado:** `[VERIFICADO]` — En `/v1/decisions`, el campo `answers` de la respuesta es una **lista (array) ordenada**. Cuando una pregunta es rechazada por el modelo, el elemento correspondiente en `answers` tiene la forma exacta `{"type": "refusal", "name": "<nombre_de_la_pregunta>"}` (donde `name` es `str | None`). No incluye probabilidades, ni valor, ni campo `reason`.
- **Fuente primaria 1 (OpenAI API Docs — Decisions Guide):** `https://developers.openai.com/api/docs/guides/decisions`
  > `A refusal answer has type: "refusal" and the question name.`
  > `const answer = decision.answers[0];`
  > `if (answer.type === "refusal") {`
  > `  console.log(\`Refused: \${answer.name}\`);`
  > `}`
- **Fuente primaria 2 (LiteLLM Decisions Docs):** `https://docs.litellm.ai/docs/decisions`
  > `A question the model will not answer comes back as {"type": "refusal", "name": ...}`
  > `A dropped question comes back as {"type": "refusal"} on /v1/decisions and is left out of answers on /v1/systemone.`
- **Fuente primaria 3 (Definición Pydantic en LiteLLM):** `https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/types/decisions.py`
  > `class OpenAIRefusalAnswer(LiteLLMPydanticObjectBase):`
  > `    type: Literal["refusal"] = "refusal"`
  > `    name: str | None`
  > `    model_config = ConfigDict(frozen=True)`

### 3.2. En el dialecto TypeSafe `POST /v1/systemone`
- **Estado:** `[VERIFICADO]` — En `/v1/systemone`, `answers` es un **mapa JSON** (`{question_id: Answer}`) cuya unión discriminada por `"type"` solo admite `"noul" | "choice" | "score"` (más `"box"` en Jev 1.14). **No existe `"type": "refusal"` dentro de `answers` en `/v1/systemone`**:
  1. **Rechazo / omisión de una pregunta individual:** la clave de esa pregunta **se omite del mapa `answers`** (comportamiento documentado por LiteLLM y observado empíricamente en vivo contra `Clef-Flash` en T1004, commit `c40a409`).
  2. **Rechazo de la petición completa (p. ej. truncamiento de estado o violación de esquema):** el servidor devuelve un código de error HTTP 4xx (`400 Bad Request` o `422 Unprocessable Entity`). En la API de TypeSafe y en `decima serve`, un `state` que excede la ventana de tokens devuelve `HTTP 422` con `"code": "STATE_TRUNCATED"`.
- **Fuente primaria 1 (`litellm/types/decisions.py`):** `https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/types/decisions.py`
  > `DecisionAnswer: TypeAlias = Annotated[`
  > `    NoulAnswer | ChoiceAnswer | ScoreAnswer | BoxAnswer,`
  > `    Field(discriminator="type"),`
  > `]`
- **Fuente primaria 2 (`litellm/llms/base_llm/decisions/transformation.py`):** `https://raw.githubusercontent.com/BerriAI/litellm/main/litellm/llms/base_llm/decisions/transformation.py`
  > `def _systemone_answer(answer: DecisionsIRAnswer) -> DecisionAnswer | None:`
  > `    match answer:`
  > `        ...`
  > `        case DecisionsIRRefusal():`
  > `            return None`
  > `def ir_to_systemone_response(response: DecisionsIRResponse, request: DecisionsIRRequest) -> DecisionsResponse:`
  > `    ...`
  > `    "answers": {key: answer for key, answer in keyed_answers if answer is not None},`
- **Fuente primaria 3 (`decima/serve.py` — error 422 `STATE_TRUNCATED`):** `https://raw.githubusercontent.com/amyrmahdy/decima/main/decima/serve.py`
  > `As on TypeSafe's API, a state that would be cut to fit the model's context is an error (422 STATE_TRUNCATED), not a silent truncation. Errors use the body {"error", "code", "detail"?}.`

### 3.3. Extensiones de abstención por pregunta en los clones open-source
1. **Decima (`decima/systemone.py`)** `[VERIFICADO]`: Si la petición incluye `min_confidence: float`, añade `"abstain": true | false` dentro de la respuesta de cada pregunta comparando la confianza TypeSafe (`|2·noul − 1|` para `noul`, `max(probabilities)` para `choice`/`score`) contra `min_confidence`.
   - **Fuente:** `https://raw.githubusercontent.com/amyrmahdy/decima/main/decima/systemone.py`
     > `min_confidence adds "abstain": true / false, comparing it with the answer's TypeSafe confidence (for noul, |2·noul − 1|).`
2. **Laya (`receptron/laya`, `convaiinnovations/laya`)** `[VERIFICADO]`: Incluye `rl_agent: { act_probability: number }` en cada respuesta, aunque en el checkpoint actual está saturado cerca de `1.0` (issue `#185`).
   - **Fuente:** `https://huggingface.co/convaiinnovations/laya/raw/main/README.md`
     > `- **rl_agent.act_probability currently reads ~1.0 (#185):** Gate on confidence (e.g. confidence >= 0.6) until the next checkpoint retrains the act/abstain head.`
3. **OpenThai-SystemOne (`iapp/OpenThai-SystemOne`)** `[VERIFICADO]`: En su cabeza de 256 slots (`<|ts_opt_0|>…<|ts_opt_254|>`), reserva el **slot 255** para *abstain*.
   - **Fuente:** `https://huggingface.co/iapp/OpenThai-SystemOne/raw/main/README.md`
     > `The 248k-token LM head is **replaced by a 256-way slot head**... Slot 255 is *abstain* (none of the options fit).`
4. **Cloudflare Clef / Clef-Flash (`schema-output.json` y SGLang `protocol.py`)** `[VERIFICADO]`: No tiene variante `refusal` ni campo `abstain` por pregunta; devuelve siempre `noul`, `choice` o `score`, o HTTP 400 si la petición es inválida.
   - **Fuente:** `https://developers.cloudflare.com/workers-ai/models/clef-flash/schema-output.json`

---

## 4. Panorama Completo de Clones Open-Source de Jev / System One y Recomendación de Integración para Tylluan

### 4.1. Inventario verificado de modelos y repositorios abiertos (2026-10)

| Modelo / Repo | Autor / Org | Arquitectura y Parámetros | Contexto | Runtime / Servido | Licencia Pesos/Código | Estado de Datos de Entrenamiento |
|---|---|---|---|---|---|---|
| **`amyrmahdy/decima-small`** (`decima-base`, `decima-agent`) | amyrmahdy | Dual-encoder late interaction: `multilingual-e5-small` + scorer 2 capas (**118M–122M**; `base`/`agent`: **321M** `mmBERT-base`) | 512 (`small`) / 8.192 (`base`) | ONNX int8 block-32 (**138 MB**), fp16 (238 MB), `decima serve` (`/v1/systemone`) | **Apache-2.0** `[VERIFICADO]` — fuente: <https://huggingface.co/amyrmahdy/decima-small/raw/main/README.md> · cita: `license: apache-2.0` / `**Apache-2.0** (model weights, ONNX exports and code)` | **Contiene NC (`ANLI`, `XNLI`, `26lang-2mil7`) y sin licencia (`AG News`, `SST-5`)** en `v1i` `[VERIFICADO]` — fuente: <https://raw.githubusercontent.com/amyrmahdy/decima/main/release/LICENSING.md> · cita: `Decima-small (v1i) was trained on ANLI, XNLI and 26lang-2mil7 (all CC BY-NC or containing it), and on AG News and SST-5 (no licence; research or non-commercial use at best)` · El subset sintético (`184k` filas Gemma-4) está separado en HF bajo **Apache-2.0** `[VERIFICADO]` — fuente: <https://huggingface.co/datasets/amyrmahdy/decima-system-one-tasks/raw/main/README.md> · cita: `license: apache-2.0` |
| **`convaiinnovations/laya`** (`laya-multilingual`, `laya-code`, `receptron/laya-onnx`) | Convai Innovations / Receptron | Encoder ModernBERT / mmBERT + cabeza RLCD (**155M / 314M / 421M**) | 512–8.192 | ONNX fp32/fp16/int8 (`receptron/laya` MIT), `llama.cpp` `/v1/systemone` (`ggml-org/Laya-GGUF`) | **Apache-2.0** (pesos) / **MIT** (runtime) `[VERIFICADO]` | Entrenado con RLCD; no declara datasets NC `[VERIFICADO]` |
| **`SupersonicLabs/Julia-1`** (`ggml-org/Julia-1-GGUF`) | Supersonic Labs | Encoder bidireccional sobre `jhu-clsp/mmBERT-small` (**144.3M**) | 8.192 | `llama.cpp` `/v1/systemone` (`ggml-org/Julia-1-GGUF`), PyTorch | **Apache-2.0** `[VERIFICADO]` | Pipeline de entrenamiento privado no incluido; pesos Apache-2.0 `[VERIFICADO]` |
| **`wfzyx/von` (Von 1.3)** | wfzyx | `answerdotai/ModernBERT-large` + cabeza *Option-Marker* `<\|opt_k\|>` (**395M**) | 8.192 | OpenVINO, PyTorch, exportación ONNX (`von export-onnx`) y `von calibrate` | **Apache-2.0** `[VERIFICADO]` | Generadores sintéticos propios en `training/` + datasets públicos `[VERIFICADO]` |
| **`vllm-sr/d3-edge`** (`d3-nano`, `d3-mini`, `d3-flash`, `d3`) | vLLM Semantic Router | Modelos multimodales sobre `Qwen3.5` / `Qwen3.8`: **`d3-edge` (0.59B)**, `d3-nano` (2.21B), `d3-mini` (4B), `d3-flash` (9B), `d3` (27B) | 32k+ | vLLM Semantic Router / PyTorch | **Apache-2.0** `[VERIFICADO]` | Publicado por vLLM Semantic Router bajo Apache-2.0 `[VERIFICADO]` |
| **`iapp/OpenThai-SystemOne`** | iApp Technology | `Qwen/Qwen3.5-0.8B` con cabeza de 256 slots (`<\|ts_opt_0\|>…254` + slot 255 *abstain*) (**0.8B**) | 32k | Ollama ≥0.35 (`/v1/systemone`, GGUF Q4 **529 MB**), PyTorch | **Apache-2.0** `[VERIFICADO]` | Sintético + multilingüe bajo Apache-2.0 `[VERIFICADO]` |
| **`com-kotobalabs/open-jev-deberta-v3-large`** | Kotoba Labs | `microsoft/deberta-v3-large` (**435M**) | 512 | PyTorch / HF Transformers | **Apache-2.0** (base MIT) `[VERIFICADO]` | Solo gold labels públicos sin ANLI/XNLI (`banking77` CC BY 4.0, `boolq` CC BY-SA 3.0, `sst5` sin licencia formal) `[VERIFICADO]` |
| **`fastino/GLiNER2.5-Decide`** (`GLiNER2.5-multi-Decide`) | Fastino | Basado en `fastino/gliner2-large-v1` (**~450M**) | 512–2.048 | PyTorch / ONNX | **Apache-2.0** `[VERIFICADO]` | Basado en GLiNER2 Apache-2.0 `[VERIFICADO]` |
| **`Cloudflare/clef-flash`** y **`Cloudflare/clef`** | Cloudflare | `Qwen3.5-9B` (**9.41B**) y `Qwen3.8-27B` (**27.36B**) + LoRA r256 + `JointSchemaHead` multimodal | 32k+ | SGLang (`/v1/systemone`, PR `#42721`), `llama.cpp` (`/v1/systemone`, PR `#29831`, texto), Workers AI | **Apache-2.0** `[VERIFICADO]` — fuente: <https://huggingface.co/Cloudflare/clef-flash/raw/main/README.md> · cita: `license: apache-2.0` (en la tarjeta oficial) | Dataset interno de Cloudflare no publicado `[VERIFICADO]` |
| **`StrandsAgents/strands-decider-2B-*`** | AWS / Strands Agents | `Qwen3.5-2B-Base` / `gemma-4-E2B-it` + LoRA (**2B** y **4B**) | 8k–32k | PyTorch / PEFT | **Apache-2.0** `[VERIFICADO]` | Sin ANLI/XNLI; usa `tasksource-jev-typed-decisions`, `snli` (BY-SA), `boolq` (BY-SA), `ag_news` y `sst5` `[VERIFICADO]` |
| **`interfaze-ai/lev`** (`ggml-org/lev-GGUF`) | Interfaze AI | `Qwen/Qwen3.5-4B` + LoRA (**4B**) | 32k | `llama.cpp` `/v1/systemone` (`ggml-org/lev-GGUF`) | **Apache-2.0** `[VERIFICADO]` | **Contiene datasets NC** (declarado explícitamente en su README) `[VERIFICADO]` |
| **`ggml-org/Kev-4B-GGUF`** (`jaredpalmer/kev`) | Jared Palmer / ggml-org | `Qwen3.5` + LoRA r16 + pointer head (**0.8B–27B**) | 32k | `llama.cpp` `/v1/systemone` | **MIT** en `ggml-org/Kev-4B-GGUF`; repo original `jaredpalmer/kev` en HF devuelve `401 Unauthorized` (retirado/privado) `[VERIFICADO]` | No verificable hoy por retirada del repo fuente `[NO ENCONTRADO]` |
| **`openjev/openjev`** y **`bespokelabs/Bespoke-Nimble-9B-v3`** | OpenJev / Bespoke Labs | 0.6B–9B | — | `llama.cpp` `/v1/systemone` | **`CC BY-NC 4.0` (No Comercial)** `[VERIFICADO]` | **Descartados** por licencia NC directa en los pesos `[VERIFICADO]` |

---

### 4.2. Recomendación de integración para Tylluan (ADR-021 Fases 2, 3 y 4)

Tylluan tiene tres restricciones arquitectónicas no negociables (AGENTS.md + ADR-018 + ADR-020 + ADR-021):
1. **Soberanía MIT y limpieza comercial:** no podemos empaquetar ni distribuir por defecto pesos con licencia `CC BY-NC` ni entrenados con datasets `NC` sin limpiar.
2. **Ancla de hardware ("cafetera-friendly" / clínica offline / CPU sin GPU):** el proveedor local por defecto debe correr en CPU con presupuesto de latencia `< 500 ms` p99 y huella de disco/RAM de ~100–300 MB.
3. **Adopción gobernada por evidencia (test pareado en sombra):** ningún modelo se adopta en `advisory` o `gate` si no supera estadísticamente a las reglas deterministas en el `decision_shadow_log`.

En consecuencia, la estrategia recomendada por fase es:

#### A. Fase 2 — Proveedor HTTP externo (`POST /v1/systemone` + adaptador `/v1/decisions` vía Inference Gateway ADR-020)
- **Qué código integrar en `tylluan-kernel`:**
  - Promover y endurecer el cliente HTTP `SystemOneDecisionProvider` (ya prototipado en `examples/decision_fabric_spike.rs:1475`) bajo el Inference Gateway de ADR-020, añadiendo:
    1. Soporte de abstención por pregunta cuando el servidor **omite una clave de `answers`** (regla estándar de `/v1/systemone`) o devuelve `"abstain": true` (extensión de Decima).
    2. En el adaptador del dialecto OpenAI `/v1/decisions`, mapeo directo de `{"type": "refusal", "name": "<id>"}` a `Answer { abstained: true, abstain_reason: Some("model_refusal") }`.
    3. Desempaquetado opcional del sobre `{"result": ...}` si el usuario configura un endpoint compatible con Cloudflare Workers AI (solo para datos `Public`, nunca `Clinical` sin aprobación explícita por petición).
- **Qué servidores y modelos externos recomendar al usuario conectar por HTTP (`/v1/systemone`):**
  1. **`llama-server` (`llama.cpp` ≥ PR `#29818` / `#29831`) u Ollama ≥ 0.35.1** sirviendo:
     - **En CPU / Edge ligero:**
       - `SupersonicLabs/Julia-1` (`ggml-org/Julia-1-GGUF`, **144.3M params**, `mmBERT-small`, **Apache-2.0**, contexto 8.192 tokens).
       - `iapp/OpenThai-SystemOne` (**0.8B**, GGUF Q4 **529 MB**, **Apache-2.0**, con slot 255 nativo de abstención).
       - `convaiinnovations/laya-multilingual` (314M, **Apache-2.0**) o `laya-code` (155M, **Apache-2.0**).
     - **En nodo LAN con GPU (`TeamNetwork` de ADR-020):**
       - `Cloudflare/clef-flash` (`ggml-org/Clef-Flash-GGUF` o vía SGLang `#42721`, **9B**, **Apache-2.0**) o `vllm-sr/d3-nano` (**2.21B**, **Apache-2.0**).

#### B. Fase 3 — Proveedor ONNX embebido en Rust (`ort` in-process, ~130 MB, CPU < 150 ms)
- **Qué arquitectura y código integrar:**
  - La arquitectura dual-encoder late-interaction de **Decima-small** (`DecimaOnnxDecisionProvider` en `examples/decision_fabric_spike.rs:1959`, 118M params, `encoder.onnx` int8 block-32 `MatMulNBits` + `scorer.onnx`, **138 MB** totales) es técnicamente el diseño ideal para CPU: en nuestras mediciones reales de Fase 0 fue la **única** variante que superó las cláusulas 1–3 (`accuracy 0.5510 vs 0.5306`, `Brier 0.2390`, `ECE 0.0851`, **`p99 = 147.0 ms <= 500 ms`**).
- **Cómo resolver el bloqueo legal de los datos de entrenamiento de Decima antes de Fase 3:**
  - **No distribuir ni auto-descargar por defecto el checkpoint `v1i` de `amyrmahdy/decima-small`** mientras incluya `ANLI`, `XNLI`, `26lang-2mil7`, `AG News` y `SST-5`.
  - Ejecutar la **"Opción C" (*Commercial-clean retrain*) documentada por el propio autor en `release/LICENSING.md` §4**:
    1. **Base encoder:** `intfloat/multilingual-e5-small` (**MIT**).
    2. **Código de entrenamiento y exportación ONNX (`MatMulNBits` block-32):** repositorio `amyrmahdy/decima` (**Apache-2.0**) `[VERIFICADO]` — fuente: <https://raw.githubusercontent.com/amyrmahdy/decima/main/release/LICENSING.md> · cita: `license: Apache-2.0` (en el repositorio y en la sección de licencias del informe técnico) |
    3. **Corpus 100% limpio para uso comercial:**
       - `amyrmahdy/decima-system-one-tasks` (**184.427 filas sintéticas generadas con Gemma-4-26B-A4B-it**, ya publicadas por separado en Hugging Face bajo **Apache-2.0** sin ninguna fila de benchmarks NC) `[VERIFICADO]` — fuente: <https://huggingface.co/datasets/amyrmahdy/decima-system-one-tasks/raw/main/README.md> · cita: `license: apache-2.0` / `184,427 synthetic System One decision tasks generated with Gemma-4-26B-A4B-it (Apache-2.0)` |
       - Datasets permisivos con atribución: `banking77` (CC BY 4.0), `CLINC150` (CC BY 3.0), `MASSIVE` (CC BY 4.0), `WANLI` (CC BY 4.0), `FarsTail` (Apache-2.0).
       - Pares propios exportados del Signal Loop / `decision_shadow_log` de Tylluan (ADR-011 / ADR-018).
    4. **En paralelo**, evaluar en `examples/decision_fabric_spike.rs` a **`SupersonicLabs/Julia-1`** (144.3M, Apache-2.0) y **`convaiinnovations/laya-code` / `laya-multilingual`** (155M / 314M, Apache-2.0) con recalibración de temperatura (Fase 4, estilo `von calibrate` de `wfzyx/von`) sobre nuestro split de entrenamiento, ya que Laya obtuvo **AUROC = 0.9221** en nuestro dataset y solo falló en calibración zero-shot.
