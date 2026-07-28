"""Download Gemma-4-E2B-it-ONNX for coordinator use.
Downloads only the files needed for text-only inference (no vision/audio encoders).
Verifies every required file exists with non-zero size.

Run once:  python -m guilds.core.download_gemma4
"""
from huggingface_hub import snapshot_download
from pathlib import Path
import sys

MODEL_ID = "onnx-community/gemma-4-E2B-it-ONNX"
REQUIRED = [
    "config.json",
    "tokenizer.json",
    "tokenizer_config.json",
    "generation_config.json",
    "chat_template.jinja",
    "onnx/embed_tokens_q4.onnx",
    "onnx/embed_tokens_q4.onnx_data",
    "onnx/decoder_model_merged_q4.onnx",
    "onnx/decoder_model_merged_q4.onnx_data",
]
ALLOW_PATTERNS = [f"{p}*" for p in REQUIRED]

def verify(path):
    """Verify all required files exist with non-zero size. Report sizes."""
    print(f"\nVerifying in: {path}")
    missing = []
    sizes = {}
    for rel in REQUIRED:
        full = Path(path) / rel
        if full.exists() and full.stat().st_size > 0:
            sz_mb = full.stat().st_size / (1024 * 1024)
            sizes[rel] = sz_mb
            print(f"  [OK] {rel} ({sz_mb:.1f} MB)")
        else:
            missing.append(rel)
            print(f"  [MISSING] {rel}")
    if missing:
        print(f"\nERROR: {len(missing)} required files missing: {missing}")
        sys.exit(1)
    total_mb = sum(sizes.values())
    print(f"\n  Total: {total_mb:.1f} MB - all required files present")
    return total_mb

def download():
    print(f"Downloading {MODEL_ID}...")
    print(f"Required files ({len(REQUIRED)}): {[r.split('/')[-1] for r in REQUIRED]}")
    path = snapshot_download(MODEL_ID, allow_patterns=ALLOW_PATTERNS)
    total_mb = verify(path)
    print(f"\nDone! Model ready at: {path}")
    return path

if __name__ == "__main__":
    download()
