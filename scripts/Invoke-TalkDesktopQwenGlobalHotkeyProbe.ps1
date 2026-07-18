[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$SmokeRoot,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$AudioOverridePath,
    [string]$TextCapturePrimer
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedBinaryPath = $BinaryPath
$requestedReleaseDir = $ReleaseDir
$requestedSmokeRoot = $SmokeRoot
$requestedApiKey = $ApiKey
$requestedApiKeyJsonPath = $ApiKeyJsonPath
$requestedAudioOverridePath = $AudioOverridePath
$requestedTextCapturePrimer = $TextCapturePrimer

$smokeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopReleaseSmoke.ps1'
if (-not (Test-Path -LiteralPath $smokeScriptPath)) {
    throw "Missing Talk desktop smoke script: $smokeScriptPath"
}
. $smokeScriptPath

function Get-TalkDesktopQwenProbeHomeDirectory {
    $candidates = @(
        [Environment]::GetEnvironmentVariable('USERPROFILE', 'Process')
        [Environment]::GetEnvironmentVariable('HOME', 'Process')
    )

    foreach ($candidate in $candidates) {
        if (-not [string]::IsNullOrWhiteSpace($candidate)) {
            return [System.IO.Path]::GetFullPath($candidate)
        }
    }

    $null
}

function Resolve-TalkDesktopQwenProbeApiKey {
    param(
        [string]$ApiKey,
        [string]$ApiKeyJsonPath
    )

    if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
        if ($ApiKey.Trim() -ne $ApiKey) {
            throw 'Talk desktop Qwen probe api key must not have leading or trailing whitespace'
        }
        return $ApiKey
    }

    if (-not [string]::IsNullOrWhiteSpace($ApiKeyJsonPath)) {
        $resolvedJsonPath = [System.IO.Path]::GetFullPath($ApiKeyJsonPath)
        if (-not (Test-Path -LiteralPath $resolvedJsonPath)) {
            throw "Talk desktop Qwen probe api key json does not exist: $resolvedJsonPath"
        }
        $record = Get-Content -LiteralPath $resolvedJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $jsonApiKey = [string]$record.apiKey
        if ([string]::IsNullOrWhiteSpace($jsonApiKey)) {
            throw "Talk desktop Qwen probe api key json is missing a non-empty apiKey field: $resolvedJsonPath"
        }
        if ($jsonApiKey.Trim() -ne $jsonApiKey) {
            throw "Talk desktop Qwen probe api key json apiKey field must not have leading or trailing whitespace: $resolvedJsonPath"
        }
        return $jsonApiKey
    }

    $envApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($envApiKey)) {
        if ($envApiKey.Trim() -ne $envApiKey) {
            throw 'TALK_PROVIDER_API_KEY must not have leading or trailing whitespace'
        }
        return $envApiKey
    }

    $homeDirectory = Get-TalkDesktopQwenProbeHomeDirectory
    if (-not [string]::IsNullOrWhiteSpace($homeDirectory)) {
        $credentialDir = Join-Path $homeDirectory '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
        if (Test-Path -LiteralPath $credentialDir) {
            $autoDiscoveredJsonPath = Join-Path $credentialDir 'manual-live.json'
            if (-not (Test-Path -LiteralPath $autoDiscoveredJsonPath)) {
                $autoDiscoveredJsonPath = Get-ChildItem -LiteralPath $credentialDir -Filter '*.json' -File -ErrorAction SilentlyContinue |
                    Sort-Object LastWriteTime -Descending |
                    Select-Object -ExpandProperty FullName -First 1
            }

            if (-not [string]::IsNullOrWhiteSpace($autoDiscoveredJsonPath)) {
                $record = Get-Content -LiteralPath $autoDiscoveredJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
                $jsonApiKey = [string]$record.apiKey
                if ([string]::IsNullOrWhiteSpace($jsonApiKey)) {
                    throw "Talk desktop Qwen probe auto-discovered api key json is missing a non-empty apiKey field: $autoDiscoveredJsonPath"
                }
                if ($jsonApiKey.Trim() -ne $jsonApiKey) {
                    throw "Talk desktop Qwen probe auto-discovered api key json apiKey field must not have leading or trailing whitespace: $autoDiscoveredJsonPath"
                }
                return $jsonApiKey
            }
        }
    }

    throw 'Talk desktop Qwen probe requires -ApiKey, -ApiKeyJsonPath, or TALK_PROVIDER_API_KEY'
}

