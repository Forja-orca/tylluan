"""NightReasoner guild: Small model reasoning and analysis.

Uses three models in a cascade, each for what it's actually built for:
1. BGE-M3 embeddings (already in kernel, 1024d) — intent routing via cosine
   similarity with pre-computed guild embeddings. Classification, not chat.
2. Gemma-4-E2B (2.3B, loaded ONNX) — reasoning tasks where actual generation
   is needed (digesting feedback, generating nightly insights).
3. SmolLM2-135M — fallback for simple pattern summarization when Gemma
   is unavailable.

Architecture rule (M19-P5): routing is a CLASSIFICATION problem, solved with
embeddings + similarity. Chat models generate text; they don't classify.
Gemma-4 is for REASONING (ADR-010 §7 Point B), not routing (Point A).

Not critical path — Python/ort is acceptable for non-blocking async guilds.
If spike validates (GO), Rust engine replaces selected paths.
"""
import json, os, time, sqlite3, sys, re
from pathlib import Path
from mcp.server.fastmcp import FastMCP
import numpy as np

mcp = FastMCP("night_reasoner")

# ── SmolLM2-135M config (fallback) ──────────────────────────────────────────
_SMOL_CACHE = Path.home() / ".cache/huggingface/hub/models--onnx-community--SmolLM2-135M-Instruct-ONNX"
_SMOL_LAYERS = 30
_SMOL_KV_HEADS = 3
_SMOL_HEAD_DIM = 64
_SMOL_VOCAB = 49152

# ── Gemma-4-E2B config (primary coordinator) ───────────────────────────────
_GEMMA_CACHE = Path.home() / ".cache/huggingface/hub/models--onnx-community--gemma-4-E2B-it-ONNX"
_GEMMA_LAYERS = 35
_GEMMA_KV_HEADS = 1          # MQA — 1 KV head
_GEMMA_HEAD_DIM = 256        # per KV head
_GEMMA_HIDDEN = 1536         # d_model
_GEMMA_VOCAB = 262144
_GEMMA_PLE_DIM = 256         # per-layer embedding dimension
_EOS_TOKENS = {1, 106}       # Gemma-4 EOS token IDs

_MAX_NEW_TOKENS = 128

# ── Lazy model sessions ─────────────────────────────────────────────────────
_smol_session = None
_gemma_embed = None
_gemma_decoder = None
_tokenizer = None

# ── Model discovery ─────────────────────────────────────────────────────────

def _find_model(cache_root: Path, pattern: str = "onnx/model_quantized.onnx"):
    """Find a file in HF cache snapshot directories."""
    for snap in cache_root.glob(f"snapshots/*/{pattern}"):
        if snap.exists():
            return str(snap)
    return None

def _find_gemma_component(component: str):
    """Find a Gemma-4 ONNX component file (q4)."""
    for snap in _GEMMA_CACHE.glob(f"snapshots/*/onnx/{component}.onnx"):
        if snap.exists():
            return str(snap)
    return None

def _gemma_available():
    return _find_gemma_component("decoder_model_merged_q4") is not None

def _smol_available():
    return _find_model(_SMOL_CACHE) is not None

# ── Tokenizer (shared, tries Gemma first) ───────────────────────────────────

def _get_tokenizer():
    global _tokenizer
    if _tokenizer is not None:
        return _tokenizer
    try:
        from tokenizers import Tokenizer
        for cache_name in [
            "models--onnx-community--gemma-4-E2B-it-ONNX",
            "models--HuggingFaceTB--SmolLM2-135M-Instruct",
            "models--onnx-community--SmolLM2-135M-Instruct-ONNX",
        ]:
            cache_dir = Path.home() / ".cache/huggingface/hub" / cache_name
            for snap in cache_dir.glob("snapshots/*/tokenizer.json"):
                if snap.exists():
                    _tokenizer = Tokenizer.from_file(str(snap))
                    sys.stderr.write(f"[night_reasoner] Loaded tokenizer from {snap}\n")
                    return _tokenizer
    except ImportError:
        sys.stderr.write("[night_reasoner] tokenizers lib not installed\n")
    return None

# ── SmolLM2 fallback path ───────────────────────────────────────────────────

def _load_smol():
    global _smol_session
    if _smol_session is not None:
        return _smol_session
    import onnxruntime as ort
    path = _find_model(_SMOL_CACHE)
    if not path:
        raise RuntimeError("SmolLM2-135M ONNX model not found")
    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 4
    providers = ort.get_available_providers()
    gpu = [p for p in providers if p not in ('CPUExecutionProvider', 'AzureExecutionProvider')]
    _smol_session = ort.InferenceSession(path, opts, providers=gpu + ['CPUExecutionProvider'] if gpu else ['CPUExecutionProvider'])
    return _smol_session

