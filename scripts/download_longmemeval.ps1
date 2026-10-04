<#
.SYNOPSIS
  Reproducible downloader & preparer for LongMemEval and BGE-M3 on Windows.

.DESCRIPTION
  Tylluan publishes 82% Recall@5 (12.9ms p50) on LongMemEval-S (see benchmarks/longmemeval_v0.12.0.json).
  However, fresh checkouts cannot reproduce these numbers because:
    1) data/longmemeval_s_subset.json is untracked (data/ is gitignored per .gitignore:12).
    2) models/bge-m3 ONNX weights are untracked (models/ is gitignored per .gitignore:22),
       and BGE-M3 is absent from maintenance.rs:get_model_registry() (tylluan-cli download-models).
  This script provides the 100% reproducible, standalone recipe to download and prepare both
  artifacts without manual intervention or closed dependencies.

  Artifact 1: data/longmemeval_s_subset.json (Evaluation Dataset)
    - Paper: "LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory" (arXiv:2410.10813, ICLR 2025)
    - Canonical Source: https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json
    - License: Apache-2.0 / MIT (Open research dataset)
    - Raw Size: ~277 MB | Subset Size: ~27 MB (first 50 questions, matching tylluan-evals limit=50)

  Artifact 2: models/bge-m3 (ONNX Embedding Model & Tokenizer)
    - Canonical Source: https://huggingface.co/BAAI/bge-m3/tree/main/onnx
    - License: MIT License (BAAI)
    - Required Files: model.onnx (~1.1 GB), model.onnx_data (~2.2 GB), Constant_7_attr__value, tokenizers (~17 MB)
    - Total Expected Size: ~3.3 GB

.PARAMETER DatasetOnly
  Download and prepare only data/longmemeval_s_subset.json.

.PARAMETER ModelOnly
  Download only models/bge-m3 ONNX weights and tokenizers.

.PARAMETER Limit
  Number of questions to extract into the subset (default: 50).

.PARAMETER Force
  Re-download artifacts even if already present locally.

.EXAMPLE
  .\scripts\download_longmemeval.ps1
  .\scripts\download_longmemeval.ps1 -DatasetOnly
  .\scripts\download_longmemeval.ps1 -Limit 50
#>

[CmdletBinding()]
param(
    [switch]$DatasetOnly,
    [switch]$ModelOnly,
    [int]$Limit = 50,
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Split-Path -Parent $ScriptDir
Set-Location $RootDir

$DataDir = Join-Path $RootDir "data"
$ModelsDir = Join-Path $RootDir "models"
$BgeDir = Join-Path $ModelsDir "bge-m3"

$DoDataset = -not $ModelOnly
$DoModel = -not $DatasetOnly

function Log-Ok   { param([string]$msg) Write-Host "[OK] $msg" -ForegroundColor Green }
function Log-Info { param([string]$msg) Write-Host "[INFO] $msg" -ForegroundColor Cyan }
function Log-Warn { param([string]$msg) Write-Host "[WARN] $msg" -ForegroundColor Yellow }
function Log-Fail { param([string]$msg) Write-Host "[ERROR] $msg" -ForegroundColor Red; exit 1 }

# Check Python
$PythonCmd = $null
if (Get-Command python -ErrorAction SilentlyContinue) {
    $PythonCmd = "python"
} elseif (Get-Command python3 -ErrorAction SilentlyContinue) {
    $PythonCmd = "python3"
} else {
    Log-Fail "Python is required to parse and slice the JSON dataset."
}

if (-not (Test-Path $DataDir)) { New-Item -ItemType Directory -Path $DataDir -Force | Out-Null }
if (-not (Test-Path $ModelsDir)) { New-Item -ItemType Directory -Path $ModelsDir -Force | Out-Null }

# ==============================================================================
# 1. LongMemEval-S Dataset
# ==============================================================================
if ($DoDataset) {
    $SubsetFile = Join-Path $DataDir "longmemeval_s_subset.json"
    $RawFile = Join-Path $DataDir "longmemeval_s_cleaned.json"
    $DatasetUrl = "https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/main/longmemeval_s_cleaned.json"

    if ((Test-Path $SubsetFile) -and (-not $Force)) {
        Log-Info "Dataset already present at $SubsetFile (use -Force to re-download)."
    } else {
        Log-Info "Downloading canonical LongMemEval-S dataset (~277 MB)..."
        Log-Info "Source: $DatasetUrl"

        if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
            & curl.exe -L --progress-bar -o $RawFile $DatasetUrl
            if ($LASTEXITCODE -ne 0) { Log-Fail "Failed to download $DatasetUrl" }
        } else {
            Invoke-WebRequest -Uri $DatasetUrl -OutFile $RawFile
        }

        Log-Info "Extracting deterministic slice of first $Limit questions (matching longmemeval.rs:66)..."
        $pyCode = @'
import json, sys
src = sys.argv[1]
dst = sys.argv[2]
limit = int(sys.argv[3])
with open(src, "r", encoding="utf-8") as f:
    data = json.load(f)
subset = data[:limit]
with open(dst, "w", encoding="utf-8") as f:
    json.dump(subset, f, indent=2, ensure_ascii=False)
print(f"Wrote {len(subset)} questions to {dst}")
'@
        & $PythonCmd -c $pyCode $RawFile $SubsetFile $Limit
        if ($LASTEXITCODE -ne 0) { Log-Fail "Failed to extract subset from $RawFile" }

        if (Test-Path $SubsetFile) {
            Log-Ok "Dataset ready: $SubsetFile ($Limit questions)"
        } else {
            Log-Fail "Failed to generate $SubsetFile"
        }
    }
}

