#!/usr/bin/env pwsh
# Tylluan Windows Installer
# Usage: irm https://raw.githubusercontent.com/Forja-orca/tylluan/main/install.ps1 | iex

param(
    [string]$Version = "latest",
    [switch]$NoStart,
    # P1 installer: skip creating <TylluanHome>\.venv and installing the Python
    # guild dependencies there (they are installed by default; the operator's
    # system Python is never touched).
    [switch]$SkipGuildDeps,
    # P1 (TL T3907): installing guild dependencies WITHOUT the hash lock is
    # opt-in. Default is fail-closed: no requirements-hashes.txt in the release
    # means no pip run, because "install these ranges from PyPI" is a
    # supply-chain gamble nobody was asked to take.
    [switch]$AllowUnhashedDeps,
    # D192: instala una instancia con nombre en vez de `default`.
    [string]$Instance = "default",
    # D192: reinstalar encima de una instancia YA declarada es explicito.
    [switch]$Upgrade,
    # H-20261122-INSTALLPS1-WINDOWS-LIMPIO: si se pasa, NO se intenta instalar el
    # VC++ Redistributable ni Python (solo se comprueban y se dice como hacerlo).
    [switch]$SkipPrereqs
)

# Execution Policy guard - detect Restricted/AllSigned before anything else
$execPolicy = Get-ExecutionPolicy -Scope CurrentUser 2>$null
if ($execPolicy -in 'Restricted', 'AllSigned') {
    Write-Host "WARNING: Your PowerShell execution policy is '$execPolicy'." -ForegroundColor Yellow
    Write-Host "This script may not run. To fix, open PowerShell as Admin and run:" -ForegroundColor Yellow
    Write-Host "  Set-ExecutionPolicy -Scope CurrentUser RemoteSigned" -ForegroundColor Cyan
    Write-Host ""
}

$Repo = "Forja-orca/tylluan"
$TylluanHome = "$env:USERPROFILE\.tylluan"
$BinDir = "$TylluanHome\bin"
$DataDir = $TylluanHome
# D192: registro de instancias con nombre (el mismo que lee `tylluan --instance`).
$RegistryFile = if ($env:TYLLUAN_INSTANCES_FILE) { $env:TYLLUAN_INSTANCES_FILE } else { Join-Path $TylluanHome "instances.toml" }

function Write-Step($Text) { Write-Host "Tylluan $Text" -ForegroundColor Cyan }
function Write-OK($Text)   { Write-Host "OK $Text" -ForegroundColor Green }
# H-20261122-INSTALLPS1-WINDOWS-LIMPIO (B3): con `irm ... | iex` no existe
# $PSScriptRoot. Dentro de iex, `exit` no sale del script: cierra la SESION de
# PowerShell del usuario, sin dejar ver el motivo (medido en la VM: la ventana
# se cerro a los 18 s con solo la linea de inicio).
$script:EnIex = -not $PSScriptRoot

function Write-Err($Text) {
    Write-Host "FAIL $Text" -ForegroundColor Red
    # Dentro de iex no se puede `exit`: se lanza el error para que el mensaje quede
    # en pantalla. Fuera de iex, `exit 1` es correcto y lo esperan los scripts de CI.
    if ($script:EnIex) { throw "FAIL $Text" }
    exit 1
}


# -- Prerrequisitos de Windows (H-20261122-INSTALLPS1-WINDOWS-LIMPIO) --------
#
# (B1) Los .exe se enlazan contra el CRT de Visual C++ en modo dinamico: sin
# vcruntime140.dll Windows NO arranca el proceso (0xC0000135 STATUS_DLL_NOT_FOUND,
# medido en la VM: `tylluan-cli install` devolvia -1073741515). Se comprueba ANTES
# de usar el CLI y, si falta, se instala el redistributable con winget.
#
# (B2) Sin un Python real >= 3.12 los 45 guilds Python no pueden crear su .venv y el
# supervisor los reintenta en bucle (readiness=failed). El alias de la Microsoft
# Store (WindowsApps\python.exe) NO es un Python: se rechaza a proposito.
function Test-VCRuntime {
    foreach ($Nombre in @('vcruntime140.dll', 'vcruntime140_1.dll')) {
        $Ruta = Join-Path $env:SystemRoot ("System32" + [IO.Path]::DirectorySeparatorChar + $Nombre)
        if (Test-Path -LiteralPath $Ruta -PathType Leaf) { return $true }
    }
    return $false
}

function Get-WingetPath {
    $Cmd = Get-Command winget -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Cmd) { return $Cmd.Source }
    return $null
}