function Write-TalkDesktopQwenProbeAudio {
    param(
        [Parameter(Mandatory = $true)][string]$AudioPath,
        [string]$PromptText = 'What is the capital of France?'
    )

    $audioDirectory = Split-Path -Parent $AudioPath
    if (-not [string]::IsNullOrWhiteSpace($audioDirectory)) {
        New-Item -ItemType Directory -Path $audioDirectory -Force | Out-Null
    }

    Add-Type -AssemblyName System.Speech
    $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
    try {
        $synth.SetOutputToWaveFile($AudioPath)
        $synth.Speak($PromptText)
    }
    finally {
        $synth.Dispose()
    }
}

function Resolve-TalkDesktopQwenProbeAudioOverridePath {
    param(
        [string]$AudioOverridePath,
        [Parameter(Mandatory = $true)][string]$SmokeRoot
    )

    if (-not [string]::IsNullOrWhiteSpace($AudioOverridePath)) {
        $resolvedAudioPath = [System.IO.Path]::GetFullPath($AudioOverridePath)
        if (-not (Test-Path -LiteralPath $resolvedAudioPath)) {
            throw "Talk desktop Qwen probe audio override does not exist: $resolvedAudioPath"
        }
        return $resolvedAudioPath
    }

    $existingProbeAudioPath = Join-Path (Get-TalkRepoRoot) '.runtime\desktop-qwen-live-probe-release-v30\probe.wav'
    if (Test-Path -LiteralPath $existingProbeAudioPath) {
        return [System.IO.Path]::GetFullPath($existingProbeAudioPath)
    }

    $generatedAudioPath = Join-Path $SmokeRoot 'probe.wav'
    Write-TalkDesktopQwenProbeAudio -AudioPath $generatedAudioPath
    [System.IO.Path]::GetFullPath($generatedAudioPath)
}

function New-TalkDesktopQwenGlobalHotkeyProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)]$Session,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$AudioOverridePath,
        [Parameter(Mandatory = $true)][string]$BinaryPath,
        [string]$InsertTargetDiagnosticPath
    )

    $insertTargetDiagnostic = $null
    if (
        -not [string]::IsNullOrWhiteSpace($InsertTargetDiagnosticPath) -and
        (Test-Path -LiteralPath $InsertTargetDiagnosticPath)
    ) {
        $insertTargetDiagnostic = Get-Content -LiteralPath $InsertTargetDiagnosticPath -Raw -Encoding UTF8 | ConvertFrom-Json
    }

    [pscustomobject][ordered]@{
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        binaryPath = $BinaryPath
        configPath = $ConfigPath
        logPath = $LogPath
        audioOverridePath = $AudioOverridePath
        snapshotPath = Join-Path $SmokeRoot 'text-target\snapshot.txt'
        insertTargetDiagnosticPath = $InsertTargetDiagnosticPath
        insertTargetOutputStrategy = if ($insertTargetDiagnostic -and $insertTargetDiagnostic.PSObject.Properties['outputStrategy']) { [string]$insertTargetDiagnostic.outputStrategy } else { $null }
        insertTargetFocusLooksEditable = if ($insertTargetDiagnostic -and $insertTargetDiagnostic.PSObject.Properties['focusLooksEditable']) { [bool]$insertTargetDiagnostic.focusLooksEditable } else { $null }
        insertTargetFocusClassName = if ($insertTargetDiagnostic -and $insertTargetDiagnostic.PSObject.Properties['focusClassName']) { [string]$insertTargetDiagnostic.focusClassName } else { $null }
        insertTargetAutomationControlType = if ($insertTargetDiagnostic -and $insertTargetDiagnostic.PSObject.Properties['automationControlType']) { [string]$insertTargetDiagnostic.automationControlType } else { $null }
        insertTargetAutomationFrameworkId = if ($insertTargetDiagnostic -and $insertTargetDiagnostic.PSObject.Properties['automationFrameworkId']) { [string]$insertTargetDiagnostic.automationFrameworkId } else { $null }
    }
}

function Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile {
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

function Resolve-TalkDesktopQwenProbeCapturedText {
    param(
        [Parameter(Mandatory = $true)][string]$SnapshotPath,
        [Parameter(Mandatory = $true)][string]$CapturedText,
        [Parameter(Mandatory = $true)][string]$ExpectedOutputText,
        [string]$PrimerText
    )

    $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
        -CapturedText $CapturedText `
        -PrimerText $PrimerText
    if ($normalizedCapturedText.Trim() -eq $ExpectedOutputText.Trim()) {
        return $normalizedCapturedText
    }

    try {
        $settledCapturedText = Wait-TalkTextCaptureContains `
            -SnapshotPath $SnapshotPath `
            -ExpectedText $ExpectedOutputText `
            -TimeoutMs 800
        return (Remove-TalkTextCapturePrimerPrefix `
            -CapturedText ([string]$settledCapturedText) `
            -PrimerText $PrimerText)
    }
    catch {
        return $normalizedCapturedText
    }
}

function Convert-TalkDesktopQwenGlobalHotkeyProbeErrorPayload {
    param([Parameter(Mandatory = $true)]$ErrorRecord)

    $errorText = [string]$ErrorRecord.Exception.Message
    if ([string]::IsNullOrWhiteSpace($errorText)) {
        return $null
    }

    try {
        $parsed = $errorText | ConvertFrom-Json -ErrorAction Stop
        if ($null -ne $parsed) {
            return $parsed
        }
    }
    catch {
    }

    $null
}

function Get-TalkDesktopQwenGlobalHotkeyProbeAttemptSmokeRoot {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)][int]$AttemptNumber
    )

    Join-Path $SmokeRoot ('attempt-{0:d2}' -f $AttemptNumber)
}

function New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [string]$Status,
        [string]$Transcript,
        [string]$OutputText,
        [Parameter(Mandatory = $true)][string]$BinaryPath,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [string]$LogPath,
        [Parameter(Mandatory = $true)][string]$AudioOverridePath,
        [string]$InsertTargetDiagnosticPath,
        [string]$CapturedText,
        $CapturedTextMatchesOutput,
        [Parameter(Mandatory = $true)][string]$FailureKind,
        [Parameter(Mandatory = $true)][string]$FailureSummary,
        [string]$FailureEvidencePath
    )

    $summaryPath = Join-Path $SmokeRoot 'qwen-global-hotkey-probe-summary.json'
    [pscustomobject][ordered]@{
        status = if ([string]::IsNullOrWhiteSpace($Status)) { 'failed' } else { $Status }
        transcript = $Transcript
        outputText = $OutputText
        binaryPath = $BinaryPath
        configPath = $ConfigPath
        logPath = $LogPath
        audioOverridePath = $AudioOverridePath
        snapshotPath = Join-Path $SmokeRoot 'text-target\snapshot.txt'
        insertTargetDiagnosticPath = $InsertTargetDiagnosticPath
        capturedText = $CapturedText
        capturedTextMatchesOutput = $CapturedTextMatchesOutput
        smokeRoot = $SmokeRoot
        summaryPath = $summaryPath
        failureKind = $FailureKind
        failureSummary = $FailureSummary
        failureEvidencePath = $FailureEvidencePath
        error = $FailureSummary
    }
}

