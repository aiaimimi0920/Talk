[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$SmokeRoot,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$AudioOverridePath,
    [int]$Count = 3,
    [int]$WarmupRuns = 0,
    [switch]$AllowFailures
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedSoakBinaryPath = $BinaryPath
$requestedSoakReleaseDir = $ReleaseDir
$requestedSoakSmokeRoot = $SmokeRoot
$requestedSoakApiKey = $ApiKey
$requestedSoakApiKeyJsonPath = $ApiKeyJsonPath
$requestedSoakAudioOverridePath = $AudioOverridePath
$requestedSoakCount = $Count
$requestedSoakWarmupRuns = $WarmupRuns
$requestedSoakAllowFailures = $AllowFailures

$probeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
if (-not (Test-Path -LiteralPath $probeScriptPath)) {
    throw "Missing Talk desktop Qwen global hotkey probe script: $probeScriptPath"
}
. $probeScriptPath

function Convert-TalkDesktopQwenGlobalHotkeySoakErrorPayload {
    param([Parameter(Mandatory = $true)]$ErrorRecord)

    $errorText = [string]$ErrorRecord.Exception.Message
    if ([string]::IsNullOrWhiteSpace($errorText)) {
        return [pscustomobject]@{
            status = 'failed'
            error = 'unknown error'
        }
    }

    try {
        $parsed = $errorText | ConvertFrom-Json -ErrorAction Stop
        if ($null -ne $parsed) {
            return $parsed
        }
    }
    catch {
    }

    [pscustomobject]@{
        status = 'failed'
        error = $errorText.Trim()
    }
}

function New-TalkDesktopQwenGlobalHotkeySoakRunRecord {
    param(
        [Parameter(Mandatory = $true)][int]$Iteration,
        [Parameter(Mandatory = $true)][datetime]$StartedAtUtc,
        [Parameter(Mandatory = $true)][datetime]$FinishedAtUtc,
        [Parameter(Mandatory = $true)][int]$DurationMs,
        $ProbeSummary,
        $ErrorRecord
    )

    $payload = if ($null -ne $ProbeSummary) {
        $ProbeSummary
    } elseif ($null -ne $ErrorRecord) {
        Convert-TalkDesktopQwenGlobalHotkeySoakErrorPayload -ErrorRecord $ErrorRecord
    } else {
        [pscustomobject]@{
            status = 'failed'
            error = 'unknown probe failure'
        }
    }

    $outputText = if ($payload.PSObject.Properties['outputText']) {
        [string]$payload.outputText
    } elseif ($payload.PSObject.Properties['output_text']) {
        [string]$payload.output_text
    } else {
        ''
    }
    $transcript = if ($payload.PSObject.Properties['transcript']) {
        [string]$payload.transcript
    } else {
        ''
    }
    $capturedText = if ($payload.PSObject.Properties['capturedText']) {
        [string]$payload.capturedText
    } else {
        ''
    }
    $smokeRoot = if ($payload.PSObject.Properties['smokeRoot']) {
        [string]$payload.smokeRoot
    } else {
        ''
    }
    $summaryPath = if ($payload.PSObject.Properties['summaryPath']) {
        [string]$payload.summaryPath
    } else {
        ''
    }
    $insertTargetDiagnosticPath = if ($payload.PSObject.Properties['insertTargetDiagnosticPath']) {
        [string]$payload.insertTargetDiagnosticPath
    } else {
        ''
    }
    $capturedTextMatchesOutput = if ($payload.PSObject.Properties['capturedTextMatchesOutput']) {
        [bool]$payload.capturedTextMatchesOutput
    } else {
        $null
    }

    [pscustomobject][ordered]@{
        iteration = $Iteration
        startedAtUtc = $StartedAtUtc.ToString('o')
        finishedAtUtc = $FinishedAtUtc.ToString('o')
        durationMs = $DurationMs
        status = [string]$payload.status
        transcript = $transcript
        outputText = $outputText
        capturedText = $capturedText
        capturedTextMatchesOutput = $capturedTextMatchesOutput
        smokeRoot = $smokeRoot
        summaryPath = $summaryPath
        insertTargetDiagnosticPath = $insertTargetDiagnosticPath
        failureKind = if ($payload.PSObject.Properties['failureKind']) { [string]$payload.failureKind } else { '' }
        failureSummary = if ($payload.PSObject.Properties['failureSummary']) { [string]$payload.failureSummary } else { '' }
        failureEvidencePath = if ($payload.PSObject.Properties['failureEvidencePath']) { [string]$payload.failureEvidencePath } else { '' }
        retryCount = if ($payload.PSObject.Properties['retryCount']) { [int]$payload.retryCount } else { 0 }
        retryReason = if ($payload.PSObject.Properties['retryReason']) { [string]$payload.retryReason } else { '' }
        error = if ($payload.PSObject.Properties['error']) { [string]$payload.error } else { '' }
    }
}

function Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded {
    param($RunRecord)

    if ($null -eq $RunRecord) {
        return $false
    }

    if (([string]$RunRecord.status) -ne 'completed') {
        return $false
    }

    if (
        $RunRecord.PSObject.Properties['capturedTextMatchesOutput'] -and
        $null -ne $RunRecord.capturedTextMatchesOutput
    ) {
        return [bool]$RunRecord.capturedTextMatchesOutput
    }

    $true
}

function Get-TalkDesktopQwenGlobalHotkeySoakInsertTargetDiagnostic {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }

    Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Measure-TalkDesktopQwenGlobalHotkeySoakInsertTargetFocus {
    param([Parameter(Mandatory = $true)][object[]]$RunRecords)

    $diagnosticRuns = 0
    $focusCapturedRuns = 0
    $focusMissingRuns = 0
    $focusSourceGuiThreadInfoRuns = 0
    $focusSourceAttachedGetFocusRuns = 0
    $focusSourceUnknownRuns = 0
    $honorConfiguredOutputRuns = 0
    $showCopyPopupOnlyRuns = 0
    $topLevelOnlyRuns = 0
    $focusMissingSuccessfulRuns = 0
    $focusMissingNonSuccessfulRuns = 0
    $topLevelOnlySuccessfulRuns = 0
    $topLevelOnlyNonSuccessfulRuns = 0

    foreach ($runRecord in @($RunRecords)) {
        $runSucceeded = Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord $runRecord
        $diagnosticPath = if ($runRecord.PSObject.Properties['insertTargetDiagnosticPath']) {
            [string]$runRecord.insertTargetDiagnosticPath
        } else {
            ''
        }
        $diagnostic = Get-TalkDesktopQwenGlobalHotkeySoakInsertTargetDiagnostic -Path $diagnosticPath
        if ($null -eq $diagnostic) {
            continue
        }

        $diagnosticRuns += 1
        $capturedFocusHandle = if ($diagnostic.PSObject.Properties['capturedFocusHandle']) {
            [string]$diagnostic.capturedFocusHandle
        } else {
            ''
        }
        if ([string]::IsNullOrWhiteSpace($capturedFocusHandle)) {
            $focusMissingRuns += 1
            if ($runSucceeded) {
                $focusMissingSuccessfulRuns += 1
            } else {
                $focusMissingNonSuccessfulRuns += 1
            }
        } else {
            $focusCapturedRuns += 1
        }

        $capturedFocusSource = if ($diagnostic.PSObject.Properties['capturedFocusSource']) {
            [string]$diagnostic.capturedFocusSource
        } else {
            ''
        }
        switch ($capturedFocusSource) {
            'gui_thread_info' {
                $focusSourceGuiThreadInfoRuns += 1
            }
            'attached_get_focus' {
                $focusSourceAttachedGetFocusRuns += 1
            }
            default {
                $focusSourceUnknownRuns += 1
            }
        }

        $outputStrategy = if ($diagnostic.PSObject.Properties['outputStrategy']) {
            [string]$diagnostic.outputStrategy
        } else {
            ''
        }
        switch ($outputStrategy) {
            'honor_configured_output' {
                $honorConfiguredOutputRuns += 1
            }
            'show_copy_popup_only' {
                $showCopyPopupOnlyRuns += 1
            }
        }

        $capturedWindowHandle = if ($diagnostic.PSObject.Properties['capturedWindowHandle']) {
            [string]$diagnostic.capturedWindowHandle
        } else {
            ''
        }
        $capturedPrimaryFocusHandle = if ($diagnostic.PSObject.Properties['capturedPrimaryFocusHandle']) {
            [string]$diagnostic.capturedPrimaryFocusHandle
        } else {
            ''
        }
        $capturedFallbackFocusHandle = if ($diagnostic.PSObject.Properties['capturedFallbackFocusHandle']) {
            [string]$diagnostic.capturedFallbackFocusHandle
        } else {
            ''
        }
        if (
            [string]::IsNullOrWhiteSpace($capturedFocusHandle) -and
            -not [string]::IsNullOrWhiteSpace($capturedWindowHandle) -and
            $capturedPrimaryFocusHandle -eq $capturedWindowHandle -and
            $capturedFallbackFocusHandle -eq $capturedWindowHandle
        ) {
            $topLevelOnlyRuns += 1
            if ($runSucceeded) {
                $topLevelOnlySuccessfulRuns += 1
            } else {
                $topLevelOnlyNonSuccessfulRuns += 1
            }
        }
    }

    $focusCapturedRate = if ($diagnosticRuns -le 0) {
        0.0
    } else {
        [math]::Round((($focusCapturedRuns / [double]$diagnosticRuns) * 100.0), 2)
    }

    [pscustomobject][ordered]@{
        diagnosticRuns = $diagnosticRuns
        focusCapturedRuns = $focusCapturedRuns
        focusMissingRuns = $focusMissingRuns
        focusCapturedRate = $focusCapturedRate
        focusSourceGuiThreadInfoRuns = $focusSourceGuiThreadInfoRuns
        focusSourceAttachedGetFocusRuns = $focusSourceAttachedGetFocusRuns
        focusSourceUnknownRuns = $focusSourceUnknownRuns
        honorConfiguredOutputRuns = $honorConfiguredOutputRuns
        showCopyPopupOnlyRuns = $showCopyPopupOnlyRuns
        topLevelOnlyRuns = $topLevelOnlyRuns
        focusMissingSuccessfulRuns = $focusMissingSuccessfulRuns
        focusMissingNonSuccessfulRuns = $focusMissingNonSuccessfulRuns
        topLevelOnlySuccessfulRuns = $topLevelOnlySuccessfulRuns
        topLevelOnlyNonSuccessfulRuns = $topLevelOnlyNonSuccessfulRuns
    }
}

function New-TalkDesktopQwenGlobalHotkeySoakSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [int]$WarmupRuns = 0,
        [Parameter(Mandatory = $true)][object[]]$RunRecords
    )

    $totalRuns = @($RunRecords).Count
    $successfulRuns = @($RunRecords | Where-Object { Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord $_ }).Count
    $failedRuns = $totalRuns - $successfulRuns
    $successRate = if ($totalRuns -le 0) {
        0.0
    } else {
        [math]::Round((($successfulRuns / [double]$totalRuns) * 100.0), 2)
    }
    $averageDurationMs = if ($totalRuns -le 0) {
        0
    } else {
        [int][math]::Round(((@($RunRecords | ForEach-Object { [double]$_.durationMs } | Measure-Object -Average).Average)), 0)
    }
    $normalizedWarmupRuns = [Math]::Max(0, [Math]::Min($WarmupRuns, $totalRuns))
    $measuredRecords = if ($normalizedWarmupRuns -ge $totalRuns) {
        @()
    } else {
        @($RunRecords | Select-Object -Skip $normalizedWarmupRuns)
    }
    $measuredRuns = @($measuredRecords).Count
    $measuredSuccessfulRuns = @($measuredRecords | Where-Object { Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord $_ }).Count
    $measuredFailedRuns = $measuredRuns - $measuredSuccessfulRuns
    $measuredSuccessRate = if ($measuredRuns -le 0) {
        0.0
    } else {
        [math]::Round((($measuredSuccessfulRuns / [double]$measuredRuns) * 100.0), 2)
    }
    $measuredAverageDurationMs = if ($measuredRuns -le 0) {
        0
    } else {
        [int][math]::Round(((@($measuredRecords | ForEach-Object { [double]$_.durationMs } | Measure-Object -Average).Average)), 0)
    }
    $insertTargetFocusMetrics = Measure-TalkDesktopQwenGlobalHotkeySoakInsertTargetFocus -RunRecords $RunRecords
    $measuredInsertTargetFocusMetrics = Measure-TalkDesktopQwenGlobalHotkeySoakInsertTargetFocus -RunRecords $measuredRecords

    [pscustomobject][ordered]@{
        smokeRoot = $SmokeRoot
        totalRuns = $totalRuns
        successfulRuns = $successfulRuns
        failedRuns = $failedRuns
        successRate = $successRate
        averageDurationMs = $averageDurationMs
        warmupRuns = $normalizedWarmupRuns
        measuredRuns = $measuredRuns
        measuredSuccessfulRuns = $measuredSuccessfulRuns
        measuredFailedRuns = $measuredFailedRuns
        measuredSuccessRate = $measuredSuccessRate
        measuredAverageDurationMs = $measuredAverageDurationMs
        insertTargetDiagnosticRuns = $insertTargetFocusMetrics.diagnosticRuns
        insertTargetFocusCapturedRuns = $insertTargetFocusMetrics.focusCapturedRuns
        insertTargetFocusMissingRuns = $insertTargetFocusMetrics.focusMissingRuns
        insertTargetFocusCapturedRate = $insertTargetFocusMetrics.focusCapturedRate
        insertTargetFocusSourceGuiThreadInfoRuns = $insertTargetFocusMetrics.focusSourceGuiThreadInfoRuns
        insertTargetFocusSourceAttachedGetFocusRuns = $insertTargetFocusMetrics.focusSourceAttachedGetFocusRuns
        insertTargetFocusSourceUnknownRuns = $insertTargetFocusMetrics.focusSourceUnknownRuns
        insertTargetHonorConfiguredOutputRuns = $insertTargetFocusMetrics.honorConfiguredOutputRuns
        insertTargetShowCopyPopupOnlyRuns = $insertTargetFocusMetrics.showCopyPopupOnlyRuns
        insertTargetTopLevelOnlyRuns = $insertTargetFocusMetrics.topLevelOnlyRuns
        insertTargetFocusMissingSuccessfulRuns = $insertTargetFocusMetrics.focusMissingSuccessfulRuns
        insertTargetFocusMissingNonSuccessfulRuns = $insertTargetFocusMetrics.focusMissingNonSuccessfulRuns
        insertTargetTopLevelOnlySuccessfulRuns = $insertTargetFocusMetrics.topLevelOnlySuccessfulRuns
        insertTargetTopLevelOnlyNonSuccessfulRuns = $insertTargetFocusMetrics.topLevelOnlyNonSuccessfulRuns
        measuredInsertTargetDiagnosticRuns = $measuredInsertTargetFocusMetrics.diagnosticRuns
        measuredInsertTargetFocusCapturedRuns = $measuredInsertTargetFocusMetrics.focusCapturedRuns
        measuredInsertTargetFocusMissingRuns = $measuredInsertTargetFocusMetrics.focusMissingRuns
        measuredInsertTargetFocusCapturedRate = $measuredInsertTargetFocusMetrics.focusCapturedRate
        measuredInsertTargetFocusSourceGuiThreadInfoRuns = $measuredInsertTargetFocusMetrics.focusSourceGuiThreadInfoRuns
        measuredInsertTargetFocusSourceAttachedGetFocusRuns = $measuredInsertTargetFocusMetrics.focusSourceAttachedGetFocusRuns
        measuredInsertTargetFocusSourceUnknownRuns = $measuredInsertTargetFocusMetrics.focusSourceUnknownRuns
        measuredInsertTargetHonorConfiguredOutputRuns = $measuredInsertTargetFocusMetrics.honorConfiguredOutputRuns
        measuredInsertTargetShowCopyPopupOnlyRuns = $measuredInsertTargetFocusMetrics.showCopyPopupOnlyRuns
        measuredInsertTargetTopLevelOnlyRuns = $measuredInsertTargetFocusMetrics.topLevelOnlyRuns
        measuredInsertTargetFocusMissingSuccessfulRuns = $measuredInsertTargetFocusMetrics.focusMissingSuccessfulRuns
        measuredInsertTargetFocusMissingNonSuccessfulRuns = $measuredInsertTargetFocusMetrics.focusMissingNonSuccessfulRuns
        measuredInsertTargetTopLevelOnlySuccessfulRuns = $measuredInsertTargetFocusMetrics.topLevelOnlySuccessfulRuns
        measuredInsertTargetTopLevelOnlyNonSuccessfulRuns = $measuredInsertTargetFocusMetrics.topLevelOnlyNonSuccessfulRuns
        runs = @($RunRecords)
    }
}

