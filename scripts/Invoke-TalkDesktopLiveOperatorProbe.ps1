[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$SmokeRoot,
    [string]$Hotkey = 'Ctrl+Alt+F18',
    [int]$TimeoutSeconds = 60,
    [string]$ExpectedText = 'Paris',
    [string]$InputDevice,
    [int]$AudioProbeSeconds = 3,
    [switch]$SkipAudioProbe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedBinaryPath = $BinaryPath
$requestedReleaseDir = $ReleaseDir
$requestedApiKey = $ApiKey
$requestedApiKeyJsonPath = $ApiKeyJsonPath
$requestedSmokeRoot = $SmokeRoot
$requestedHotkey = $Hotkey
$requestedTimeoutSeconds = $TimeoutSeconds
$requestedExpectedText = $ExpectedText
$requestedInputDevice = $InputDevice
$requestedAudioProbeSeconds = $AudioProbeSeconds
$requestedSkipAudioProbe = $SkipAudioProbe

$startScriptPath = Join-Path $PSScriptRoot 'Start-TalkDesktop.ps1'
if (-not (Test-Path -LiteralPath $startScriptPath)) {
    throw "Missing Talk desktop launch script: $startScriptPath"
}
. $startScriptPath

$smokeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopReleaseSmoke.ps1'
if (-not (Test-Path -LiteralPath $smokeScriptPath)) {
    throw "Missing Talk desktop smoke script: $smokeScriptPath"
}
. $smokeScriptPath

function Get-TalkDesktopLiveOperatorLogsDir {
    param([Parameter(Mandatory = $true)][string]$EffectiveConfigPath)

    Join-Path (Split-Path -Parent $EffectiveConfigPath) '.runtime\talk-desktop\logs'
}

function New-TalkDesktopLiveOperatorProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)]$LaunchSummary,
        [Parameter(Mandatory = $true)]$Session,
        [string]$LogPath,
        [string]$CapturedText,
        [string]$InputDevice,
        $AudioProbe
    )

    [pscustomobject][ordered]@{
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        capturedText = [string]$CapturedText
        inputDevice = [string]$InputDevice
        audioProbe = $AudioProbe
        releaseDir = [string]$LaunchSummary.releaseDir
        binaryPath = [string]$LaunchSummary.binaryPath
        baseConfigPath = [string]$LaunchSummary.baseConfigPath
        effectiveConfigPath = [string]$LaunchSummary.effectiveConfigPath
        processId = [int]$LaunchSummary.processId
        logPath = $LogPath
        snapshotPath = Join-Path $SmokeRoot 'text-target\snapshot.txt'
    }
}

