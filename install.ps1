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

Write-Step "Detecting latest release..."
if ($Version -eq "latest") {
    try {
        $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -ErrorAction Stop
        $Version = $Release.tag_name -replace '^v'
    } catch {
        Write-Err "Could not detect latest version: $_"
    }
}

$Archive = "tylluan-${Target}.tar.gz"
$Url = "https://github.com/$Repo/releases/download/v$Version/$Archive"

Write-Step "Downloading Tylluan v$Version ($Target)..."
New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
$OutFile = Join-Path $BinDir $Archive
try {
    Invoke-WebRequest -Uri $Url -OutFile $OutFile -ErrorAction Stop
} catch {
    Write-Err "Download failed: $_"
}

Write-Step "Extracting..."
try {
    tar -xzf $OutFile -C $BinDir --strip-components=1
} catch {
    Write-Err "Extraction failed. Ensure tar is available (Windows 10 1803+ or install 7zip)."
}
Remove-Item $OutFile -Force

Write-OK "Tylluan v$Version installed to $BinDir"

$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$PathEntries = $UserPath -split ';'
if ($PathEntries -notcontains $BinDir) {
    $NewPath = "$UserPath;$BinDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    $env:PATH = "$env:PATH;$BinDir"
    Write-OK "Added $BinDir to PATH"
    Write-Host "   → Open a NEW terminal for PATH to take effect in other apps." -ForegroundColor Yellow
}

Write-Host ""
Write-Host "   ┌──────────────────────────────────────────────────────┐" -ForegroundColor White
Write-Host "   │  tylluan-cli start        # Start the kernel         │" -ForegroundColor White
Write-Host "   │  curl -s 127.0.0.1:3030/health  # Verify it's up     │" -ForegroundColor White
Write-Host "   └──────────────────────────────────────────────────────┘" -ForegroundColor White
Write-Host ""
Write-Host "   📄 Auth token (auto-generated on first boot):" -ForegroundColor White
Write-Host "       .tylluan-token     (in kernel working directory)" -ForegroundColor White
Write-Host ""
Write-Host "   🔗 Connect your MCP client with this config:" -ForegroundColor White
Write-Host '       { "mcpServers": { "tylluan": { "type": "sse", "url": "http://127.0.0.1:3030/sse" } } }' -ForegroundColor White
