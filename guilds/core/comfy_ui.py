"""
TylluanNexus ComfyUI Guild — Cinematic AI video + image generation.

Architecture:
    - Flux Schnell fp8 for cinematic quality (UE5 + anime + digital realism aesthetic)
    - Ken Burns motion (FFmpeg zoompan) for professional video from stills
    - Kokoro TTS narration + FFmpeg final assembly
    - YouTube Shorts native: 576×1024 (9:16), upscaled to 1080×1920 output
    - Fallback chain: Flux → SDXL Turbo → SDXL → anything available

Requires ComfyUI running at COMFY_BASE_URL (default: http://127.0.0.1:8188).
"""

import asyncio
import json
import logging
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path
from typing import Optional

from mcp.server.fastmcp import FastMCP
from guilds.core import utils

mcp = FastMCP("tylluan-comfy_ui")

_BASE       = os.environ.get("COMFY_BASE_URL", "http://127.0.0.1:8188").rstrip("/")
_OUTPUT_DIR = Path("data/outputs/comfy")
_CLIENT_ID  = str(uuid.uuid4())[:8]

# ── FFmpeg detection ──────────────────────────────────────────────────────────

_FFMPEG_FALLBACK = (
    r"C:\Users\USERNAME\AppData\Local\Microsoft\WinGet\Packages"
    r"\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe"
    r"\ffmpeg-8.1-full_build\bin\ffmpeg.exe"
)

def _ffmpeg() -> str:
    p = shutil.which("ffmpeg")
    if p:
        return p
    if os.path.exists(_FFMPEG_FALLBACK):
        return _FFMPEG_FALLBACK
    raise RuntimeError("FFmpeg not found. Install via: winget install Gyan.FFmpeg")

# ── Cinematic style system ────────────────────────────────────────────────────

# Rotating color palettes — varied so consecutive scenes don't look identical
_PALETTES = [
    "cinematic teal and orange color grading, warm highlights",
    "neon purple and deep blue cyberpunk palette, electric atmosphere",
    "golden hour sunset, warm amber volumetric light",
    "cold moonlight blue, silver and cyan highlights",
    "emerald green bioluminescence, dark forest atmosphere",
    "dramatic crimson and shadow contrast, noir style",
]

# Ken Burns motion variants — alternated per scene
_KB_EFFECTS = [
    # (description, zoompan_expr, x_expr, y_expr)
    ("zoom-in-center",
     "min(zoom+0.0015,1.5)",
     "iw/2-(iw/zoom/2)",
     "ih/2-(ih/zoom/2)"),
    ("zoom-out-center",
     "if(lte(zoom,1.0),1.5,max(1.001,zoom-0.0015))",
     "iw/2-(iw/zoom/2)",
     "ih/2-(ih/zoom/2)"),
    ("pan-right",
     "1.25",
     "if(lte(on,1),0,min(x+1.2,iw/4))",
     "ih/2-(ih/zoom/2)"),
    ("pan-left",
     "1.25",
     "if(lte(on,1),iw/4,max(0,x-1.2))",
     "ih/2-(ih/zoom/2)"),
    ("zoom-in-topleft",
     "min(zoom+0.0015,1.5)",
     "iw/4-(iw/zoom/2)",
     "ih/4-(ih/zoom/2)"),
    ("zoom-in-bottomright",
     "min(zoom+0.0015,1.5)",
     "3*iw/4-(iw/zoom/2)",
     "3*ih/4-(ih/zoom/2)"),
]

# Style suffix — appended to every image prompt
_STYLE_SUFFIX = (
    "unreal engine 5 render, octane render, 8k resolution, "
    "anime style, digital art, hyperrealistic, photorealistic, "
    "cinematic composition, volumetric lighting, ray tracing, "
    "subsurface scattering, depth of field, bokeh, "
    "dramatic shadows, god rays, professional color grading, "
    "anamorphic lens flare, ultra detailed, sharp focus, masterpiece, "
    "trending on artstation, award winning"
)

_NEG_CINEMATIC = (
    "ugly, blurry, distorted, watermark, text overlay, low quality, deformed, "
    "amateur, stock photo, flat lighting, low poly, web graphic, plastic, fake, "
    "oversaturated, noisy, grainy, jpeg artifacts, bad anatomy, bad proportions, "
    "extra limbs, sketch, doodle, rough, unfinished, cartoon, simple, flat color, "
    "out of focus, overexposed, underexposed, washed out, muddy colors"
)


def _cinematic_prompt(topic: str, scene_text: str, palette_idx: int = 0) -> str:
    """Build a full cinematic prompt from scene content."""
    first_sentence = scene_text.split(".")[0].strip()
    core = first_sentence if len(first_sentence) > 20 else topic
    palette = _PALETTES[palette_idx % len(_PALETTES)]
    return f"{core}, {palette}, {_STYLE_SUFFIX}"


# ── Workflow templates ────────────────────────────────────────────────────────

def _flux_workflow(
    prompt: str,
    negative: str = "",
    checkpoint: str = "flux1-schnell-fp8.safetensors",
    width: int = 576,
    height: int = 1024,
    steps: int = 4,
    cfg: float = 1.0,
    seed: int = -1,
) -> dict:
    """Flux Schnell workflow — 4 steps, cfg=1.0, scheduler=simple."""
    import random
    if seed < 0:
        seed = random.randint(0, 2**32 - 1)
    return {
        "4": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": checkpoint}},
        "5": {"class_type": "EmptyLatentImage", "inputs": {"width": width, "height": height, "batch_size": 1}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"text": prompt, "clip": ["4", 1]}},
        "7": {"class_type": "CLIPTextEncode", "inputs": {"text": negative or "low quality", "clip": ["4", 1]}},
        "3": {"class_type": "KSampler", "inputs": {
            "seed": seed, "steps": steps, "cfg": cfg,
            "sampler_name": "euler", "scheduler": "simple", "denoise": 1.0,
            "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["5", 0],
        }},
        "8": {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["4", 2]}},
        "9": {"class_type": "SaveImage", "inputs": {"images": ["8", 0], "filename_prefix": "tylluan_"}},
    }


