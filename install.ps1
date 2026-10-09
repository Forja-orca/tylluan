#!/usr/bin/env pwsh
# Tylluan Windows Installer
# Usage: irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex

param(
    [string]$Version = "latest"
)

# Execution Policy guard — detect Restricted/AllSigned before anything else
$execPolicy = Get-ExecutionPolicy -Scope CurrentUser 2>$null
if ($execPolicy -in 'Restricted', 'AllSigned') {
    Write-Host "WARNING: Your PowerShell execution policy is '$execPolicy'." -ForegroundColor Yellow
    Write-Host "This script may not run. To fix, open PowerShell as Admin and run:" -ForegroundColor Yellow
    Write-Host "  Set-ExecutionPolicy -Scope CurrentUser RemoteSigned" -ForegroundColor Cyan
    Write-Host ""
}

$Repo = "Forja-orca/tylluan"
$BinDir = "$env:USERPROFILE\.tylluan\bin"
$DataDir = "$env:USERPROFILE\.tylluan"

function Write-Step($Text) { Write-Host "Tylluan $Text" -ForegroundColor Cyan }
function Write-OK($Text)   { Write-Host "OK $Text" -ForegroundColor Green }
function Write-Err($Text)  { Write-Host "FAIL $Text" -ForegroundColor Red; exit 1 }

# PROCESSOR_ARCHITEW6432 tells the real OS arch even under 32-bit PowerShell on 64-bit Windows
$Arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    "AMD64"  { $Target = "x86_64-pc-windows-msvc" }
    "ARM64"  { $Target = "aarch64-pc-windows-msvc" }
    default { Write-Err "Unsupported architecture: $Arch. Tylluan supports x86_64 and ARM64 on Windows." }
}

Write-Host "=== Tylluan Installer ===" -ForegroundColor White
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

# `install --profile portable` writes tylluan.toml AND boots the kernel
# itself (chdir's to the config dir first so the kernel finds it). If a
# config already exists it refuses without --force, so start directly --
# from the config dir, otherwise the kernel would not discover the file.
$ConfigPath = Join-Path $DataDir "tylluan.toml"
if (Test-Path $ConfigPath) {
    Write-Step "Existing tylluan.toml found — keeping it. Starting kernel..."
    try {
        $null = Start-Process -FilePath "$BinDir\tylluan-cli" -ArgumentList "start" -WorkingDirectory $DataDir -NoNewWindow -PassThru -ErrorAction Stop
    } catch {
        Write-Err "Failed to start Tylluan: $_"
    }
} else {
    Write-Step "Installing portable profile (writes tylluan.toml + starts kernel)..."
    & "$BinDir\tylluan-cli" install --profile portable
    if ($LASTEXITCODE -ne 0) {
        Write-Err "tylluan-cli install failed (exit code $LASTEXITCODE)"
    }
}

Write-Step "Waiting for kernel to be ready..."
$Ready = $false
$ErrorCount = 0
for ($i = 0; $i -lt 30; $i++) {
    try {
        $Response = Invoke-WebRequest -Uri "http://127.0.0.1:47004/health" -UseBasicParsing -ErrorAction Stop
        if ($Response.StatusCode -eq 200) {
            $Ready = $true
            break
        }
    } catch {
        $ErrorCount++
    }
    Write-Host "." -NoNewline
    Start-Sleep -Seconds 1
}
Write-Host ""
if (-not $Ready) {
    Write-Err "Kernel did not start within 30 seconds. Check $DataDir\logs\"
}

# Verify the binary responds
$Status = & "$BinDir\tylluan-cli" status 2>&1
if ($LASTEXITCODE -eq 0) {
    Write-OK "Tylluan v$Version is running at http://127.0.0.1:47004"
    Write-Host ""
    Write-Host "  Binary:    $BinDir\tylluan-nexus.exe" -ForegroundColor Cyan
    Write-Host "  CLI:       $BinDir\tylluan-cli.exe" -ForegroundColor Cyan
    Write-Host "  Config:    $ConfigPath" -ForegroundColor Cyan
    Write-Host "  Logs:      $DataDir\logs\" -ForegroundColor Cyan
} else {
    Write-Warning "'tylluan-cli status' returned error (try in a new terminal): $Status"
    Write-OK "Tylluan v$Version installed to $BinDir (kernel may need manual start)"
}

Write-Host ""
Write-Host "Connect your MCP client:" -ForegroundColor White
Write-Host ""
Write-Host "  Claude Desktop (~/.claude/claude_desktop_config.json):" -ForegroundColor White
Write-Host '  {'
Write-Host '    "mcpServers": {'
Write-Host '      "tylluan": { "type": "sse",'
Write-Host '        "url": "http://127.0.0.1:47004/sse" }'
Write-Host '    }'
Write-Host '  }'
Write-Host ""
Write-Host "  Claude Code:" -ForegroundColor White
Write-Host '    /mcp add tylluan sse http://127.0.0.1:47004/sse'
Write-Host ""
Write-Host "  Cursor:" -ForegroundColor White
Write-Host "    Add MCP server: http://127.0.0.1:47004/sse"
Write-Host ""
Write-Host "  curl (verify):" -ForegroundColor White
Write-Host "    curl http://127.0.0.1:47004/health"
Write-Host ""
Write-Host "For better retrieval (BGE-M3):" -ForegroundColor Yellow
Write-Host "  tylluan-cli download-models"
Write-Host ""

# ── Python guilds (optional — the 46 Python tool plugins) ──────────────
# Detection only: NEVER pip-install into the user's system Python from an
# installer (it mutates the user's environment without consent, and PEP 668
# blocks it on major Linux distros anyway). Print the exact command instead.
Write-Host "Python guilds (46 tools, optional):" -ForegroundColor White
$GuildPy = $null
foreach ($Cand in @("python", "python3")) {
    $Cmd = Get-Command $Cand -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Cmd) { $GuildPy = $Cmd.Source; break }
}
if (-not $GuildPy) {
    Write-Host "  Python 3.12+ not found — Python guilds won't run (kernel + MCP memory unaffected)." -ForegroundColor Yellow
    Write-Host "  Install Python 3.12+: https://www.python.org/downloads/" -ForegroundColor Yellow
} else {
    $VerOk = $false
    $Ver = ""
    try {
        & $GuildPy -c "import sys; sys.exit(0 if sys.version_info >= (3,12) else 1)" 2>$null | Out-Null
        $VerOk = ($LASTEXITCODE -eq 0)
        $Ver = ((& $GuildPy --version 2>&1) | Out-String).Trim()
    } catch {
        $Ver = ""
    }
    if (-not $VerOk) {
        if ($Ver -notmatch '^Python \d') { $Ver = "Python not usable" }
        Write-Host "  $Ver — guilds need Python 3.12+. Upgrade: https://www.python.org/downloads/" -ForegroundColor Yellow
    } else {
        & $GuildPy -c "import mcp, fastmcp, psutil" 2>$null | Out-Null
        if ($LASTEXITCODE -eq 0) {
            Write-Host "  $Ver + guild deps OK" -ForegroundColor Green
        } else {
            $ReqDir = "https://raw.githubusercontent.com/Forja-orca/tylluan/main/guilds"
            if (Test-Path "guilds\requirements.txt") { $ReqDir = "guilds" }
            Write-Host "  $Ver found, guild deps missing. Install them with:" -ForegroundColor Yellow
            Write-Host "    $GuildPy -m pip install -r $ReqDir/requirements.txt" -ForegroundColor Yellow
        }
    }
}
Write-Host ""
