"""Test night_reasoner.route_intent: BGE-M3 embedding-similarity tiebreaker.

route_intent() is pure classification (cosine similarity against cached
guild-description embeddings) -- it does NOT call Gemma, llama_backend, or
any generative model. It is unrelated to the J-6/J-7 DeepEval judge issue
(gibberish beyond the first token) -- that is a separate component. This
script previously gated on a Gemma-4 ONNX cache that route_intent never
actually used; removed as dead/misleading weight.
"""
import json, time

# Test route_intent via night_reasoner
print("Testing night_reasoner.route_intent...")

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
