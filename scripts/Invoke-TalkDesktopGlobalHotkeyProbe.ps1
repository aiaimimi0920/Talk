[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$SmokeRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedBinaryPath = $BinaryPath
$requestedReleaseDir = $ReleaseDir
$requestedSmokeRoot = $SmokeRoot

$smokeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopReleaseSmoke.ps1'
if (-not (Test-Path -LiteralPath $smokeScriptPath)) {
    throw "Missing Talk desktop smoke script: $smokeScriptPath"
}
. $smokeScriptPath

function Write-TalkGlobalHotkeyProbeSummaryFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Summary
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $Path,
        (($Summary | ConvertTo-Json -Depth 6) + [Environment]::NewLine),
        $utf8NoBom
    )
}

function New-TalkDesktopGlobalHotkeyProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)]$Result
    )

    $scenario = [string]$Result.Scenario
    [pscustomobject][ordered]@{
        scenario = $scenario
        binaryPath = [string]$Result.BinaryPath
        configPath = [string]$Result.ConfigPath
        status = [string]$Result.Status
        capturedText = [string]$Result.CapturedText
        logPath = [string]$Result.LogPath
        providerRequestsPath = [string]$Result.ProviderRequestsPath
        snapshotPath = Join-Path $SmokeRoot (Join-Path $scenario 'text-target\snapshot.txt')
    }
}

function Invoke-TalkDesktopGlobalHotkeyProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$SmokeRoot
    )

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-global-hotkey-probe-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $resolvedSmokeRoot -Force | Out-Null

    $result = @(Invoke-TalkDesktopReleaseSmoke `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -SmokeRoot $resolvedSmokeRoot `
            -Scenario @('openai-compatible-audio-input-insert-success'))[0]

    $summary = New-TalkDesktopGlobalHotkeyProbeSummary `
        -SmokeRoot $resolvedSmokeRoot `
        -Result $result
    $summaryPath = Join-Path $resolvedSmokeRoot 'global-hotkey-probe-summary.json'
    $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
    $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath

    Write-TalkGlobalHotkeyProbeSummaryFile -Path $summaryPath -Summary $summary
    $summary
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopGlobalHotkeyProbe `
        -BinaryPath $requestedBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -SmokeRoot $requestedSmokeRoot
}
