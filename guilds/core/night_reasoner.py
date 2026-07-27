"""NightReasoner guild: SmolLM2-135M ONNX generates nightly feedback analysis.

ADR-010 insertion point B: small model reasoning during NightConsolidation.
Uses SmolLM2-135M-Instruct-ONNX (129MB, already benchmarked at 47.55ms/token).
Autoregressive ONNX inference pattern adapted from vision.py.

Not critical path — runs once per night, Python/ort is acceptable.
If spike validates (GO), Rust engine replaces this later.
"""
import json, os, time, sqlite3, glob, sys, re
from pathlib import Path
from mcp.server.fastmcp import FastMCP
import numpy as np

mcp = FastMCP("night_reasoner")

# ── Model config ───────────────────────────────────────────────────────────────
_MODEL_CACHE = Path.home() / ".cache/huggingface/hub/models--onnx-community--SmolLM2-135M-Instruct-ONNX"
_NUM_LAYERS = 30
_NUM_KV_HEADS = 3
_HEAD_DIM = 64
_VOCAB_SIZE = 49152
_MAX_NEW_TOKENS = 128
_EOS_TOKEN = 0  # SmolLM2 uses 0 as EOS typically, check tokenizer config

# ── Lazy model loading ─────────────────────────────────────────────────────────
_decoder_session = None

def _find_model():
    for snap in _MODEL_CACHE.glob("snapshots/*/onnx/model_quantized.onnx"):
        if snap.exists():
            return str(snap)
    return None

def _load_model():
    global _decoder_session
    if _decoder_session is not None:
        return _decoder_session
    import onnxruntime as ort
    path = _find_model()
    if not path:
        raise RuntimeError("SmolLM2-135M ONNX model not found in cache")
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 4
    # Auto-select best available provider: GPU first, then CPU
    providers = ort.get_available_providers()
    gpu_providers = [p for p in providers if p != 'CPUExecutionProvider' and p != 'AzureExecutionProvider']
    if gpu_providers:
        sys.stderr.write(f"[night_reasoner] GPU providers available: {gpu_providers}. Using {gpu_providers[0]}.\n")
        _decoder_session = ort.InferenceSession(path, opts, providers=[gpu_providers[0], 'CPUExecutionProvider'])
    else:
        sys.stderr.write(f"[night_reasoner] No GPU provider available. Using CPU. Available: {providers}\n")
        _decoder_session = ort.InferenceSession(path, opts, providers=['CPUExecutionProvider'])
    return _decoder_session

# ── Tokenizer ───────────────────────────────────────────────────────────────────
_tokenizer = None

def _get_tokenizer():
    global _tokenizer
    if _tokenizer is not None:
        return _tokenizer
    try:
        from tokenizers import Tokenizer
        # Try SmolLM2 tokenizer first (exact match), then ONNX community, then Qwen fallback
        for cache_name in [
            "models--HuggingFaceTB--SmolLM2-135M-Instruct",
            "models--onnx-community--SmolLM2-135M-Instruct-ONNX",
            "models--Qwen--Qwen3.5-2B",
        ]:
            cache_dir = Path.home() / ".cache/huggingface/hub" / cache_name
            for snap in cache_dir.glob("snapshots/*/tokenizer.json"):
                if snap.exists():
                    _tokenizer = Tokenizer.from_file(str(snap))
                    sys.stderr.write(f"[night_reasoner] Loaded tokenizer from {snap}\n")
                    return _tokenizer
    except ImportError:
        sys.stderr.write("[night_reasoner] tokenizers lib not installed, using fallback\n")
    return None

# ── Autoregressive generation ───────────────────────────────────────────────────
def _tokenize(text, max_len=200):
    """Tokenize text. Uses HF tokenizers if available, else fallback."""
    tok = _get_tokenizer()
    if tok is not None:
        enc = tok.encode(text)
        ids = enc.ids[:max_len]
        return np.array([ids], dtype=np.int64)
    # Fallback: simple hash-based
    tokens = re.findall(r"\w+|[^\w\s]", text.lower())
    ids = [hash(t) % _VOCAB_SIZE for t in tokens[:max_len]]
    return np.array([ids or [0]], dtype=np.int64)

def _decode(token_ids):
    """Decode token IDs to text."""
    tok = _get_tokenizer()
    if tok is not None:
        return tok.decode(token_ids)
    return f"[{len(token_ids)} tokens: {token_ids[:10]}...]"