# Devuelve $true si tras intentarlo hay runtime. NUNCA finge: si no puede, deja
# el comando exacto en pantalla para que lo ejecute la persona.
function Install-VCRuntime {
    $Winget = Get-WingetPath
    if (-not $Winget) {
        Write-Host "  No puedo instalar el runtime de Visual C++: no hay winget." -ForegroundColor Yellow
        Write-Host "  Instalalo a mano (pide permisos de administrador):" -ForegroundColor Yellow
        Write-Host "    https://aka.ms/vs/17/release/vc_redist.x64.exe" -ForegroundColor Cyan
        return $false
    }
    Write-Host "  Falta el runtime de Visual C++ (vcruntime140.dll): instalando con winget..." -ForegroundColor Yellow
    & $Winget install --id Microsoft.VCRedist.2015+.x64 --exact --silent `
        --accept-package-agreements --accept-source-agreements --disable-interactivity 2>&1 | Out-Null
    $Codigo = $LASTEXITCODE
    # 0 = instalado, -1978335189 = ya estaba instalado. Cualquier otro codigo es fallo real.
    if ((Test-VCRuntime)) {
        Write-OK "Visual C++ runtime presente"
        return $true
    }
    Write-Host "  winget termino con codigo $Codigo y el runtime sigue sin aparecer." -ForegroundColor Yellow
    Write-Host "  Instalalo a mano: https://aka.ms/vs/17/release/vc_redist.x64.exe" -ForegroundColor Yellow
    return $false
}

# Ruta del alias de la Microsoft Store: parece python pero no lo es.
function Test-EsAliasDeLaTienda([string]$Ruta) {
    return ($Ruta -match '\\WindowsApps\\')
}

# Devuelve @{Path=...; Version=...} de un Python REAL >= 3.12, o $null.
function Resolve-RealPython {
    $Candidatos = New-Object System.Collections.Generic.List[string]
    # 1) uno que hubiera instalado winget en el perfil del usuario (no entra en PATH
    #    de la sesion actual hasta abrir otra consola)
    foreach ($V in @('312', '313')) {
        $Candidatos.Add((Join-Path $env:LOCALAPPDATA "Programs\Python\Python$V\python.exe"))
    }
    # 2) lo que haya en PATH, saltando el alias de la Tienda
    foreach ($Nombre in @('python', 'python3')) {
        $Cmd = Get-Command $Nombre -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($Cmd) { $Candidatos.Add($Cmd.Source) }
    }
    foreach ($Ruta in $Candidatos) {
        if (-not (Test-Path -LiteralPath $Ruta -PathType Leaf)) { continue }
        if (Test-EsAliasDeLaTienda $Ruta) { continue }
        $Salida = (& $Ruta --version 2>&1 | Out-String).Trim()
        if ($Salida -match '^Python (\d+)\.(\d+)') {
            $Mayor = [int]$Matches[1]; $Menor = [int]$Matches[2]
            if ($Mayor -gt 3 -or ($Mayor -eq 3 -and $Menor -ge 12)) {
                return @{ Path = $Ruta; Version = $Salida }
            }
        }
    }
    return $null
}

function Install-PythonWinget {
    $Winget = Get-WingetPath
    if (-not $Winget) { return $false }
    Write-Host "  Falta Python 3.12+: instalando con winget (en tu perfil, sin tocar el sistema)..." -ForegroundColor Yellow
    & $Winget install --id Python.Python.3.12 --exact --scope user --silent `
        --accept-package-agreements --accept-source-agreements --disable-interactivity 2>&1 | Out-Null
    return [bool](Resolve-RealPython)
}

# D-65: strict checksum gate. Throws when the file is empty, has an unexpected

# entry, omits an expected binary, or any SHA-256 does not match. Kept as a
# standalone function so tests can execute it without running the installer.
#
# P1 installer (TL order 2026-11-10): releases now ship guilds/ inside the
# tarball and checksums.txt covers `guilds/**` as well as the two binaries.
# Those entries are accepted ONLY through $AllowedPrefix (never "whatever the
# file happens to list"), and every accepted name still has to be a relative,
# traversal-free path under the extraction root: the entry list comes from the
# same archive it verifies, so it must never be able to point outside of it.
function Test-TylluanReleaseChecksums {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ChecksPath,
        [Parameter(Mandatory = $true)][string[]]$ExpectedFiles,
        [string]$AllowedPrefix = "guilds/"
    )

    if (-not (Test-Path -LiteralPath $ChecksPath -PathType Leaf)) {
        throw "checksums.txt not found at $ChecksPath"
    }

    $root = Split-Path -Parent $ChecksPath
    $seen = @{}
    $entries = 0
    foreach ($Line in [System.IO.File]::ReadAllLines($ChecksPath)) {
        $trimmed = $Line.Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed)) { continue }
        if ($trimmed -notmatch '^([0-9A-Fa-f]{64})\s+\*?(.+)$') {
            throw "Malformed checksum line: '$trimmed'"
        }
        $entries++
        $want = $Matches[1].ToLowerInvariant()
        $name = $Matches[2].Trim()
        $isExpected = $ExpectedFiles -contains $name
        $isShippedExtra = $false
        if ($AllowedPrefix -and $name.StartsWith($AllowedPrefix, [System.StringComparison]::Ordinal)) {
            # Relative path, '/' separated, no drive letter, no '.', no '..'
            # segment, no backslashes (a Windows separator inside a name would
            # let a corrupted manifest address an unrelated file).
            if ($name -notmatch '^[A-Za-z0-9._-]+(/[A-Za-z0-9._-]+)+$' -or $name -match '(^|/)\.\.?(/|$)') {
                throw "checksums.txt contains an unsafe entry: $name"
            }
            $isShippedExtra = $true
        }
        if (-not ($isExpected -or $isShippedExtra)) {
            throw "checksums.txt contains an unexpected entry: $name"
        }
        if ($seen.ContainsKey($name)) {
            throw "checksums.txt contains a duplicate entry: $name"
        }
        $file = Join-Path $root $name
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) {
            throw "checksums.txt names a missing file: $name"
        }
        $got = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant()
        if ($got -ne $want) {
            throw "SHA-256 mismatch for ${name}: expected $want, got $got"
        }
        $seen[$name] = $true
    }

    if ($entries -eq 0) {
        throw "checksums.txt is empty; refusing to install unverified binaries"
    }
    foreach ($expected in $ExpectedFiles) {
        if (-not $seen.ContainsKey($expected)) {
            throw "checksums.txt has no entry for required binary: $expected"
        }
    }
    return $true
}

