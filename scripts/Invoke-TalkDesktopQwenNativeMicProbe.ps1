[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$SmokeRoot,
    [string]$Hotkey = 'Ctrl+Alt+F14',
    [int]$TimeoutSeconds = 45,
    [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
    [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
    [string]$ProviderChatModel = 'qwen3.7-plus',
    [string]$PromptText = 'What is the capital of France?',
    [string]$ExpectedText = 'Paris',
    [string]$SpeakerWavPath,
    [string]$InputDevice,
    [string]$SpeakerOutputDevice
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
$requestedProviderAudioTranscriptionsEndpoint = $ProviderAudioTranscriptionsEndpoint
$requestedProviderChatCompletionsEndpoint = $ProviderChatCompletionsEndpoint
$requestedProviderTranscriptionTransport = $ProviderTranscriptionTransport
$requestedProviderTranscriptionModel = $ProviderTranscriptionModel
$requestedProviderChatModel = $ProviderChatModel
$requestedPromptText = $PromptText
$requestedExpectedText = $ExpectedText
$requestedSpeakerWavPath = $SpeakerWavPath
$requestedInputDevice = $InputDevice
$requestedSpeakerOutputDevice = $SpeakerOutputDevice

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

function New-TalkDesktopQwenNativeMicProbeConfigContent {
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
    } else {
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
max_recording_seconds = 15
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

function Write-TalkDesktopQwenNativeMicProbeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [string]$InputDevice,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus'
    )

    $scenarioRoot = Split-Path -Parent $ConfigPath
    $audioDir = Join-Path $scenarioRoot 'audio'
    $logsDir = Join-Path $scenarioRoot 'logs'
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    $configText = New-TalkDesktopQwenNativeMicProbeConfigContent `
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
    [System.IO.File]::WriteAllText($ConfigPath, $configText, $utf8NoBom)
}

function Get-TalkDesktopTextCaptureSnapshotText {
    param([string]$SnapshotPath)

    if ([string]::IsNullOrWhiteSpace($SnapshotPath)) {
        return ''
    }
    if (-not (Test-Path -LiteralPath $SnapshotPath)) {
        return ''
    }

    [string](Get-Content -LiteralPath $SnapshotPath -Raw)
}

function Test-TalkDesktopQwenNativeMicProbeExpectedTextMatch {
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

function Write-TalkDesktopQwenNativeMicProbeSpeakerAudio {
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

function Resolve-TalkDesktopQwenNativeMicProbeSpeakerAudioPath {
    param(
        [string]$SpeakerWavPath,
        [string]$SmokeRoot,
        [string]$PromptText = 'What is the capital of France?'
    )

    if (-not [string]::IsNullOrWhiteSpace($SpeakerWavPath)) {
        return [System.IO.Path]::GetFullPath($SpeakerWavPath)
    }

    $generatedAudioPath = Join-Path $SmokeRoot 'probe.wav'
    Write-TalkDesktopQwenNativeMicProbeSpeakerAudio `
        -AudioPath $generatedAudioPath `
        -PromptText $PromptText
    [System.IO.Path]::GetFullPath($generatedAudioPath)
}

function Resolve-TalkDesktopQwenNativeMicProbeSpeakerOutputDevice {
    param(
        [string]$SpeakerOutputDevice,
        [string]$InputDevice
    )

    if (-not [string]::IsNullOrWhiteSpace($SpeakerOutputDevice)) {
        return $SpeakerOutputDevice
    }
    if ([string]::IsNullOrWhiteSpace($InputDevice)) {
        return $null
    }
    if ($InputDevice -match 'Virtual Mic') {
        return 'Virtual Speakers'
    }
    $null
}

function Format-TalkDesktopQwenNativeMicProbeWindowHandle {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Handle)

    if ($Handle -eq [System.IntPtr]::Zero) {
        throw 'Talk desktop native mic probe window handle must not be zero'
    }

    ('0x{0:X}' -f $Handle.ToInt64())
}

function Get-TalkDesktopQwenNativeMicProbeInsertTargetEnvironmentOverrides {
    param([Parameter(Mandatory = $true)]$Target)

    $targetHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $Target -Name 'Hwnd'
    if ($null -eq $targetHwnd -or $targetHwnd -eq [System.IntPtr]::Zero) {
        throw 'Talk desktop native mic probe target must expose a non-zero Hwnd'
    }

    $overrides = [ordered]@{
        TALK_DESKTOP_INSERT_TARGET_WINDOW = (Format-TalkDesktopQwenNativeMicProbeWindowHandle -Handle $targetHwnd)
    }

    $textBoxHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $Target -Name 'TextBoxHwnd'
    if ($null -ne $textBoxHwnd -and $textBoxHwnd -ne [System.IntPtr]::Zero) {
        $overrides['TALK_DESKTOP_INSERT_TARGET_FOCUS'] =
            Format-TalkDesktopQwenNativeMicProbeWindowHandle -Handle $textBoxHwnd
    }

    $overrides
}

function Initialize-TalkDesktopQwenNativeMicProbeRoot {
    param([Parameter(Mandatory = $true)][string]$SmokeRoot)

    if (Test-Path -LiteralPath $SmokeRoot) {
        Remove-Item -LiteralPath $SmokeRoot -Recurse -Force
    }
    New-Item -ItemType Directory -Path $SmokeRoot -Force | Out-Null
}

function Get-TalkDesktopProbeWavSignalSummary {
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
    } else {
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

function Test-TalkDesktopAudioProbeHasSignal {
    param($ProbeSummary)

    if ($null -eq $ProbeSummary) {
        return $false
    }
    if ([bool]$ProbeSummary.silent) {
        return $false
    }
    return ([double]$ProbeSummary.peak -gt 0)
}

function Invoke-TalkDesktopNativeAudioSignalProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [string]$SpeakerWavPath,
        [string]$SpeakerOutputDevice,
        [string]$TalkBinaryPath
    )

    $speakerJob = $null
    try {
        if (-not [string]::IsNullOrWhiteSpace($SpeakerWavPath)) {
            $resolvedSpeakerWavPath = [System.IO.Path]::GetFullPath($SpeakerWavPath)
            $resolvedTalkBinaryPath = if ([string]::IsNullOrWhiteSpace($TalkBinaryPath)) {
                ''
            } else {
                [System.IO.Path]::GetFullPath($TalkBinaryPath)
            }
            $resolvedSpeakerOutputDevice = if ([string]::IsNullOrWhiteSpace($SpeakerOutputDevice)) {
                ''
            } else {
                $SpeakerOutputDevice
            }

            $speakerJob = Start-Job -ScriptBlock {
                param(
                    [string]$AudioPath,
                    [string]$ProbeTalkBinaryPath,
                    [string]$OutputDevice
                )

                Start-Sleep -Milliseconds 300

                if (-not [string]::IsNullOrWhiteSpace($OutputDevice)) {
                    if ([string]::IsNullOrWhiteSpace($ProbeTalkBinaryPath)) {
                        throw 'Talk desktop native mic preflight output route requires a Talk binary path'
                    }
                    & $ProbeTalkBinaryPath play-wav --file $AudioPath --output-device $OutputDevice
                    if ($LASTEXITCODE -ne 0) {
                        throw "Talk desktop native mic preflight play-wav failed for output device [$OutputDevice]"
                    }
                    return
                }

                Add-Type -AssemblyName System.Windows.Forms
                [System.Media.SystemSounds]::Asterisk.Play()
                Start-Sleep -Milliseconds 200

                $player = New-Object System.Media.SoundPlayer $AudioPath
                try {
                    $player.Load()
                    $player.PlaySync()
                    Start-Sleep -Milliseconds 350
                    $player.PlaySync()
                }
                finally {
                    $player.Dispose()
                }
            } -ArgumentList $resolvedSpeakerWavPath, $resolvedTalkBinaryPath, $resolvedSpeakerOutputDevice
        }

        $probeSummary = Start-TalkDesktop `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -ConfigPath $ConfigPath `
            -ProbeAudio `
            -ProbeSeconds 3

        if ($null -ne $speakerJob) {
            Wait-TalkDesktopProbeSpeakerJob -Job $speakerJob
            if ($speakerJob.State -ne 'Completed') {
                $speakerErrorText = (($speakerJob.ChildJobs | ForEach-Object { $_.Error | ForEach-Object { $_.ToString() } }) -join '; ').Trim()
                if ([string]::IsNullOrWhiteSpace($speakerErrorText)) {
                    $speakerErrorText = 'unknown speaker playback failure'
                }
                throw "Talk desktop native mic preflight speaker playback failed: $speakerErrorText"
            }
        }

        $probeSummary
    }
    finally {
        if ($null -ne $speakerJob) {
            Remove-TalkDesktopProbeSpeakerJob -Job $speakerJob
        }
    }
}

function Wait-TalkDesktopProbeSpeakerJob {
    param($Job)

    if ($null -eq $Job) {
        return
    }

    Wait-Job -Job $Job | Out-Null
}

function Remove-TalkDesktopProbeSpeakerJob {
    param($Job)

    if ($null -eq $Job) {
        return
    }

    Remove-Job -Job $Job -Force -ErrorAction SilentlyContinue
}

function Invoke-TalkDesktopProbeSpeech {
    param(
        [Parameter(Mandatory = $true)][string]$Text
    )

    Add-Type -AssemblyName System.Speech
    $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
    try {
        $synth.Volume = 100
        $synth.Rate = -1
        $synth.Speak($Text)
        Start-Sleep -Milliseconds 350
        $synth.Speak($Text)
    }
    finally {
        $synth.Dispose()
    }
}

function Invoke-TalkDesktopProbeSpeakerWav {
    param(
        [Parameter(Mandatory = $true)][string]$AudioPath,
        [string]$TalkBinaryPath,
        [string]$OutputDevice
    )

    if (-not (Test-Path -LiteralPath $AudioPath)) {
        throw "Talk desktop native mic probe speaker wav does not exist: $AudioPath"
    }

    if (-not [string]::IsNullOrWhiteSpace($OutputDevice)) {
        if ([string]::IsNullOrWhiteSpace($TalkBinaryPath)) {
            throw 'Talk desktop native mic probe speaker output device requires a Talk binary path'
        }
        & $TalkBinaryPath play-wav --file $AudioPath --output-device $OutputDevice
        if ($LASTEXITCODE -ne 0) {
            throw "Talk desktop native mic probe play-wav failed for output device [$OutputDevice]"
        }
        return
    }

    Add-Type -AssemblyName System.Windows.Forms
    [System.Media.SystemSounds]::Asterisk.Play()
    Start-Sleep -Milliseconds 200

    $player = New-Object System.Media.SoundPlayer $AudioPath
    try {
        $player.Load()
        $player.PlaySync()
        Start-Sleep -Milliseconds 350
        $player.PlaySync()
    }
    finally {
        $player.Dispose()
    }
}

function New-TalkDesktopQwenNativeMicProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)]$Session,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [string]$LogPath,
        [Parameter(Mandatory = $true)][string]$BinaryPath,
        [Parameter(Mandatory = $true)][string]$SpokenText,
        [string]$ExpectedText,
        [string]$InputDevice
    )

    [pscustomobject][ordered]@{
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        binaryPath = $BinaryPath
        configPath = $ConfigPath
        logPath = $LogPath
        spokenText = $SpokenText
        expectedText = $ExpectedText
        inputDevice = $InputDevice
        snapshotPath = Join-Path $SmokeRoot 'text-target\snapshot.txt'
    }
}

