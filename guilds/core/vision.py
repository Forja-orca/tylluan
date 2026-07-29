"""
TylluanNexus Vision Guild — Sovereign Multimodal Analysis (SmolVLM2 ONNX Edition).

Preprocessing: PIL + numpy only. No torch, no torchvision. Python 3.14 compatible.
Tokenization:  AutoTokenizer (text-only, no torchvision dependency).
Inference:     3-part ONNX pipeline (vision_encoder → embed_tokens → decoder_model_merged).

Status: operational
"""

CAPABILITIES = {
    "process_execution": False,
    "filesystem_scope": ["/"],
    "network_hosts": [],
}

import sys
import os
import json
import asyncio

# Redirect stdout → stderr immediately so onnxruntime/numpy warnings can't
# corrupt the JSON-RPC framing that FastMCP writes on stdout.
os.environ.setdefault("ORT_LOGGING_LEVEL", "3")      # ERROR only
os.environ.setdefault("TF_CPP_MIN_LOG_LEVEL", "3")   # suppress TF noise if present

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tylluan-vision")

_MODEL_ID  = "HuggingFaceTB/SmolVLM2-256M-Instruct"

_vision_session  = None
_embed_session   = None
_decoder_session = None
_tokenizer       = None   # AutoTokenizer — no torchvision needed
_preproc_cfg     = None   # dict from preprocessor_config.json
_MODEL_LOADED    = False
_MODEL_AVAILABLE = False
_model_path      = None

_NUM_HIDDEN_LAYERS = 30
_NUM_KV_HEADS      = 3
_HEAD_DIM          = 64
_IMAGE_TOKEN_ID    = 49190
_EOS_TOKEN_ID      = 49279
_MAX_NEW_TOKENS    = 48
_MAX_NEW_TOKENS_OCR = 192
_MAX_NEW_TOKENS_EXTRACT = 128
_IMAGE_TOKEN_STR   = "<image>"

_PROVIDER          = "CPUExecutionProvider"   # detected at load time
_PROVIDER_LOADED   = False


def _detect_available_provider() -> tuple[str, list]:
    """Real ONNX Runtime provider detection — no hardcoding, no assumption.
    Returns (chosen_provider, all_available_providers). Cheap: does not load
    any model, only queries onnxruntime's compiled-in provider list.
    """
    try:
        import onnxruntime as ort
        available = list(ort.get_available_providers())
    except Exception as e:
        return "unavailable", [f"onnxruntime import failed: {e}"]

    if "DmlExecutionProvider" in available:
        return "DmlExecutionProvider", available
    if "CUDAExecutionProvider" in available:
        return "CUDAExecutionProvider", available
    return "CPUExecutionProvider", available


# ── Model loading ─────────────────────────────────────────────────────────────

def _resolve_model_dir() -> str:
    from huggingface_hub import snapshot_download
    # Uses the default HF hub cache (~/.cache/huggingface/hub) -- same
    # location every other model in this codebase resolves from (BGE-M3,
    # Qwen3.5-2B, Gemma-4-E2B, SmolLM2...). A separate ~/.tylluan/models_cache
    # was never populated, so this always fell through to the degraded
    # fallback even though the model was already downloaded and sitting in
    # the standard cache the whole time (found 2026-07-27).
    return snapshot_download(
        repo_id=_MODEL_ID,
        local_files_only=True,
        ignore_patterns=["*.bin", "*.pt", "*.safetensors",
                         "*bnb4*", "*fp16*", "*int8*", "*q4*", "*quantized*", "*uint8*"],
    )


def _load_preproc_config(model_path: str) -> dict:
    path = os.path.join(model_path, "preprocessor_config.json")
    defaults = {
        "image_mean": [0.5, 0.5, 0.5],
        "image_std":  [0.5, 0.5, 0.5],
        "rescale_factor": 1.0 / 255.0,
        "max_image_size": {"longest_edge": 512},
        "resample": 1,
    }
    if not os.path.exists(path):
        return defaults
    try:
        with open(path) as f:
            return {**defaults, **json.load(f)}
    except Exception:
        return defaults


