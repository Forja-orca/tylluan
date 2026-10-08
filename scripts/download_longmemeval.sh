#!/usr/bin/env bash
# scripts/download_longmemeval.sh — Reproducible downloader & preparer for LongMemEval and BGE-M3.
#
# Context & Rationale (2026-10-05, ADR-018 & Frente 2, Turn 915/916):
#   Tylluan publishes 82% Recall@5 (12.9ms p50) on LongMemEval-S (see benchmarks/longmemeval_v0.12.0.json).
#   However, fresh checkouts cannot reproduce these numbers because:
#     1) data/longmemeval_s_subset.json is untracked (data/ is gitignored per .gitignore:12).
#     2) models/bge-m3 ONNX weights are untracked (models/ is gitignored per .gitignore:22),
#        and BGE-M3 is absent from maintenance.rs:get_model_registry() (tylluan-cli download-models).
#   This script provides the 100% reproducible, standalone recipe to download and prepare both
#   artifacts without manual intervention or closed dependencies.
#
# Artifact 1: data/longmemeval_s_subset.json (Evaluation Dataset)
#   - Benchmark Paper: "LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory"
#     (Di Wu, Cheng Li et al., arXiv:2410.10813, ICLR 2025)
#   - Canonical Source: https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json
#   - License: Apache-2.0 / MIT (Open research dataset)
#   - Raw Size: ~277 MB (500 questions with complete multi-session haystack)
#   - Subset Size: ~27 MB (first 50 questions, matching tylluan-evals limit=50 and longmemeval.rs:66)
#
# Artifact 2: models/bge-m3 (ONNX Embedding Model & Tokenizer)
#   - Canonical Source: https://huggingface.co/BAAI/bge-m3/tree/main/onnx
#   - License: MIT License (BAAI)
#   - Engine: FastEmbed (fastembed-rs v5.8.0, crates/tylluan-kernel/src/router/embeddings.rs)
#   - Required Files:
#       onnx/model.onnx (~1.1 GB, ONNX graph)
#       onnx/model.onnx_data (~2.2 GB, external weights tensor)
#       onnx/Constant_7_attr__value (~1 KB)
#       tokenizer.json (~17.1 MB), config.json, special_tokens_map.json, tokenizer_config.json
#   - Total Expected Size: ~3.3 GB
#
# Usage:
#   scripts/download_longmemeval.sh              # Download and prepare both dataset and BGE-M3
#   scripts/download_longmemeval.sh --dataset    # Only download & extract data/longmemeval_s_subset.json
#   scripts/download_longmemeval.sh --model      # Only download models/bge-m3 ONNX files
#   scripts/download_longmemeval.sh --limit 50   # Specify subset size (default: 50)
#   scripts/download_longmemeval.sh --force      # Re-download even if already present
#
# Verification / Run:
#   cargo run --release -p tylluan-evals -- --suite longmemeval --limit 50
#   # With thread cap (cafetera-friendly, prevents saturating all CPU cores):
#   cargo run --release -p tylluan-evals -- --suite longmemeval --limit 50 --threads 4
#   # Or via environment variable override:
#   TYLLUAN_ORT_THREADS=4 cargo run --release -p tylluan-evals -- --suite longmemeval --limit 50

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

DATA_DIR="$ROOT_DIR/data"
MODELS_DIR="$ROOT_DIR/models"
BGE_DIR="$MODELS_DIR/bge-m3"

DO_DATASET=1
DO_MODEL=1
LIMIT=50
FORCE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dataset|--dataset-only)
            DO_DATASET=1
            DO_MODEL=0
            shift
            ;;
        --model|--model-only|--models-only)
            DO_DATASET=0
            DO_MODEL=1
            shift
            ;;
        --limit)
            LIMIT="$2"
            shift 2
            ;;
        --force)
            FORCE=1
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--dataset] [--model] [--limit N] [--force]"
            exit 0
            ;;
        *)
            echo "Unknown argument: $1"
            exit 1
            ;;
    esac
done

ok()   { echo "✅ $1"; }
info() { echo "ℹ️  $1"; }
warn() { echo "⚠️  $1"; }
fail() { echo "❌ $1"; exit 1; }

# Detect Python interpreter
PYTHON_CMD=""
if command -v python3 &>/dev/null; then
    PYTHON_CMD="python3"
elif command -v python &>/dev/null; then
    PYTHON_CMD="python"
else
    fail "Python (python or python3) is required to process the JSON dataset."
fi

