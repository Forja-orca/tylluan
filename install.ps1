#!/usr/bin/env pwsh
# Tylluan Windows Installer
# Usage: irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex

param(
    [string]$Version = "latest"
)

$Repo = "Forja-orca/tylluan"
$BinDir = "$env:USERPROFILE\.tylluan\bin"
$Target = "x86_64-pc-windows-msvc"

function Write-Step($Text) { Write-Host "🔹 $Text" -ForegroundColor Cyan }
function Write-OK($Text)   { Write-Host "✅ $Text" -ForegroundColor Green }
function Write-Err($Text)  { Write-Host "❌ $Text" -ForegroundColor Red; exit 1 }

# --- detect latest version ---
Write-Step "Detecting latest release..."
if ($Version -eq "latest") {
    $ApiUrl = "https://api.github.com/repos/$Repo/releases/latest"
    try {
        $Release = Invoke-RestMethod -Uri $ApiUrl -ErrorAction Stop
        $Version = $Release.tag_name -replace '^v'
    } catch {
        Write-Err "Could not detect latest version: $_"
    }
}

$Archive = "tylluan-${Target}.tar.gz"
$Url = "https://github.com/$Repo/releases/download/v$Version/$Archive"

# --- download ---
Write-Step "Downloading Tylluan v$Version ($Target)..."
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
$OutFile = Join-Path $BinDir $Archive
try {
    Invoke-WebRequest -Uri $Url -OutFile $OutFile -ErrorAction Stop
} catch {
    Write-Err "Download failed: $_"
}

# --- extract ---
Write-Step "Extracting..."
try {
    # tar is available in Windows 10 1803+ and Windows 11
    tar -xzf $OutFile -C $BinDir --strip-components=1
} catch {
    Write-Err "Extraction failed. Ensure tar is available (Windows 10 1803+ or install 7zip)."
}
Remove-Item $OutFile -Force

Write-OK "Tylluan v$Version installed to $BinDir"

# --- PATH setup ---
$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($UserPath -notlike "*$BinDir*") {
    $NewPath = "$UserPath;$BinDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    $env:PATH = "$env:PATH;$BinDir"
    Write-OK "Added $BinDir to user PATH"
}

Write-Host ""
Write-Host "   Run:  tylluan-cli start" -ForegroundColor White
Write-Host "   Then: curl http://127.0.0.1:3030/health" -ForegroundColor White