def _tokenize_smol(text, max_len=200):
    tok = _get_tokenizer()
    if tok is not None:
        enc = tok.encode(text)
        return np.array([enc.ids[:max_len]], dtype=np.int64)
    tokens = re.findall(r"\w+|[^\w\s]", text.lower())
    ids = [hash(t) % _SMOL_VOCAB for t in tokens[:max_len]]
    return np.array([ids or [0]], dtype=np.int64)

def _decode_smol(token_ids):
    tok = _get_tokenizer()
    if tok is not None:
        return tok.decode(token_ids)
    return f"[{len(token_ids)} tokens]"

def _generate_smol(prompt, max_tokens=None):
    sess = _load_smol()
    max_tokens = max_tokens or _MAX_NEW_TOKENS
    input_ids = _tokenize_smol(prompt, max_len=200)
    seq_len = input_ids.shape[1]
    attention_mask = np.ones_like(input_ids, dtype=np.int64)
    position_ids = np.arange(seq_len, dtype=np.int64).reshape(1, -1)

    kv_cache = {}
    for i in range(_SMOL_LAYERS):
        for kv in ("key", "value"):
            kv_cache[f"past_key_values.{i}.{kv}"] = np.zeros(
                (1, _SMOL_KV_HEADS, 0, _SMOL_HEAD_DIM), dtype=np.float32)

    output_names = ["logits"] + [
        f"present.{i}.{kv}" for i in range(_SMOL_LAYERS) for kv in ("key", "value")]

    generated = []
    for step in range(max_tokens):
        feed = {"input_ids": input_ids, "attention_mask": attention_mask, "position_ids": position_ids, **kv_cache}
        outputs = sess.run(output_names, feed)
        next_token = int(np.argmax(outputs[0][0, -1, :]))
        generated.append(next_token)
        if next_token == 0:
            break
        for i in range(_SMOL_LAYERS):
            kv_cache[f"past_key_values.{i}.key"] = outputs[1 + i * 2]
            kv_cache[f"past_key_values.{i}.value"] = outputs[2 + i * 2]
        input_ids = np.array([[next_token]], dtype=np.int64)
        past_len = kv_cache["past_key_values.0.key"].shape[2]
        position_ids = np.array([[past_len]], dtype=np.int64)
        attention_mask = np.ones((1, past_len + 1), dtype=np.int64)
    return _decode_smol(generated) if generated else "[no output]"

# ── Gemma-4 coordinator path ────────────────────────────────────────────────

def _log_model_structure(session, label):
    """Log every input and output of an ONNX session with name and shape.
    This is the structural diagnostic that was missing before — hardcoded shape
    assumptions killed three attempts today."""
    sys.stderr.write(f"[night_reasoner] === {label} model structure ===\n")
    for inp in session.get_inputs():
        sys.stderr.write(f"  INPUT  {inp.name}: shape={inp.shape}, type={inp.type}\n")
    for out in session.get_outputs():
        sys.stderr.write(f"  OUTPUT {out.name}: shape={out.shape}, type={out.type}\n")
    sys.stderr.write(f"[night_reasoner] === end {label} ===\n")

def _load_gemma():
    global _gemma_embed, _gemma_decoder
    if _gemma_decoder is not None:
        return _gemma_embed, _gemma_decoder
    import onnxruntime as ort

    embed_path = _find_gemma_component("embed_tokens_q4")
    decoder_path = _find_gemma_component("decoder_model_merged_q4")
    if not embed_path or not decoder_path:
        raise RuntimeError(f"Gemma-4-E2B not found in cache (embed={embed_path}, decoder={decoder_path})")

    opts = ort.SessionOptions()
    opts.intra_op_num_threads = 4
    providers = ort.get_available_providers()
    gpu = [p for p in providers if p not in ('CPUExecutionProvider', 'AzureExecutionProvider')]
    provider_list = gpu + ['CPUExecutionProvider'] if gpu else ['CPUExecutionProvider']

    sys.stderr.write(f"[night_reasoner] Gemma providers: {provider_list}\n")
    _gemma_embed = ort.InferenceSession(embed_path, opts, providers=provider_list)
    _gemma_decoder = ort.InferenceSession(decoder_path, opts, providers=provider_list)

    _log_model_structure(_gemma_embed, "embed_tokens_q4")
    _log_model_structure(_gemma_decoder, "decoder_model_merged_q4")

    return _gemma_embed, _gemma_decoder