# ==============================================================================
# 2. BGE-M3 ONNX Weights & Tokenizer
# ==============================================================================
if ($DoModel) {
    $OnnxDir = Join-Path $BgeDir "onnx"
    $OnnxModelFile = Join-Path $OnnxDir "model.onnx"
    $OnnxDataFile = Join-Path $OnnxDir "model.onnx_data"

    if ((Test-Path $OnnxModelFile) -and (Test-Path $OnnxDataFile) -and (-not $Force)) {
        Log-Info "BGE-M3 ONNX weights already present at $BgeDir (use -Force to re-download)."
    } else {
        if (-not (Test-Path $OnnxDir)) { New-Item -ItemType Directory -Path $OnnxDir -Force | Out-Null }
        Log-Info "Downloading BGE-M3 ONNX model (~3.3 GB total)..."
        Log-Info "Repository: BAAI/bge-m3 (License: MIT)"

        if (Get-Command huggingface-cli -ErrorAction SilentlyContinue) {
            Log-Info "Using huggingface-cli for parallel chunked download..."
            & huggingface-cli download BAAI/bge-m3 `
                --include "onnx/*" "tokenizer.json" "config.json" "special_tokens_map.json" "tokenizer_config.json" `
                --local-dir $BgeDir
            if ($LASTEXITCODE -ne 0) { Log-Fail "huggingface-cli download failed" }
        } else {
            Log-Info "huggingface-cli not found, falling back to direct stream download..."
            $BaseUrl = "https://huggingface.co/BAAI/bge-m3/resolve/main"
            $Files = @(
                "onnx/model.onnx",
                "onnx/model.onnx_data",
                "onnx/Constant_7_attr__value",
                "tokenizer.json",
                "config.json",
                "special_tokens_map.json",
                "tokenizer_config.json"
            )

            foreach ($rel in $Files) {
                $target = Join-Path $BgeDir $rel
                $parent = Split-Path -Parent $target
                if (-not (Test-Path $parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }

                if ((Test-Path $target) -and (-not $Force)) {
                    Log-Info "Already downloaded: $rel"
                    continue
                }

                Log-Info "Downloading $rel..."
                $fileUrl = "$BaseUrl/$rel"
                if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
                    & curl.exe -L --progress-bar -o $target $fileUrl
                    if ($LASTEXITCODE -ne 0) { Log-Fail "Failed to download $fileUrl" }
                } else {
                    Invoke-WebRequest -Uri $fileUrl -OutFile $target
                }
            }
        }

        if ((Test-Path $OnnxModelFile) -and (Test-Path $OnnxDataFile)) {
            Log-Ok "BGE-M3 ONNX model ready at: $BgeDir"
        } else {
            Log-Fail "BGE-M3 download incomplete (missing model.onnx or model.onnx_data)"
        }
    }
}

Write-Host ""
Log-Ok "LongMemEval artifact provisioning complete."
Write-Host "To execute the reproducible benchmark suite, run:"
Write-Host "  cargo run --release -p tylluan-evals -- --suite longmemeval --limit $Limit" -ForegroundColor White
Write-Host "To execute with Jina cross-encoder reranker:"
Write-Host "  cargo run --release -p tylluan-evals -- --suite longmemeval --limit $Limit --reranker" -ForegroundColor White