def _load_vision_model():
    global _vision_session, _embed_session, _decoder_session
    global _tokenizer, _preproc_cfg, _MODEL_LOADED, _MODEL_AVAILABLE, _model_path
    global _PROVIDER, _PROVIDER_LOADED

    if _MODEL_LOADED:
        return
    _MODEL_LOADED = True

    try:
        _model_path  = _resolve_model_dir()
        _preproc_cfg = _load_preproc_config(_model_path)
        onnx_dir     = os.path.join(_model_path, "onnx")

        import onnxruntime as ort
        opts = ort.SessionOptions()
        opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        opts.intra_op_num_threads      = 4

        vision_path  = os.path.join(onnx_dir, "vision_encoder.onnx")
        embed_path   = os.path.join(onnx_dir, "embed_tokens.onnx")
        decoder_path = os.path.join(onnx_dir, "decoder_model_merged.onnx")

        for p in (vision_path, embed_path, decoder_path):
            if not os.path.exists(p):
                print(f"Vision guild: ONNX missing: {p}", file=sys.stderr, flush=True)
                return

        # Auto-detect DirectML if available (M25-A)
        if not _PROVIDER_LOADED:
            _PROVIDER, _ = _detect_available_provider()
            _PROVIDER_LOADED = True

        providers = [_PROVIDER]
        _vision_session  = ort.InferenceSession(vision_path,  opts, providers=providers)
        _embed_session   = ort.InferenceSession(embed_path,   opts, providers=providers)
        _decoder_session = ort.InferenceSession(decoder_path, opts, providers=providers)

        # AutoTokenizer has no torchvision dependency — safe on Python 3.14
        from transformers import AutoTokenizer
        _tokenizer = AutoTokenizer.from_pretrained(_model_path, local_files_only=True)

        _MODEL_AVAILABLE = True
        print(f"Vision guild: SmolVLM2 ONNX ready ({_PROVIDER}, PIL+numpy, no torch)", file=sys.stderr, flush=True)

    except Exception as e:
        print(f"Vision guild: load failed — {e}", file=sys.stderr, flush=True)
        _MODEL_AVAILABLE = False


# ── Image preprocessing (PIL + numpy, zero torch dependency) ──────────────────

def _preprocess_image(img, cfg: dict):
    """
    Resize + normalize a PIL image into ONNX-ready numpy arrays.

    Returns:
        pixel_values:         float32 [1, 3, H, W]
        pixel_attention_mask: bool    [1, H, W]
    """
    import numpy as np
    from PIL import Image

    longest = cfg.get("max_image_size", {}).get("longest_edge", 512)
    resample = cfg.get("resample", 1)  # 1 = PIL.Image.LANCZOS

    # Fit within longest_edge while preserving aspect ratio
    w, h    = img.size
    scale   = min(longest / max(w, h), 1.0)
    new_w   = max(1, round(w * scale))
    new_h   = max(1, round(h * scale))
    img     = img.resize((new_w, new_h), resample)

    # Pad to square (longest_edge × longest_edge) with black
    canvas  = Image.new("RGB", (longest, longest), (0, 0, 0))
    canvas.paste(img, (0, 0))

    arr  = np.array(canvas, dtype=np.float32)          # [H, W, 3]
    arr  = arr * cfg.get("rescale_factor", 1.0 / 255)  # → [0, 1]
    mean = np.array(cfg.get("image_mean", [0.5, 0.5, 0.5]), dtype=np.float32)
    std  = np.array(cfg.get("image_std",  [0.5, 0.5, 0.5]), dtype=np.float32)
    arr  = (arr - mean) / std                          # normalize

    pixel_values         = arr.transpose(2, 0, 1)[np.newaxis, np.newaxis]  # [1, 1, 3, H, W]
    pixel_attention_mask = np.ones((1, 1, longest, longest), dtype=np.bool_)  # [1, 1, H, W]
    return pixel_values, pixel_attention_mask


# ── ONNX inference ────────────────────────────────────────────────────────────

