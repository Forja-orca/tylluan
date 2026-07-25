# Benchmark Empírico ADR-010: Medición Real de Modelos ONNX en Disco

**Fecha:** 2026-07-25  
**Entorno de Ejecución:** Windows | 56 Hilos Lógicos | 221.9 GB RAM  
**Motor:** ONNX Runtime (`ort`)  
**Metodología:** Inferencia en vivo (`sess.run`) exclusivamente sobre modelos descargados en disco. Modelos no descargados figuran estrictamente como `No Instalado`.

---

## 1. Mediciones Empíricas Reales

### Model: `bge_m3` (0.69 MB en disco)
- **Estado:** 🟢 `Medido en Vivo (Real)`
- **Ruta:** `.fastembed_cache/models--BAAI--bge-m3/snapshots\5617a9f61b028005a4858fdac845db406aefb181\onnx\model.onnx`
- **Tiempo de Carga:** `4215.52 ms`
- **Latencia p50 (16 tokens):** `90.94 ms` | **p95:** `98.1 ms` | **Mean:** `92.39 ms`
- **Throughput Real:** `10.8 seq/sec`

### Model: `distilbert_base_uncased` (64.57 MB en disco)
- **Estado:** 🟢 `Medido en Vivo (Real)`
- **Ruta:** `C:\Users\FoRJa/.cache/huggingface/hub/models--Xenova--distilbert-base-uncased/snapshots\5d73105e39e322b779ff7a8fcd11530fa579165b\onnx\model_quantized.onnx`
- **Tiempo de Carga:** `602.69 ms`
- **Latencia p50 (16 tokens):** `20.12 ms` | **p95:** `20.78 ms` | **Mean:** `20.27 ms`
- **Throughput Real:** `49.3 seq/sec`

### Model: `t5_small_encoder` (33.99 MB en disco)
- **Estado:** 🟢 `Medido en Vivo (Real)`
- **Ruta:** `C:\Users\FoRJa/.cache/huggingface/hub/models--Xenova--t5-small/snapshots\4e0d91096b13cb313b43a14f35fdbb311a6d9728\onnx\encoder_model_quantized.onnx`
- **Tiempo de Carga:** `213.83 ms`
- **Latencia p50 (16 tokens):** `5.42 ms` | **p95:** `5.7 ms` | **Mean:** `5.41 ms`
- **Throughput Real:** `184.8 seq/sec`

### Model: `smollm2_135m` (129.37 MB en disco)
- **Estado:** 🟢 `Medido en Vivo (Real)`
- **Ruta:** `C:\Users\FoRJa/.cache/huggingface/hub/models--onnx-community--SmolLM2-135M-Instruct-ONNX/snapshots\b8a5c0f183b78c55955a5364f610c36668b5e681\onnx\model_quantized.onnx`
- **Tiempo de Carga:** `2418.82 ms`
- **Latencia p50 (16 tokens):** `47.55 ms` | **p95:** `48.06 ms` | **Mean:** `47.53 ms`
- **Throughput Real:** `21.0 seq/sec`

### Model: `smollm2_360m`
- **Estado:** ⚠️ `No Instalado en Disco` (File not found matching C:\Users\FoRJa/.cache/huggingface/hub/models--onnx-community--SmolLM2-360M-Instruct-ONNX/snapshots/*/onnx/model_q4f16.onnx)

### Model: `qwen3_17b`
- **Estado:** ⚠️ `No Instalado en Disco` (File not found matching C:\Users\FoRJa/.cache/huggingface/hub/models--onnx-community--Qwen3-1.7B-ONNX/snapshots/*/onnx/model_q4f16.onnx)

---

## 2. Comparativa Directa entre T5-Small vs SmolLM2-135M

| Métrica | T5-Small Encoder (Quantized) | SmolLM2-135M (Quantized) |
| :--- | :--- | :--- |
| **Tamaño en Disco** | `33.99 MB` | `129.37 MB` |
| **Tiempo Carga ONNX** | `213.83 ms` | `2418.82 ms` |
| **Latencia Inferencia p50** | `5.42 ms` | `47.55 ms` |
| **Latencia Inferencia p95** | `5.7 ms` | `48.06 ms` |
| **Throughput (Seq/sec)** | `184.8` | `21.0` |
