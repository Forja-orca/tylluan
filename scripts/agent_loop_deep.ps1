# Agent Loop — Deep (2026-09-21, decision de Jose)
# El patron correcto para trabajo autonomo 24/7: UNA sesion controlada por
# intervalo, nunca un enjambre. Este runner es invocado por la scheduled
# task "Tylluan-Deep-Loop" cada N minutos; revisa Coloquio, trabaja si hay
# tarea para Deep, y termina (el intervalo es el "sueno").
#
# - Lock anti-solapamiento: si una corrida anterior sigue activa, esta sale
#   sin hacer nada (nunca dos instancias del loop a la vez).
# - El prompt del loop instruye: revisar el canal de tareas/Coloquio,
#   trabajar si hay tarea para deep, reportar en Coloquio, o responder IDLE.

$ErrorActionPreference = "Stop"
$lockFile = Join-Path $env:TEMP "tylluan_deep_loop.lock"
$opencode = "C:\Users\FoRJa\AppData\Roaming\npm\opencode.cmd"

# Anti-solapamiento: lock exclusivo de corta vida.
if (Test-Path $lockFile) {
    $age = (Get-Date) - (Get-Item $lockFile).LastWriteTime
    if ($age.TotalMinutes -lt 30) {
        Write-Output "[deep-loop] corrida anterior activa, saliendo (intervalo hara el siguiente check)"
        exit 0
    }
    Remove-Item $lockFile -Force
}
New-Item -ItemType File -Path $lockFile -Force | Out-Null
try {
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
    Write-Output "[deep-loop] check iniciado $(Get-Date -Format 'HH:mm:ss')"
    & $opencode run $loopPrompt --log-level ERROR 2>&1 | Out-Host
    Write-Output "[deep-loop] check completado $(Get-Date -Format 'HH:mm:ss')"
} finally {
    Remove-Item $lockFile -Force -ErrorAction SilentlyContinue
}