def _onnx_generate(
    pixel_values, pixel_attention_mask, input_ids, attention_mask,
    precomputed_features=None, max_tokens=None
) -> str:
    import numpy as np
    import time as _time
    import sys as _sys

    # Timing breakdown requested 2026-07-27 (Coloquio) to settle whether the
    # 28-57s latency is Python<->Rust transport overhead or real DirectML
    # decoder compute -- do not assume, measure.
    _t_encoder = 0.0
    _t_embed_initial = 0.0
    _t_decoder_total = 0.0
    _t_embed_loop_total = 0.0
    _n_decoder_calls = 0

    # Step 1: vision encoder (skip if pre-computed)
    if precomputed_features is not None:
        flat_features = precomputed_features
    else:
        _t0 = _time.perf_counter()
        vis_out       = _vision_session.run(
            ["image_features"],
            {"pixel_values": pixel_values.astype(np.float32),
             "pixel_attention_mask": pixel_attention_mask.astype(np.bool_)},
        )
        _t_encoder = _time.perf_counter() - _t0
        image_features = vis_out[0]
        flat_features  = image_features.reshape(-1, image_features.shape[-1])

    # Step 2: embed text tokens
    _t0 = _time.perf_counter()
    emb_out       = _embed_session.run(
        ["inputs_embeds"], {"input_ids": input_ids.astype(np.int64)}
    )
    _t_embed_initial = _time.perf_counter() - _t0
    inputs_embeds = emb_out[0]                                    # [1, seq, hidden]

    # Step 3: replace <image> positions with vision features
    image_positions = np.where(input_ids[0] == _IMAGE_TOKEN_ID)[0]
    n_replace       = min(len(image_positions), flat_features.shape[0])
    if n_replace > 0:
        inputs_embeds[0, image_positions[:n_replace], :] = flat_features[:n_replace]

    # Step 4: autoregressive decoder loop
    max_tokens = max_tokens if max_tokens is not None else _MAX_NEW_TOKENS
    seq_len = inputs_embeds.shape[1]
    kv_cache = {
        f"past_key_values.{i}.{kv}": np.zeros((1, _NUM_KV_HEADS, 0, _HEAD_DIM), dtype=np.float32)
        for i in range(_NUM_HIDDEN_LAYERS) for kv in ("key", "value")
    }
    position_ids  = np.arange(seq_len, dtype=np.int64).reshape(1, -1)
    attn_mask     = attention_mask.astype(np.int64)
    current_embs  = inputs_embeds
    generated_ids = []

    output_names = ["logits"] + [
        f"present.{i}.{kv}" for i in range(_NUM_HIDDEN_LAYERS) for kv in ("key", "value")
    ]

    for _ in range(max_tokens):
        _t0 = _time.perf_counter()
        outputs    = _decoder_session.run(
            output_names,
            {"inputs_embeds": current_embs.astype(np.float32),
             "attention_mask": attn_mask,
             "position_ids": position_ids,
             **kv_cache},
        )
        _t_decoder_total += _time.perf_counter() - _t0
        _n_decoder_calls += 1
        next_token = int(np.argmax(outputs[0][0, -1, :]))
        generated_ids.append(next_token)
        if next_token == _EOS_TOKEN_ID:
            break

        for i in range(_NUM_HIDDEN_LAYERS):
            kv_cache[f"past_key_values.{i}.key"]   = outputs[1 + i * 2]
            kv_cache[f"past_key_values.{i}.value"] = outputs[2 + i * 2]

        _t0 = _time.perf_counter()
        new_emb      = _embed_session.run(
            ["inputs_embeds"],
            {"input_ids": np.array([[next_token]], dtype=np.int64)}
        )[0]
        _t_embed_loop_total += _time.perf_counter() - _t0
        current_embs = new_emb
        past_len     = kv_cache["past_key_values.0.key"].shape[2]
        position_ids = np.array([[past_len]], dtype=np.int64)
        attn_mask    = np.ones((1, past_len + 1), dtype=np.int64)

    ttft = _t_encoder + _t_embed_initial + (_t_decoder_total / _n_decoder_calls if _n_decoder_calls else 0)
    tpot = (_t_decoder_total + _t_embed_loop_total) / _n_decoder_calls if _n_decoder_calls > 1 else 0.0
    total = _t_encoder + _t_embed_initial + _t_decoder_total + _t_embed_loop_total
    print(
        f"[vision timing] encoder={_t_encoder*1000:.1f}ms "
        f"embed_initial={_t_embed_initial*1000:.1f}ms "
        f"decoder_loop_total={_t_decoder_total*1000:.1f}ms ({_n_decoder_calls} tokens, "
        f"avg={_t_decoder_total/_n_decoder_calls*1000 if _n_decoder_calls else 0:.1f}ms/tok) "
        f"embed_loop_total={_t_embed_loop_total*1000:.1f}ms "
        f"TTFT={ttft*1000:.1f}ms TPOT={tpot*1000:.1f}ms "
        f"TOTAL_ONNX={total*1000:.1f}ms",
        file=_sys.stderr,
    )

    return _tokenizer.decode(generated_ids, skip_special_tokens=True).strip()


# ── Helpers ───────────────────────────────────────────────────────────────────

def _degraded_response() -> str:
    return json.dumps({
        "description": "[vision: model unavailable — check kernel logs for details]",
        "ocr": "",
        "node_id": None,
        "status": "degraded",
        "triples_extracted": 0,
    }, ensure_ascii=False)


def _build_prompt_text(prompt: str, n_image_tokens: int) -> str:
    """
    Build the chat prompt string with exactly n_image_tokens <image> placeholders.
    Matches SmolVLM2 chat template: <|im_start|>User:<image>...<end_of_utterance>\\nAssistant:
    """
    image_str = _IMAGE_TOKEN_STR * n_image_tokens
    return f"<|im_start|>User:{image_str}\n{prompt}<end_of_utterance>\nAssistant:"