def _init_gemma_kv(decoder_session, batch_size=1):
    """Initialize KV cache for Gemma-4 decoder.

    Uses the same pattern as the official ONNX Runtime Python example:
    past_key_values are always [batch, num_kv_heads, seq_len=0, head_dim].
    Dimensions 0 (batch) and 2 (seq_len) are dynamic; 1 (num_heads) and
    3 (head_dim) are fixed in the ONNX graph and we read them directly.
    """
    kv_cache = {}
    for inp in decoder_session.get_inputs():
        if inp.name.startswith("past_key_values"):
            s = inp.shape
            dtype = np.float32 if inp.type == "tensor(float)" else np.float16
            # Dynamic dims in ONNX shape can be strings ('batch_size', 'past_sequence_length') or ints
            b = batch_size if (s[0] is None or isinstance(s[0], str)) else int(s[0])
            h = 1 if (s[1] is None or isinstance(s[1], str)) else int(s[1])
            d = 256 if (s[3] is None or isinstance(s[3], str)) else int(s[3])
            kv_cache[inp.name] = np.zeros([b, h, 0, d], dtype=dtype)
    # Verify every past_key_values input has a corresponding present output
    present_names = [o.name for o in decoder_session.get_outputs() if o.name.startswith("present.")]
    for pn in present_names:
        kv_name = pn.replace("present.", "past_key_values.")
        if kv_name not in kv_cache:
            sys.stderr.write(f"[night_reasoner] WARNING: {pn} has no matching past_key_values input! "
                           f"Known KV inputs: {list(kv_cache.keys())}\n")
    return kv_cache

def _tokenize_gemma(text, max_len=200):
    """Tokenize for Gemma-4 using the 262K vocab tokenizer."""
    tok = _get_tokenizer()
    if tok is not None:
        enc = tok.encode(text)
        ids = enc.ids[:max_len]
        # Ensure BOS token (2) is present for Gemma-4
        if not ids or ids[0] != 2:
            ids = [2] + ids
        return np.array([ids], dtype=np.int64)
    raise RuntimeError("No tokenizer available for Gemma-4")

def _decode_gemma(token_ids):
    tok = _get_tokenizer()
    if tok is not None:
        text = tok.decode(token_ids)
        return text
    return f"[{len(token_ids)} tokens]"

def _generate_gemma(prompt, max_tokens=None):
    """Generate with Gemma-4-E2B using embed_tokens + decoder_merged pattern."""
    embed, decoder = _load_gemma()
    max_tokens = max_tokens or _MAX_NEW_TOKENS

    input_ids = _tokenize_gemma(prompt, max_len=300)
    seq_len = input_ids.shape[1]
    attention_mask = np.ones((1, seq_len), dtype=np.int64)
    position_ids = np.arange(seq_len, dtype=np.int64).reshape(1, -1)
    num_logits_to_keep = np.array(1, dtype=np.int64)

    # Discover KV cache structure from decoder inputs
    # Hardcode KV cache input names; we discover output names dynamically
    kv_cache = _init_gemma_kv(decoder, batch_size=1)

    # Discover output names for KV present values
    present_names = [o.name for o in decoder.get_outputs() if o.name.startswith("present.")]

    generated = []
    for step in range(max_tokens):
        # Embed: input_ids → inputs_embeds + per_layer_inputs
        inputs_embeds, per_layer_inputs = embed.run(None, {"input_ids": input_ids})

        decoder_inputs = {
            "inputs_embeds": inputs_embeds,
            "attention_mask": attention_mask,
            "per_layer_inputs": per_layer_inputs,
            "position_ids": position_ids,
            "num_logits_to_keep": num_logits_to_keep,
            **kv_cache,
        }
        outputs = decoder.run(None, decoder_inputs)
        logits = outputs[0]

        # Update KV cache from present outputs
        for j, name in enumerate(present_names):
            kv_cache[name.replace("present.", "past_key_values.")] = outputs[1 + j]

        next_token = int(np.argmax(logits[0, -1, :]))
        generated.append(next_token)

        if next_token in _EOS_TOKENS:
            break

        # Prepare for next step: single token
        input_ids = np.array([[next_token]], dtype=np.int64)
        past_len = kv_cache[present_names[0].replace("present.", "past_key_values.")].shape[2]
        position_ids = np.array([[past_len]], dtype=np.int64)
        attention_mask = np.ones((1, past_len + 1), dtype=np.int64)

        if step % 20 == 0:
            sys.stderr.write(f"  gemma token {step+1}/{max_tokens}\n")

    return _decode_gemma(generated) if generated else "[no output]"

