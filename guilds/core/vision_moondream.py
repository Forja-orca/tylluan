"""
TylluanNexus Moondream Guild — lightweight local vision (0.5B).

Uses the `moondream` pip package (vikhyatk/moondream):
    model = md.vl(model="moondream-2-latest")
    encoded = model.encode_image(PIL.Image)
    answer  = model.query(encoded, question)["answer"]
    caption = model.caption(encoded)["caption"]

Status: spike
"""

import sys
import os
import json
import asyncio

os.environ.setdefault("ORT_LOGGING_LEVEL", "3")

from mcp.server.fastmcp import FastMCP

mcp = FastMCP("tylluan-moondream")

_CACHE_DIR = r"E:\TylluanMCPo3\.fastembed_cache"

_model  = None
_loaded = False

def _load_model():
    global _model, _loaded
    if _loaded:
        return
    try:
        import moondream as md
        from huggingface_hub import snapshot_download
        model_path = snapshot_download(
            repo_id="vikhyatk/moondream-2-latest",
            cache_dir=_CACHE_DIR,
            local_files_only=False,
        )
        print(f"[moondream] model cached at {model_path}", file=sys.stderr)
        _model = md.vl(model="moondream-2-latest")
        _loaded = True
        print(f"[moondream] model loaded successfully", file=sys.stderr)
    except Exception as e:
        print(f"[moondream] FATAL: {e}", file=sys.stderr)
        raise

def _load_image(path: str):
    from PIL import Image
    if not os.path.isfile(path):
        raise FileNotFoundError(f"image not found: {path}")
    return Image.open(path).convert("RGB")

@mcp.tool()
async def analyze_image(image_path: str, prompt: str = "Describe this image in detail.") -> str:
    """Analyze an image using Moondream 0.5B vision model.
    Args:
        image_path: Absolute path to the local image file.
        prompt:     Question or instruction for the vision model.
    Returns:
        JSON with description, model, status.
    """
    _load_model()
    try:
        img = _load_image(image_path)
        encoded = _model.encode_image(img)
        answer  = _model.query(encoded, prompt)["answer"]
        return json.dumps({
            "description": answer,
            "model": "moondream-0.5b",
            "status": "ok",
        }, ensure_ascii=False)
    except Exception as e:
        return json.dumps({"error": str(e), "status": "error"})

@mcp.tool()
async def caption_image(image_path: str) -> str:
    """Generate a caption for an image using Moondream 0.5B.
    Args:
        image_path: Absolute path to the local image file.
    Returns:
        JSON with caption, model, status.
    """
    _load_model()
    try:
        img = _load_image(image_path)
        encoded = _model.encode_image(img)
        caption = _model.caption(encoded)["caption"]
        return json.dumps({
            "caption": caption,
            "model": "moondream-0.5b",
            "status": "ok",
        }, ensure_ascii=False)
    except Exception as e:
        return json.dumps({"error": str(e), "status": "error"})

if __name__ == "__main__":
    from guilds.core import utils
    utils.safe_mcp_run(mcp)
