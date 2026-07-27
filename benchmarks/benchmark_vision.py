"""Vision Guild Benchmark: SmolVLM2-256M ONNX real inference (production path).

Reuses what already works instead of chasing a new dependency (Moondream
requires Pillow 10.4.0, which has no Python 3.14 Windows wheel -- a real
blocker, not worth fighting). SmolVLM2-256M ONNX is the guild already
running in production (guilds/core/vision.py) -- zero new installs, zero
new environment risk.
"""
import asyncio
import os
import time

from PIL import Image, ImageDraw


def build_real_test_image(path: str) -> None:
    """A structured image (shapes + text), not a blank color square, so
    the model has real content to describe."""
    img = Image.new("RGB", (400, 300), color=(255, 255, 255))
    draw = ImageDraw.Draw(img)
    draw.rectangle([40, 40, 200, 160], fill=(66, 135, 245), outline=(0, 0, 0))
    draw.ellipse([220, 60, 340, 180], fill=(235, 87, 87), outline=(0, 0, 0))
    draw.text((40, 220), "TYLLUAN VISION TEST", fill=(0, 0, 0))
    img.save(path)


async def run_benchmark():
    print("=== Vision Guild Benchmark: SmolVLM2-256M ONNX (production path, real inference) ===")

    test_img_path = os.path.join(os.path.expanduser("~"), ".tylluan", "test_vision_input.png")
    os.makedirs(os.path.dirname(test_img_path), exist_ok=True)
    build_real_test_image(test_img_path)
    print(f"Test image: {test_img_path} (blue rectangle + red ellipse + text, not a blank color)")

    t0 = time.time()
    Image.open(test_img_path).convert("RGB")
    pil_latency_ms = (time.time() - t0) * 1000
    print(f"PIL preprocessing latency: {pil_latency_ms:.2f}ms")

    from guilds.core.vision import vision_analyze

    print("\nRunning REAL SmolVLM2-256M ONNX inference (no dry-run, no mock)...")
    t1 = time.time()
    try:
        result = await vision_analyze(test_img_path, prompt="Describe the shapes and any text in this image.")
    except Exception as e:
        print(f"STATUS: FAILED_REAL_INFERENCE -- {e}")
        return
    inference_latency_ms = (time.time() - t1) * 1000

    print(f"Inference latency: {inference_latency_ms:.2f}ms")
    print(f"\nModel output:\n  {result!r}")

    result_lower = str(result).lower()
    is_degraded = '"status": "degraded"' in result_lower or "model unavailable" in result_lower
    mentions_shape = any(w in result_lower for w in ("rectangle", "square", "circle", "ellipse", "oval", "blue", "red"))
    mentions_text = "tylluan" in result_lower or "text" in result_lower
    print(f"\nSanity check -- output mentions a shape/color: {mentions_shape}")
    print(f"Sanity check -- output mentions text/writing: {mentions_text}")

    if is_degraded:
        print("\nSTATUS: FAILED_MODEL_NOT_CACHED -- vision.py returned its degraded fallback, not real inference.")
        print("Real cause: guilds/core/vision.py expects the model under ~/.tylluan/models_cache")
        print("(local_files_only=True, by design -- no silent network calls). That directory")
        print("does not exist in this environment -- the model was never downloaded there.")
        print("Not fixed here: needs an explicit one-time download step into that exact path")
        print("before this benchmark can report a real result.")
    elif not (mentions_shape or mentions_text):
        print("\nSTATUS: SUSPECT -- model produced non-degraded output but it doesn't reference")
        print("anything in the actual test image. Inspect manually before trusting this as a real pass.")
    else:
        print("\nSTATUS: SUCCESS -- real inference, output references the actual image content.")


if __name__ == "__main__":
    asyncio.run(run_benchmark())
