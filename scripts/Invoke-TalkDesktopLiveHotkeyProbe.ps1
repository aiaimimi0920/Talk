[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$SmokeRoot,
    [string]$Hotkey = 'Ctrl+Alt+F20',
    [int]$InitialDelaySeconds = 12,
    [int]$RecordingSeconds = 6,
    [int]$AudioProbeSeconds = 3,
    [int]$TimeoutSeconds = 30,
    [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
    [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
    [string]$ProviderChatModel = 'qwen3.7-plus',
    [string]$PromptText = '请回复测试成功。请回复测试成功。',
    [string]$ExpectedText = '测试成功',
    [string]$InputDevice = '麦克风'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedBinaryPath = $BinaryPath
$requestedReleaseDir = $ReleaseDir
$requestedApiKey = $ApiKey
$requestedApiKeyJsonPath = $ApiKeyJsonPath
$requestedSmokeRoot = $SmokeRoot
$requestedHotkey = $Hotkey
$requestedInitialDelaySeconds = $InitialDelaySeconds
$requestedRecordingSeconds = $RecordingSeconds
$requestedAudioProbeSeconds = $AudioProbeSeconds
$requestedTimeoutSeconds = $TimeoutSeconds
$requestedProviderAudioTranscriptionsEndpoint = $ProviderAudioTranscriptionsEndpoint
$requestedProviderChatCompletionsEndpoint = $ProviderChatCompletionsEndpoint
$requestedProviderTranscriptionTransport = $ProviderTranscriptionTransport
$requestedProviderTranscriptionModel = $ProviderTranscriptionModel
$requestedProviderChatModel = $ProviderChatModel
$requestedPromptText = $PromptText
$requestedExpectedText = $ExpectedText
$requestedInputDevice = $InputDevice

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

function New-TalkDesktopLiveHotkeyProbeConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [string]$InputDevice,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus'
    )

    $inputDeviceLine = if ([string]::IsNullOrWhiteSpace($InputDevice)) {
        ''
    }
    else {
        "input_device = `"$InputDevice`""
    }

    @"
voice_mode = "command"

[trigger]
mode = "push_to_talk"
toggle_shortcut = "$Hotkey"

[audio]
backend = "native_windows"
$inputDeviceLine
max_recording_seconds = 12
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "openai_compatible"
transcription_transport = "$ProviderTranscriptionTransport"
audio_transcriptions_endpoint = "$ProviderAudioTranscriptionsEndpoint"
chat_completions_endpoint = "$ProviderChatCompletionsEndpoint"
transcription_model = "$ProviderTranscriptionModel"
chat_model = "$ProviderChatModel"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "clipboard_paste"
restore_clipboard = true
clipboard_backend = "native_windows"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkDesktopLiveHotkeyProbeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [string]$InputDevice,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus'
    )

    $audioDir = Join-Path $ScenarioRoot 'audio'
    $logsDir = Join-Path $ScenarioRoot 'logs'
    New-Item -ItemType Directory -Path $ScenarioRoot -Force | Out-Null

    $configText = New-TalkDesktopLiveHotkeyProbeConfigContent `
        -Hotkey $Hotkey `
        -AudioDir $audioDir `
        -LogsDir $logsDir `
        -InputDevice $InputDevice `
        -ProviderAudioTranscriptionsEndpoint $ProviderAudioTranscriptionsEndpoint `
        -ProviderChatCompletionsEndpoint $ProviderChatCompletionsEndpoint `
        -ProviderTranscriptionTransport $ProviderTranscriptionTransport `
        -ProviderTranscriptionModel $ProviderTranscriptionModel `
        -ProviderChatModel $ProviderChatModel
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $ConfigPath,
        ($configText.Trim() + [Environment]::NewLine),
        $utf8NoBom
    )
}

function Test-TalkDesktopLiveHotkeyProbeExpectedTextMatch {
    param(
        [string]$ExpectedText,
        [string]$OutputText,
        [string]$CapturedText
    )

    if ([string]::IsNullOrWhiteSpace($ExpectedText)) {
        return $true
    }
    if (-not [string]::IsNullOrWhiteSpace($OutputText) -and $OutputText -like "*$ExpectedText*") {
        return $true
    }
    if (-not [string]::IsNullOrWhiteSpace($CapturedText) -and $CapturedText -like "*$ExpectedText*") {
        return $true
    }
    return $false
}

function Test-TalkDesktopLiveHotkeyAudioProbeHasSignal {
    param($ProbeSummary)

    if ($null -eq $ProbeSummary) {
        return $false
    }
    if ([bool]$ProbeSummary.silent) {
        return $false
    }
    return ([double]$ProbeSummary.peak -gt 0)
}

function Invoke-TalkDesktopLiveHotkeyAudioProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$Hotkey,
        [string]$InputDevice,
        [int]$AudioProbeSeconds
    )

    $resolvedTalkReleaseDir = Resolve-TalkDesktopLaunchReleaseDir -ReleaseDir $ReleaseDir -BinaryPath $BinaryPath
    $resolvedTalkBinaryPath = Resolve-TalkDesktopLaunchTalkBinaryPath -ReleaseDir $resolvedTalkReleaseDir
    $resolvedBaseConfigPath = Resolve-TalkDesktopLaunchConfigPath -ReleaseDir $resolvedTalkReleaseDir
    $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
        -BaseConfigPath $resolvedBaseConfigPath `
        -Hotkey $Hotkey `
        -InputDevice $InputDevice
    $readinessReport = Invoke-TalkDesktopLaunchReadiness `
        -TalkBinaryPath $resolvedTalkBinaryPath `
        -EffectiveConfigPath $effectiveConfigPath `
        -WorkingDirectory $resolvedTalkReleaseDir
    $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
    $probeReport = Invoke-TalkDesktopLaunchAudioProbe `
        -TalkBinaryPath $resolvedTalkBinaryPath `
        -EffectiveConfigPath $effectiveConfigPath `
        -ProbeSeconds $AudioProbeSeconds

    New-TalkDesktopLaunchAudioProbeSummary -ProbeReport $probeReport -Inventory $inventory
}