# PROCESSOR_ARCHITEW6432 tells the real OS arch even under 32-bit PowerShell on 64-bit Windows
$Arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
switch ($Arch) {
    "AMD64"  { $Target = "x86_64-pc-windows-msvc" }
    "ARM64"  { Write-Err "Windows ARM64 is not supported by the published releases yet. Use an x86_64 Windows machine or build from source: docs/getting-started/QUICKSTART.md" }
    default { Write-Err "Unsupported architecture: $Arch. Tylluan releases support x86_64 on Windows." }
}

Write-Host "=== Tylluan Installer ===" -ForegroundColor White
Write-Step "Detected: Windows ($Target)"

$Archive = "tylluan-${Target}.tar.gz"

function Is-GitHubUrl($TestUrl) {
    if (-not $TestUrl) { return $false }
    try {
        $uri = [System.Uri]$TestUrl
        return ($uri.Host -eq "github.com" -or $uri.Host.EndsWith(".github.com") -or $uri.Host.EndsWith(".githubusercontent.com"))
    } catch {
        return $false
    }
}

$GitHubHeaders = @{}
if ($env:GITHUB_TOKEN) {
    $GitHubHeaders["Authorization"] = "Bearer $env:GITHUB_TOKEN"
}

if ($env:TYLLUAN_DOWNLOAD_URL) {
    $Url = $env:TYLLUAN_DOWNLOAD_URL
    Write-Step "Using explicit download URL: $Url"
    Write-Step "Fetching Tylluan ($Target)..."
} elseif ($env:TYLLUAN_VERSION) {
    $Version = $env:TYLLUAN_VERSION -replace '^v'
    $Url = "https://github.com/$Repo/releases/download/v$Version/$Archive"
    Write-Step "Using explicit version: v$Version ($Target)..."
    Write-Step "Downloading Tylluan v$Version ($Target)..."
} elseif ($Version -ne "latest") {
    $Version = $Version -replace '^v'
    $Url = "https://github.com/$Repo/releases/download/v$Version/$Archive"
    Write-Step "Using specified version: v$Version ($Target)..."
    Write-Step "Downloading Tylluan v$Version ($Target)..."
} else {
    Write-Step "Detecting latest release..."
    # Resolve latest release tag without api.github.com to avoid anonymous API rate limits (T1128).
    # Inspect the redirect Location header from HEAD https://github.com/<repo>/releases/latest
    $ResolvedVersion = $null
    $LatestUrl = "https://github.com/$Repo/releases/latest"
    try {
        $Req = [System.Net.HttpWebRequest]::Create($LatestUrl)
        $Req.Method = "HEAD"
        $Req.AllowAutoRedirect = $false
        if ($env:GITHUB_TOKEN) {
            $Req.Headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
        }
        $Resp = $Req.GetResponse()
        $Loc = $Resp.GetResponseHeader("Location")
        $Resp.Close()
        if ($Loc -match '/tag/v?([^/]+)$') {
            $ResolvedVersion = $Matches[1]
        }
    } catch {
        # Fallback via Invoke-WebRequest HEAD
        try {
            $HeadParams = @{
                Uri = $LatestUrl
                Method = "Head"
                UseBasicParsing = $true
                ErrorAction = "Stop"
            }
            if ($GitHubHeaders.Count -gt 0) { $HeadParams["Headers"] = $GitHubHeaders }
            $HeadResp = Invoke-WebRequest @HeadParams
            $FinalUri = $HeadResp.BaseResponse.ResponseUri.AbsoluteUri
            if ($FinalUri -match '/tag/v?([^/]+)$') {
                $ResolvedVersion = $Matches[1]
            }
        } catch {}
    }

    if ($ResolvedVersion) {
        $Version = $ResolvedVersion
        $Url = "https://github.com/$Repo/releases/download/v$Version/$Archive"
        Write-Step "Downloading Tylluan v$Version ($Target)..."
    } else {
        # Direct download redirect fallback
        $Url = "https://github.com/$Repo/releases/latest/download/$Archive"
        Write-Step "Downloading Tylluan latest ($Target)..."
    }
}

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
$OutFile = Join-Path $BinDir $Archive
try {
    if (Test-Path $Url) {
        Copy-Item -Path $Url -Destination $OutFile -Force
    } else {
        $WebParams = @{
            Uri = $Url
            OutFile = $OutFile
            UseBasicParsing = $true
            ErrorAction = "Stop"
        }
        if ($GitHubHeaders.Count -gt 0 -and (Is-GitHubUrl $Url)) {
            $WebParams["Headers"] = $GitHubHeaders
        }
        Invoke-WebRequest @WebParams
    }
} catch {
    Write-Err "Download failed: $_"
}

