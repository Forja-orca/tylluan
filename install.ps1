#!/usr/bin/env pwsh
# Tylluan Windows Installer — v0.12.0
# Usage: irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex

param(
    [string]$Version = "latest"
)

$Repo = "Forja-orca/tylluan"
$BinDir = "$env:USERPROFILE\.tylluan\bin"
$DataDir = "$env:USERPROFILE\.tylluan"

function Write-Step($Text) { Write-Host "Tylluan $Text" -ForegroundColor Cyan }
function Write-OK($Text)   { Write-Host "OK $Text" -ForegroundColor Green }
function Write-Err($Text)  { Write-Host "FAIL $Text" -ForegroundColor Red; exit 1 }

$Arch = $env:PROCESSOR_ARCHITECTURE
switch ($Arch) {
    "AMD64"  { $Target = "x86_64-pc-windows-msvc" }
    "ARM64"  { $Target = "aarch64-pc-windows-msvc" }
    default { Write-Err "Unsupported architecture: $Arch. Tylluan supports x86_64 and ARM64 on Windows." }
}

Write-Host "=== Tylluan Installer - v0.12.0 ===" -ForegroundColor White
Write-Step "Detected: Windows ($Target)"

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

$UserPath = [Environment]::GetEnvironmentVariable("PATH", "User")
$PathEntries = $UserPath -split ';'
if ($PathEntries -notcontains $BinDir) {
    $NewPath = "$UserPath;$BinDir"
    [Environment]::SetEnvironmentVariable("PATH", $NewPath, "User")
    $env:PATH = "$env:PATH;$BinDir"
    Write-OK "Added $BinDir to PATH"
    Write-Host "   Open a NEW terminal for PATH to take effect in other apps." -ForegroundColor Yellow
}

Write-Step "Starting Tylluan..."
$Process = Start-Process -FilePath "$BinDir\tylluan-cli" -ArgumentList "start --profile portable" -NoNewWindow -PassThru

Write-Step "Waiting for kernel to be ready..."
$Ready = $false
for ($i = 0; $i -lt 30; $i++) {
    try {
        $Response = Invoke-WebRequest -Uri "http://127.0.0.1:3030/health" -UseBasicParsing -ErrorAction Stop
        if ($Response.StatusCode -eq 200) {
            $Ready = $true
            break
        }
    } catch {
        # still starting
    }
    Write-Host "." -NoNewline
    Start-Sleep -Seconds 1
}
Write-Host ""
if (-not $Ready) {
    Write-Err "Kernel did not start within 30 seconds. Check $DataDir\logs\"
}

Write-OK "Tylluan is running at http://127.0.0.1:3030"
Write-Host ""

Write-Host "Connect your MCP client:" -ForegroundColor White
Write-Host ""
Write-Host "  Claude Desktop (~/.claude/claude_desktop_config.json):" -ForegroundColor White
Write-Host '  {'
Write-Host '    "mcpServers": {'
Write-Host '      "tylluan": { "type": "sse",'
Write-Host '        "url": "http://127.0.0.1:3030/sse" }'
Write-Host '    }'
Write-Host '  }'
Write-Host ""
Write-Host "  Claude Code:" -ForegroundColor White
Write-Host '    /mcp add tylluan sse http://127.0.0.1:3030/sse'
Write-Host ""
Write-Host "  Cursor:" -ForegroundColor White
Write-Host "    Add MCP server: http://127.0.0.1:3030/sse"
Write-Host ""
Write-Host "  curl (verify):" -ForegroundColor White
Write-Host "    curl http://127.0.0.1:3030/health"
Write-Host ""
Write-Host "For better retrieval (BGE-M3):" -ForegroundColor Yellow
Write-Host "  tylluan-cli download-models"
Write-Host ""
Write-OK "Tylluan v$Version installed to $BinDir"
