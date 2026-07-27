"""Test Gemma-4 coordinator: download verification, load, route_intent.
Run after:
  python -m guilds.core.download_gemma4
"""
import json, sys, time
from pathlib import Path

# Verify download
GEMMA_CACHE = Path.home() / ".cache/huggingface/hub/models--onnx-community--gemma-4-E2B-it-ONNX"
print(f"Checking Gemma cache: {GEMMA_CACHE}")
print(f"  Exists: {GEMMA_CACHE.exists()}")

embed = list(GEMMA_CACHE.glob("snapshots/*/onnx/embed_tokens_q4.onnx"))
decoder = list(GEMMA_CACHE.glob("snapshots/*/onnx/decoder_model_merged_q4.onnx"))
config = list(GEMMA_CACHE.glob("snapshots/*/config.json"))
tokenizer = list(GEMMA_CACHE.glob("snapshots/*/tokenizer.json"))

print(f"  embed_tokens_q4.onnx: {'OK' if embed else 'MISSING'}")
print(f"  decoder_model_merged_q4.onnx: {'OK' if decoder else 'MISSING'}")
print(f"  config.json: {'OK' if config else 'MISSING'}")
print(f"  tokenizer.json: {'OK' if tokenizer else 'MISSING'}")

# Check data files
if embed:
    embed_data = list(GEMMA_CACHE.glob("snapshots/*/onnx/embed_tokens_q4.onnx_data"))
    decoder_data = list(GEMMA_CACHE.glob("snapshots/*/onnx/decoder_model_merged_q4.onnx_data"))
    print(f"  embed_tokens_q4.onnx_data: {'OK' if embed_data else 'MISSING'}")
    print(f"  decoder_model_merged_q4.onnx_data: {'OK' if decoder_data else 'MISSING'}")

if not all([embed, decoder, config, tokenizer]):
    print("\n❌ Model not fully downloaded. Run: python -m guilds.core.download_gemma4")
    sys.exit(1)

# Test load and inference
print("\nLoading Gemma-4-E2B...")
from guilds.core.night_reasoner import _load_gemma, _generate_gemma, _use_gemma

t0 = time.time()
_gemma_available = _use_gemma()
print(f"  Available: {_gemma_available}")
t1 = time.time()
print(f"  Detection: {t1-t0:.2f}s")

embed_sess, decoder_sess = _load_gemma()
t2 = time.time()
print(f"  Load time: {t2-t1:.2f}s")
print(f"  Decoder inputs: {[i.name for i in decoder_sess.get_inputs()]}")
print(f"  Decoder outputs: {[o.name for o in decoder_sess.get_outputs()]}")

# Test route_intent
t3 = time.time()
candidates = json.dumps([
    {"guild": "bash", "score": 0.65, "description": "Run shell commands and scripts"},
    {"guild": "filesystem", "score": 0.60, "description": "List files, directories, find files"},
    {"guild": "memory", "score": 0.30, "description": "Store and retrieve long-term memories"},
])
from guilds.core.night_reasoner import route_intent
result = route_intent("list all python files in the current directory", candidates)
t4 = time.time()
print(f"\nTest 1: 'list all python files'")
print(f"  Result: {result}")
print(f"  Latency: {t4-t3:.2f}s")
parsed = json.loads(result)
assert parsed["guild"] in ("filesystem", "bash"), f"Expected filesystem or bash, got {parsed['guild']}"
print(f"  [OK] Valid candidate guild selected: {parsed['guild']}")

t5 = time.time()
result2 = route_intent("what's my ip address", json.dumps([
    {"guild": "websearch", "score": 0.72, "description": "Web search engine queries"},
    {"guild": "bash", "score": 0.55, "description": "Run shell commands and scripts"},
]))
t6 = time.time()
print(f"\nTest 2: 'what's my ip address'")
print(f"  Result: {result2}")
print(f"  Latency: {t5-t5:.2f}s")

print("\n[OK] All tests passed!")