function Write-TalkDesktopLiveOperatorProbeSummaryFile {
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

function Test-TalkDesktopLiveOperatorAudioProbeHasSignal {
    param($ProbeSummary)

    if ($null -eq $ProbeSummary) {
        return $false
    }
    if ([bool]$ProbeSummary.silent) {
        return $false
    }
    return ([double]$ProbeSummary.peak -gt 0)
}

function Invoke-TalkDesktopLiveOperatorAudioProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$Hotkey,
        [string]$InputDevice,
        [int]$AudioProbeSeconds
    )

    $resolvedReleaseDir = Resolve-TalkDesktopLaunchReleaseDir -ReleaseDir $ReleaseDir -BinaryPath $BinaryPath
    $resolvedBinaryPath = Resolve-TalkDesktopLaunchBinaryPath -BinaryPath $BinaryPath -ReleaseDir $resolvedReleaseDir
    $resolvedTalkBinaryPath = Resolve-TalkDesktopLaunchTalkBinaryPath -ReleaseDir $resolvedReleaseDir
    $resolvedBaseConfigPath = Resolve-TalkDesktopLaunchConfigPath -ReleaseDir $resolvedReleaseDir
    $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
        -BaseConfigPath $resolvedBaseConfigPath `
        -Hotkey $Hotkey `
        -InputDevice $InputDevice
    $readinessReport = Invoke-TalkDesktopLaunchReadiness `
        -TalkBinaryPath $resolvedTalkBinaryPath `
        -EffectiveConfigPath $effectiveConfigPath `
        -WorkingDirectory $resolvedReleaseDir
    $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
    $probeReport = Invoke-TalkDesktopLaunchAudioProbe `
        -TalkBinaryPath $resolvedTalkBinaryPath `
        -EffectiveConfigPath $effectiveConfigPath `
        -ProbeSeconds $AudioProbeSeconds
    $probeSummary = New-TalkDesktopLaunchAudioProbeSummary `
        -ProbeReport $probeReport `
        -Inventory $inventory
    $launchSummary = New-TalkDesktopLaunchSummary `
        -ReleaseDir $resolvedReleaseDir `
        -BinaryPath $resolvedBinaryPath `
        -BaseConfigPath $resolvedBaseConfigPath `
        -EffectiveConfigPath $effectiveConfigPath `
        -ProcessId 0

    [pscustomobject]@{
        LaunchSummary = $launchSummary
        AudioProbe = $probeSummary
    }
}

function Invoke-TalkDesktopLiveOperatorProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$SmokeRoot,
        [string]$Hotkey = 'Ctrl+Alt+F18',
        [int]$TimeoutSeconds = 60,
        [string]$ExpectedText = 'Paris',
        [string]$InputDevice,
        [int]$AudioProbeSeconds = 3,
        [switch]$SkipAudioProbe
    )

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-live-operator-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $resolvedSmokeRoot -Force | Out-Null

    Ensure-TalkDesktopSmokeWin32Type
    $target = $null
    $launchSummary = $null
    $instance = $null
    $audioProbe = $null
    try {
        if ((-not $SkipAudioProbe) -and -not [string]::IsNullOrWhiteSpace($InputDevice)) {
            Write-Host ''
            Write-Host 'Talk live operator audio preflight is ready.' -ForegroundColor Green
            Write-Host ("Input device: {0}" -f $InputDevice)
            Write-Host ("Speak normally for {0}s after the countdown." -f $AudioProbeSeconds)
            for ($countdown = 3; $countdown -ge 1; $countdown--) {
                Write-Host ("Starting native audio probe in {0}..." -f $countdown)
                Start-Sleep -Seconds 1
            }

            $preflight = Invoke-TalkDesktopLiveOperatorAudioProbe `
                -BinaryPath $BinaryPath `
                -ReleaseDir $ReleaseDir `
                -Hotkey $Hotkey `
                -InputDevice $InputDevice `
                -AudioProbeSeconds $AudioProbeSeconds
            $launchSummary = $preflight.LaunchSummary
            $audioProbe = $preflight.AudioProbe

            if (-not (Test-TalkDesktopLiveOperatorAudioProbeHasSignal -ProbeSummary $audioProbe)) {
                $failureReason = 'Live operator audio probe captured only silence; speak louder or fix the selected input device'
                $summary = New-TalkDesktopLiveOperatorProbeSummary `
                    -SmokeRoot $resolvedSmokeRoot `
                    -LaunchSummary $launchSummary `
                    -Session ([pscustomobject]@{
                        status = 'failed'
                        transcript = $null
                        output_text = $null
                    }) `
                    -LogPath '' `
                    -CapturedText '' `
                    -InputDevice $InputDevice `
                    -AudioProbe $audioProbe
                $summaryPath = Join-Path $resolvedSmokeRoot 'live-operator-probe-summary.json'
                $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue $failureReason
                $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
                $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
                Write-TalkDesktopLiveOperatorProbeSummaryFile -Path $summaryPath -Summary $summary
                throw $failureReason
            }
        }

        $target = Start-TalkTextCaptureTarget -ScenarioRoot $resolvedSmokeRoot
        Set-TalkDesktopForegroundWindow -Hwnd $target.Hwnd | Out-Null

        $launchSummary = Start-TalkDesktop `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -ApiKey $ApiKey `
            -ApiKeyJsonPath $ApiKeyJsonPath `
            -Hotkey $Hotkey `
            -InputDevice $InputDevice

        $instance = [pscustomobject]@{
            Process = Get-Process -Id $launchSummary.processId -ErrorAction Stop
            Hwnd = (Find-WindowByProcessIdAndClass -TargetProcessId $launchSummary.processId -ClassName 'TalkDesktopMessageWindow' -TimeoutMs 10000)
        }
        Set-TalkDesktopForegroundWindow -Hwnd $target.Hwnd | Out-Null

        Write-Host ''
        Write-Host 'Talk live operator probe is ready.' -ForegroundColor Green
        Write-Host ("Press and hold [{0}], speak, then release. Waiting up to {1}s for a completed session." -f $Hotkey, $TimeoutSeconds)
        Write-Host ("Foreground target: {0}" -f $target.WindowTitle)
        Write-Host ("Desktop config: {0}" -f $launchSummary.effectiveConfigPath)
        Write-Host ''

        $logsDir = Get-TalkDesktopLiveOperatorLogsDir -EffectiveConfigPath $launchSummary.effectiveConfigPath
        try {
            $log = Wait-LatestSessionLog -LogsDir $logsDir -TimeoutMs ($TimeoutSeconds * 1000)
        }
        catch {
            $failureReason = $_.Exception.Message
            $summary = New-TalkDesktopLiveOperatorProbeSummary `
                -SmokeRoot $resolvedSmokeRoot `
                -LaunchSummary $launchSummary `
                -Session ([pscustomobject]@{
                    status = 'failed'
                    transcript = $null
                    output_text = $null
                }) `
                -LogPath '' `
                -CapturedText '' `
                -InputDevice $InputDevice `
                -AudioProbe $audioProbe
            $summaryPath = Join-Path $resolvedSmokeRoot 'live-operator-probe-summary.json'
            $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue $failureReason
            $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
            $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
            Write-TalkDesktopLiveOperatorProbeSummaryFile -Path $summaryPath -Summary $summary
            throw
        }
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json

        $capturedText = ''
        if ($session.status -eq 'completed') {
            $expectedInsertedText = if (-not [string]::IsNullOrWhiteSpace([string]$session.output_text)) {
                [string]$session.output_text
            } elseif (-not [string]::IsNullOrWhiteSpace($ExpectedText)) {
                $ExpectedText
            } else {
                ''
            }

            if (-not [string]::IsNullOrWhiteSpace($expectedInsertedText)) {
                $capturedText = Wait-TalkTextCaptureContainsWithForegroundRefresh `
                    -Hwnd $target.Hwnd `
                    -SnapshotPath $target.SnapshotPath `
                    -ExpectedText $expectedInsertedText `
                    -TimeoutMs 10000
            } elseif (Test-Path -LiteralPath $target.SnapshotPath) {
                $capturedText = [string](Get-Content -LiteralPath $target.SnapshotPath -Raw)
            }
        } elseif (Test-Path -LiteralPath $target.SnapshotPath) {
            $capturedText = [string](Get-Content -LiteralPath $target.SnapshotPath -Raw)
        }

        $summary = New-TalkDesktopLiveOperatorProbeSummary `
            -SmokeRoot $resolvedSmokeRoot `
            -LaunchSummary $launchSummary `
            -Session $session `
            -LogPath $log.FullName `
            -CapturedText $capturedText `
            -InputDevice $InputDevice `
            -AudioProbe $audioProbe
        $summaryPath = Join-Path $resolvedSmokeRoot 'live-operator-probe-summary.json'
        $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
        $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
        Write-TalkDesktopLiveOperatorProbeSummaryFile -Path $summaryPath -Summary $summary
        $summary
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkTextCaptureTarget -Target $target
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopLiveOperatorProbe `
        -BinaryPath $requestedBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -ApiKey $requestedApiKey `
        -ApiKeyJsonPath $requestedApiKeyJsonPath `
        -SmokeRoot $requestedSmokeRoot `
        -Hotkey $requestedHotkey `
        -TimeoutSeconds $requestedTimeoutSeconds `
        -ExpectedText $requestedExpectedText `
        -InputDevice $requestedInputDevice `
        -AudioProbeSeconds $requestedAudioProbeSeconds `
        -SkipAudioProbe:$requestedSkipAudioProbe
}