function Write-TalkDesktopQwenGlobalHotkeySoakSummaryFile {
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
        (($Summary | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
        $utf8NoBom
    )
}

function Invoke-TalkDesktopQwenGlobalHotkeySoak {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$SmokeRoot,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$AudioOverridePath,
        [int]$Count = 3,
        [int]$WarmupRuns = 0,
        [switch]$AllowFailures
    )

    if ($Count -le 0) {
        throw 'Talk desktop Qwen global hotkey soak count must be greater than 0'
    }
    if ($WarmupRuns -lt 0) {
        throw 'Talk desktop Qwen global hotkey soak warmup runs must be greater than or equal to 0'
    }
    if ($WarmupRuns -gt $Count) {
        throw 'Talk desktop Qwen global hotkey soak warmup runs must be less than or equal to count'
    }

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-qwen-global-hotkey-soak-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $resolvedSmokeRoot -Force | Out-Null

    $runRecords = New-Object System.Collections.Generic.List[object]
    foreach ($iteration in 1..$Count) {
        $startedAtUtc = [datetime]::UtcNow
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $runRoot = Join-Path $resolvedSmokeRoot ('run-{0:D2}' -f $iteration)
            $probeSummary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath $BinaryPath `
                -ReleaseDir $ReleaseDir `
                -SmokeRoot $runRoot `
                -ApiKey $ApiKey `
                -ApiKeyJsonPath $ApiKeyJsonPath `
                -AudioOverridePath $AudioOverridePath
            $stopwatch.Stop()
            $runRecords.Add((
                New-TalkDesktopQwenGlobalHotkeySoakRunRecord `
                    -Iteration $iteration `
                    -StartedAtUtc $startedAtUtc `
                    -FinishedAtUtc ([datetime]::UtcNow) `
                    -DurationMs ([int]$stopwatch.ElapsedMilliseconds) `
                    -ProbeSummary $probeSummary
            )) | Out-Null
        }
        catch {
            $stopwatch.Stop()
            $runRecords.Add((
                New-TalkDesktopQwenGlobalHotkeySoakRunRecord `
                    -Iteration $iteration `
                    -StartedAtUtc $startedAtUtc `
                    -FinishedAtUtc ([datetime]::UtcNow) `
                    -DurationMs ([int]$stopwatch.ElapsedMilliseconds) `
                    -ErrorRecord $_
            )) | Out-Null
        }
    }

    $summary = New-TalkDesktopQwenGlobalHotkeySoakSummary `
        -SmokeRoot $resolvedSmokeRoot `
        -WarmupRuns $WarmupRuns `
        -RunRecords $runRecords.ToArray()
    $summaryPath = Join-Path $resolvedSmokeRoot 'qwen-global-hotkey-soak-summary.json'
    $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
    Write-TalkDesktopQwenGlobalHotkeySoakSummaryFile -Path $summaryPath -Summary $summary

    if ($summary.failedRuns -gt 0 -and -not $AllowFailures) {
        $summary | Add-Member -Force -NotePropertyName failureReason -NotePropertyValue ("Talk desktop Qwen global hotkey soak observed {0} failed run(s) out of {1}" -f $summary.failedRuns, $summary.totalRuns)
        throw ($summary | ConvertTo-Json -Depth 8 -Compress)
    }

    $summary
}

function Invoke-TalkDesktopQwenGlobalHotkeySoakEntryPoint {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$SmokeRoot,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$AudioOverridePath,
        [int]$Count = 3,
        [int]$WarmupRuns = 0,
        [switch]$AllowFailures
    )

    Invoke-TalkDesktopQwenGlobalHotkeySoak `
        -BinaryPath $BinaryPath `
        -ReleaseDir $ReleaseDir `
        -SmokeRoot $SmokeRoot `
        -ApiKey $ApiKey `
        -ApiKeyJsonPath $ApiKeyJsonPath `
        -AudioOverridePath $AudioOverridePath `
        -Count $Count `
        -WarmupRuns $WarmupRuns `
        -AllowFailures:$AllowFailures
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopQwenGlobalHotkeySoakEntryPoint `
        -BinaryPath $requestedSoakBinaryPath `
        -ReleaseDir $requestedSoakReleaseDir `
        -SmokeRoot $requestedSoakSmokeRoot `
        -ApiKey $requestedSoakApiKey `
        -ApiKeyJsonPath $requestedSoakApiKeyJsonPath `
        -AudioOverridePath $requestedSoakAudioOverridePath `
        -Count $requestedSoakCount `
        -WarmupRuns $requestedSoakWarmupRuns `
        -AllowFailures:$requestedSoakAllowFailures
}
