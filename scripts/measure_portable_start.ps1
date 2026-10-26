# Measure cold starts of the published `portable` profile (BM25 only, no model downloads).
#
# Usage (Windows PowerShell, any directory):
#   .\scripts\measure_portable_start.ps1 -Archive <tylluan-x86_64-pc-windows-msvc.tar.gz> -Runs 4 -Port 47012
#
# For each run it extracts the release into a fresh scratch directory, writes the portable
# config, starts the kernel on the given port with its own data directory, and records:
#   - seconds until GET /health answers 200 ("healthy")
#   - working set (resident memory) of the kernel process after 10 s idle
#   - non-loopback TCP connections and UDP sockets owned by the kernel after those 10 s
#
# Safety: pick a free port. The script refuses 47004 (the default and usual production port),
# puts the test port in the config, passes it on the command line, and stops the kernel it
# started if it ever listens on 47004. It stops only the PID it started.
#
# NOTE: do NOT use `tylluan-cli install` here. It spawns a kernel detached, with the default
# port, and that is exactly what this script must avoid.
param(
    [Parameter(Mandatory = $true)][string]$Archive,
    [int]$Runs = 4,
    [int]$Port = 47012,
    [string]$Scratch = (Join-Path $env:TEMP "tylluan-measure")
)
$ErrorActionPreference = "Stop"
if ($Port -eq 47004) { throw "Refusing to use the production port 47004." }
if (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue) { throw "Port $Port is already in use." }

# Same settings `tylluan-cli install --profile portable` writes in v0.17.1 (BM25 only, reranker off).
$configTemplate = @'
[nexus]
host = "127.0.0.1"
port = __PORT__
dev_mode = false
transport = ["http", "sse"]

[memory]
db_path = "./data/tylluan.db"
embedding_model = "none"
vector_dimensions = 0

[silva]
reranker_enabled = false

[timeouts]
system_guild_ms = 15_000
analysis_guild_ms = 60_000
heavy_guild_ms = 180_000
tool_call_secs = 3_600
handshake_secs = 120
mcp_heartbeat_ms = 8_000
lazy_timeout_secs = 300

[guilds.core]
always_on = ["bash", "memory", "filesystem"]
'@

$results = @()
for ($i = 1; $i -le $Runs; $i++) {
    $dir = Join-Path $Scratch "run$i"
    if (Test-Path $dir) { Remove-Item -Recurse -Force $dir }
    New-Item -ItemType Directory -Path $dir | Out-Null
    # Windows' own bsdtar by full path: a Git Bash `tar` earlier on PATH reads `C:` as a remote host.
    & (Join-Path $env:SystemRoot "System32/tar.exe") -xzf $Archive -C $dir
    $configTemplate.Replace("__PORT__", "$Port") | Set-Content -Encoding UTF8 (Join-Path $dir "tylluan.toml")

    $env:TYLLUAN_DATA_DIR = Join-Path $dir "data"
    New-Item -ItemType Directory -Path $env:TYLLUAN_DATA_DIR -Force | Out-Null
    $started = Get-Date
    $proc = Start-Process -FilePath (Join-Path $dir "tylluan-nexus.exe") -ArgumentList "--port $Port" `
        -WorkingDirectory $dir -PassThru -WindowStyle Hidden `
        -RedirectStandardOutput (Join-Path $dir "out.log") -RedirectStandardError (Join-Path $dir "err.log")

    $healthy = $null
    for ($t = 0; $t -lt 600; $t++) {
        Start-Sleep -Milliseconds 100
        if (Get-NetTCPConnection -OwningProcess $proc.Id -LocalPort 47004 -State Listen -ErrorAction SilentlyContinue) {
            Stop-Process -Id $proc.Id -Force
            throw "Test kernel (PID $($proc.Id)) bound the production port 47004; stopped it."
        }
        try {
            $r = Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 -Uri "http://127.0.0.1:$Port/health"
            if ($r.StatusCode -eq 200) { $healthy = ((Get-Date) - $started).TotalSeconds; break }
        } catch { }
    }
    Start-Sleep -Seconds 10
    $proc.Refresh()
    $rssMb = [math]::Round($proc.WorkingSet64 / 1MB, 1)
    $outbound = @(Get-NetTCPConnection -OwningProcess $proc.Id -ErrorAction SilentlyContinue |
        Where-Object { $_.State -ne "Listen" -and $_.RemoteAddress -notin @("127.0.0.1", "::1", "0.0.0.0", "::") })
    $udp = @(Get-NetUDPEndpoint -OwningProcess $proc.Id -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalAddress -notin @("127.0.0.1", "::1") })
    $results += [pscustomobject]@{
        run = $i
        healthy_seconds = if ($healthy) { [math]::Round($healthy, 2) } else { $null }
        resident_mb_after_10s = $rssMb
        outbound_tcp_connections = $outbound.Count
        non_loopback_udp_sockets = $udp.Count
    }
    Stop-Process -Id $proc.Id -Force
    Remove-Item Env:TYLLUAN_DATA_DIR
    Start-Sleep -Seconds 2
}
$results | Format-Table -AutoSize
$results | ConvertTo-Json | Set-Content -Encoding UTF8 (Join-Path $Scratch "results.json")
Write-Host "Results: $(Join-Path $Scratch 'results.json')"
