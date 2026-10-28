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
} finally {
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host 'All install.ps1 checksum tests passed.'