mkdir -p "$DATA_DIR"
mkdir -p "$MODELS_DIR"

# ==============================================================================
# 1. LongMemEval-S Dataset
# ==============================================================================
if [[ $DO_DATASET -eq 1 ]]; then
    SUBSET_FILE="$DATA_DIR/longmemeval_s_subset.json"
    RAW_FILE="$DATA_DIR/longmemeval_s_cleaned.json"
    DATASET_URL="https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json"

    if [[ -f "$SUBSET_FILE" && $FORCE -eq 0 ]]; then
        info "Dataset already present at $SUBSET_FILE (use --force to re-download)."
    else
        info "Downloading canonical LongMemEval-S dataset (~277 MB)..."
        info "Source: $DATASET_URL"
        if ! curl -L --progress-bar -o "$RAW_FILE" "$DATASET_URL"; then
            fail "Failed to download $DATASET_URL"
        fi

        info "Extracting deterministic slice of first $LIMIT questions (matching longmemeval.rs:66)..."
        "$PYTHON_CMD" -c "
import json, sys
src = sys.argv[1]
dst = sys.argv[2]
limit = int(sys.argv[3])
with open(src, 'r', encoding='utf-8') as f:
    data = json.load(f)
subset = data[:limit]
with open(dst, 'w', encoding='utf-8') as f:
    json.dump(subset, f, indent=2, ensure_ascii=False)
print(f'Wrote {len(subset)} questions to {dst}')
" "$RAW_FILE" "$SUBSET_FILE" "$LIMIT"

        if [[ -f "$SUBSET_FILE" ]]; then
            ok "Dataset ready: $SUBSET_FILE ($LIMIT questions)"
        else
            fail "Failed to generate $SUBSET_FILE"
        fi
    fi
fi

# ==============================================================================
# 2. BGE-M3 ONNX Weights & Tokenizer
# ==============================================================================
if [[ $DO_MODEL -eq 1 ]]; then
    ONNX_MODEL_FILE="$BGE_DIR/onnx/model.onnx"
    ONNX_DATA_FILE="$BGE_DIR/onnx/model.onnx_data"

    if [[ -f "$ONNX_MODEL_FILE" && -f "$ONNX_DATA_FILE" && $FORCE -eq 0 ]]; then
        info "BGE-M3 ONNX weights already present at $BGE_DIR (use --force to re-download)."
    else
        mkdir -p "$BGE_DIR/onnx"
        info "Downloading BGE-M3 ONNX model (~3.3 GB total)..."
        info "Repository: BAAI/bge-m3 (License: MIT)"

        if command -v huggingface-cli &>/dev/null; then
            info "Using huggingface-cli for parallel chunked download..."
            huggingface-cli download BAAI/bge-m3 \
                --include "onnx/*" "tokenizer.json" "config.json" "special_tokens_map.json" "tokenizer_config.json" \
                --local-dir "$BGE_DIR"
        else
            info "huggingface-cli not found, falling back to direct stream download via Python/curl..."
            BASE_URL="https://huggingface.co/BAAI/bge-m3/resolve/main"
            FILES=(
                "onnx/model.onnx"
                "onnx/model.onnx_data"
                "onnx/Constant_7_attr__value"
                "tokenizer.json"
                "config.json"
                "special_tokens_map.json"
                "tokenizer_config.json"
            )

            for rel in "${FILES[@]}"; do
                target="$BGE_DIR/$rel"
                mkdir -p "$(dirname "$target")"
                if [[ -f "$target" && $FORCE -eq 0 ]]; then
                    info "Already downloaded: $rel"
                    continue
                fi
                info "Downloading $rel..."
                file_url="$BASE_URL/$rel"
                if ! curl -L --progress-bar -o "$target" "$file_url"; then
                    fail "Failed to download $file_url"
                fi
            done
        fi

        if [[ -f "$ONNX_MODEL_FILE" && -f "$ONNX_DATA_FILE" ]]; then
            ok "BGE-M3 ONNX model ready at: $BGE_DIR"
        else
            fail "BGE-M3 download incomplete (missing model.onnx or model.onnx_data)"
        fi
    fi
fi

echo ""
ok "LongMemEval artifact provisioning complete."
echo "To execute the reproducible benchmark suite, run:"
echo "  cargo run --release -p tylluan-evals -- --suite longmemeval --limit $LIMIT"
echo "To execute with Jina cross-encoder reranker:"
echo "  cargo run --release -p tylluan-evals -- --suite longmemeval --limit $LIMIT --reranker"