Write-Step "Extracting and verifying..."
$Stage = Join-Path ([System.IO.Path]::GetTempPath()) ("tylluan-stage-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $Stage -Force | Out-Null
try {
    tar -xzf $OutFile -C $Stage --strip-components=1
} catch {
    Remove-Item $Stage -Recurse -Force -ErrorAction SilentlyContinue
    Write-Err "Extraction failed. Ensure tar is available (Windows 10 1803+ or install 7zip)."
}
$Checks = Join-Path $Stage "checksums.txt"
try {
    Test-TylluanReleaseChecksums -ChecksPath $Checks -ExpectedFiles @("tylluan-nexus.exe", "tylluan-cli.exe") | Out-Null
} catch {
    Remove-Item $Stage -Recurse -Force -ErrorAction SilentlyContinue
    Write-Err "Release checksum verification failed: $_"
}
Copy-Item -Path (Join-Path $Stage "*") -Destination $BinDir -Recurse -Force
Remove-Item $Stage -Recurse -Force -ErrorAction SilentlyContinue
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

# -- Instancias con nombre (D192) ---------------------------------------
# `default` es la de siempre (%USERPROFILE%\.tylluan). Cualquier otro
# nombre vive en .tylluan\instances\<nombre>, con su propio DATA_DIR y su
# propio puerto, y queda declarado en $RegistryFile. El registro se escribe en
# modo append-only: instalar una instancia NUNCA reescribe los bloques
# existentes, y en particular nunca toca el de `default`.
#
# D194/T4437: el puerto por defecto NO se escribe aqui — se pregunta al CLI, que
# es el dueno del valor (crates/tylluan-cli/src/instances.rs, DEFAULT_PORT). Este
# script solo lo copia al registro y usa $Port en todo lo demas (health, PID,
# `start`). Asi el numero vive en un solo sitio.
# Se toma la ULTIMA linea (si el CLI escribe un aviso por stdout antes del numero)
# y se valida el rango: un puerto de 6 cifras no es un puerto.
# -- Prerrequisitos (B1): ANTES de ejecutar el CLI -------------------------
# Si falta el runtime de Visual C++, `tylluan-cli` ni siquiera arranca y el
# instalador moria con un codigo numerico sin explicacion.
Write-Step "Comprobando el runtime de Visual C++..."
if (-not (Test-VCRuntime)) {
    $Instalado = $false
    if (-not $SkipPrereqs) { $Instalado = Install-VCRuntime }
    if (-not $Instalado) {
        Write-Err ("Falta el runtime de Visual C++ (vcruntime140.dll) y sin el los binarios de Tylluan no arrancan. " +
            "Instalalo con:  winget install --id Microsoft.VCRedist.2015+.x64 --exact    " +
            "o descarga https://aka.ms/vs/17/release/vc_redist.x64.exe, y vuelve a ejecutar este instalador.")
    }
} else {
    Write-OK "Visual C++ runtime presente"
}

$CliExe = Join-Path $BinDir "tylluan-cli.exe"

# B1 (verificacion): si Windows no puede cargar el .exe, $LASTEXITCODE queda en
# -1073741515 (0xC0000135). Se comprueba aqui, con el motivo escrito, en vez de
# dejar que el fallo aparezca como "exit code -1073741515" tres pasos despues.
& $CliExe --version 2>$null | Out-Null
if ($LASTEXITCODE -eq -1073741515) {
    Write-Err ("tylluan-cli.exe no arranca: falta una DLL (0xC0000135). Suele ser el runtime de Visual C++: " +
        "winget install --id Microsoft.VCRedist.2015+.x64 --exact  o  https://aka.ms/vs/17/release/vc_redist.x64.exe")
}
$DefaultPortLines = @(& $CliExe --print-default-port 2>$null | ForEach-Object { "$_" })

$DefaultInstancePort = if ($DefaultPortLines.Count -gt 0) { $DefaultPortLines[-1].Trim() } else { "" }

# D195 (1): el Install Smoke instala con el CLI **publicado**, que puede ser
# anterior a las instancias. Se le pregunta; si no sabe, se instala en modo
# clasico (default, sin --port), que es lo que entiende.
if ($DefaultInstancePort -match '^\d+$' -and [int]$DefaultInstancePort -ge 1 -and [int]$DefaultInstancePort -le 65535) {
    $CliHasInstances = $true
} else {
    $CliHasInstances = $false
    # FALLBACK (2): valor historico, SOLO para un CLI publicado anterior a D194
    # (v0.17.2 y anteriores), que no conoce --print-default-port.
    $DefaultInstancePort = 47004
}

if ($Instance -notmatch '^[A-Za-z0-9_-]+$') {
    Write-Err "Nombre de instancia invalido: '$Instance' (usa letras, numeros, '-' o '_')"
}

# D195: con un CLI que no conoce instancias, pedir una con nombre es un error
# claro y temprano (no un `start --instance` que reventaria al final).
if (-not $CliHasInstances -and $Instance -ne "default") {
    Write-Err "El binario instalado ($CliExe) no soporta instancias con nombre: no conoce '--instance'. Instala sin -Instance, o usa un release posterior."
}

New-Item -ItemType Directory -Force -Path $TylluanHome | Out-Null
if (-not (Test-Path -LiteralPath $RegistryFile -PathType Leaf)) {
    New-Item -ItemType File -Force -Path $RegistryFile | Out-Null
}

function Get-InstanceBlock([string]$Name) {
    $out = New-Object System.Collections.Generic.List[string]
    $inside = $false
    foreach ($line in (Get-Content -LiteralPath $RegistryFile -ErrorAction SilentlyContinue)) {
        $trim = $line.Trim()
        if ($trim -match '^\[instances\.([A-Za-z0-9_-]+)\]$') {
            $inside = ($Matches[1] -eq $Name)
            continue
        }
        if ($trim.StartsWith('[')) { $inside = $false; continue }
        if ($inside) { $out.Add($line) }
    }
    return ($out -join "`n")
}

function Test-InstanceDeclared([string]$Name) {
    $b = Get-InstanceBlock $Name
    return -not [string]::IsNullOrWhiteSpace($b)
}

function Get-InstancePort([string]$Name) {
    $b = Get-InstanceBlock $Name
    if ($b -match '(?m)^\s*port\s*=\s*(\d+)') { return [int]$Matches[1] }
    return 0
}

function Add-RegistryBlock([string]$Text) {
    $existing = ""
    if (Test-Path -LiteralPath $RegistryFile) {
        $existing = Get-Content -LiteralPath $RegistryFile -Raw -ErrorAction SilentlyContinue
    }
    # Un TOML valido puede no acabar en salto de linea: sin esto el bloque nuevo
    # se pegaria a la ultima linea del anterior.
    if ($existing -and -not $existing.EndsWith("`n")) { Add-Content -LiteralPath $RegistryFile -Value "" }
    Add-Content -LiteralPath $RegistryFile -Value $Text
}

function Test-PortFree([int]$Port) {
    $listener = $null
    try {
        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, $Port)
        $listener.Start()
        return $true
    } catch { return $false }
    finally { if ($listener) { try { $listener.Stop() } catch { } } }
}

