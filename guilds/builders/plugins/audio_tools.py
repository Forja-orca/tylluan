import json
import logging
import os
from pathlib import Path

import httpx
from mcp.server.fastmcp import FastMCP

from guilds.core import utils

mcp = FastMCP("tylluan-audio-tools")

COMFYUI_URL = (os.environ.get("COMFYUI_URL") or "http://localhost:8188").rstrip("/")


@mcp.tool()
async def audio_transcribe(
    audio_path: str,
    language: str = "",
    model: str = "base",
) -> str:
    """Transcribe audio to text using Whisper via ComfyUI. Requires ComfyUI with Whisper node."""
    ap = Path(audio_path)
    if not ap.exists():
        return f"Error: audio file not found: {audio_path}"

    try:
        async with httpx.AsyncClient(timeout=5) as client:
            r = await client.get(f"{COMFYUI_URL}/system_stats")
            r.raise_for_status()
    except Exception as e:
        return f"ComfyUI not reachable at {COMFYUI_URL}: {e}"

    try:
        async with httpx.AsyncClient(timeout=30) as client:
            files = {"image": (ap.name, ap.read_bytes(), "audio/mpeg")}
            r = await client.post(f"{COMFYUI_URL}/upload/image", data={"subfolder": "audio"}, files=files)
            r.raise_for_status()
            upload = r.json()
        uploaded_name = upload.get("name", ap.name)
    except Exception as e:
        return f"Upload to ComfyUI failed: {e}"

    workflow = {
        "1": {"class_type": "LoadAudio", "inputs": {"audio": uploaded_name, "subfolder": "audio"}},
        "2": {"class_type": "WhisperTranscribe", "inputs": {"audio": ["1", 0], "model": f"whisper-{model}", "language": language or "auto"}},
    }
    try:
        async with httpx.AsyncClient(timeout=10) as client:
            r = await client.post(f"{COMFYUI_URL}/prompt", json={"prompt": workflow})
            r.raise_for_status()
            prompt_id = r.json()["prompt_id"]
    except Exception as e:
        return f"Workflow queue failed: {e}"

    deadline = 300.0
    polled = 0.0
    while polled < deadline:
        await asyncio.sleep(2)
        polled += 2
        try:
            async with httpx.AsyncClient() as client:
                r = await client.get(f"{COMFYUI_URL}/history/{prompt_id}")
                hist = r.json()
        except Exception:
            continue
        entry = hist.get(prompt_id)
        if not entry:
            continue
        status = entry.get("status", {})
        if status.get("completed"):
            outputs = entry.get("outputs", {})
            for node_out in outputs.values():
                text = node_out.get("text")
                if text:
                    return f"Transcription:\n{''.join(text) if isinstance(text, list) else text}"
            return f"Workflow completed but no text output. Raw: {json.dumps(outputs)[:500]}"
        if status.get("status_str") == "error":
            return "Whisper workflow failed in ComfyUI"

    return "Transcription timed out (5 min)."


if __name__ == "__main__":
    utils.safe_mcp_run(mcp)
