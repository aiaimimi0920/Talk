param(
    [string]$ProductPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Test-TalkProductRelease {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$ProductPath)

    $resolvedProductPath = [System.IO.Path]::GetFullPath($ProductPath)
    if (-not (Test-Path -LiteralPath $resolvedProductPath -PathType Container)) {
        throw "Talk product release directory does not exist: $resolvedProductPath"
    }

    $files = @(Get-ChildItem -LiteralPath $resolvedProductPath -Recurse -File)
    $relativeFiles = @($files | ForEach-Object {
        $_.FullName.Substring($resolvedProductPath.Length).TrimStart('\\')
    } | Sort-Object)
    $expectedFiles = @('Talk.exe', 'talk.toml')
    if (($relativeFiles -join '|') -ne ($expectedFiles -join '|')) {
        throw "Talk product release must contain exactly Talk.exe and talk.toml; found: $($relativeFiles -join ', ')"
    }
    if (@(Get-ChildItem -LiteralPath $resolvedProductPath -Recurse -Directory).Count -ne 0) {
        throw 'Talk product release must not contain subdirectories'
    }

    $executablePath = Join-Path $resolvedProductPath 'Talk.exe'
    [byte[]]$bytes = [System.IO.File]::ReadAllBytes($executablePath)
    $trailerLength = 60
    if ($bytes.Length -lt $trailerLength) {
        throw 'Talk.exe is too small to contain an embedded runtime payload trailer'
    }
    $magic = [System.Text.Encoding]::ASCII.GetBytes('TLPAY001')
    $magicOffset = $bytes.Length - $trailerLength
    for ($index = 0; $index -lt $magic.Length; $index++) {
        if ($bytes[$magicOffset + $index] -ne $magic[$index]) {
            throw 'Talk.exe embedded runtime payload magic is missing'
        }
    }

    [pscustomobject]@{
        ProductPath = $resolvedProductPath
        Files = $relativeFiles
        ExecutableBytes = $bytes.Length
        PayloadTrailer = 'TLPAY001'
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Test-TalkProductRelease -ProductPath $ProductPath
}
