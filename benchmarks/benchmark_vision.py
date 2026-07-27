"""Vision Guild Benchmark: Moondream 0.5B real inference latency + output.

Runs actual model inference (analyze_image) on a real test image -- not
just an import check. The previous version of this file imported
analyze_image but never called it, reporting "STATUS: SUCCESS" without a
single model inference having occurred (found and flagged twice in
Coloquio: turns 260 and 279 -- fixed here for real).
"""
import asyncio
import os
import time

from PIL import Image, ImageDraw


def build_real_test_image(path: str) -> None:
    """A synthetic-but-structured image (not a blank color square) so the
    model has actual content to describe: shapes + text, not noise."""
    img = Image.new("RGB", (400, 300), color=(255, 255, 255))
    draw = ImageDraw.Draw(img)
    draw.rectangle([40, 40, 200, 160], fill=(66, 135, 245), outline=(0, 0, 0))
    draw.ellipse([220, 60, 340, 180], fill=(235, 87, 87), outline=(0, 0, 0))
    draw.text((40, 220), "TYLLUAN VISION TEST", fill=(0, 0, 0))
    img.save(path)


async def run_benchmark():
    print("=== Vision Guild Benchmark: Moondream 0.5B (real inference) ===")

    test_img_path = os.path.join(os.path.expanduser("~"), ".tylluan", "test_vision_input.png")
    os.makedirs(os.path.dirname(test_img_path), exist_ok=True)
    build_real_test_image(test_img_path)
    print(f"Test image: {test_img_path} (blue rectangle + red ellipse + text, not a blank color)")

    t0 = time.time()
    img = Image.open(test_img_path).convert("RGB")
    pil_latency_ms = (time.time() - t0) * 1000
    print(f"PIL preprocessing latency: {pil_latency_ms:.2f}ms")

    from guilds.core.vision_moondream import analyze_image

    print("\nLoading Moondream 0.5B and running REAL inference (not a dry-run)...")
    t1 = time.time()
    try:
        description = await analyze_image(test_img_path, prompt="Describe the shapes and any text in this image.")
    except Exception as e:
        print(f"STATUS: FAILED_REAL_INFERENCE -- {e}")
        return
    inference_latency_ms = (time.time() - t1) * 1000

    print(f"Inference latency: {inference_latency_ms:.2f}ms")
    print(f"\nModel output:\n  {description!r}")

    mentions_shape = any(w in description.lower() for w in ("rectangle", "square", "circle", "ellipse", "oval", "blue", "red"))
    mentions_text = "tylluan" in description.lower() or "text" in description.lower()
    print(f"\nSanity check -- output mentions a shape/color: {mentions_shape}")
    print(f"Sanity check -- output mentions text/writing: {mentions_text}")
    if not (mentions_shape or mentions_text):
        print("WARNING: model output doesn't reference anything in the actual image -- inspect manually before trusting this as a real pass.")

    print(f"\nSTATUS: {'SUCCESS' if description.strip() else 'EMPTY_OUTPUT'}")


if __name__ == "__main__":
    asyncio.run(run_benchmark())
