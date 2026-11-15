$ErrorActionPreference = 'Stop'

$install = Join-Path $PSScriptRoot '..\install.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
    (Resolve-Path -LiteralPath $install).Path,
    [ref]$tokens,
    [ref]$parseErrors
)
if ($parseErrors.Count -gt 0) {
    throw "install.ps1 has parser errors: $($parseErrors -join '; ')"
}

$functionAst = $ast.FindAll(
    {
        param($node)
        $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -eq 'Test-TylluanReleaseChecksums'
    },
    $true
) | Select-Object -First 1
if (-not $functionAst) {
    throw 'Test-TylluanReleaseChecksums was not found in install.ps1'
}
Invoke-Expression $functionAst.Extent.Text

$root = Join-Path ([System.IO.Path]::GetTempPath()) ("tylluan-checksum-test-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $root -Force | Out-Null
try {
    $nexus = Join-Path $root 'tylluan-nexus.exe'
    $cli = Join-Path $root 'tylluan-cli.exe'
    [System.IO.File]::WriteAllBytes($nexus, [System.Text.Encoding]::UTF8.GetBytes('nexus-payload'))
    [System.IO.File]::WriteAllBytes($cli, [System.Text.Encoding]::UTF8.GetBytes('cli-payload'))

    function Write-Checksums([string]$path, [string[]]$names) {
        $lines = foreach ($name in $names) {
            $file = Join-Path $root $name
            $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $file).Hash.ToLowerInvariant()
            "$hash  $name"
        }
        [System.IO.File]::WriteAllLines($path, [string[]]$lines)
    }

    $full = Join-Path $root 'full.txt'
    Write-Checksums $full @('tylluan-nexus.exe', 'tylluan-cli.exe')
    if (-not (Test-TylluanReleaseChecksums -ChecksPath $full -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe'))) {
        throw 'complete checksums.txt should pass'
    }
    Write-Host 'OK complete checksums'

    $empty = Join-Path $root 'empty.txt'
    New-Item -ItemType File -Path $empty -Force | Out-Null
    $emptyFailed = $false
    try {
        Test-TylluanReleaseChecksums -ChecksPath $empty -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
    } catch {
        $emptyFailed = $true
    }
    if (-not $emptyFailed) { throw 'empty checksums.txt must fail' }
    Write-Host 'OK empty checksums rejected'

    $partial = Join-Path $root 'partial.txt'
    Write-Checksums $partial @('tylluan-nexus.exe')
    $partialFailed = $false
    try {
        Test-TylluanReleaseChecksums -ChecksPath $partial -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
    } catch {
        $partialFailed = $true
    }
    if (-not $partialFailed) { throw 'partial checksums.txt must fail' }
    Write-Host 'OK partial checksums rejected'

    # P1 installer (TL order 2026-11-10): releases ship guilds/ inside the
    # tarball and checksums.txt covers `guilds/**` too. These cases pin the
    # relaxation: guild entries are verified, not ignored, and nothing outside
    # the binaries + the `guilds/` prefix (and nothing that could escape the
    # extraction root) may ever be accepted.
    $guildFiles = @('guilds/requirements.txt', 'guilds/core/bash.py', 'guilds/core/memory.py')
    foreach ($rel in $guildFiles) {
        $abs = Join-Path $root ($rel -replace '/', '\')
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $abs) | Out-Null
        [System.IO.File]::WriteAllBytes($abs, [System.Text.Encoding]::UTF8.GetBytes("payload-of-$rel"))
    }

    $withGuilds = Join-Path $root 'with-guilds.txt'
    Write-Checksums $withGuilds (@('tylluan-nexus.exe', 'tylluan-cli.exe') + $guildFiles)
    if (-not (Test-TylluanReleaseChecksums -ChecksPath $withGuilds -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe'))) {
        throw 'checksums.txt covering guilds/ should pass'
    }
    Write-Host 'OK guilds/ entries are verified'

    # A tampered guild file must fail: the whole point of covering guilds is
    # that the bytes on disk are the bytes that were built.
    $tampered = Join-Path $root 'guilds\core\bash.py'
    [System.IO.File]::WriteAllBytes($tampered, [System.Text.Encoding]::UTF8.GetBytes('tampered'))
    $tamperFailed = $false
    try {
        Test-TylluanReleaseChecksums -ChecksPath $withGuilds -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
    } catch {
        $tamperFailed = $true
    }
    if (-not $tamperFailed) { throw 'a modified guilds/ file must fail verification' }
    [System.IO.File]::WriteAllBytes($tampered, [System.Text.Encoding]::UTF8.GetBytes('payload-of-guilds/core/bash.py'))
    Write-Host 'OK tampered guilds/ file rejected'

    # Entries outside the whitelist stay rejected (no blanket "trust the list").
    $outside = Join-Path $root 'outside.txt'
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $nexus).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllLines($outside, [string[]]@(
        "$hash  tylluan-nexus.exe",
        "$hash  tylluan-cli.exe",
        "$hash  guilds-evil/requirements.txt",
        "$hash  setup.exe"
    ))
    $outsideFailed = $false
    try {
        Test-TylluanReleaseChecksums -ChecksPath $outside -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
    } catch {
        $outsideFailed = $true
    }
    if (-not $outsideFailed) { throw 'entries outside the guilds/ prefix must fail' }
    Write-Host 'OK entries outside guilds/ rejected'

    # Traversal / separator smuggling inside the allowed prefix must fail, and
    # the ONLY thing allowed to reject it is the name check: every entry below
    # resolves to a file that EXISTS with the CORRECT hash, so a case that
    # passes because of a missing file (or a hash mismatch) would be worthless.
    # Mutation check (TL T3907): removing the `..` guard must turn this red.
    $escapeFile = Join-Path $root 'escape.txt'
    [System.IO.File]::WriteAllBytes($escapeFile, [System.Text.Encoding]::UTF8.GetBytes('escape-payload'))
    $escapeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $escapeFile).Hash.ToLowerInvariant()
    $innerFile = Join-Path $root 'guilds\x.txt'
    [System.IO.File]::WriteAllBytes($innerFile, [System.Text.Encoding]::UTF8.GetBytes('inner-payload'))
    $innerHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $innerFile).Hash.ToLowerInvariant()
    # Ojo: `$hash` es el del nexus. Usar el suyo para cada binario, o el caso
    # "pasa" por un mismatch de hash en vez de por la comprobacion que prueba.
    $cliHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $cli).Hash.ToLowerInvariant()

    # Control positivo: el MISMO fichero con nombre limpio pasa, asi que el
    # rechazo de abajo no puede venir del hash ni de la existencia.
    $cleanCase = Join-Path $root 'clean-inner.txt'
    [System.IO.File]::WriteAllLines($cleanCase, [string[]]@(
        "$hash  tylluan-nexus.exe",
        "$cliHash  tylluan-cli.exe",
        "$innerHash  guilds/x.txt"
    ))
    if (-not (Test-TylluanReleaseChecksums -ChecksPath $cleanCase -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe'))) {
        throw 'fixture roto: guilds/x.txt con hash correcto debe pasar'
    }
    Write-Host 'OK control positivo guilds/x.txt (hash correcto, fichero existe)'

    $evilCases = @(
        @{ name = 'guilds/../escape.txt'; hash = $escapeHash },
        @{ name = 'guilds/..\escape.txt'; hash = $escapeHash },
        @{ name = 'guilds/./x.txt';       hash = $innerHash },
        @{ name = 'guilds//x.txt';        hash = $innerHash }
    )
    foreach ($evil in $evilCases) {
        $evilPath = Join-Path $root 'evil.txt'
        [System.IO.File]::WriteAllLines($evilPath, [string[]]@(
            "$hash  tylluan-nexus.exe",
            "$cliHash  tylluan-cli.exe",
            "$($evil.hash)  $($evil.name)"
        ))
        $evilFailed = $false
        $reason = ''
        try {
            Test-TylluanReleaseChecksums -ChecksPath $evilPath -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
        } catch {
            $evilFailed = $true
            $reason = "$_"
        }
        if (-not $evilFailed) { throw "unsafe entry must fail: $($evil.name)" }
        if ($reason -notmatch 'unsafe entry') {
            throw "unsafe entry rejected for the WRONG reason ($($evil.name)): $reason"
        }
    }
    Write-Host 'OK path traversal inside guilds/ rejected by the name check (not by a missing file)'

    # Duplicates stay rejected even when both are inside the prefix.
    $dup = Join-Path $root 'duplicate.txt'
    $reqHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $root 'guilds\requirements.txt')).Hash.ToLowerInvariant()
    [System.IO.File]::WriteAllLines($dup, [string[]]@(
        "$hash  tylluan-nexus.exe",
        "$hash  tylluan-cli.exe",
        "$reqHash  guilds/requirements.txt",
        "$reqHash  guilds/requirements.txt"
    ))
    $dupFailed = $false
    try {
        Test-TylluanReleaseChecksums -ChecksPath $dup -ExpectedFiles @('tylluan-nexus.exe', 'tylluan-cli.exe') | Out-Null
    } catch {
        $dupFailed = $true
    }
    if (-not $dupFailed) { throw 'duplicate entry must fail' }
    Write-Host 'OK duplicate guilds/ entry rejected'
    # T4130 (TL): la comprobacion de nombres de install.ps1 debe aceptar TODO
    # nombre real que el repo empaca en guilds/. Se toma la regex del propio
    # install.ps1 (no una copia que pueda divergir) y se aplica a la salida real
    # de `git ls-files -- guilds`.
    $installText = Get-Content -Raw -LiteralPath $install
    # Ojo: `-notmatch '...'` aparece antes para el checksum de 64 hex, así que
    # se busca la línea de la regex de nombres (la que contiene A-Za-z0-9).
    $nameLine = ($installText -split "`n" | Where-Object { $_ -match 'A-Za-z0-9\._\-' } | Select-Object -First 1)
    if (-not $nameLine) {
        throw 'no se encontro la linea de la regex de nombres en install.ps1'
    }
    $regexMatch = [regex]::Match($nameLine, "-notmatch '([^']+)'")
    if (-not $regexMatch.Success) {
        throw 'no se encontro la regex de nombres en install.ps1'
    }
    $nameRegex = [regex]::new($regexMatch.Groups[1].Value)
    $repoRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
    $realFiles = @(& git -C $repoRoot ls-files -- guilds)
    if ($realFiles.Count -eq 0) {
        throw 'git ls-files no devolvio ficheros de guilds/'
    }
    $rechazados = @($realFiles | Where-Object { -not $nameRegex.IsMatch($_) })
    if ($rechazados.Count -gt 0) {
        throw ("la regex de nombres rechaza ficheros REALES de guilds/: {0}" -f ($rechazados -join ', '))
    }
    Write-Host ("OK la regex de nombres acepta los {0} ficheros reales de guilds/" -f $realFiles.Count)

} finally {
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host 'All install.ps1 checksum tests passed.'
