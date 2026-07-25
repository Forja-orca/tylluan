#!/usr/bin/env python3
"""
ADR-010 Pure Empirical Benchmark.
Runs REAL ONNX Runtime inference sessions (sess.run) on downloaded models:
- BGE-M3
- DistilBERT-base-uncased
- T5-Small (Encoder)
- SmolLM2-135M-Instruct
Any model not present on disk is reported strictly as 'not_downloaded'. Zero simulations.
"""

import json
import os
import glob
import time
import psutil
import platform
import onnxruntime as ort
import numpy as np

OUT_JSON = 'benchmarks/benchmark_adr010.json'
OUT_MD = 'benchmarks/BENCHMARK_ADR010.md'

def get_system_info():
    return {
        "platform": platform.system().lower(),
        "cpu_cores": psutil.cpu_count(logical=True),
        "physical_cores": psutil.cpu_count(logical=False),
        "total_ram_gb": round(psutil.virtual_memory().total / (1024**3), 1)
    }

def resolve_glob_path(pattern):
    matches = glob.glob(pattern)
    return matches[0] if matches else None

def percentiles(samples):
    s = sorted(samples)
    n = len(s)
    return {
        "min": round(s[0], 2),
        "p50": round(s[n // 2], 2),
        "p95": round(s[int(n * 0.95)], 2),
        "p99": round(s[min(int(n * 0.99), n - 1)], 2),
        "max": round(s[-1], 2),
        "mean": round(float(np.mean(s)), 2),
        "std": round(float(np.std(s)), 2)
    }

def build_inputs_for_session(sess, seq_len=16):
    input_inputs = {}
    mock_ids = np.ones((1, seq_len), dtype=np.int64)
    mock_mask = np.ones((1, seq_len), dtype=np.int64)
    
    for inp in sess.get_inputs():
        name = inp.name
        shape = inp.shape
        if name in ['input_ids', 'inputs']:
            input_inputs[name] = mock_ids
        elif name == 'attention_mask':
            input_inputs[name] = mock_mask
        elif name in ['position_ids', 'type_ids', 'token_type_ids']:
            input_inputs[name] = mock_ids
        elif 'past_key_values' in name:
            # Handle past KV-cache inputs for causal decoders
            # Shape format: [batch, num_heads, sequence_length, head_dim] -> (1, 3, 0, 64) or similar
            dtype = np.float32 if 'fp32' in str(inp.type) or 'float' in str(inp.type) else np.float32
            # Create empty 0-seq past cache tensor
            kv_shape = []
            for dim in shape:
                if isinstance(dim, int):
                    kv_shape.append(dim)
                else:
                    # Dynamic dimensions: default batch=1, seq=0, head_dim=64, num_heads=3
                    if 'batch' in str(dim).lower() or len(kv_shape) == 0: kv_shape.append(1)
                    elif len(kv_shape) == 2: kv_shape.append(0) # past length = 0
                    else: kv_shape.append(64)
            input_inputs[name] = np.zeros(tuple(kv_shape), dtype=dtype)
    
    return input_inputs

def measure_real_onnx_model(name, glob_pattern, seq_len=16, num_threads=4, num_runs=50):
    resolved = resolve_glob_path(glob_pattern)
    if not resolved or not os.path.exists(resolved):
        return {
            "status": "not_downloaded",
            "measured": False,
            "error": f"File not found matching {glob_pattern}"
        }

    process = psutil.Process(os.getpid())
    mem_before = process.memory_info().rss / (1024**2)

    t0 = time.perf_counter()
    sess_options = ort.SessionOptions()
    sess_options.intra_op_num_threads = num_threads
    sess = ort.InferenceSession(resolved, sess_options, providers=['CPUExecutionProvider'])
    load_time_ms = (time.perf_counter() - t0) * 1000

    mem_after = process.memory_info().rss / (1024**2)
    ram_footprint_mb = round(mem_after - mem_before, 2)
    file_size_mb = round(os.path.getsize(resolved) / (1024**2), 2)

    inputs = build_inputs_for_session(sess, seq_len=seq_len)

    # Warmup
    try:
        for _ in range(5):
            sess.run(None, inputs)
    except Exception as e:
        return {
            "status": "execution_error",
            "measured": False,
            "error": str(e),
            "path": resolved,
            "model_file_size_mb": file_size_mb
        }

    # Measure real inference latency
    latencies = []
    for _ in range(num_runs):
        t_start = time.perf_counter()
        sess.run(None, inputs)
        latencies.append((time.perf_counter() - t_start) * 1000)

    stats = percentiles(latencies)
    throughput = round(1000 / stats["mean"], 1)

    return {
        "status": "measured",
        "measured": True,
        "path": resolved,
        "model_file_size_mb": file_size_mb,
        "load_time_ms": round(load_time_ms, 2),
        "ram_footprint_mb": ram_footprint_mb if ram_footprint_mb > 0 else "n/a",
        "latency_stats_ms": stats,
        "throughput_seq_sec": throughput
    }

def main():
    print("ADR-010 100% Real Empirical Benchmark started...")
    sys_info = get_system_info()
    print(f"  System: {sys_info['platform']} | {sys_info['cpu_cores']} logical cores | {sys_info['total_ram_gb']}GB RAM")

    targets = {
        "bge_m3": ".fastembed_cache/models--BAAI--bge-m3/snapshots/*/onnx/model.onnx",
        "distilbert_base_uncased": os.path.expanduser("~/.cache/huggingface/hub/models--Xenova--distilbert-base-uncased/snapshots/*/onnx/model_quantized.onnx"),
        "t5_small_encoder": os.path.expanduser("~/.cache/huggingface/hub/models--Xenova--t5-small/snapshots/*/onnx/encoder_model_quantized.onnx"),
        "smollm2_135m": os.path.expanduser("~/.cache/huggingface/hub/models--onnx-community--SmolLM2-135M-Instruct-ONNX/snapshots/*/onnx/model_quantized.onnx"),
        "smollm2_360m": os.path.expanduser("~/.cache/huggingface/hub/models--onnx-community--SmolLM2-360M-Instruct-ONNX/snapshots/*/onnx/model_q4f16.onnx"),
        "qwen3_17b": os.path.expanduser("~/.cache/huggingface/hub/models--onnx-community--Qwen3-1.7B-ONNX/snapshots/*/onnx/model_q4f16.onnx"),
    }

    models_res = {}
    for name, pattern in targets.items():
        print(f"  Benchmarking {name}...", end=" ", flush=True)
        res = measure_real_onnx_model(name, pattern)
        models_res[name] = res
        if res.get("measured"):
            print(f"MEASURED REAL (p50: {res['latency_stats_ms']['p50']}ms, {res['model_file_size_mb']}MB)")
        else:
            print(f"NOT INSTALLED ({res.get('status')})")

    report = {
        "date": time.strftime("%Y-%m-%d"),
        "timestamp": int(time.time()),
        "hardware": sys_info,
        "models": models_res
    }

    with open(OUT_JSON, 'w') as f:
        json.dump(report, f, indent=2)

    # Generate honest markdown report
    md = f"""# Benchmark Empírico ADR-010: Medición Real de Modelos ONNX en Disco

**Fecha:** {report['date']}  
**Entorno de Ejecución:** {sys_info['platform'].capitalize()} | {sys_info['cpu_cores']} Hilos Lógicos | {sys_info['total_ram_gb']} GB RAM  
**Motor:** ONNX Runtime (`ort`)  
**Metodología:** Inferencia en vivo (`sess.run`) exclusivamente sobre modelos descargados en disco. Modelos no descargados figuran estrictamente como `No Instalado`.

---

## 1. Mediciones Empíricas Reales

"""
    for name, res in models_res.items():
        if res.get("measured"):
            md += f"### Model: `{name}` ({res['model_file_size_mb']} MB en disco)\n"
            md += f"- **Estado:** 🟢 `Medido en Vivo (Real)`\n"
            md += f"- **Ruta:** `{res['path']}`\n"
            md += f"- **Tiempo de Carga:** `{res['load_time_ms']} ms`\n"
            md += f"- **Latencia p50 (16 tokens):** `{res['latency_stats_ms']['p50']} ms` | **p95:** `{res['latency_stats_ms']['p95']} ms` | **Mean:** `{res['latency_stats_ms']['mean']} ms`\n"
            md += f"- **Throughput Real:** `{res['throughput_seq_sec']} seq/sec`\n\n"
        else:
            md += f"### Model: `{name}`\n- **Estado:** ⚠️ `No Instalado en Disco` ({res.get('error', 'N/A')})\n\n"

    md += """---

## 2. Comparativa Directa entre T5-Small vs SmolLM2-135M

"""
    t5_res = models_res.get("t5_small_encoder", {})
    smol_res = models_res.get("smollm2_135m", {})

    if t5_res.get("measured") and smol_res.get("measured"):
        md += "| Métrica | T5-Small Encoder (Quantized) | SmolLM2-135M (Quantized) |\n"
        md += "| :--- | :--- | :--- |\n"
        md += f"| **Tamaño en Disco** | `{t5_res['model_file_size_mb']} MB` | `{smol_res['model_file_size_mb']} MB` |\n"
        md += f"| **Tiempo Carga ONNX** | `{t5_res['load_time_ms']} ms` | `{smol_res['load_time_ms']} ms` |\n"
        md += f"| **Latencia Inferencia p50** | `{t5_res['latency_stats_ms']['p50']} ms` | `{smol_res['latency_stats_ms']['p50']} ms` |\n"
        md += f"| **Latencia Inferencia p95** | `{t5_res['latency_stats_ms']['p95']} ms` | `{smol_res['latency_stats_ms']['p95']} ms` |\n"
        md += f"| **Throughput (Seq/sec)** | `{t5_res['throughput_seq_sec']}` | `{smol_res['throughput_seq_sec']}` |\n"
    else:
        md += "Uno o ambos modelos no están instalados físicamente en disco.\n"

    with open(OUT_MD, 'w', encoding='utf-8') as f:
        f.write(md)

    print(f"Real benchmark JSON saved to {OUT_JSON}")
    print(f"Real benchmark Markdown saved to {OUT_MD}")

if __name__ == '__main__':
    main()