# ── Auto-select best model for task ─────────────────────────────────────────

_USE_GEMMA = None

def _use_gemma():
    global _USE_GEMMA
    if _USE_GEMMA is not None:
        return _USE_GEMMA
    _USE_GEMMA = _gemma_available()
    if _USE_GEMMA:
        sys.stderr.write("[night_reasoner] Gemma-4-E2B available — using as primary coordinator\n")
    else:
        sys.stderr.write("[night_reasoner] Gemma-4-E2B not found — falling back to SmolLM2-135M\n")
    return _USE_GEMMA

# ── MCP Tools ───────────────────────────────────────────────────────────────

@mcp.tool()
def analyze_feedback(min_age_secs: int = 300) -> str:
    """Analyze today's recall feedback and generate a nightly reasoning report.

    Uses SmolLM2-135M for lightweight pattern summarization (this is a
    simple aggregation task, not a routing decision — Gemma-4 would be
    overkill here).

    Args:
        min_age_secs: Minimum age of feedback rows to consider (default 5min).
    """
    import sqlite3
    data_dir = Path("data")
    silva = sqlite3.connect(str(data_dir / "silva.db"))
    audit = sqlite3.connect(str(data_dir / "audit.db"))

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

    try:
        prompt = f"""You are Tylluan's nightly memory analyst.
Today's feedback: {useful or 0} useful, {not_useful or 0} not-useful, {pending or 0} pending.
Total toward LightReranker: {(total or 0)}/5000.
Write a 2-sentence recommendation for tomorrow."""
        gen = _generate_smol(prompt, max_tokens=30)
        report += f"\n## SmolLM2 Analysis\n{gen}\n"
    except Exception as e:
        report += f"\n[SmolLM2 unavailable: {e}]\n"

    return report

@mcp.tool()
def reason_about(query: str) -> str:
    """Ask the coordinator model to reason about a specific question.

    Uses Gemma-4-E2B (2.3B) when available, falls back to SmolLM2-135M.
    For testing reasoning capability — not wired to production paths.

    Args:
        query: What to reason about.
    """
    if _use_gemma():
        try:
            return _generate_gemma(query, max_tokens=64)
        except Exception as e:
            return f"[Gemma-4 error, fallback: {e}]"
    try:
        return _generate_smol(query, max_tokens=50)
    except Exception as e:
        return f"[SmolLM2 error: {e}]"

# ── Embedding router (replaces chat-based route_intent) ─────────────────────
# Architecture: this is a CLASSIFIER, not a chat model. Uses BGE-M3 embeddings
# via the kernel's /api/v1/embed endpoint. Guild description embeddings are
# cached once and reused across calls. No autoregressive generation.

def _resolve_kernel_base():
    import os as _os
    if "KERNEL_BASE" in _os.environ:
        return _os.environ["KERNEL_BASE"]
    port_file = Path(__file__).resolve().parent.parent.parent / "data" / "active_port.json"
    try:
        data = json.loads(port_file.read_text())
        port = data.get("port", 4000)
        return f"http://127.0.0.1:{port}"
    except Exception:
        return "http://127.0.0.1:4000"

_KERNEL_URL = _resolve_kernel_base()
_guild_embed_cache = {}
_guild_embed_cache_built = False
_math = __import__("math")

def _cosine_sim(a, b):
    dot = sum(x * y for x, y in zip(a, b))
    na = _math.sqrt(sum(x * x for x in a))
    nb = _math.sqrt(sum(y * y for y in b))
    return dot / (na * nb + 1e-10)