function Write-TalkDesktopQwenNativeMicProbeSummaryFile {
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

function Invoke-TalkDesktopQwenNativeMicProbe {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$SmokeRoot,
        [string]$Hotkey = 'Ctrl+Alt+F14',
        [int]$TimeoutSeconds = 45,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus',
        [string]$PromptText = 'What is the capital of France?',
        [string]$ExpectedText = 'Paris',
        [string]$SpeakerWavPath,
        [string]$InputDevice,
        [string]$SpeakerOutputDevice
    )

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-qwen-native-mic-probe-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    Initialize-TalkDesktopQwenNativeMicProbeRoot -SmokeRoot $resolvedSmokeRoot

    Ensure-TalkDesktopSmokeWin32Type
    $target = $null
    $launchSummary = $null
    $instance = $null
    $audioProbe = $null
    $resolvedSpeakerWavPath = Resolve-TalkDesktopQwenNativeMicProbeSpeakerAudioPath `
        -SpeakerWavPath $SpeakerWavPath `
        -SmokeRoot $resolvedSmokeRoot `
        -PromptText $PromptText
    $resolvedSpeakerOutputDevice = Resolve-TalkDesktopQwenNativeMicProbeSpeakerOutputDevice `
        -SpeakerOutputDevice $SpeakerOutputDevice `
        -InputDevice $InputDevice
    $resolvedReleaseDir = Resolve-TalkDesktopLaunchReleaseDir -ReleaseDir $ReleaseDir -BinaryPath $BinaryPath
    $resolvedBinaryPath = Resolve-TalkDesktopLaunchBinaryPath -BinaryPath $BinaryPath -ReleaseDir $resolvedReleaseDir
    $resolvedTalkBinaryPath = Resolve-TalkDesktopLaunchTalkBinaryPath -ReleaseDir $resolvedReleaseDir
    $textCapturePrimer = 'talkprimerready'
    $insertTargetEnvironmentOverrides = $null
    try {
        $target = Start-TalkTextCaptureTarget -ScenarioRoot $resolvedSmokeRoot
        $insertTargetEnvironmentOverrides =
            Get-TalkDesktopQwenNativeMicProbeInsertTargetEnvironmentOverrides -Target $target
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
        Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd $target.Hwnd `
            -SnapshotPath $target.SnapshotPath `
            -PrimerText $textCapturePrimer `
            -ChildHwnd $target.TextBoxHwnd | Out-Null
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null

        $configPath = Join-Path $resolvedSmokeRoot 'config.toml'
        Write-TalkDesktopQwenNativeMicProbeConfig `
            -ConfigPath $configPath `
            -Hotkey $Hotkey `
            -InputDevice $InputDevice `
            -ProviderAudioTranscriptionsEndpoint $ProviderAudioTranscriptionsEndpoint `
            -ProviderChatCompletionsEndpoint $ProviderChatCompletionsEndpoint `
            -ProviderTranscriptionTransport $ProviderTranscriptionTransport `
            -ProviderTranscriptionModel $ProviderTranscriptionModel `
            -ProviderChatModel $ProviderChatModel

        $audioProbe = Invoke-TalkDesktopNativeAudioSignalProbe `
            -BinaryPath $BinaryPath `
            -ReleaseDir $ReleaseDir `
            -ConfigPath $configPath `
            -SpeakerWavPath $resolvedSpeakerWavPath `
            -SpeakerOutputDevice $resolvedSpeakerOutputDevice `
            -TalkBinaryPath $resolvedTalkBinaryPath
        if (-not (Test-TalkDesktopAudioProbeHasSignal -ProbeSummary $audioProbe)) {
            $failureReason = 'Native audio probe captured only silence; input device routing is unusable'
            $summary = New-TalkDesktopQwenNativeMicProbeSummary `
                -SmokeRoot $resolvedSmokeRoot `
                -Session ([pscustomobject]@{
                    status = 'failed'
                    transcript = $null
                    output_text = $null
                }) `
                -ConfigPath $configPath `
                -LogPath '' `
                -BinaryPath $resolvedBinaryPath `
                -SpokenText $PromptText `
                -ExpectedText $ExpectedText `
                -InputDevice $InputDevice
            $summary | Add-Member -NotePropertyName capturedText -NotePropertyValue ''
            $summary | Add-Member -NotePropertyName matchedExpectedText -NotePropertyValue $false
            $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue $failureReason
            $summary | Add-Member -NotePropertyName speakerWavPath -NotePropertyValue $resolvedSpeakerWavPath
            $summary | Add-Member -NotePropertyName speakerOutputDevice -NotePropertyValue ([string]$resolvedSpeakerOutputDevice)
            $summary | Add-Member -NotePropertyName recordedWavPath -NotePropertyValue ([string]$audioProbe.artifactPath)
            $summary | Add-Member -NotePropertyName recordedWavSignal -NotePropertyValue $null
            $summary | Add-Member -NotePropertyName audioProbe -NotePropertyValue $audioProbe
            $summaryPath = Join-Path $resolvedSmokeRoot 'qwen-native-mic-probe-summary.json'
            $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
            $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
            Write-TalkDesktopQwenNativeMicProbeSummaryFile -Path $summaryPath -Summary $summary
            throw $failureReason
        }

        $previousInsertTargetWindow = [Environment]::GetEnvironmentVariable('TALK_DESKTOP_INSERT_TARGET_WINDOW', 'Process')
        $previousInsertTargetFocus = [Environment]::GetEnvironmentVariable('TALK_DESKTOP_INSERT_TARGET_FOCUS', 'Process')
        try {
            [Environment]::SetEnvironmentVariable(
                'TALK_DESKTOP_INSERT_TARGET_WINDOW',
                [string]$insertTargetEnvironmentOverrides['TALK_DESKTOP_INSERT_TARGET_WINDOW'],
                'Process'
            )
            [Environment]::SetEnvironmentVariable(
                'TALK_DESKTOP_INSERT_TARGET_FOCUS',
                [string]$insertTargetEnvironmentOverrides['TALK_DESKTOP_INSERT_TARGET_FOCUS'],
                'Process'
            )
            $launchSummary = Start-TalkDesktop `
                -BinaryPath $BinaryPath `
                -ReleaseDir $ReleaseDir `
                -ConfigPath $configPath `
                -ApiKey $ApiKey `
                -ApiKeyJsonPath $ApiKeyJsonPath
        }
        finally {
            [Environment]::SetEnvironmentVariable('TALK_DESKTOP_INSERT_TARGET_WINDOW', $previousInsertTargetWindow, 'Process')
            [Environment]::SetEnvironmentVariable('TALK_DESKTOP_INSERT_TARGET_FOCUS', $previousInsertTargetFocus, 'Process')
        }

        $instance = [pscustomobject]@{
            Process = Get-Process -Id $launchSummary.processId -ErrorAction Stop
            Hwnd = (Find-WindowByProcessIdAndClass -TargetProcessId $launchSummary.processId -ClassName 'TalkDesktopMessageWindow' -TimeoutMs 10000)
        }
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
        $sessionCapture = Invoke-TalkDesktopPinnedWindowOperation -Hwnd $target.Hwnd -ScriptBlock {
            Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
            if ($target.TextBoxHwnd -and $target.TextBoxHwnd -ne [System.IntPtr]::Zero) {
                Select-TalkTextCaptureTargetChildText -ChildHwnd $target.TextBoxHwnd | Out-Null
            }
            Invoke-TalkDesktopGlobalHotkeyOperation -Shortcut $Hotkey -ScriptBlock {
                Start-Sleep -Milliseconds 700
                Invoke-TalkDesktopProbeSpeakerWav `
                    -AudioPath $resolvedSpeakerWavPath `
                    -TalkBinaryPath $resolvedTalkBinaryPath `
                    -OutputDevice $resolvedSpeakerOutputDevice
                Start-Sleep -Milliseconds 500
            }

            $probeLog = Wait-LatestSessionLog -LogsDir (Join-Path $resolvedSmokeRoot 'logs') -TimeoutMs ($TimeoutSeconds * 1000)
            $probeSession = Get-Content -LiteralPath $probeLog.FullName -Raw | ConvertFrom-Json
            $probeCapturedText = $null
            $probeMatchedExpectedText = $false
            $probeCaptureFailure = $null
            if ($probeSession.status -eq 'failed' -and -not [string]::IsNullOrWhiteSpace([string]$probeSession.error)) {
                $probeCaptureFailure = [string]$probeSession.error
            }
            if ($probeSession.status -eq 'completed' -and -not [string]::IsNullOrWhiteSpace($ExpectedText)) {
                try {
                    $expectedInsertedText = if (-not [string]::IsNullOrWhiteSpace([string]$probeSession.output_text)) {
                        [string]$probeSession.output_text
                    } else {
                        $ExpectedText
                    }
                    $probeCapturedText = Wait-TalkTextCaptureContainsWithForegroundRefresh `
                        -Hwnd $target.Hwnd `
                        -SnapshotPath $target.SnapshotPath `
                        -ExpectedText $expectedInsertedText `
                        -TimeoutMs 10000
                    $probeCapturedText = Remove-TalkTextCapturePrimerPrefix `
                        -CapturedText ([string]$probeCapturedText) `
                        -PrimerText $textCapturePrimer
                    $probeMatchedExpectedText = $true
                }
                catch {
                    $probeCaptureFailure = $_.Exception.Message
                    $probeCapturedText = Get-TalkDesktopTextCaptureSnapshotText -SnapshotPath $target.SnapshotPath
                    $probeCapturedText = Remove-TalkTextCapturePrimerPrefix `
                        -CapturedText ([string]$probeCapturedText) `
                        -PrimerText $textCapturePrimer
                }
            }

            [pscustomobject]@{
                Log = $probeLog
                Session = $probeSession
                CapturedText = [string]$probeCapturedText
                MatchedExpectedText = [bool]$probeMatchedExpectedText
                CaptureFailure = [string]$probeCaptureFailure
            }
        }

        $log = $sessionCapture.Log
        $session = $sessionCapture.Session
        $recordedWavPath = Join-Path $resolvedSmokeRoot ("audio\{0}.wav" -f [string]$session.id)
        $recordedWavSignal = Get-TalkDesktopProbeWavSignalSummary -AudioPath $recordedWavPath

        $capturedText = [string]$sessionCapture.CapturedText
        $matchedExpectedText = [bool](Test-TalkDesktopQwenNativeMicProbeExpectedTextMatch `
                -ExpectedText $ExpectedText `
                -OutputText ([string]$session.output_text) `
                -CapturedText $capturedText)
        $captureFailure = [string]$sessionCapture.CaptureFailure
        if (-not $captureFailure -and -not $matchedExpectedText) {
            $captureFailure = "Talk native mic probe completed, but output did not contain expected text [$ExpectedText]"
        }
        if ($null -ne $recordedWavSignal -and $recordedWavSignal.peak -eq 0) {
            $captureFailure = if ([string]::IsNullOrWhiteSpace($captureFailure)) {
                'Recorded native microphone audio was silent'
            } else {
                "$captureFailure; recorded native microphone audio was silent"
            }
        }

        $summary = New-TalkDesktopQwenNativeMicProbeSummary `
            -SmokeRoot $resolvedSmokeRoot `
            -Session $session `
            -ConfigPath $configPath `
            -LogPath $log.FullName `
            -BinaryPath $launchSummary.binaryPath `
            -SpokenText $PromptText `
            -ExpectedText $ExpectedText `
            -InputDevice $InputDevice
        $summary | Add-Member -NotePropertyName capturedText -NotePropertyValue ([string]$capturedText)
        $summary | Add-Member -NotePropertyName matchedExpectedText -NotePropertyValue ([bool]$matchedExpectedText)
        $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue ([string]$captureFailure)
        $summary | Add-Member -NotePropertyName speakerWavPath -NotePropertyValue $resolvedSpeakerWavPath
        $summary | Add-Member -NotePropertyName speakerOutputDevice -NotePropertyValue ([string]$resolvedSpeakerOutputDevice)
        $summary | Add-Member -NotePropertyName recordedWavPath -NotePropertyValue ([string]$recordedWavPath)
        $summary | Add-Member -NotePropertyName recordedWavSignal -NotePropertyValue $recordedWavSignal
        $summary | Add-Member -NotePropertyName audioProbe -NotePropertyValue $audioProbe
        $summaryPath = Join-Path $resolvedSmokeRoot 'qwen-native-mic-probe-summary.json'
        $summary | Add-Member -NotePropertyName smokeRoot -NotePropertyValue $resolvedSmokeRoot
        $summary | Add-Member -NotePropertyName summaryPath -NotePropertyValue $summaryPath
        Write-TalkDesktopQwenNativeMicProbeSummaryFile -Path $summaryPath -Summary $summary
        if ($captureFailure) {
            throw $captureFailure
        }
        $summary
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkTextCaptureTarget -Target $target
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopQwenNativeMicProbe `
        -BinaryPath $requestedBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -ApiKey $requestedApiKey `
        -ApiKeyJsonPath $requestedApiKeyJsonPath `
        -SmokeRoot $requestedSmokeRoot `
        -Hotkey $requestedHotkey `
        -TimeoutSeconds $requestedTimeoutSeconds `
        -ProviderAudioTranscriptionsEndpoint $requestedProviderAudioTranscriptionsEndpoint `
        -ProviderChatCompletionsEndpoint $requestedProviderChatCompletionsEndpoint `
        -ProviderTranscriptionTransport $requestedProviderTranscriptionTransport `
        -ProviderTranscriptionModel $requestedProviderTranscriptionModel `
        -ProviderChatModel $requestedProviderChatModel `
        -PromptText $requestedPromptText `
        -ExpectedText $requestedExpectedText `
        -SpeakerWavPath $requestedSpeakerWavPath `
        -InputDevice $requestedInputDevice `
        -SpeakerOutputDevice $requestedSpeakerOutputDevice
}
