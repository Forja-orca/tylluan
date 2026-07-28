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

    # Call twice in the SAME process to actually measure whether the first
    # call's cost is DirectML/ONNX session JIT compilation (one-time, paid
    # once per process) versus real per-call model latency. Two separate
    # `python -m` invocations are two cold starts, not a real first-vs-second
    # comparison -- Deep's amortization hypothesis needs this to be tested
    # honestly, not assumed.
    print("\nRunning REAL SmolVLM2-256M ONNX inference, call #1 (no dry-run, no mock)...")
    t1 = time.time()
    try:
        result = await vision_analyze(test_img_path, prompt="Describe the shapes and any text in this image.")
    except Exception as e:
        print(f"STATUS: FAILED_REAL_INFERENCE -- {e}")
        return
    first_call_latency_ms = (time.time() - t1) * 1000
    print(f"Call #1 latency: {first_call_latency_ms:.2f}ms")

    print("\nRunning call #2, SAME process, session already loaded/compiled...")
    t2 = time.time()
    try:
        result2 = await vision_analyze(test_img_path, prompt="Describe the shapes and any text in this image.")
    except Exception as e:
        print(f"Call #2 FAILED: {e}")
        result2 = None
    second_call_latency_ms = (time.time() - t2) * 1000 if result2 is not None else None

    print(f"Call #2 latency: {second_call_latency_ms:.2f}ms" if second_call_latency_ms is not None else "Call #2: failed")
    if second_call_latency_ms is not None:
        speedup = first_call_latency_ms / second_call_latency_ms if second_call_latency_ms > 0 else float("inf")
        print(f"Speedup call1/call2: {speedup:.1f}x")
        if second_call_latency_ms < 5000:
            print("CONFIRMED: second call is under 5s -- Deep's JIT-amortization hypothesis holds within a warm process.")
        else:
            print("NOT CONFIRMED: second call is still >= 5s within the same warm process -- the JIT-amortization hypothesis does not hold as stated.")

    inference_latency_ms = first_call_latency_ms
    print(f"\nModel output (call #1):\n  {result!r}")

    result_lower = str(result).lower()
    is_degraded = '"status": "degraded"' in result_lower or "model unavailable" in result_lower
    mentions_shape = any(w in result_lower for w in ("rectangle", "square", "circle", "ellipse", "oval", "blue", "red"))
    mentions_text = "tylluan" in result_lower or "text" in result_lower
    print(f"\nSanity check -- output mentions a shape/color: {mentions_shape}")
    print(f"Sanity check -- output mentions text/writing: {mentions_text}")

    if is_degraded:
        print("\nSTATUS: FAILED_MODEL_INCOMPLETE -- vision.py returned its degraded fallback, not real inference.")
        print("Real cause (2026-07-27): the HF snapshot for HuggingFaceTB/SmolVLM2-256M-Instruct")
        print("exists in the standard cache (~/.cache/huggingface/hub) but its onnx/ subfolder is")
        print("missing the actual .onnx weight files (vision_encoder.onnx not present) -- only")
        print("config/tokenizer files were ever downloaded, not the ONNX weights themselves.")
        print("Not fixed here: needs a real (re-)download of the onnx/ subfolder before this")
        print("benchmark can report a genuine result.")
    elif not (mentions_shape or mentions_text):
        print("\nSTATUS: SUSPECT -- model produced non-degraded output but it doesn't reference")
        print("anything in the actual test image. Inspect manually before trusting this as a real pass.")
    else:
        print("\nSTATUS: SUCCESS -- real inference, output references the actual image content.")


if __name__ == "__main__":
    asyncio.run(run_benchmark())