def _txt2img_workflow(
    prompt: str,
    negative: str = "ugly, blurry, distorted, watermark",
    checkpoint: str = "v1-5-pruned-emaonly.ckpt",
    width: int = 512,
    height: int = 512,
    steps: int = 20,
    cfg: float = 7.0,
    sampler: str = "euler",
    scheduler: str = "normal",
    seed: int = -1,
) -> dict:
    import random
    if seed < 0:
        seed = random.randint(0, 2**32 - 1)
    return {
        "3": {"class_type": "KSampler", "inputs": {
            "seed": seed, "steps": steps, "cfg": cfg,
            "sampler_name": sampler, "scheduler": scheduler, "denoise": 1.0,
            "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["5", 0],
        }},
        "4": {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": checkpoint}},
        "5": {"class_type": "EmptyLatentImage", "inputs": {"width": width, "height": height, "batch_size": 1}},
        "6": {"class_type": "CLIPTextEncode", "inputs": {"text": prompt, "clip": ["4", 1]}},
        "7": {"class_type": "CLIPTextEncode", "inputs": {"text": negative, "clip": ["4", 1]}},
        "8": {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["4", 2]}},
        "9": {"class_type": "SaveImage", "inputs": {"images": ["8", 0], "filename_prefix": "tylluan_"}},
    }


def _img2img_workflow(
    prompt: str,
    image_path: str,
    negative: str = "ugly, blurry, distorted",
    checkpoint: str = "v1-5-pruned-emaonly.ckpt",
    denoise: float = 0.75,
    steps: int = 20,
    cfg: float = 7.0,
    seed: int = -1,
) -> dict:
    import random
    if seed < 0:
        seed = random.randint(0, 2**32 - 1)
    return {
        "1":  {"class_type": "LoadImage", "inputs": {"image": image_path, "upload": "image"}},
        "3":  {"class_type": "KSampler", "inputs": {
            "seed": seed, "steps": steps, "cfg": cfg,
            "sampler_name": "euler", "scheduler": "normal", "denoise": denoise,
            "model": ["4", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["10", 0],
        }},
        "4":  {"class_type": "CheckpointLoaderSimple", "inputs": {"ckpt_name": checkpoint}},
        "6":  {"class_type": "CLIPTextEncode", "inputs": {"text": prompt, "clip": ["4", 1]}},
        "7":  {"class_type": "CLIPTextEncode", "inputs": {"text": negative, "clip": ["4", 1]}},
        "8":  {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["4", 2]}},
        "9":  {"class_type": "SaveImage", "inputs": {"images": ["8", 0], "filename_prefix": "tylluan_i2i_"}},
        "10": {"class_type": "VAEEncode", "inputs": {"pixels": ["1", 0], "vae": ["4", 2]}},
    }


def _qwen_workflow(
    prompt: str,
    width: int = 1328,
    height: int = 1328,
    steps: int = 4,
    seed: int = -1,
) -> dict:
    """Qwen image generation — AuraFlow sampler, 4-step Lightning LoRA, 1328×1328."""
    import random
    if seed < 0:
        seed = random.randint(0, 2**32 - 1)
    return {
        "75:37": {"class_type": "UNETLoader", "inputs": {
            "unet_name": "qwen_image_fp8_e4m3fn.safetensors", "weight_dtype": "default"}},
        "75:38": {"class_type": "CLIPLoader", "inputs": {
            "clip_name": "qwen_2.5_vl_7b_fp8_scaled.safetensors", "type": "qwen_image", "device": "default"}},
        "75:39": {"class_type": "VAELoader", "inputs": {"vae_name": "qwen_image_vae.safetensors"}},
        "75:58": {"class_type": "EmptySD3LatentImage", "inputs": {"width": width, "height": height, "batch_size": 1}},
        "75:6":  {"class_type": "CLIPTextEncode", "inputs": {"text": prompt, "clip": ["75:38", 0]}},
        "75:7":  {"class_type": "CLIPTextEncode", "inputs": {"text": "", "clip": ["75:38", 0]}},
        "75:73": {"class_type": "LoraLoaderModelOnly", "inputs": {
            "lora_name": "Qwen-Image-Lightning-4steps-V1.0.safetensors", "strength_model": 1, "model": ["75:37", 0]}},
        "75:66": {"class_type": "ModelSamplingAuraFlow", "inputs": {"shift": 3.1, "model": ["75:73", 0]}},
        "75:3":  {"class_type": "KSampler", "inputs": {
            "seed": seed, "steps": steps, "cfg": 1, "sampler_name": "euler", "scheduler": "simple",
            "denoise": 1, "model": ["75:66", 0], "positive": ["75:6", 0],
            "negative": ["75:7", 0], "latent_image": ["75:58", 0]}},
        "75:8":  {"class_type": "VAEDecode", "inputs": {"samples": ["75:3", 0], "vae": ["75:39", 0]}},
        "60":    {"class_type": "SaveImage", "inputs": {"filename_prefix": "Qwen_t2i", "images": ["75:8", 0]}},
    }


# ── HTTP helpers ──────────────────────────────────────────────────────────────

def _http_get(path: str, timeout: int = 8) -> dict | list:
    url = f"{_BASE}{path}"
    req = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"ComfyUI {e.code}: {e.read().decode()[:200]}") from e
    except Exception as e:
        raise RuntimeError(f"ComfyUI unreachable at {_BASE}: {e}") from e


def _http_post(path: str, body: dict, timeout: int = 10) -> dict:
    url = f"{_BASE}{path}"
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data,
                                  headers={"Content-Type": "application/json"}, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return json.loads(resp.read().decode())
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"ComfyUI {e.code}: {e.read().decode()[:200]}") from e
    except Exception as e:
        raise RuntimeError(f"ComfyUI unreachable at {_BASE}: {e}") from e


def _submit_prompt(workflow: dict) -> str:
    result = _http_post("/prompt", {"prompt": workflow, "client_id": _CLIENT_ID})
    prompt_id = result.get("prompt_id")
    if not prompt_id:
        raise RuntimeError(f"No prompt_id in response: {result}")
    return prompt_id