def _embed_text(text):
    """Call kernel's /api/v1/embed, return 1024-dim vector."""
    import urllib.request as _urllib, json as _json
    data = _json.dumps({"text": text}).encode("utf-8")
    req = _urllib.Request(
        f"{_KERNEL_URL}/api/v1/embed",
        data=data,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with _urllib.urlopen(req, timeout=30) as resp:
            result = _json.loads(resp.read())
            if "embedding" in result:
                return result["embedding"]
            raise RuntimeError(f"embed API returned: {result}")
    except Exception as e:
        raise RuntimeError(f"embed API error: {e}")

def _warmup_embed():
    """Warmup: pre-compute guild description embeddings from catalog.
    Cache survives across calls within the guild process.
    """
    global _guild_embed_cache_built
    if _guild_embed_cache_built:
        return
    descriptions = {
        "bash": "Run shell commands and scripts",
        "filesystem": "List files, find files, show directory contents, file operations",
        "memory": "Store and retrieve long-term memories and knowledge",
        "search": "Semantic and keyword search across indexed content",
        "websearch": "Web search engine queries and result fetching",
        "browser": "Web browser automation with CDP protocol",
        "code": "Code modification and generation across languages",
        "database": "Database query and schema management",
        "pdf": "PDF document reading and text extraction",
        "vision": "Image analysis and OCR using vision models",
        "code_reviewer": "Code review and quality checks",
        "coloquio": "Multi-agent conversation channels",
        "mcp_bridge": "External MCP server integration bridge",
        "code_graph": "Code dependency graph and structure analysis",
        "comfy_ui": "Image generation via ComfyUI workflow",
        "n8n_bridge": "n8n workflow automation trigger and management",
        "scrapling": "Web scraping and content extraction from URLs",
        "data_tools": "JSON, YAML, CSV data manipulation tools",
        "formatter": "Code formatter: Ruff, Prettier, Rustfmt",
        "sequential_thinking": "Step-by-step reasoning and analysis",
        "night_reasoner": "Nightly reasoning and feedback analysis",
        "council": "Multi-voice decision making and tradeoff analysis",
        "ast_surgeon": "AST manipulation and code transformation",
        "audio_tools": "Audio file processing and conversion",
        "ffmpeg_tools": "FFmpeg multimedia processing tools",
        "screenshot_tools": "Screen capture and screenshot utilities",
        "clipboard_tools": "Clipboard read and write utilities",
        "local_llm_proxy": "Local LLM inference proxy and requests",
        "biome_warden": "Biome code quality linting and formatting",
        "audit": "Security audit and system integrity checks",
        "cron_scheduler": "Scheduled task and cron job management",
        "coordinator": "Orchestrate multi-step tasks and plan execution",
        "git": "Git version control operations",
        "docker": "Docker container management",
        "monitor": "System monitoring and process observation",
        "whats_new": "Unread messages and updates from channels",
        "coloquio_digest": "Coloquio channel digest and summary",
        "knowledge": "Knowledge graph and semantic network operations",
    }
    count = 0
    for guild, desc in descriptions.items():
        try:
            embed_text = f"{guild}: {desc}"
            _guild_embed_cache[guild] = _embed_text(embed_text)
            count += 1
        except Exception as e:
            sys.stderr.write(f"[night_reasoner] warmup embed failed for {guild}: {e}\n")
    sys.stderr.write(f"[night_reasoner] embedding cache: {count} guilds warmed up\n")
    _guild_embed_cache_built = True

@mcp.tool()
def route_intent(intent: str, candidates: str) -> str:
    """Route an ambiguous intent using BGE-M3 embedding similarity.

    Embeds the intent and compares cosine similarity against guild description
    embeddings (cached from kernel's BGE-M3). Returns the best-matching guild
    with similarity score. No autoregressive generation — classification only.

    Called by matcher.rs when top guild scores are too close for keyword
    disambiguation.

    Args:
        intent: The natural language intent to route.
        candidates: JSON list of {guild, score, description} for top candidates.
    """
    import json as _json
    try:
        cand_list = _json.loads(candidates)
    except Exception:
        cand_list = []

    if not cand_list:
        return _json.dumps({"guild": "unknown", "confidence": 0, "reasoning": "no candidates"})

    try:
        _warmup_embed()
        intent_emb = _embed_text(intent)
    except Exception as e:
        # Fallback: use top candidate from keyword matcher
        return _json.dumps({
            "guild": cand_list[0].get("guild", "unknown"),
            "confidence": 0,
            "reasoning": f"embedding unavailable: {e}",
            "note": "fallback to top candidate",
        })

    best_guild = cand_list[0].get("guild", "unknown")
    best_sim = -1
    similarities = {}
    for c in cand_list:
        g = c.get("guild", "")
        if g in _guild_embed_cache:
            sim = _cosine_sim(intent_emb, _guild_embed_cache[g])
            similarities[g] = round(sim, 4)
            if sim > best_sim:
                best_sim = sim
                best_guild = g

    return _json.dumps({
        "guild": best_guild,
        "confidence": round(best_sim, 4),
        "similarities": similarities,
        "reasoning": f"cosine similarity: {best_guild}={best_sim:.3f}",
    })

if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