function Invoke-TalkDesktopQwenGlobalHotkeyProbeAttempt {
    param(
        [Parameter(Mandatory = $true)][string]$ResolvedBinaryPath,
        [Parameter(Mandatory = $true)][string]$ResolvedSmokeRoot,
        [Parameter(Mandatory = $true)][string]$ResolvedApiKey,
        [Parameter(Mandatory = $true)][string]$ResolvedAudioOverridePath,
        [string]$ResolvedTextCapturePrimer
    )

    $hotkey = 'Ctrl+Alt+F15'
    $configPath = Join-Path $ResolvedSmokeRoot 'config.toml'
    $logsDir = Join-Path $ResolvedSmokeRoot 'logs'
    Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig `
        -ConfigPath $configPath `
        -Hotkey $hotkey `
        -ChatCompletionsEndpoint 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' `
        -OutputMode 'clipboard_paste' `
        -ClipboardBackend 'native_windows'

    $target = $null
    $instance = $null
    try {
        Ensure-TalkDesktopSmokeWin32Type
        $target = Start-TalkTextCaptureTarget -ScenarioRoot $ResolvedSmokeRoot
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $ResolvedBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides @{
                TALK_PROVIDER_API_KEY = $ResolvedApiKey
                TALK_DESKTOP_AUDIO_FILE_OVERRIDE = $ResolvedAudioOverridePath
            }

        Start-Sleep -Milliseconds 500
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
        if (-not [string]::IsNullOrWhiteSpace($ResolvedTextCapturePrimer)) {
            try {
                Invoke-TalkTextCaptureTargetPrimer `
                    -Hwnd $target.Hwnd `
                    -SnapshotPath $target.SnapshotPath `
                    -PrimerText $ResolvedTextCapturePrimer `
                    -ChildHwnd $target.TextBoxHwnd | Out-Null
                Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
            }
            catch {
                $errorMessage = [string]$_.Exception.Message
                $failure = Get-TalkDesktopPrimerFailureClassification `
                    -Scenario 'qwen-global-hotkey-probe' `
                    -TargetWindowTitle 'Talk Smoke Text Target' `
                    -ErrorMessage $errorMessage
                if ($null -ne $failure) {
                    $capturedText = if (Test-Path -LiteralPath $target.SnapshotPath) {
                        Get-Content -LiteralPath $target.SnapshotPath -Raw -Encoding UTF8
                    } else {
                        ''
                    }
                    $failureEvidencePath = Join-Path $ResolvedSmokeRoot 'failure-diagnostic.json'
                    $failureDiagnostic = [pscustomobject][ordered]@{
                        scenario = 'qwen-global-hotkey-probe'
                        failureKind = [string]$failure.FailureKind
                        failureSummary = [string]$failure.FailureSummary
                        primerText = $ResolvedTextCapturePrimer
                        capturedText = if ([string]::IsNullOrWhiteSpace($capturedText)) { $null } else { $capturedText }
                        foregroundTail = [string]$failure.ForegroundTail
                        errorMessage = $errorMessage
                        snapshotPath = $target.SnapshotPath
                        stage = 'primer'
                    }
                    Write-TalkDesktopSmokeJson -Path $failureEvidencePath -Value $failureDiagnostic
                    $summary = New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary `
                        -SmokeRoot $ResolvedSmokeRoot `
                        -BinaryPath $ResolvedBinaryPath `
                        -ConfigPath $configPath `
                        -AudioOverridePath $ResolvedAudioOverridePath `
                        -CapturedText $capturedText `
                        -CapturedTextMatchesOutput $false `
                        -FailureKind ([string]$failure.FailureKind) `
                        -FailureSummary ([string]$failure.FailureSummary) `
                        -FailureEvidencePath $failureEvidencePath
                    Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
                    throw ($summary | ConvertTo-Json -Depth 8 -Compress)
                }

                throw
            }
        }
        try {
            $capturedText = Invoke-TalkDesktopPinnedWindowOperation -Hwnd $target.Hwnd -ScriptBlock {
                Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
                if (
                    -not [string]::IsNullOrWhiteSpace($ResolvedTextCapturePrimer) -and
                    $target.TextBoxHwnd -and
                    $target.TextBoxHwnd -ne [System.IntPtr]::Zero
                ) {
                    Select-TalkTextCaptureTargetChildText -ChildHwnd $target.TextBoxHwnd | Out-Null
                }

                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Start-Sleep -Milliseconds 250
                Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey

                Wait-TalkTextCaptureContainsWithForegroundRefresh `
                    -Hwnd $target.Hwnd `
                    -SnapshotPath $target.SnapshotPath `
                    -ExpectedText 'Paris' `
                    -TimeoutMs 30000
            }
        }
        catch {
            $errorMessage = [string]$_.Exception.Message
            $log = Find-LatestSessionLogIfAvailable -LogsDir $logsDir
            if ($null -ne $log) {
                $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
                $sessionStatus = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'status')
                $sessionTranscript = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'transcript')
                $sessionOutputText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'output_text')
                $sessionError = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'error')
                $capturedText = if (Test-Path -LiteralPath $target.SnapshotPath) {
                    Get-Content -LiteralPath $target.SnapshotPath -Raw -Encoding UTF8
                } else {
                    ''
                }
                $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
                if ($sessionStatus -ne 'completed') {
                    $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
                        -CapturedText ([string]$capturedText) `
                        -PrimerText $ResolvedTextCapturePrimer
                    $failureSummary = if ([string]::IsNullOrWhiteSpace($sessionError)) {
                        "Talk desktop Qwen probe session ended with status [$sessionStatus] before producing output_text"
                    } else {
                        $sessionError
                    }
                    $summary = New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary `
                        -SmokeRoot $ResolvedSmokeRoot `
                        -Status $sessionStatus `
                        -Transcript $sessionTranscript `
                        -OutputText $sessionOutputText `
                        -BinaryPath $ResolvedBinaryPath `
                        -ConfigPath $configPath `
                        -LogPath $log.FullName `
                        -AudioOverridePath $ResolvedAudioOverridePath `
                        -InsertTargetDiagnosticPath $insertTargetDiagnosticPath `
                        -CapturedText $normalizedCapturedText `
                        -CapturedTextMatchesOutput $false `
                        -FailureKind 'session_failed' `
                        -FailureSummary $failureSummary `
                        -FailureEvidencePath ''
                    Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
                    throw ($summary | ConvertTo-Json -Depth 8 -Compress)
                }
                $expectedOutputText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'output_text')
                if ([string]::IsNullOrWhiteSpace($expectedOutputText)) {
                    $expectedOutputText = 'Paris'
                }
                $failure = Get-TalkDesktopInsertFailureClassification `
                    -Scenario 'qwen-global-hotkey-probe' `
                    -ExpectedOutputText $expectedOutputText `
                    -TargetWindowTitle 'Talk Smoke Text Target' `
                    -Session $session `
                    -ErrorMessage $errorMessage
                if ($null -ne $failure) {
                    $failureEvidencePath = Join-Path $ResolvedSmokeRoot 'failure-diagnostic.json'
                    $failureDiagnostic = [pscustomobject][ordered]@{
                        scenario = 'qwen-global-hotkey-probe'
                        failureKind = [string]$failure.FailureKind
                        failureSummary = [string]$failure.FailureSummary
                        expectedOutputText = $expectedOutputText
                        capturedText = if ([string]::IsNullOrWhiteSpace($capturedText)) { $null } else { $capturedText }
                        foregroundTail = [string]$failure.ForegroundTail
                        errorMessage = $errorMessage
                        snapshotPath = $target.SnapshotPath
                        logPath = $log.FullName
                        insertTargetDiagnosticPath = $insertTargetDiagnosticPath
                    }
                    Write-TalkDesktopSmokeJson -Path $failureEvidencePath -Value $failureDiagnostic
                    $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
                        -CapturedText ([string]$capturedText) `
                        -PrimerText $ResolvedTextCapturePrimer
                    $capturedTextMatchesOutput =
                        $normalizedCapturedText.Trim() -eq ([string]$expectedOutputText).Trim()
                    $summary = New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary `
                        -SmokeRoot $ResolvedSmokeRoot `
                        -Status ([string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'status')) `
                        -Transcript ([string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'transcript')) `
                        -OutputText ([string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'output_text')) `
                        -BinaryPath $ResolvedBinaryPath `
                        -ConfigPath $configPath `
                        -LogPath $log.FullName `
                        -AudioOverridePath $ResolvedAudioOverridePath `
                        -InsertTargetDiagnosticPath $insertTargetDiagnosticPath `
                        -CapturedText $normalizedCapturedText `
                        -CapturedTextMatchesOutput $capturedTextMatchesOutput `
                        -FailureKind ([string]$failure.FailureKind) `
                        -FailureSummary ([string]$failure.FailureSummary) `
                        -FailureEvidencePath $failureEvidencePath
                    Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
                    throw ($summary | ConvertTo-Json -Depth 8 -Compress)
                }
            }

            throw
        }

        $log = Wait-LatestSessionLog -LogsDir $logsDir -TimeoutMs 30000
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        $sessionStatus = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'status')
        $sessionTranscript = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'transcript')
        $sessionOutputText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'output_text')
        $sessionError = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'error')
        if ($sessionStatus -ne 'completed') {
            $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
                -CapturedText ([string]$capturedText) `
                -PrimerText $ResolvedTextCapturePrimer
            $failureSummary = if ([string]::IsNullOrWhiteSpace($sessionError)) {
                "Talk desktop Qwen probe session ended with status [$sessionStatus] before producing output_text"
            } else {
                $sessionError
            }
            $summary = New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary `
                -SmokeRoot $ResolvedSmokeRoot `
                -Status $sessionStatus `
                -Transcript $sessionTranscript `
                -OutputText $sessionOutputText `
                -BinaryPath $ResolvedBinaryPath `
                -ConfigPath $configPath `
                -LogPath $log.FullName `
                -AudioOverridePath $ResolvedAudioOverridePath `
                -InsertTargetDiagnosticPath (Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName) `
                -CapturedText $normalizedCapturedText `
                -CapturedTextMatchesOutput $false `
                -FailureKind 'session_failed' `
                -FailureSummary $failureSummary `
                -FailureEvidencePath ''
            Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
            throw ($summary | ConvertTo-Json -Depth 8 -Compress)
        }
        if ([string]::IsNullOrWhiteSpace($sessionOutputText)) {
            $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
                -CapturedText ([string]$capturedText) `
                -PrimerText $ResolvedTextCapturePrimer
            $summary = New-TalkDesktopQwenGlobalHotkeyProbeFailureSummary `
                -SmokeRoot $ResolvedSmokeRoot `
                -Status $sessionStatus `
                -Transcript $sessionTranscript `
                -OutputText $sessionOutputText `
                -BinaryPath $ResolvedBinaryPath `
                -ConfigPath $configPath `
                -LogPath $log.FullName `
                -AudioOverridePath $ResolvedAudioOverridePath `
                -InsertTargetDiagnosticPath (Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName) `
                -CapturedText $normalizedCapturedText `
                -CapturedTextMatchesOutput $false `
                -FailureKind 'session_missing_output' `
                -FailureSummary 'Expected Talk desktop Qwen probe session to produce output_text' `
                -FailureEvidencePath ''
            Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
            throw ($summary | ConvertTo-Json -Depth 8 -Compress)
        }
        $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName

        $summary = New-TalkDesktopQwenGlobalHotkeyProbeSummary `
            -SmokeRoot $ResolvedSmokeRoot `
            -Session $session `
            -ConfigPath $configPath `
            -LogPath $log.FullName `
            -AudioOverridePath $ResolvedAudioOverridePath `
            -BinaryPath $ResolvedBinaryPath `
            -InsertTargetDiagnosticPath $insertTargetDiagnosticPath
        $normalizedCapturedText = Resolve-TalkDesktopQwenProbeCapturedText `
            -SnapshotPath $target.SnapshotPath `
            -CapturedText ([string]$capturedText) `
            -ExpectedOutputText $sessionOutputText `
            -PrimerText $ResolvedTextCapturePrimer
        $capturedTextMatchesOutput =
            $normalizedCapturedText.Trim() -eq $sessionOutputText.Trim()
        $summary | Add-Member -NotePropertyName capturedText -NotePropertyValue $normalizedCapturedText
        $summary | Add-Member -NotePropertyName capturedTextMatchesOutput -NotePropertyValue $capturedTextMatchesOutput
        $summaryPath = Join-Path $ResolvedSmokeRoot 'qwen-global-hotkey-probe-summary.json'
        $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $ResolvedSmokeRoot
        $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath

        Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summaryPath -Summary $summary
        $summary
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkTextCaptureTarget -Target $target
    }
}

function Invoke-TalkDesktopQwenGlobalHotkeyProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$SmokeRoot,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$AudioOverridePath,
        [string]$TextCapturePrimer
    )

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-qwen-global-hotkey-probe-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $resolvedSmokeRoot -Force | Out-Null

    $resolvedBinaryPath = Resolve-TalkDesktopBinaryPath -BinaryPath $BinaryPath -ReleaseDir $ReleaseDir
    $resolvedApiKey = Resolve-TalkDesktopQwenProbeApiKey -ApiKey $ApiKey -ApiKeyJsonPath $ApiKeyJsonPath
    $resolvedAudioOverridePath = Resolve-TalkDesktopQwenProbeAudioOverridePath -AudioOverridePath $AudioOverridePath -SmokeRoot $resolvedSmokeRoot
    $resolvedTextCapturePrimer = [string]$TextCapturePrimer
    $maxHostileForegroundRetries = 1
    $retryCount = 0

    while ($true) {
        $attemptSmokeRoot = Get-TalkDesktopQwenGlobalHotkeyProbeAttemptSmokeRoot `
            -SmokeRoot $resolvedSmokeRoot `
            -AttemptNumber ($retryCount + 1)
        if (Test-Path -LiteralPath $attemptSmokeRoot) {
            Remove-Item -LiteralPath $attemptSmokeRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
        New-Item -ItemType Directory -Path $attemptSmokeRoot -Force | Out-Null

        try {
            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbeAttempt `
                -ResolvedBinaryPath $resolvedBinaryPath `
                -ResolvedSmokeRoot $attemptSmokeRoot `
                -ResolvedApiKey $resolvedApiKey `
                -ResolvedAudioOverridePath $resolvedAudioOverridePath `
                -ResolvedTextCapturePrimer $resolvedTextCapturePrimer
            if ($retryCount -gt 0) {
                $summary | Add-Member -Force -NotePropertyName retryCount -NotePropertyValue $retryCount
                $summary | Add-Member -Force -NotePropertyName retryReason -NotePropertyValue 'hostile_foreground_environment'
                Write-TalkDesktopQwenGlobalHotkeyProbeSummaryFile -Path $summary.summaryPath -Summary $summary
            }
            return $summary
        }
        catch {
            $payload = Convert-TalkDesktopQwenGlobalHotkeyProbeErrorPayload -ErrorRecord $_
            if ($null -eq $payload) {
                throw
            }

            $failureKind = if ($payload.PSObject.Properties['failureKind']) {
                [string]$payload.failureKind
            } else {
                ''
            }
            if (
                $failureKind -eq 'hostile_foreground_environment' -and
                $retryCount -lt $maxHostileForegroundRetries
            ) {
                $retryCount += 1
                continue
            }

            throw ($payload | ConvertTo-Json -Depth 8 -Compress)
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopQwenGlobalHotkeyProbe `
        -BinaryPath $requestedBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -SmokeRoot $requestedSmokeRoot `
        -ApiKey $requestedApiKey `
        -ApiKeyJsonPath $requestedApiKeyJsonPath `
        -AudioOverridePath $requestedAudioOverridePath `
        -TextCapturePrimer $requestedTextCapturePrimer
}