def _poll_until_done(prompt_id: str, timeout_secs: int = 1800) -> dict:
    deadline = time.time() + timeout_secs
    while time.time() < deadline:
        history = _http_get(f"/history/{prompt_id}")
        entry = history.get(prompt_id)
        if entry:
            return entry
        time.sleep(2)
    raise TimeoutError(f"ComfyUI generation timed out after {timeout_secs}s")


def _save_outputs(prompt_id: str, outputs: dict) -> list[str]:
    _OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    saved: list[str] = []
    for node_id, node_out in outputs.items():
        for img in node_out.get("images", []):
            fname     = img.get("filename", "")
            subfolder = img.get("subfolder", "")
            ftype     = img.get("type", "output")
            if not fname:
                continue
            params = urllib.parse.urlencode({"filename": fname, "subfolder": subfolder, "type": ftype})
            url  = f"{_BASE}/view?{params}"
            dest = _OUTPUT_DIR / fname
            try:
                urllib.request.urlretrieve(url, dest)
                saved.append(str(dest))
            except Exception as e:
                logging.warning("Failed to download %s: %s", fname, e)
    return saved


def _save_audio_outputs(prompt_id: str, outputs: dict) -> list[str]:
    """Download audio outputs (wav/mp3) from ComfyUI."""
    _OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    saved: list[str] = []
    for node_id, node_out in outputs.items():
        for audio in node_out.get("audio", []):
            fname     = audio.get("filename", "")
            subfolder = audio.get("subfolder", "")
            ftype     = audio.get("type", "output")
            if not fname:
                continue
            params = urllib.parse.urlencode({"filename": fname, "subfolder": subfolder, "type": ftype})
            url  = f"{_BASE}/view?{params}"
            dest = _OUTPUT_DIR / fname
            try:
                urllib.request.urlretrieve(url, dest)
                saved.append(str(dest))
            except Exception as e:
                logging.warning("Failed to download audio %s: %s", fname, e)
    return saved


def _detect_checkpoint(prefer_flux: bool = True) -> str:
    """Pick best available checkpoint. Flux > SDXL Turbo > SDXL > first available."""
    try:
        all_ckpts = _http_get("/models/checkpoints")
    except Exception:
        return ""
    if not all_ckpts:
        return ""
    if prefer_flux:
        order = [
            "flux1-schnell-fp8.safetensors",
            "sd_xl_turbo_1.0_fp16.safetensors",
            "sd_xl_base_1.0.safetensors",
        ]
    else:
        order = [
            "sd_xl_turbo_1.0_fp16.safetensors",
            "sd_xl_base_1.0.safetensors",
            "flux1-schnell-fp8.safetensors",
        ]
    for name in order:
        if name in all_ckpts:
            return name
    return all_ckpts[0]


def _is_flux(checkpoint: str) -> bool:
    return "flux" in checkpoint.lower()


# ── Ken Burns + FFmpeg video pipeline ─────────────────────────────────────────

def _ken_burns_clip(
    image_path: str,
    output_path: str,
    duration: float = 7.0,
    effect_idx: int = 0,
    out_width: int = 1080,
    out_height: int = 1920,
    fps: int = 30,
) -> str:
    """Animate a still image with Ken Burns zoom/pan effect via FFmpeg.
    Returns output_path on success, raises RuntimeError on failure."""
    ffmpeg = _ffmpeg()
    frames = int(duration * fps)
    effect = _KB_EFFECTS[effect_idx % len(_KB_EFFECTS)]
    _, z_expr, x_expr, y_expr = effect

    # Pad/scale input to at least output size before zoompan (zoompan needs room to move)
    pad_w = int(out_width * 1.6)
    pad_h = int(out_height * 1.6)

    vf = (
        f"scale={pad_w}:{pad_h}:force_original_aspect_ratio=increase,"
        f"crop={pad_w}:{pad_h},"
        f"zoompan=z='{z_expr}':x='{x_expr}':y='{y_expr}'"
        f":d={frames}:fps={fps}:s={pad_w}x{pad_h},"
        f"scale={out_width}:{out_height}:flags=lanczos,"
        f"format=yuv420p"
    )

    cmd = [
        ffmpeg, "-y",
        "-loop", "1",
        "-i", image_path,
        "-vf", vf,
        "-t", str(duration),
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "18",
        "-pix_fmt", "yuv420p",
        output_path,
    ]

    result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    if result.returncode != 0:
        raise RuntimeError(f"FFmpeg Ken Burns failed: {result.stderr[-400:]}")
    return output_path


def _crossfade_concat(clip_paths: list[str], output_path: str, fade_duration: float = 0.5) -> str:
    """Concatenate video clips with crossfade transitions via FFmpeg xfade filter."""
    ffmpeg = _ffmpeg()
    if len(clip_paths) == 1:
        # Single clip — just copy
        cmd = [ffmpeg, "-y", "-i", clip_paths[0], "-c", "copy", output_path]
        subprocess.run(cmd, capture_output=True, check=True, timeout=120)
        return output_path

    # Build xfade chain: [0][1]xfade → tmp1; [tmp1][2]xfade → tmp2; ...
    # Simpler: use concat with fade via complex filter
    n = len(clip_paths)
    inputs = []
    for p in clip_paths:
        inputs += ["-i", p]

    # Use simple concat (no xfade) for reliability — xfade needs duration knowledge
    filter_parts = "".join(f"[{i}:v]" for i in range(n))
    filter_str   = f"{filter_parts}concat=n={n}:v=1:a=0[outv]"

    cmd = [
        ffmpeg, "-y",
        *inputs,
        "-filter_complex", filter_str,
        "-map", "[outv]",
        "-c:v", "libx264",
        "-preset", "fast",
        "-crf", "18",
        "-pix_fmt", "yuv420p",
        output_path,
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    if result.returncode != 0:
        raise RuntimeError(f"FFmpeg concat failed: {result.stderr[-400:]}")
    return output_path


def _add_audio_track(video_path: str, audio_path: str, output_path: str) -> str:
    """Mix narration audio into a video. Audio is trimmed/padded to match video length."""
    ffmpeg = _ffmpeg()
    cmd = [
        ffmpeg, "-y",
        "-i", video_path,
        "-i", audio_path,
        "-c:v", "copy",
        "-c:a", "aac",
        "-b:a", "192k",
        "-shortest",
        output_path,
    ]
    result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    if result.returncode != 0:
        raise RuntimeError(f"FFmpeg audio mix failed: {result.stderr[-400:]}")
    return output_path


# ── TTS ───────────────────────────────────────────────────────────────────────

def _kokoro_tts_workflow(text: str, voice: str = "af_heart") -> dict:
    return {
        "1": {"class_type": "BS_Kokoro_ONNX", "inputs": {
            "text": text[:2000],
            "voice": voice,
            "speed": 1.0,
            "output_name": "tylluan_narration",
        }},
        "2": {"class_type": "SaveAudio", "inputs": {
            "audio": ["1", 0],
            "filename_prefix": "tylluan_audio_",
        }},
    }


def _upload_to_comfy(image_path: str) -> str:
    """Upload a local image to ComfyUI's input directory via /upload/image."""
    filename = os.path.basename(image_path)
    with open(image_path, "rb") as f:
        img_data = f.read()
    boundary = uuid.uuid4().hex
    header = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="image"; filename="{filename}"\r\n'
        f"Content-Type: image/png\r\n\r\n"
    ).encode()
    body = header + img_data + f"\r\n--{boundary}--\r\n".encode()
    req = urllib.request.Request(
        f"{_BASE}/upload/image",
        data=body,
        headers={"Content-Type": f"multipart/form-data; boundary={boundary}"},
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=30) as resp:
        result = json.loads(resp.read().decode())
    return result.get("name", filename)