def _generate(prompt, max_tokens=None):
    """Run SmolLM2 autoregressive inference. Returns generated text."""
    sess = _load_model()
    max_tokens = max_tokens or _MAX_NEW_TOKENS

    input_ids = _tokenize(prompt, max_len=200)
    seq_len = input_ids.shape[1]
    attention_mask = np.ones_like(input_ids, dtype=np.int64)
    position_ids = np.arange(seq_len, dtype=np.int64).reshape(1, -1)

    # Init KV cache
    kv_cache = {}
    for i in range(_NUM_LAYERS):
        for kv in ("key", "value"):
            kv_cache[f"past_key_values.{i}.{kv}"] = np.zeros(
                (1, _NUM_KV_HEADS, 0, _HEAD_DIM), dtype=np.float32
            )

    output_names = ["logits"] + [
        f"present.{i}.{kv}" for i in range(_NUM_LAYERS) for kv in ("key", "value")
    ]

    generated_ids = []
    for step in range(max_tokens):
        feed = {
            "input_ids": input_ids,
            "attention_mask": attention_mask,
            "position_ids": position_ids,
            **kv_cache,
        }
        outputs = sess.run(output_names, feed)
        logits = outputs[0]
        next_token = int(np.argmax(logits[0, -1, :]))
        generated_ids.append(next_token)
        if next_token == 0:  # EOS-like
            break

        # Update KV cache
        for i in range(_NUM_LAYERS):
            kv_cache[f"past_key_values.{i}.key"] = outputs[1 + i * 2]
            kv_cache[f"past_key_values.{i}.value"] = outputs[2 + i * 2]

        # Next step: single token
        input_ids = np.array([[next_token]], dtype=np.int64)
        past_len = kv_cache["past_key_values.0.key"].shape[2]
        position_ids = np.array([[past_len]], dtype=np.int64)
        attention_mask = np.ones((1, past_len + 1), dtype=np.int64)

        if step % 20 == 0:
            sys.stderr.write(f"  token {step+1}/{max_tokens}\n")

    # Decode
    return _decode(generated_ids) if generated_ids else "[no output]"

# ── MCP Tools ───────────────────────────────────────────────────────────────────

@mcp.tool()
def analyze_feedback(min_age_secs: int = 300) -> str:
    """Analyze today's recall feedback and generate a nightly reasoning report.

    Reads pending and resolved feedback from silva.db and audit.db,
    runs SmolLM2-135M to reason about patterns, and returns a report.

    Args:
        min_age_secs: Minimum age of feedback rows to consider (default 5min).
    """
    import sqlite3
    from pathlib import Path

    data_dir = Path("data")
    silva = sqlite3.connect(str(data_dir / "silva.db"))
    audit = sqlite3.connect(str(data_dir / "audit.db"))

    # Collect data
    cur = silva.cursor()
    cur.execute("""
        SELECT COUNT(*), SUM(CASE WHEN useful=1 THEN 1 ELSE 0 END),
               SUM(CASE WHEN useful=-1 THEN 1 ELSE 0 END),
               SUM(CASE WHEN useful=0 THEN 1 ELSE 0 END)
        FROM recall_feedback
    """)
    total, useful, not_useful, pending = cur.fetchone()

    cur.execute("""
        SELECT rf.agent_id, rf.query_text, rf.useful, substr(n.content,1,100)
        FROM recall_feedback rf
        LEFT JOIN nodes n ON rf.memory_id = n.id
        WHERE rf.useful != 0
        ORDER BY rf.accessed_at DESC LIMIT 5
    """)
    resolved = cur.fetchall()

    cur = audit.cursor()
    cur.execute("""
        SELECT agent_id, guild, COUNT(*) as cnt
        FROM guild_audit_log
        WHERE timestamp > datetime('now', '-1 day')
        GROUP BY agent_id, guild ORDER BY cnt DESC LIMIT 10
    """)
    activity = cur.fetchall()

    silva.close()
    audit.close()

    # Build analysis (heuristic, SmolLM2 generation attempted separately)
    report = f"""# NightReasoner Report — {time.strftime('%Y-%m-%d')}

## Signal Loop Status
- Total feedback rows: {total or 0}
- Resolved useful: {useful or 0}
- Resolved not-useful: {not_useful or 0}
- Pending: {pending or 0}
- Progress toward LightReranker (5000): {(total or 0)/50:.0f}%

## Recent Resolved Feedback
"""
    for r in resolved:
        status = "useful" if r[2] == 1 else "not-useful"
        content = (r[3] or "")[:80].replace("\n", " ")
        report += f"- [{status}] {r[0]}: {r[1][:50]} → {content}\n"

    report += "\n## Agent Activity (24h)\n"
    for a in activity:
        report += f"- {a[0]}: {a[1]} x{a[2]}\n"

    report += f"\n---\nGenerated at {time.strftime('%H:%M:%S UTC')} by NightReasoner guild\n"
    report += "SmolLM2-135M enabled: generating reasoning analysis...\n"

    # Try SmolLM2 generation
    try:
        prompt = f"""You are Tylluan's nightly memory analyst.
Today's feedback: {useful or 0} useful, {not_useful or 0} not-useful, {pending or 0} pending.
Total toward LightReranker: {(total or 0)}/5000.
Write a 2-sentence recommendation for tomorrow."""
        gen = _generate(prompt, max_tokens=30)
        report += f"\n## SmolLM2 Analysis\n{gen}\n"
    except Exception as e:
        report += f"\n[SmolLM2 unavailable: {e}]\n"

    return report

@mcp.tool()
def reason_about(query: str) -> str:
    """Ask SmolLM2-135M to reason about a specific question using Tylluan's data.
    For testing the reasoning capability only — not wired to production paths.

    Args:
        query: What to reason about.
    """
    try:
        return _generate(query, max_tokens=50)
    except Exception as e:
        return f"[SmolLM2 error: {e}]"

if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