# ── Shared inference helper ───────────────────────────────────────────────────

async def _run_vision(image_path: str, prompt: str, max_tokens: int = None) -> str:
    """Run vision inference and write to SilvaDB. Shared by all three tools."""
    _load_vision_model()
    if not _MODEL_AVAILABLE:
        return _degraded_response()
    if not os.path.exists(image_path):
        return json.dumps({"description": "[vision: file not found]", "error": f"File not found: {image_path}", "status": "error"}, ensure_ascii=False)

    try:
        import numpy as np
        from PIL import Image

        def do_inference():
            img = Image.open(image_path).convert("RGB")
            pixel_values, pixel_attention_mask = _preprocess_image(img, _preproc_cfg)

            vis_out = _vision_session.run(
                ["image_features"],
                {"pixel_values": pixel_values.astype(np.float32),
                 "pixel_attention_mask": pixel_attention_mask.astype(np.bool_)},
            )
            flat_features = vis_out[0].reshape(-1, vis_out[0].shape[-1])
            n_img_tokens  = flat_features.shape[0]

            text = _build_prompt_text(prompt, n_img_tokens)
            enc  = _tokenizer(text, return_tensors="np", add_special_tokens=False)

            return _onnx_generate(
                pixel_values, pixel_attention_mask,
                enc["input_ids"], enc["attention_mask"],
                precomputed_features=flat_features, max_tokens=max_tokens,
            )

        result = await asyncio.to_thread(do_inference)

        node_id = None
        try:
            from guilds.core import silva_utils
            node_id = silva_utils.add_node(
                content=f"[vision:{os.path.basename(image_path)}] {result}",
                node_type="image",
                metadata={"source": image_path, "prompt": prompt, "guild": "vision"},
            )
            if node_id:
                silva_utils.write_edge(
                    source=os.path.basename(image_path),
                    predicate="described_by",
                    target=node_id,
                    metadata={"confidence": 1.0, "source": "vision_guild"},
                )
        except Exception as mem_err:
            print(f"vision: SilvaDB write failed (non-fatal): {mem_err}", file=sys.stderr)

        return json.dumps({"description": result, "node_id": node_id, "provider": _PROVIDER, "status": "ok"}, ensure_ascii=False)

    except Exception as e:
        return json.dumps({"description": "[vision: inference error]", "error": str(e), "status": "error"}, ensure_ascii=False)


# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
async def vision_device_status() -> str:
    """Report the REAL ONNX execution provider state for this guild — no
    hardcoding, no assumption of CPU. Cheap: does not load the model.
    Returns JSON with:
        active_provider:    provider actually in use if the model has been
                             loaded already, else the provider that WOULD be
                             chosen (same detection logic, run fresh).
        available_providers: full list onnxruntime reports as compiled in.
        model_loaded:        whether inference sessions are already warm.
    """
    if _MODEL_LOADED and _PROVIDER_LOADED:
        active, available = _PROVIDER, None
        _, available = _detect_available_provider()
    else:
        active, available = _detect_available_provider()
    return json.dumps({
        "guild": "vision",
        "active_provider": active,
        "available_providers": available,
        "model_loaded": bool(_MODEL_LOADED and _vision_session is not None),
        "status": "ok",
    }, ensure_ascii=False)


@mcp.tool()
async def vision_analyze(
    image_path: str,
    prompt: str = "Describe this image in detail.",
) -> str:
    """Analyze an image using SmolVLM2-256M ONNX (PIL+numpy, no torch).
    Args:
        image_path: Absolute path to the local image file.
        prompt:     Question or instruction for the vision model.
    Returns:
        JSON with description, node_id, provider, status.
    """
    return await _run_vision(image_path, prompt)


@mcp.tool()
async def vision_extract(image_path: str, schema: str = "JSON object with key-value pairs") -> str:
    """Extract structured data from an image as JSON. Extended token budget."""
    return await _run_vision(
        image_path,
        f"Analyze the image and return a structured response in {schema}. Output ONLY JSON.",
        max_tokens=_MAX_NEW_TOKENS_EXTRACT,
    )


@mcp.tool()
async def vision_ocr(image_path: str) -> str:
    """Extract all text from an image (OCR). Extended token budget for full text."""
    return await _run_vision(
        image_path,
        "Perform OCR on this image. Extract all text accurately.",
        max_tokens=_MAX_NEW_TOKENS_OCR,
    )


if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