function Get-TalkDesktopLiveHotkeyProbeWavSignalSummary {
    param([string]$AudioPath)

    if ([string]::IsNullOrWhiteSpace($AudioPath) -or -not (Test-Path -LiteralPath $AudioPath)) {
        return $null
    }

    $bytes = [System.IO.File]::ReadAllBytes($AudioPath)
    if ($bytes.Length -lt 44) {
        return $null
    }

    $offset = 12
    $channels = 0
    $sampleRate = 0
    $bitsPerSample = 0
    $dataOffset = 0
    $dataSize = 0

    while ($offset + 8 -le $bytes.Length) {
        $chunkId = [System.Text.Encoding]::ASCII.GetString($bytes, $offset, 4)
        $chunkSize = [BitConverter]::ToUInt32($bytes, $offset + 4)
        $chunkDataOffset = $offset + 8
        if ($chunkId -eq 'fmt ') {
            $channels = [BitConverter]::ToUInt16($bytes, $chunkDataOffset + 2)
            $sampleRate = [BitConverter]::ToUInt32($bytes, $chunkDataOffset + 4)
            $bitsPerSample = [BitConverter]::ToUInt16($bytes, $chunkDataOffset + 14)
        }
        elseif ($chunkId -eq 'data') {
            $dataOffset = $chunkDataOffset
            $dataSize = $chunkSize
            break
        }

        $offset = $chunkDataOffset + $chunkSize
        if (($chunkSize % 2) -eq 1) {
            $offset += 1
        }
    }

    if ($channels -le 0 -or $sampleRate -le 0 -or $bitsPerSample -ne 16 -or $dataSize -le 0) {
        return $null
    }

    $sampleCount = [int]($dataSize / 2)
    $sumSquares = 0.0
    $peak = 0.0
    for ($index = 0; $index -lt $sampleCount; $index++) {
        $sample = [BitConverter]::ToInt16($bytes, $dataOffset + ($index * 2))
        $normalized = $sample / 32767.0
        $absolute = [math]::Abs($normalized)
        if ($absolute -gt $peak) {
            $peak = $absolute
        }
        $sumSquares += ($normalized * $normalized)
    }

    $rms = if ($sampleCount -gt 0) {
        [math]::Sqrt($sumSquares / $sampleCount)
    }
    else {
        0.0
    }

    [pscustomobject][ordered]@{
        sampleRate = [int]$sampleRate
        channels = [int]$channels
        bitsPerSample = [int]$bitsPerSample
        durationSeconds = [math]::Round((($sampleCount / [double]$channels) / $sampleRate), 3)
        peak = [math]::Round($peak, 6)
        rms = [math]::Round($rms, 6)
    }
}

function New-TalkDesktopLiveHotkeyProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [Parameter(Mandatory = $true)]$Session,
        [string]$CapturedText,
        [string]$ExpectedText,
        [string]$PromptText,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [string]$InputDevice,
        [string]$LogPath,
        [string]$SnapshotPath,
        [string]$AudioPath,
        [string]$ConfigPath,
        [int]$ProcessId,
        $AudioProbe,
        $AudioSignal
    )

    [pscustomobject][ordered]@{
        scenarioRoot = $ScenarioRoot
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        error = [string]$Session.error
        capturedText = [string]$CapturedText
        expectedText = [string]$ExpectedText
        promptText = [string]$PromptText
        matchedExpected = [bool](Test-TalkDesktopLiveHotkeyProbeExpectedTextMatch `
                -ExpectedText $ExpectedText `
                -OutputText ([string]$Session.output_text) `
                -CapturedText $CapturedText)
        hotkey = $Hotkey
        inputDevice = [string]$InputDevice
        logPath = $LogPath
        snapshotPath = $SnapshotPath
        audioPath = $AudioPath
        audioProbe = $AudioProbe
        audioSignal = $AudioSignal
        configPath = $ConfigPath
        processId = $ProcessId
    }
}

function Write-TalkDesktopLiveHotkeyProbeSummaryFile {
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

function Invoke-TalkDesktopLiveHotkeyProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$SmokeRoot,
        [string]$Hotkey = 'Ctrl+Alt+F20',
        [int]$InitialDelaySeconds = 12,
        [int]$RecordingSeconds = 6,
        [int]$AudioProbeSeconds = 3,
        [int]$TimeoutSeconds = 30,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus',
        [string]$PromptText = '请回复测试成功。请回复测试成功。',
        [string]$ExpectedText = '测试成功',
        [string]$InputDevice = '麦克风'
    )

    if ($InitialDelaySeconds -lt 0) {
        throw 'Talk desktop live hotkey probe initial delay must be greater than or equal to 0'
    }
    if ($RecordingSeconds -le 0) {
        throw 'Talk desktop live hotkey probe recording seconds must be greater than 0'
    }
    if ($AudioProbeSeconds -le 0) {
        throw 'Talk desktop live hotkey probe audio probe seconds must be greater than 0'
    }
    if ($TimeoutSeconds -le 0) {
        throw 'Talk desktop live hotkey probe timeout seconds must be greater than 0'
    }

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-live-hotkey-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    }
    else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }

    Ensure-TalkDesktopSmokeWin32Type

    $configPath = Join-Path $resolvedSmokeRoot 'config.toml'
    Write-TalkDesktopLiveHotkeyProbeConfig `
        -ConfigPath $configPath `
        -Hotkey $Hotkey `
        -ScenarioRoot $resolvedSmokeRoot `
        -InputDevice $InputDevice `
        -ProviderAudioTranscriptionsEndpoint $ProviderAudioTranscriptionsEndpoint `
        -ProviderChatCompletionsEndpoint $ProviderChatCompletionsEndpoint `
        -ProviderTranscriptionTransport $ProviderTranscriptionTransport `
        -ProviderTranscriptionModel $ProviderTranscriptionModel `
        -ProviderChatModel $ProviderChatModel

    $target = $null
    $instance = $null
    $launchSummary = $null
    $audioProbe = $null
    try {
        Write-Host ''
        Write-Host 'Talk live hotkey audio preflight is ready.' -ForegroundColor Green
        if ([string]::IsNullOrWhiteSpace($InputDevice)) {
            Write-Host 'Input device: <default>'
        }
        else {
            Write-Host ("Input device: {0}" -f $InputDevice)
        }
        Write-Host ("Speak normally for {0}s after the countdown." -f $AudioProbeSeconds)
        for ($countdown = 3; $countdown -ge 1; $countdown--) {
            Write-Host ("Starting native audio probe in {0}..." -f $countdown)
            Start-Sleep -Seconds 1
        }

        $audioProbe = Invoke-TalkDesktopLiveHotkeyAudioProbe `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -Hotkey $Hotkey `
            -InputDevice $InputDevice `
            -AudioProbeSeconds $AudioProbeSeconds

        Write-Host ("Preflight selected input: {0}" -f $(if ([string]::IsNullOrWhiteSpace([string]$audioProbe.selectedInputDevice)) { '<unknown>' } else { [string]$audioProbe.selectedInputDevice }))
        Write-Host ("Preflight signal: duration={0}s peak={1} rms={2}" -f $audioProbe.durationSeconds, $audioProbe.peak, $audioProbe.rms)

        if (-not (Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary $audioProbe)) {
            $failureReason = 'Live hotkey audio probe captured only silence; speak louder or fix the selected input device'
            $summary = New-TalkDesktopLiveHotkeyProbeSummary `
                -ScenarioRoot $resolvedSmokeRoot `
                -Session ([pscustomobject]@{
                    status = 'failed'
                    transcript = $null
                    output_text = $null
                    error = $failureReason
                }) `
                -CapturedText '' `
                -ExpectedText $ExpectedText `
                -PromptText $PromptText `
                -Hotkey $Hotkey `
                -InputDevice $InputDevice `
                -LogPath '' `
                -SnapshotPath (Join-Path $resolvedSmokeRoot 'text-target\snapshot.txt') `
                -AudioPath '' `
                -ConfigPath $configPath `
                -ProcessId 0 `
                -AudioProbe $audioProbe `
                -AudioSignal $null
            $summaryPath = Join-Path $resolvedSmokeRoot 'live-hotkey-probe-summary.json'
            $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue $failureReason
            $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
            Write-TalkDesktopLiveHotkeyProbeSummaryFile -Path $summaryPath -Summary $summary
            throw $failureReason
        }

        $target = Start-TalkTextCaptureTarget -ScenarioRoot $resolvedSmokeRoot
        Set-TalkDesktopForegroundWindow -Hwnd $target.Hwnd | Out-Null

        $launchSummary = Start-TalkDesktop `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -ConfigPath $configPath `
            -ApiKey $ApiKey `
            -ApiKeyJsonPath $ApiKeyJsonPath

        $instance = [pscustomobject]@{
            Process = Get-Process -Id $launchSummary.processId -ErrorAction Stop
            Hwnd = (Find-WindowByProcessIdAndClass -TargetProcessId $launchSummary.processId -ClassName 'TalkDesktopMessageWindow' -TimeoutMs 10000)
        }

        Write-Host ''
        Write-Host 'Talk live hotkey probe is ready.' -ForegroundColor Green
        Write-Host ("Hotkey: {0}" -f $Hotkey)
        Write-Host ("Input device: {0}" -f $InputDevice)
        Write-Host ("Auto hold starts after {0}s and releases after {1}s." -f $InitialDelaySeconds, $RecordingSeconds)
        Write-Host ("Recommended spoken prompt: {0}" -f $PromptText)
        Write-Host ("Foreground target: {0}" -f $target.WindowTitle)
        Write-Host ("Desktop config: {0}" -f $configPath)
        Write-Host ''

        if ($InitialDelaySeconds -gt 0) {
            Start-Sleep -Seconds $InitialDelaySeconds
        }

        1..3 | ForEach-Object {
            [console]::Beep(1200, 180)
            Start-Sleep -Milliseconds 200
        }

        Set-TalkDesktopForegroundWindow -Hwnd $target.Hwnd | Out-Null
        Invoke-TalkDesktopGlobalHotkeyOperation -Shortcut $Hotkey -ScriptBlock {
            Start-Sleep -Seconds $RecordingSeconds
        }

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $resolvedSmokeRoot 'logs') -TimeoutMs ($TimeoutSeconds * 1000)
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json

        $snapshotPath = Join-Path $resolvedSmokeRoot 'text-target\snapshot.txt'
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
                    -SnapshotPath $snapshotPath `
                    -ExpectedText $expectedInsertedText `
                    -TimeoutMs ($TimeoutSeconds * 1000)
            } elseif (Test-Path -LiteralPath $snapshotPath) {
                $capturedText = [string](Get-Content -LiteralPath $snapshotPath -Raw)
            }
        } elseif (Test-Path -LiteralPath $snapshotPath) {
            $capturedText = [string](Get-Content -LiteralPath $snapshotPath -Raw)
        }
        $audioArtifact = Get-ChildItem -LiteralPath (Join-Path $resolvedSmokeRoot 'audio') -Filter '*.wav' -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending |
            Select-Object -First 1
        $audioPath = if ($audioArtifact) { $audioArtifact.FullName } else { '' }
        $audioSignal = Get-TalkDesktopLiveHotkeyProbeWavSignalSummary -AudioPath $audioPath

        $summary = New-TalkDesktopLiveHotkeyProbeSummary `
            -ScenarioRoot $resolvedSmokeRoot `
            -Session $session `
            -CapturedText $capturedText `
            -ExpectedText $ExpectedText `
            -PromptText $PromptText `
            -Hotkey $Hotkey `
            -InputDevice $InputDevice `
            -LogPath $log.FullName `
            -SnapshotPath $snapshotPath `
            -AudioPath $audioPath `
            -ConfigPath $configPath `
            -ProcessId $launchSummary.processId `
            -AudioProbe $audioProbe `
            -AudioSignal $audioSignal
        $summaryPath = Join-Path $resolvedSmokeRoot 'live-hotkey-probe-summary.json'
        $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
        Write-TalkDesktopLiveHotkeyProbeSummaryFile -Path $summaryPath -Summary $summary
        $summary
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkTextCaptureTarget -Target $target
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopLiveHotkeyProbe `
        -BinaryPath $requestedBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -ApiKey $requestedApiKey `
        -ApiKeyJsonPath $requestedApiKeyJsonPath `
        -SmokeRoot $requestedSmokeRoot `
        -Hotkey $requestedHotkey `
        -InitialDelaySeconds $requestedInitialDelaySeconds `
        -RecordingSeconds $requestedRecordingSeconds `
        -AudioProbeSeconds $requestedAudioProbeSeconds `
        -TimeoutSeconds $requestedTimeoutSeconds `
        -ProviderAudioTranscriptionsEndpoint $requestedProviderAudioTranscriptionsEndpoint `
        -ProviderChatCompletionsEndpoint $requestedProviderChatCompletionsEndpoint `
        -ProviderTranscriptionTransport $requestedProviderTranscriptionTransport `
        -ProviderTranscriptionModel $requestedProviderTranscriptionModel `
        -ProviderChatModel $requestedProviderChatModel `
        -PromptText $requestedPromptText `
        -ExpectedText $requestedExpectedText `
        -InputDevice $requestedInputDevice
}