function Get-NextFreePort {
    $declared = @()
    foreach ($line in (Get-Content -LiteralPath $RegistryFile -ErrorAction SilentlyContinue)) {
        if ($line -match '^\s*port\s*=\s*(\d+)') { $declared += [int]$Matches[1] }
    }
    for ($candidate = 47005; $candidate -lt 47100; $candidate++) {
        if (($declared -notcontains $candidate) -and (Test-PortFree $candidate)) { return $candidate }
    }
    return 0
}

# Sin `[instances.default]`, un registro que solo declare otra instancia hace que
# `tylluan` (sin -Instance) falle cerrado. Se deja SIEMPRE declarado.
if (-not (Test-InstanceDeclared "default")) {
    Add-RegistryBlock "`n[instances.default]`nport = $DefaultInstancePort"
    Write-Host "Declarada [instances.default] port = $DefaultInstancePort en $RegistryFile" -ForegroundColor DarkGray
}

if ($Instance -eq "default") {
    $DataDir = $TylluanHome
    $Port = Get-InstancePort "default"
    if (-not $Port) { $Port = $DefaultInstancePort }
    Write-Host "Instancia: default ($DataDir, puerto $Port)" -ForegroundColor Cyan
} else {
    $DataDir = Join-Path $TylluanHome "instances\$Instance"
    if (Test-InstanceDeclared $Instance) {
        if (-not $Upgrade) {
            Write-Err "La instancia '$Instance' ya existe en $RegistryFile. Reinstalar encima es explicito: repite con -Upgrade."
        }
        $Port = Get-InstancePort $Instance
        if (-not $Port) { Write-Err "La instancia '$Instance' esta declarada sin puerto en $RegistryFile." }
        Write-Host "Instancia: $Instance (-Upgrade) -> $DataDir, puerto $Port" -ForegroundColor Cyan
    } else {
        if ($Upgrade) {
            Write-Err "No existe la instancia '$Instance': -Upgrade solo reinstala una ya declarada."
        }
        $Port = Get-NextFreePort
        if (-not $Port) { Write-Err "No hay puerto libre a partir de 47005." }
        # TOML: las barras invertidas son secuencias de escape, asi que la ruta se
        # escribe con '/' (PathBuf la acepta igual en Windows).
        $DataDirToml = $DataDir.Replace('\', '/')
        Add-RegistryBlock "`n[instances.$Instance]`nport = $Port`ndata_dir = `"$DataDirToml/data`""
        Write-OK "Instancia '$Instance' declarada en $RegistryFile (puerto $Port)."
    }
}
Write-Host ""

# -- Python guilds (the 45 Python tool plugins) --------------------------
# Policy: NEVER pip-install into the user's system Python from an installer
# (it mutates the user's environment without consent, and PEP 668 blocks it on
# major Linux distros anyway). The operator's Python only *creates* a venv;
# dependencies go into <TylluanHome>\.venv, which the kernel already prefers
# over the system interpreter (registry/guild_process.rs, find_python()).
#
# D192: una sola .venv para todas las instancias, a proposito. `find_python()`
# busca `.venv` primero en la cadena del **ejecutable** (guild_process.rs:2391)
# y `<home>\.tylluan\.venv` es el segundo eslabon de `<home>\.tylluan\bin\
# tylluan-nexus.exe`: si existiera, ganaria siempre a la de la instancia.
Write-Host "Python guilds (45 tools):" -ForegroundColor White
$VenvDir = Join-Path $TylluanHome ".venv"
$VenvPy = Join-Path $VenvDir "Scripts\python.exe"
$GuildReq = Join-Path $BinDir "guilds"
if (-not (Test-Path (Join-Path $GuildReq "requirements.txt"))) { $GuildReq = "guilds" }
$ReqHashes = Join-Path $GuildReq "requirements-hashes.txt"
$ReqPlain = Join-Path $GuildReq "requirements.txt"
$GuildImport = 'from mcp.server.fastmcp import FastMCP; import psutil'

function Write-GuildDepsManualHint {
    if (Test-Path $ReqHashes) {
        Write-Host "  Install them later with:" -ForegroundColor Yellow
        Write-Host "    `"$VenvPy`" -m pip install --require-hashes -r `"$ReqHashes`"" -ForegroundColor Yellow
    } else {
        Write-Host "  SIN VERIFICACION DE HASHES: este release no trae requirements-hashes.txt." -ForegroundColor Red
        Write-Host "  No hay un comando verificado que darte: vuelve a descargar el release" -ForegroundColor Red
        Write-Host "  (deberia incluir guilds/requirements-hashes.txt) o revisa la cadena de suministro." -ForegroundColor Red
    }
}

if ($SkipGuildDeps) {
    Write-Host "  Skipped (-SkipGuildDeps)." -ForegroundColor Yellow
    Write-GuildDepsManualHint
} else {
    # B2: solo un Python REAL >= 3.12 sirve. El alias de la Microsoft Store
    # (WindowsApps\python.exe) responde a `python` y no es un Python: con el, el
    # `.venv` no se crea y el kernel queda en readiness=failed con los guilds en
    # bucle de reinicio (medido en la VM).
    $PyReal = Resolve-RealPython
    if (-not $PyReal -and -not $SkipPrereqs) {
        if (Install-PythonWinget) { $PyReal = Resolve-RealPython }
    }
    $GuildPy = if ($PyReal) { $PyReal.Path } else { $null }
    if ($PyReal) {
        Write-Host "  Python: $($PyReal.Version) ($($PyReal.Path))" -ForegroundColor White
    }
    $VenvReady = $false
    if (Test-Path -LiteralPath $VenvPy -PathType Leaf) {
        & $VenvPy -c $GuildImport 2>$null | Out-Null
        $VenvReady = ($LASTEXITCODE -eq 0)
    }
    if ($VenvReady) {
        Write-Host "  guild deps OK in $VenvDir" -ForegroundColor Green
    } elseif (-not $GuildPy) {
        # Sin Python real NO se finge que la instalacion esta completa: el kernel
        # arranca, pero los 45 guilds Python no van a poder correr.
        Write-Host "  AVISO: no hay un Python 3.12+ real en esta maquina." -ForegroundColor Yellow
        Write-Host "  Los guilds Python NO van a funcionar (el kernel y la memoria MCP si)." -ForegroundColor Yellow
        Write-Host "  Instalalo con UNA de estas:" -ForegroundColor Yellow
        Write-Host "    winget install --id Python.Python.3.12 --exact --scope user" -ForegroundColor Cyan
        Write-Host "    https://www.python.org/downloads/release/python-3120/" -ForegroundColor Cyan
        Write-Host "  (El alias de la Microsoft Store no sirve: no crea .venv.)" -ForegroundColor Yellow
        Write-Host "  Despues, repite este instalador para completar los guilds." -ForegroundColor Yellow
        if (Test-Path -LiteralPath $VenvPy -PathType Leaf) { Write-GuildDepsManualHint }
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
            Write-Host "  $Ver - guilds need Python 3.12+. Upgrade: https://www.python.org/downloads/" -ForegroundColor Yellow
        } else {
            if (-not (Test-Path -LiteralPath $VenvPy -PathType Leaf)) {
                Write-Host "  Creating $VenvDir with $Ver ..." -ForegroundColor White
                try {
                    & $GuildPy -m venv $VenvDir
                } catch {
                    $LASTEXITCODE = 1
                }
            }
            if (-not (Test-Path -LiteralPath $VenvPy -PathType Leaf)) {
                Write-Host "  Could not create $VenvDir." -ForegroundColor Yellow
                Write-GuildDepsManualHint
            } else {
                Write-Host "  Installing guild dependencies into $VenvDir (one time, this can take a few minutes)..." -ForegroundColor White
                if (Test-Path $ReqHashes) {
                    & $VenvPy -m pip install --require-hashes --disable-pip-version-check -r $ReqHashes
                } elseif ($AllowUnhashedDeps) {
                    Write-Host "  AVISO: SIN VERIFICACION DE HASHES (el release no trae requirements-hashes.txt)." -ForegroundColor Red
                    Write-Host "  Sigues con -AllowUnhashedDeps: se instalan rangos sin comprobar." -ForegroundColor Red
                    & $VenvPy -m pip install --disable-pip-version-check -r $ReqPlain
                } else {
                    Write-Host "  SIN VERIFICACION DE HASHES: el release no trae requirements-hashes.txt." -ForegroundColor Red
                    Write-Host "  NO instalo dependencias Python sin hashes (falla cerrado)." -ForegroundColor Red
                    Write-Host "  Si aceptas el riesgo, repite con -AllowUnhashedDeps." -ForegroundColor Red
                }
                & $VenvPy -c $GuildImport 2>$null | Out-Null
                if ($LASTEXITCODE -eq 0) {
                    Write-Host "  guild deps OK in $VenvDir" -ForegroundColor Green
                } else {
                    Write-Host "  Guild dependencies are NOT installed (pip did not finish)." -ForegroundColor Yellow
                    Write-GuildDepsManualHint
                }
            }
        }
    }
}
Write-Host ""
# D195 (1): argumentos de `--port` para `install`. Vacio para `default` (su
# puerto ya es el que escribe el config) y solo si el CLI lo anuncia.
$InstallPortArg = @()
if ($Instance -ne "default") {
    $InstallHelp = (& "$BinDir\tylluan-cli" install --help 2>&1 | Out-String)
    if ($InstallHelp -match '--port') {
        $InstallPortArg = @('--port', "$Port")
    } elseif ($CliHasInstances) {
        Write-Err "Este binario no acepta 'install --port': no puedo fijar el puerto de la instancia '$Instance'."
    }
}

# `install --profile portable` writes tylluan.toml AND boots the kernel
# itself (chdir's to the config dir first so the kernel finds it). If a
# config already exists it refuses without --force, so start directly --
# from the config dir, otherwise the kernel would not discover the file.
$ConfigPath = Join-Path $DataDir "tylluan.toml"
$DisplayVer = if ($Version -eq "latest" -or -not $Version) { "latest" } else { "v$Version" }
$KernelDataDir = if ($env:TYLLUAN_DATA_DIR) { $env:TYLLUAN_DATA_DIR } else { Join-Path $DataDir "data" }
$PidFile = Join-Path $KernelDataDir "tylluan-nexus.pid"
$OldPid = $null
if (Test-Path $PidFile) {
    $OldRaw = Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($OldRaw -match '^\d+$') { $OldPid = [int]$OldRaw }
}
if ($NoStart) {
    if (Test-Path $ConfigPath) {
        Write-Step "Existing tylluan.toml found - keeping it. Kernel not started (--no-start)."
    } else {
        Write-Step "Installing portable profile (writes tylluan.toml, no start)..."
        # P1 installer: --dir fija el destino. Sin el, el CLI resuelve HOME
        # antes que USERPROFILE y podria escribir el tylluan.toml en otro home
        # que el $BinDir/$DataDir de este instalador (y el .venv quedaria
        # huerfano: los guilds caerian al Python del sistema).
        # --port (D192): el config generado tiene que declarar el puerto de ESTA
        # instancia. D195: solo si el CLI lo anuncia; para `default` no hace falta.
        & "$BinDir\tylluan-cli" install --profile portable --no-start --dir "$DataDir" @InstallPortArg
        if ($LASTEXITCODE -ne 0) {
            Write-Err "tylluan-cli install failed (exit code $LASTEXITCODE)"
        }
    }
    Write-OK "Tylluan $DisplayVer installed to $BinDir (kernel not started, --no-start)"
} else {
    if (Test-Path $ConfigPath) {
        Write-Step "Existing tylluan.toml found - keeping it. Starting kernel..."
        try {
            $StartArgs = if ($Instance -eq "default") { "start" } else { "start --instance $Instance" }
            $null = Start-Process -FilePath "$BinDir\tylluan-cli" -ArgumentList $StartArgs -WorkingDirectory $DataDir -NoNewWindow -PassThru -ErrorAction Stop
        } catch {
            Write-Err "Failed to start Tylluan: $_"
        }
    } else {
        Write-Step "Installing portable profile (writes tylluan.toml + starts kernel)..."
        if ($Instance -eq "default") {
            & "$BinDir\tylluan-cli" install --profile portable --dir "$DataDir" @InstallPortArg
            if ($LASTEXITCODE -ne 0) {
                Write-Err "tylluan-cli install failed (exit code $LASTEXITCODE)"
            }
        } else {
            # En modo instancia el config se escribe sin arrancar y el arranque va
            # por `start --instance`: es el unico camino que le pasa al kernel su
            # TYLLUAN_DATA_DIR (si no, usaria .\data-port-<puerto>).
            & "$BinDir\tylluan-cli" install --profile portable --no-start --dir "$DataDir" @InstallPortArg
            if ($LASTEXITCODE -ne 0) {
                Write-Err "tylluan-cli install failed (exit code $LASTEXITCODE)"
            }
            $null = Start-Process -FilePath "$BinDir\tylluan-cli" -ArgumentList "start --instance $Instance" -WorkingDirectory $DataDir -NoNewWindow -PassThru -ErrorAction Stop
        }
    }

    $KernelPid = $null
    for ($i = 0; $i -lt 30; $i++) {
        if (Test-Path $PidFile) {
            $RawPid = Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($RawPid -match '^\d+$') {
                $KernelPid = [int]$RawPid
                if (Get-Process -Id $KernelPid -ErrorAction SilentlyContinue) { break }
            }
        }
        Start-Sleep -Seconds 1
    }
    if (-not $KernelPid) {
        Write-Err "Kernel did not write a live PID file at $PidFile. Check $DataDir\logs\"
    }
    if ($OldPid -and $KernelPid -eq $OldPid) {
        Write-Err "PID file still points to the previous kernel (PID $OldPid); refusing to report success."
    }

    Write-Step "Waiting for kernel to be ready..."
    $Ready = $false
    $ErrorCount = 0
    for ($i = 0; $i -lt 30; $i++) {
        try {
            # D212 (DEC-20261118-18): se lee `readiness` del JSON, no solo el codigo HTTP.
            # `degraded` sirve (200) y se avisa; `failed` tambien llega como 200; si no mejora se avisa al final.
            $Response = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/health" -UseBasicParsing -ErrorAction Stop
            $Health = $null
            try { $Health = $Response.Content | ConvertFrom-Json } catch { $Health = $null }
            $Readiness = if ($Health -and $Health.readiness) { $Health.readiness } else { $null }
            # Compatibilidad: las releases anteriores a D212 no emiten `readiness`.
            # Si no viene, vale el 200 (que es lo que siempre significo "kernel arriba").
            $SinReadiness = [string]::IsNullOrEmpty($Readiness)
            # `/health` responde 200 siempre que el kernel esta vivo; la salud real va en el JSON.
            # `ok` se acepta al instante; `degraded`/`failed` se esperan y, si siguen asi en el
            # ultimo intento, se AVISA y se termina: el kernel SI esta arriba.
            $Responde = (-not $SinReadiness) -or ($Response.StatusCode -eq 200)
            if ($Readiness -eq "ok" -or ($SinReadiness -and $Response.StatusCode -eq 200) -or
                (($i -eq 29) -and $Responde)) {
                if (-not (Get-Process -Id $KernelPid -ErrorAction SilentlyContinue)) {
                    Write-Err "Port $Port answers but PID $KernelPid is not alive: another kernel is occupying it."
                }
                if ($Readiness -ne "ok" -and -not $SinReadiness) {
                    $Down = if ($null -ne $Health.guilds_down) { @($Health.guilds_down) -join ", " } else { "?" }
                    Write-Host "WARN kernel up but readiness=$Readiness (always-on guilds down: $Down); memory and recall work, guild tools may not." -ForegroundColor Yellow
                }
                Write-OK "Tylluan $DisplayVer is running at http://127.0.0.1:$Port (PID $KernelPid, readiness=$Readiness)"
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

    $Status = & "$BinDir\tylluan-cli" status 2>&1
    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "  Binary:    $BinDir\tylluan-nexus.exe" -ForegroundColor Cyan
        Write-Host "  CLI:       $BinDir\tylluan-cli.exe" -ForegroundColor Cyan
        Write-Host "  Config:    $ConfigPath" -ForegroundColor Cyan
        Write-Host "  Logs:      $DataDir\logs\" -ForegroundColor Cyan
    } else {
        Write-Warning "'tylluan-cli status' returned error (try in a new terminal): $Status"
    }
}

Write-Host ""
Write-Host "Connect your MCP client:" -ForegroundColor White
Write-Host ""
Write-Host "  Claude Desktop (~/.claude/claude_desktop_config.json):" -ForegroundColor White
Write-Host '  {'
Write-Host '    "mcpServers": {'
Write-Host '      "tylluan": { "type": "sse",'
Write-Host ("        `"url`": `"http://127.0.0.1:$Port/sse`" }")
Write-Host '    }'
Write-Host '  }'
Write-Host ""
Write-Host "  Claude Code:" -ForegroundColor White
Write-Host "    /mcp add tylluan sse http://127.0.0.1:$Port/sse"
Write-Host ""
Write-Host "  Cursor:" -ForegroundColor White
Write-Host "    Add MCP server: http://127.0.0.1:$Port/sse"
Write-Host ""
Write-Host "  curl (verify):" -ForegroundColor White
Write-Host "    curl http://127.0.0.1:$Port/health"
Write-Host ""
Write-Host "For better retrieval (BGE-M3):" -ForegroundColor Yellow
Write-Host "  tylluan-cli download-models"
Write-Host ""

# A handled "guild deps missing" probe above leaves $LASTEXITCODE = 1. The install itself
# succeeded, so do not leak that code to callers (CI wrappers treat it as failure).
$global:LASTEXITCODE = 0
