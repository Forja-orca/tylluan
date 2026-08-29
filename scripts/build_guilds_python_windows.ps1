# =============================================================================
# Tylluan — Portable Python Environment Builder for Guilds (Windows x86_64)
# =============================================================================
# Uses Astral uv and python-build-standalone to produce a self-contained,
# portable Python 3.12 environment with all guild dependencies preinstalled.
#
# Idempotent: can be executed repeatedly without re-downloading existing files.
# =============================================================================

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = (Resolve-Path "$ScriptDir\..").Path

$TargetDir = Join-Path $RepoRoot "guilds-python\windows-x86_64"
$RequirementsFile = Join-Path $RepoRoot "guilds\requirements.txt"

Write-Host "=== [1/4] Checking prerequisites ==="
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Host "ERROR: 'uv' is not installed. Install it with:" -ForegroundColor Red
    Write-Host "  powershell -ExecutionPolicy ByPass -c 'irm https://astral.sh/uv/install.ps1 | iex'" -ForegroundColor Yellow
    exit 1
}
Write-Host "OK: Using uv version: $(uv --version)"

Write-Host "=== [2/4] Installing standalone Python 3.12 ==="
if (-not (Test-Path $TargetDir)) {
    New-Item -ItemType Directory -Path $TargetDir -Force | Out-Null
}
uv python install --install-dir $TargetDir 3.12

# Locate the installed Python binary
$PythonBin = Get-ChildItem -Path $TargetDir -Recurse -Filter "python.exe" |
    Where-Object { $_.DirectoryName -notmatch "venv" } |
    Select-Object -First 1 -ExpandProperty FullName

if (-not $PythonBin -or -not (Test-Path $PythonBin)) {
    Write-Host "ERROR: Failed to locate installed python.exe in $TargetDir" -ForegroundColor Red
    exit 1
}

Write-Host "OK: Python standalone binary located at: $PythonBin"
& $PythonBin --version

Write-Host "=== [3/4] Installing guild dependencies from requirements.txt ==="
if (-not (Test-Path $RequirementsFile)) {
    Write-Host "ERROR: Requirements file not found: $RequirementsFile" -ForegroundColor Red
    exit 1
}

# Use --break-system-packages because python-build-standalone marks itself EXTERNALLY-MANAGED
$env:PYTHONIOENCODING = "utf-8"
uv pip install --break-system-packages -r $RequirementsFile --python $PythonBin

Write-Host "=== [4/4] Verifying portable environment ==="
$env:PYTHONNOUSERSITE = "1"
& $PythonBin -c "import mcp, fastmcp, psutil; print('OK: Imported core dependencies successfully in portable environment')"

Write-Host "=== Guilds portable Python build complete! ==="
Write-Host "Target directory: $TargetDir"
Write-Host "Python binary:    $PythonBin"
