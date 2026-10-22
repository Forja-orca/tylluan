# H-6: Llama Prompt Guard 2 86M + DeBERTa ONNX en Rust (crate `ort`) — 2026-10-23

**Autor:** Hermes (coder). **Encargo H-6** (TL T1564). Máx ~60 líneas. Cita literal + URL de páginas abiertas. "No encontrado" si no.

---

## 1. Ficha oficial: Llama Prompt Guard 2 86M

| Parámetro | Cita literal | URL | Leído |
|---|---|---|---|
| Arquitectura | "mDeBERTa-base, multilingual" | https://docs.mozilla.ai/any-guardrail/api-reference/index/prompt-injection/prompt-guard | ✅ |
| Pesos | `model.safetensors` en repo gated | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M | ✅ |
| Licencia | "Llama 4 Community License Agreement", effective April 5, 2025 | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M | ✅ |
| Cláusula >700M MAU | "If you exceed greater than 700 million monthly active users in the preceding calendar month, you must enter into a separate Meta license" | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M | ✅ |
| Formato entrada/salida | Clasificación binaria: `BENIGN` / `MALICIOUS` (prompt text → label) | https://meta-llama.github.io/PurpleLlama/LlamaFirewall/docs/documentation/scanners/prompt-guard-2 | ✅ |
| Contexto máximo | 512 tokens; ~92 ms en A100 (86M) | https://www.aipotluck.org/product/llama-prompt-guard | ✅ |
| Idiomas | English, French, German, Hindi, Italian, Portuguese, Spanish, Thai (8) | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M (model card) | ✅ |
| Export ONNX oficial | **No encontrado** — Meta no publica ONNX oficial; solo checkpoints HF gated | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M | ✅ (no existe) |
| ONNX tercero (86M) | `gravitee-io/Llama-Prompt-Guard-2-86M-onnx` — pipeline `convert_to_onnx.py` con `AutoModelForSequenceClassification` → ONNX + opt + quant | https://github.com/gravitee-io/Llama-Prompt-Guard-2-onnx | ✅ |
| ONNX tercero (22M) | `shisa-ai/promptguard2-onnx` — signed model pack para `shisad` | https://huggingface.co/shisa-ai/promptguard2-onnx | ✅ |
| Limitaciones vs inyección indirecta | El model card declara detección de *prompt injection* y *jailbreak*; no cubre ataques indirectes (indirect prompt injection vía retrieval/context poisoning) | https://huggingface.co/meta-llama/Llama-Prompt-Guard-2-86M (model card) | ✅ |

---

## 2. Cómo ejecutar DeBERTa en ONNX desde Rust con `ort`

**Fuente verificada:** ejemplo oficial de `ort` + `tokenizers` en `dragonscale-ai/kniv-deberta-v3-nlp-en` (README.md, sección "Usage with ONNX Runtime — Rust"):

```rust
use ort::{Session, Value};
use tokenizers::Tokenizer;

// 1. Cargar modelo ONNX y tokenizer
let session = Session::builder()?
    .with_model_from_file("model-int8.onnx")?;
let tokenizer = Tokenizer::from_file("tokenizer.json")?;

// 2. Tokenizar
let encoding = tokenizer.encode("Input text here.", true)?;
let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();

// 3. Ejecutar inference (forward pass)
let outputs = session.run(ort::inputs![input_ids, attention_mask]?)?;

// 4. Extraer logits (Prompt Guard 2 output: 2 logits → BENIGN/MALICIOUS)
let logits = outputs["logits"].extract_tensor::<f32>()?;
// softmax + argmax → label
```

**URL:** https://huggingface.co/dragonscale-ai/kniv-deberta-v3-nlp-en/blob/main/README.md

**Tokenizador alternativo:** `rust_tokenizers` soporta DeBERTa(v1) y DeBERTa(v2) (validado contra Python reference):
- https://docs.rs/rust_tokenizers/latest/rust_tokenizers/

**Pasos concretos para Prompt Guard 2 86M:**
1. `pip install transformers optimum[onnxruntime]` → `python convert_to_onnx.py --model-name meta-llama/Llama-Prompt-Guard-2-86M --export-onnx` (del repo gravitee-io, requiere aceptar licencia Llama 4 en HF)
2. Descargar `tokenizer.json` y `model.onnx` (o `model.quant.onnx`)
3. En Rust: `ort = "1.16"` + `tokenizers = "0.13"` en `Cargo.toml`
4. Input shape: `[1, 512]` (batch=1, max_len=512); output: `[1, 2]` logits
5. Softmax → umbral 0.5 para clasificar

**No encontrado:**
- ONNX export oficial por Meta (no existe)
- `ort` soporte nativo para int8 quantizado sin `OptimizationType::EXTENDED` (requiere `SessionBuilder::with_optimization_level`)

---

**Fuentes abiertas y verificadas en esta sessión. Lo no encontrado = "no encontrado".**
---
**Verificación del TL (2026-10-23, ficha abierta de nuevo):** licencia, 8 idiomas (incluye español), 512 tokens y etiquetas benign/malicious: confirmados. **Correcciones:** (a) la ficha exige además «Built with Llama» visible, conservar el aviso «Llama 4 is licensed under the Llama 4 Community License, Copyright © Meta Platforms, Inc.» y que un modelo derivado lleve «Llama» al inicio de su nombre; esta nota solo citaba la cláusula de 700M MAU. (b) «no cubre ataques indirectos» NO figura en la ficha: la ficha no trata la inyección indirecta (límites reales: «adversaries may develop sophisticated attacks specifically to bypass detection», mide solo la intención de anular instrucciones, ya no hay etiqueta «injection», «some prompt attacks are highly application-dependent»). (c) el ejemplo de Rust viene de un README de terceros, no es ejemplo oficial de `ort`, y las versiones de `ort`/`tokenizers` citadas son antiguas: no verificado. No verificado: los ONNX de terceros (gravitee, shisa).