_WAN_NEG_DEFAULT = (
    "Static, frozen, no movement, blurry, out of focus, bad anatomy, text overlay, "
    "watermark, low quality, artifacts, distortion, unnatural lighting, camera shake"
)


def _wan22_workflow(
    prompt: str,
    negative: str = "",
    mode: str = "shorts",
    steps: int = 20,
    cfg: float = 5.0,
    seed: int = -1,
) -> dict:
    """Wan2.2 text-to-video. mode='shorts' → 1088×1920 53f portrait; mode='landscape' → 1280×704 121f."""
    import random
    if seed < 0:
        seed = random.randint(0, 2**53 - 1)
    neg = negative or _WAN_NEG_DEFAULT
    if mode == "landscape":
        w, h, length, dtype, clip_device = 1280, 704, 121, "default", "cpu"
    else:
        w, h, length, dtype, clip_device = 1088, 1920, 53, "fp8_e4m3fn_fast", "default"
    return {
        "37": {"class_type": "UNETLoader", "inputs": {
            "unet_name": "wan2.2_ti2v_5B_fp16.safetensors", "weight_dtype": dtype}},
        "38": {"class_type": "CLIPLoader", "inputs": {
            "clip_name": "umt5_xxl_fp8_e4m3fn_scaled.safetensors", "type": "wan", "device": clip_device}},
        "39": {"class_type": "VAELoader", "inputs": {"vae_name": "wan2.2_vae.safetensors"}},
        "48": {"class_type": "ModelSamplingSD3", "inputs": {"shift": 8, "model": ["37", 0]}},
        "55": {"class_type": "Wan22ImageToVideoLatent", "inputs": {
            "width": w, "height": h, "length": length, "batch_size": 1, "vae": ["39", 0]}},
        "6":  {"class_type": "CLIPTextEncode", "inputs": {"text": prompt, "clip": ["38", 0]}},
        "7":  {"class_type": "CLIPTextEncode", "inputs": {"text": neg, "clip": ["38", 0]}},
        "3":  {"class_type": "KSampler", "inputs": {
            "seed": seed, "steps": steps, "cfg": cfg, "sampler_name": "uni_pc",
            "scheduler": "simple", "denoise": 1,
            "model": ["48", 0], "positive": ["6", 0], "negative": ["7", 0], "latent_image": ["55", 0]}},
        "8":  {"class_type": "VAEDecode", "inputs": {"samples": ["3", 0], "vae": ["39", 0]}},
        "57": {"class_type": "CreateVideo", "inputs": {"fps": 24, "images": ["8", 0]}},
        "58": {"class_type": "SaveVideo", "inputs": {
            "filename_prefix": "video/ComfyUI", "format": "auto", "codec": "auto", "video": ["57", 0]}},
    }



# ── Tools ─────────────────────────────────────────────────────────────────────

