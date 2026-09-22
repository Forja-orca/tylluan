# Agent Loop — Deep (2026-09-21, decision de Jose)
# UNA sesion controlada por intervalo, nunca un enjambre. Invocado por la
# scheduled task "Tylluan-Deep-Loop" cada 10 minutos.
#
# Fix 2026-09-21 (verificado por auditoria cruzada, LastTaskResult=1):
# - El gotcha de PS 5.1: $ErrorActionPreference="Stop" + 2>&1 convierte el
#   stderr de comandos nativos en error terminante -> el script moria en la
#   invocacion de opencode. Eliminado: la invocacion va por Start-Process
#   con redireccion a LOG, nunca por el pipeline.
# - WorkingDirectory explicito a la raiz del repo (la tarea arranca en
#   System32 si no se fija) -> opencode run tiene contexto de proyecto.
# - Log persistente obligatorio: toda corrida deja evidencia en
#   E:\tylluan\data\logs\deep_loop.log (rotacion basica a 200KB).
# - Exit code explicito: 0 siempre que la invocacion exista (el resultado
#   del agente se registra en el log), exit 1 solo si el runner mismo falla.

$ErrorActionPreference = "Stop"
$lockFile = Join-Path $env:TEMP "tylluan_deep_loop.lock"
$repoRoot = "E:\tylluan"
$logDir = Join-Path $repoRoot "data\logs"
$logFile = Join-Path $logDir "deep_loop.log"
$opencode = "C:\Users\FoRJa\AppData\Roaming\npm\opencode.cmd"
$runId = Get-Date -Format "yyyy-MM-dd HH:mm:ss"

function Write-Log([string]$msg) {
    if (-not (Test-Path $logDir)) { New-Item -ItemType Directory -Path $logDir -Force | Out-Null }
    $line = "[$runId] $msg"
    Add-Content -Path $logFile -Value $line -Encoding UTF8
    Write-Output $line
}

# Anti-solapamiento.
if (Test-Path $lockFile) {
    $age = (Get-Date) - (Get-Item $lockFile).LastWriteTime
    if ($age.TotalMinutes -lt 30) {
        Write-Log "corrida anterior activa, saliendo"
        exit 0
    }
    Remove-Item $lockFile -Force
}
New-Item -ItemType File -Path $lockFile -Force | Out-Null

try {
    Set-Location $repoRoot

    $loopPrompt = @"
Eres Deep, agente backend Rust + guilds Python de la flota Tylluan.
Este es tu ciclo periodico de revision.

1. Revisa el canal 'general' de Coloquio (y el canal 'tareas' si existe)
   buscando trabajo asignado a @deep o al equipo que te corresponda.
2. Si hay una tarea para ti: trabajala (investiga, implementa, verifica)
   y reporta el resultado en Coloquio siguiendo la cadena establecida.
3. Si no hay nada para ti: responde unicamente 'IDLE' y termina.
4. Nunca modifiques trabajo de otros agentes. Nunca ejecutes nada sin
   verificacion. Si una tarea necesita rebuild del kernel o decision de
   Jose, dejala anotada en Coloquio y termina.
"@

    Write-Log "check iniciado"

    $agentOut = Join-Path $logDir "deep_loop_run_$([DateTime]::Now.ToString('HHmmss')).out"
    $agentErr = Join-Path $logDir "deep_loop_run_$([DateTime]::Now.ToString('HHmmss')).err"

    # Invocacion sin el gotcha de stderr y con techo de tiempo estricto (WORK_PROTOCOL.md §7).
    $proc = Start-Process -FilePath $opencode -ArgumentList @("run", $loopPrompt, "--log-level", "ERROR") `
        -WorkingDirectory $repoRoot -RedirectStandardOutput $agentOut -RedirectStandardError $agentErr `
        -NoNewWindow -PassThru

    $timeoutMs = 900000 # 15 minutos
    $finished = $proc.WaitForExit($timeoutMs)
    if (-not $finished) {
        Write-Log "TIMEOUT: opencode run excedio 15 min, forzando terminacion de PID $($proc.Id) y arbol de procesos"
        & taskkill.exe /F /T /PID $proc.Id 2>&1 | Out-Null
        Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
        $code = 124
    } else {
        try { [void]$proc.WaitForExit() } catch {}
        try { $proc.Refresh() } catch {}
        $code = if ($proc.HasExited -and $null -ne $proc.ExitCode) { [int]$proc.ExitCode } else { 0 }
    }

    Write-Log "opencode run terminado (exit=$code)"
    if ($code -ne 0) {
        $errTail = ""
        if (Test-Path $agentErr) {
            $errTail = (Get-Content $agentErr -Tail 3 -ErrorAction SilentlyContinue) -join " | "
        }
        Write-Log "ERROR opencode exit=$code stderr: $errTail"
    }

    # Rotacion basica del log principal.
    if ((Get-Item $logFile -ErrorAction SilentlyContinue).Length -gt 200KB) {
        Remove-Item "$logFile.old" -Force -ErrorAction SilentlyContinue
        Rename-Item $logFile "$logFile.old"
    }

    Write-Log "check completado"
    exit 0
} catch {
    $err = $_.Exception.Message
    Write-Log "RUNNER FAIL: $err"
    exit 1
} finally {
    Remove-Item $lockFile -Force -ErrorAction SilentlyContinue
}