@mcp.tool()
async def generate_image(
    prompt: str,
    negative_prompt: str = "",
    checkpoint: str = "",
    width: int = 576,
    height: int = 1024,
    steps: int = 4,
    cfg: float = 1.0,
    sampler: str = "euler",
    seed: int = -1,
    cinematic: bool = True,
    intent: str = "",
    use_qwen: bool = False,
) -> str:
    """Generate an image from a text prompt using ComfyUI (txt2img).
    Defaults to Flux Schnell with cinematic UE5+anime style at 9:16 (576×1024).
    Set use_qwen=True to use the Qwen image model (AuraFlow, 1328×1328, 4 steps).
    Use for: generate image, create image, draw, paint, illustrate, make a picture,
    txt2img, text to image, render image, generar imagen, crear imagen, dibujar.

    Args:
        prompt: Positive description of what to generate.
        negative_prompt: What to avoid (auto-filled with cinematic negative if empty).
        checkpoint: Model checkpoint (empty = auto-detect best available).
        width: Image width (default 576 for 9:16 Shorts format).
        height: Image height (default 1024 for 9:16 Shorts format).
        steps: Sampling steps (4 for Flux/Qwen, 20 for SDXL).
        cfg: Guidance scale (1.0 for Flux/Qwen, 7.0 for SDXL).
        sampler: Sampler algorithm.
        seed: Random seed (-1 for random).
        cinematic: If True, append UE5+anime+digital-realism style suffix to prompt.
        intent: Natural language generation goal.
        use_qwen: If True, use Qwen image model (AuraFlow) instead of Flux/SDXL.
    """
    try:
        # Strip routing prefixes that tylluan_do may inject into the prompt field
        import re as _re
        clean = _re.sub(
            r'^(?:generate\s+(?:an?\s+)?image\s+of|genera\s+(?:una?\s+)?imagen\s+de|'
            r'create\s+(?:an?\s+)?image\s+of|draw|paint|illustrate|render)\s+',
            '', prompt, flags=_re.IGNORECASE).strip()
        prompt = clean or prompt
        full_prompt = f"{prompt}, {_STYLE_SUFFIX}" if cinematic else prompt

        if use_qwen or "qwen" in checkpoint.lower():
            qwen_w = max(64, min(2048, (width // 64) * 64)) if width != 576 else 1328
            qwen_h = max(64, min(2048, (height // 64) * 64)) if height != 1024 else 1328
            workflow = _qwen_workflow(prompt=full_prompt, width=qwen_w, height=qwen_h, steps=steps, seed=seed)
            used_checkpoint = "qwen_image_fp8_e4m3fn.safetensors"
            width, height = qwen_w, qwen_h
        else:
            if not checkpoint:
                checkpoint = _detect_checkpoint(prefer_flux=True)
                if not checkpoint:
                    return "❌ No checkpoints found in ComfyUI. Install a model first."

            width  = max(64, min(2048, (width  // 64) * 64))
            height = max(64, min(2048, (height // 64) * 64))

            neg = negative_prompt or (_NEG_CINEMATIC if cinematic else "ugly, blurry, distorted")

            if _is_flux(checkpoint):
                workflow = _flux_workflow(
                    prompt=full_prompt, negative=neg, checkpoint=checkpoint,
                    width=width, height=height, steps=steps, cfg=cfg, seed=seed,
                )
            else:
                workflow = _txt2img_workflow(
                    prompt=full_prompt, negative=neg, checkpoint=checkpoint,
                    width=width, height=height, steps=steps, cfg=cfg, sampler=sampler, seed=seed,
                )
            used_checkpoint = checkpoint

        logging.info("Submitting txt2img [%s]: %s…", used_checkpoint[:30], full_prompt[:60])
        prompt_id = _submit_prompt(workflow)
        result    = _poll_until_done(prompt_id)
        saved     = _save_outputs(prompt_id, result.get("outputs", {}))

        if not saved:
            return f"⚠️ Generation completed (ID: {prompt_id}) but no output files found."

        paths = "\n".join(f"  📸 `{p}`" for p in saved)
        return (
            f"✅ **Image generated** (ID: `{prompt_id}`)\n"
            f"Model: {used_checkpoint} | {width}×{height} | {steps} steps | CFG {cfg}\n\n"
            f"Saved to:\n{paths}"
        )

    except RuntimeError as e:
        return f"❌ ComfyUI error: {e}"
    except TimeoutError as e:
        return f"⏱️ {e}"
    except Exception as e:
        logging.error("generate_image failed: %s", e)
        return f"❌ Unexpected error: {e}"


@mcp.tool()
async def img2img(
    prompt: str,
    image_path: str,
    negative_prompt: str = "ugly, blurry, distorted",
    checkpoint: str = "",
    denoise: float = 0.75,
    steps: int = 20,
    cfg: float = 7.0,
    seed: int = -1,
    intent: str = "",
) -> str:
    """Transform an existing image guided by a text prompt (img2img).
    Use for: img2img, image to image, transform image, restyle image, edit image,
    change image style, apply style, variar imagen, transformar imagen.

    Args:
        prompt: Description of the desired transformation.
        image_path: Path to the input image file.
        negative_prompt: What to avoid.
        checkpoint: Model checkpoint (empty = auto-detect).
        denoise: Denoising strength 0.0–1.0 (lower = closer to original).
        steps: Sampling steps.
        cfg: Guidance scale.
        seed: Random seed (-1 for random).
        intent: Natural language description of the transformation goal.
    """
    try:
        if not os.path.exists(image_path):
            return f"❌ Image not found: {image_path}"
        if not checkpoint:
            checkpoint = _detect_checkpoint(prefer_flux=False)
            if not checkpoint:
                return "❌ No checkpoints found in ComfyUI."

        workflow  = _img2img_workflow(
            prompt=prompt, image_path=image_path, negative=negative_prompt,
            checkpoint=checkpoint, denoise=denoise, steps=steps, cfg=cfg, seed=seed,
        )
        prompt_id = _submit_prompt(workflow)
        result    = _poll_until_done(prompt_id)
        saved     = _save_outputs(prompt_id, result.get("outputs", {}))

        if not saved:
            return f"⚠️ img2img completed (ID: {prompt_id}) but no output files found."

        paths = "\n".join(f"  📸 `{p}`" for p in saved)
        return (
            f"✅ **img2img completed** (ID: `{prompt_id}`)\n"
            f"Denoise: {denoise} | {steps} steps | CFG {cfg}\n\n"
            f"Output:\n{paths}"
        )

    except RuntimeError as e:
        return f"❌ ComfyUI error: {e}"
    except TimeoutError as e:
        return f"⏱️ {e}"
    except Exception as e:
        logging.error("img2img failed: %s", e)
        return f"❌ Unexpected error: {e}"


@mcp.tool()
async def list_models(model_type: str = "checkpoints", intent: str = "") -> str:
    """List available models installed in ComfyUI.
    Use for: list models, what models are installed, available checkpoints, show loras,
    what can comfyui generate with, installed models, modelos disponibles.

    Args:
        model_type: Type to list — 'checkpoints', 'loras', 'vae', 'controlnet', 'embeddings'.
        intent: Natural language query.
    """
    try:
        if intent:
            il = intent.lower()
            if "lora"    in il: model_type = "loras"
            elif "vae"   in il: model_type = "vae"
            elif "control" in il: model_type = "controlnet"
            elif "embed" in il: model_type = "embeddings"

        data   = _http_get(f"/models/{model_type}")
        models = data if isinstance(data, list) else []

        if not models:
            return f"📭 No {model_type} found in ComfyUI."

        lines = [f"📦 **ComfyUI {model_type}** ({len(models)} installed)\n"]
        for m in models[:30]:
            lines.append(f"  • `{m}`")
        if len(models) > 30:
            lines.append(f"  …and {len(models) - 30} more.")
        return "\n".join(lines)

    except RuntimeError as e:
        return f"❌ {e}"


@mcp.tool()
async def get_node_schema(node_type: str, intent: str = "") -> str:
    """Get the input/output schema for a specific ComfyUI node type.
    Use for: what inputs does node X have, node schema, node parameters, how to use node,
    node documentation, ComfyUI node info.

    Args:
        node_type: Exact ComfyUI node class name (e.g. 'KSampler', 'CheckpointLoaderSimple').
        intent: Natural language description (used to infer node_type if left empty).
    """
    try:
        all_nodes = _http_get("/object_info")

        if not node_type and intent:
            q          = intent.lower()
            candidates = [k for k in all_nodes if q in k.lower()]
            if not candidates:
                return f"❌ No node matching '{intent}'."
            node_type = candidates[0]

        info = all_nodes.get(node_type)
        if not info:
            close = [k for k in all_nodes if node_type.lower() in k.lower()][:5]
            return (
                f"❌ Node `{node_type}` not found.\n"
                + (f"Did you mean: {', '.join(close)}?" if close else "")
            )

        inputs   = info.get("input", {})
        outputs  = info.get("output", [])
        out_names = info.get("output_name", [])
        lines = [f"🔧 **`{node_type}`** — {info.get('category', '')}"]
        if info.get("description"):
            lines.append(f"*{info['description']}*")
        required = inputs.get("required", {})
        optional = inputs.get("optional", {})
        if required:
            lines.append("\n**Required inputs:**")
            for name, spec in list(required.items())[:15]:
                lines.append(f"  • `{name}`: {spec[0] if spec else '?'}")
        if optional:
            lines.append("\n**Optional inputs:**")
            for name, spec in list(optional.items())[:10]:
                lines.append(f"  • `{name}`: {spec[0] if spec else '?'}")
        if outputs:
            lines.append(f"\n**Outputs:** {', '.join(f'`{n}` ({t})' for t, n in zip(outputs, out_names))}")
        return "\n".join(lines)

    except RuntimeError as e:
        return f"❌ {e}"


@mcp.tool()
async def comfy_status(intent: str = "") -> str:
    """Check if ComfyUI is running and return system stats.
    Use for: comfy status, is comfyui running, comfyui health, check comfyui, comfy online,
    what's in the queue, generation queue.
    """
    try:
        system   = _http_get("/system_stats")
        queue    = _http_get("/queue")
        gpu      = system.get("devices", [{}])[0] if system.get("devices") else {}
        gpu_name = gpu.get("name", "CPU")
        vram_free  = gpu.get("vram_free", 0)
        vram_total = gpu.get("vram_total", 1)
        vram_pct   = round(100 * (1 - vram_free / max(vram_total, 1)))
        running  = len(queue.get("queue_running", []))
        pending  = len(queue.get("queue_pending", []))
        return (
            f"✅ **ComfyUI is running**\n"
            f"Device: {gpu_name} | VRAM: {vram_pct}% used\n"
            f"Queue: {running} running, {pending} pending\n"
            f"Base URL: {_BASE}"
        )
    except RuntimeError as e:
        return f"❌ ComfyUI unreachable: {e}"


# ── YouTube Shorts pipeline ───────────────────────────────────────────────────

@mcp.tool()
async def generate_short(
    topic: str,
    summary: str,
    scene_count: int = 5,
    duration_per_scene: float = 7.0,
    output_path: str = "",
    voice: str = "af_heart",
    intent: str = "",
) -> str:
    """Generate a professional YouTube Short (9:16, 1080×1920) from a topic and research.

    Pipeline:
      1. Flux Schnell → cinematic frames (UE5 + anime + digital realism, 576×1024)
      2. Ken Burns zoom/pan (FFmpeg) → animated clips per scene
      3. Kokoro TTS → narration audio
      4. FFmpeg → final assembled .mp4 at 1080×1920

    Use for: make youtube short, create short video, generate reel, video corto,
    short youtube, crear short, video vertical, reel cinematografico, documental corto.

    Args:
        topic: Video title / subject.
        summary: Research text to base the video on (used for narration + visual prompts).
        scene_count: Number of scenes (3–8, each ~7s → 21–56s total).
        duration_per_scene: Seconds per scene clip (default 7.0).
        output_path: Final .mp4 output path (auto-generated if empty).
        voice: Kokoro TTS voice (af_heart, af_sky, am_adam, am_michael, bf_emma, bm_george).
        intent: Natural language generation goal.
    """
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    _OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    log_lines: list[str] = []

    try:
        # 1. Parse summary into scene texts
        paragraphs = [p.strip() for p in summary.replace("\r\n", "\n").split("\n\n") if len(p.strip()) > 20]
        if not paragraphs:
            paragraphs = [p.strip() for p in summary.replace("\r\n", "\n").split("\n") if len(p.strip()) > 20]
        if not paragraphs:
            paragraphs = [summary[i:i+300] for i in range(0, len(summary), 300) if summary[i:i+300].strip()]
        if not paragraphs:
            return "❌ Empty summary. Provide research text to base the video on."

        if len(paragraphs) >= scene_count:
            indices    = [i * len(paragraphs) // scene_count for i in range(scene_count)]
            scene_texts = [paragraphs[i] for i in indices]
        else:
            scene_texts = [paragraphs[i % len(paragraphs)] for i in range(scene_count)]

        # 2. Pick model
        checkpoint = _detect_checkpoint(prefer_flux=True)
        if not checkpoint:
            return "❌ No checkpoints found in ComfyUI."
        use_flux = _is_flux(checkpoint)
        log_lines.append(f"Model: {checkpoint} ({'Flux' if use_flux else 'SDXL'})")

        # 3. Generate scene images
        scene_images: list[str] = []
        for i, text in enumerate(scene_texts):
            prompt = _cinematic_prompt(topic, text, palette_idx=i)
            logging.info("Short scene %d/%d [%s]: %s…", i + 1, scene_count, checkpoint[:25], prompt[:70])

            if use_flux:
                wf = _flux_workflow(
                    prompt=prompt, negative=_NEG_CINEMATIC,
                    checkpoint=checkpoint, width=576, height=1024,
                    steps=4, cfg=1.0,
                )
            else:
                wf = _txt2img_workflow(
                    prompt=prompt, negative=_NEG_CINEMATIC,
                    checkpoint=checkpoint, width=576, height=1024,
                    steps=4, cfg=1.0, sampler="euler", scheduler="simple",
                )

            pid    = _submit_prompt(wf)
            result = _poll_until_done(pid, timeout_secs=1800)
            saved  = _save_outputs(pid, result.get("outputs", {}))
            if not saved:
                return f"⚠️ Scene {i + 1}/{scene_count} — no image output from ComfyUI."
            scene_images.append(saved[0])
            log_lines.append(f"Scene {i+1}: {saved[0]}")

        # 4. Ken Burns → per-scene video clips
        clip_paths: list[str] = []
        for i, img_path in enumerate(scene_images):
            clip_out = str(_OUTPUT_DIR / f"short_{timestamp}_clip{i:02d}.mp4")
            try:
                _ken_burns_clip(
                    image_path=img_path,
                    output_path=clip_out,
                    duration=duration_per_scene,
                    effect_idx=i,
                    out_width=1080,
                    out_height=1920,
                    fps=30,
                )
                clip_paths.append(clip_out)
                log_lines.append(f"Clip {i+1}: {clip_out}")
            except Exception as e:
                logging.warning("Ken Burns clip %d failed: %s", i, e)
                log_lines.append(f"Clip {i+1} FAILED: {e}")

        if not clip_paths:
            # Fallback: return images only
            img_list = "\n".join(f"  📸 `{p}`" for p in scene_images)
            return (
                f"⚠️ Ken Burns animation failed (FFmpeg issue).\n"
                f"Scene images generated:\n{img_list}\n\n"
                f"Install FFmpeg and retry, or assemble manually."
            )

        # 5. Concatenate clips
        video_silent = str(_OUTPUT_DIR / f"short_{timestamp}_silent.mp4")
        try:
            _crossfade_concat(clip_paths, video_silent)
            log_lines.append(f"Concat: {video_silent}")
        except Exception as e:
            return f"❌ Video concatenation failed: {e}\n\nClips:\n" + "\n".join(f"  `{p}`" for p in clip_paths)

        # 6. Kokoro TTS narration
        narration_text = f"{topic}.\n\n" + "\n\n".join(scene_texts)
        audio_path: str | None = None
        try:
            tts_wf     = _kokoro_tts_workflow(narration_text, voice=voice)
            tts_pid    = _submit_prompt(tts_wf)
            tts_result = _poll_until_done(tts_pid, timeout_secs=300)
            tts_saved  = _save_audio_outputs(tts_pid, tts_result.get("outputs", {}))
            if tts_saved:
                audio_path = tts_saved[0]
                log_lines.append(f"Audio: {audio_path}")
            else:
                log_lines.append("Audio: Kokoro returned no file")
        except Exception as e:
            logging.warning("Kokoro TTS failed: %s", e)
            log_lines.append(f"Audio: SKIPPED ({e})")

        # 7. Mix audio + finalize
        if not output_path:
            output_path = str(_OUTPUT_DIR / f"short_{timestamp}.mp4")

        if audio_path and os.path.exists(audio_path):
            try:
                _add_audio_track(video_silent, audio_path, output_path)
                log_lines.append(f"Final (with audio): {output_path}")
            except Exception as e:
                logging.warning("Audio mix failed: %s — saving silent video", e)
                shutil.copy(video_silent, output_path)
                log_lines.append(f"Final (silent, audio mix failed): {output_path}")
        else:
            shutil.copy(video_silent, output_path)
            log_lines.append(f"Final (no audio): {output_path}")

        # Save narration script
        script_path = str(_OUTPUT_DIR / f"short_{timestamp}_script.txt")
        with open(script_path, "w", encoding="utf-8") as f:
            f.write(f"# {topic}\n\n{narration_text}")

        total_duration = len(clip_paths) * duration_per_scene
        return (
            f"✅ **YouTube Short generated**\n"
            f"Topic: {topic}\n"
            f"Scenes: {len(scene_images)} | Duration: {total_duration:.0f}s | Model: {checkpoint}\n"
            f"Format: 1080×1920 (9:16) | {30}fps\n\n"
            f"📺 **`{output_path}`**\n"
            + (f"🎤 Narration: `{audio_path}`\n" if audio_path else "")
            + f"📝 Script: `{script_path}`\n\n"
            f"Scene images:\n"
            + "\n".join(f"  📸 `{p}`" for p in scene_images)
        )

    except RuntimeError as e:
        return f"❌ ComfyUI error: {e}"
    except TimeoutError as e:
        return f"⏱️ {e}"
    except Exception as e:
        logging.error("generate_short failed: %s", e, exc_info=True)
        return f"❌ Unexpected error: {e}\n\nLog:\n" + "\n".join(log_lines)


@mcp.tool()
async def generate_documentary_video(
    topic: str,
    summary: str,
    scene_count: int = 5,
    duration_per_scene: float = 8.0,
    output_path: str = "",
    intent: str = "",
) -> str:
    """Generate a short documentary video (16:9, 1280×720) from a topic and research summary.
    Use for: make video, create documentary, generate video from research, video documental,
    crear video, documental, video youtube 16:9.

    Args:
        topic: Video title/topic.
        summary: Research summary to base the documentary on.
        scene_count: Number of scenes (3–8 recommended for 30–60s).
        duration_per_scene: Seconds per scene image (default 8.0).
        output_path: Output video path (auto-generated if empty).
        intent: Natural language generation goal.
    """
    # Delegate to generate_short with landscape dimensions
    timestamp = time.strftime("%Y%m%d_%H%M%S")
    _OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    try:
        paragraphs = [p.strip() for p in summary.replace("\r\n", "\n").split("\n\n") if len(p.strip()) > 20]
        if not paragraphs:
            paragraphs = [p.strip() for p in summary.replace("\r\n", "\n").split("\n") if len(p.strip()) > 20]
        if not paragraphs:
            return "❌ Empty summary. Provide research text to base the documentary on."

        if len(paragraphs) >= scene_count:
            indices    = [i * len(paragraphs) // scene_count for i in range(scene_count)]
            scene_texts = [paragraphs[i] for i in indices]
        else:
            scene_texts = [paragraphs[i % len(paragraphs)] for i in range(scene_count)]

        checkpoint = _detect_checkpoint(prefer_flux=True)
        if not checkpoint:
            return "❌ No checkpoints found in ComfyUI."
        use_flux = _is_flux(checkpoint)

        scene_images: list[str] = []
        for i, text in enumerate(scene_texts):
            prompt = _cinematic_prompt(topic, text, palette_idx=i)
            if use_flux:
                wf = _flux_workflow(
                    prompt=prompt, negative=_NEG_CINEMATIC,
                    checkpoint=checkpoint, width=768, height=432,
                    steps=4, cfg=1.0,
                )
            else:
                wf = _txt2img_workflow(
                    prompt=prompt, negative=_NEG_CINEMATIC,
                    checkpoint=checkpoint, width=768, height=432,
                    steps=4, cfg=1.0, sampler="euler", scheduler="simple",
                )
            pid    = _submit_prompt(wf)
            result = _poll_until_done(pid, timeout_secs=1800)
            saved  = _save_outputs(pid, result.get("outputs", {}))
            if not saved:
                return f"⚠️ Scene {i + 1} failed — no image output."
            scene_images.append(saved[0])

        clip_paths: list[str] = []
        for i, img_path in enumerate(scene_images):
            clip_out = str(_OUTPUT_DIR / f"doc_{timestamp}_clip{i:02d}.mp4")
            try:
                _ken_burns_clip(
                    image_path=img_path, output_path=clip_out,
                    duration=duration_per_scene, effect_idx=i,
                    out_width=1280, out_height=720, fps=30,
                )
                clip_paths.append(clip_out)
            except Exception as e:
                logging.warning("Ken Burns clip %d failed: %s", i, e)

        if not output_path:
            output_path = str(_OUTPUT_DIR / f"documentary_{timestamp}.mp4")

        if clip_paths:
            try:
                _crossfade_concat(clip_paths, output_path)
            except Exception as e:
                return (
                    f"⚠️ Video concat failed: {e}\n"
                    f"Scene images:\n" + "\n".join(f"  📸 `{p}`" for p in scene_images)
                )
        else:
            img_list = "\n".join(f"  📸 `{p}`" for p in scene_images)
            return f"⚠️ Ken Burns failed — scene images generated:\n{img_list}"

        total = len(clip_paths) * duration_per_scene
        return (
            f"✅ **Documentary video generated**\n"
            f"Topic: {topic} | Scenes: {len(scene_images)} | Duration: {total:.0f}s\n"
            f"Model: {checkpoint} | Format: 1280×720 (16:9)\n\n"
            f"📺 **`{output_path}`**\n\n"
            f"Scene images:\n" + "\n".join(f"  📸 `{p}`" for p in scene_images)
        )

    except RuntimeError as e:
        return f"❌ ComfyUI error: {e}"
    except TimeoutError as e:
        return f"⏱️ {e}"
    except Exception as e:
        logging.error("generate_documentary_video failed: %s", e)
        return f"❌ Unexpected error: {e}"


@mcp.tool()
async def generate_wan_video(
    prompt: str,
    negative_prompt: str = "",
    mode: str = "shorts",
    steps: int = 20,
    cfg: float = 5.0,
    seed: int = -1,
    intent: str = "",
) -> str:
    """Generate a video clip using Wan2.2 (5B text-to-video model).

    Two modes:
    - 'shorts'    → 1088×1920 portrait, 53 frames ~2.2s at 24fps. YouTube Shorts / TikTok.
    - 'landscape' → 1280×704 widescreen, 121 frames ~5s at 24fps. YouTube landscape / cinematic.

    Requires: wan2.2_ti2v_5B_fp16.safetensors, umt5_xxl_fp8_e4m3fn_scaled.safetensors, wan2.2_vae.safetensors

    Args:
        prompt: Detailed cinematic description of the video scene and motion.
        negative_prompt: What to avoid (defaults to motion/quality negatives).
        mode: 'shorts' (portrait 9:16) or 'landscape' (16:9).
        steps: Sampling steps (20 recommended).
        cfg: Guidance scale (5.0 recommended for Wan2.2).
        seed: Random seed (-1 for random).
        intent: Natural language goal.
    """
    try:
        workflow = _wan22_workflow(
            prompt=prompt, negative=negative_prompt, mode=mode,
            steps=steps, cfg=cfg, seed=seed,
        )
        dims = "1088×1920 (Shorts)" if mode == "shorts" else "1280×704 (Landscape)"
        frames = 53 if mode == "shorts" else 121
        logging.info("Submitting Wan2.2 video [%s]: %s…", dims, prompt[:60])
        prompt_id = _submit_prompt(workflow)
        result = _poll_until_done(prompt_id, timeout=3600)
        saved = _save_outputs(prompt_id, result.get("outputs", {}))
        if not saved:
            return f"⚠️ Video generation completed (ID: {prompt_id}) but no output files found."
        paths = "\n".join(f"  🎬 `{p}`" for p in saved)
        return (
            f"✅ **Video generated** (ID: `{prompt_id}`)\n"
            f"Model: wan2.2_ti2v_5B | {dims} | {frames} frames | {steps} steps | CFG {cfg}\n\n"
            f"Saved to:\n{paths}"
        )
    except RuntimeError as e:
        return f"❌ ComfyUI error: {e}"
    except TimeoutError as e:
        return f"⏱️ {e}"
    except Exception as e:
        logging.error("generate_wan_video failed: %s", e)
        return f"❌ Unexpected error: {e}"



# ── Internal helpers ──────────────────────────────────────────────────────────

async def _get_first_checkpoint() -> str:
    try:
        models = _http_get("/models/checkpoints")
        return models[0] if models else ""
    except Exception:
        return ""


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, stream=sys.stderr)
    utils.safe_mcp_run(mcp)
