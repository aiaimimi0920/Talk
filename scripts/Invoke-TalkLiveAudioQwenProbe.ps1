[CmdletBinding()]
param(
    [string]$TalkBinaryPath,
    [string]$ReleaseDir,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$SmokeRoot,
    [string]$InputDevice,
    [int]$CaptureSeconds = 3,
    [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
    [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
    [string]$ProviderChatModel = 'qwen3.7-plus',
    [string]$SpokenPromptText = 'What is the capital of France?',
    [string]$ExpectedText = 'Paris'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedTalkBinaryPath = $TalkBinaryPath
$requestedReleaseDir = $ReleaseDir
$requestedApiKey = $ApiKey
$requestedApiKeyJsonPath = $ApiKeyJsonPath
$requestedSmokeRoot = $SmokeRoot
$requestedInputDevice = $InputDevice
$requestedCaptureSeconds = $CaptureSeconds
$requestedProviderAudioTranscriptionsEndpoint = $ProviderAudioTranscriptionsEndpoint
$requestedProviderChatCompletionsEndpoint = $ProviderChatCompletionsEndpoint
$requestedProviderTranscriptionTransport = $ProviderTranscriptionTransport
$requestedProviderTranscriptionModel = $ProviderTranscriptionModel
$requestedProviderChatModel = $ProviderChatModel
$requestedSpokenPromptText = $SpokenPromptText
$requestedExpectedText = $ExpectedText

$smokeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopReleaseSmoke.ps1'
if (-not (Test-Path -LiteralPath $smokeScriptPath)) {
    throw "Missing Talk desktop smoke script: $smokeScriptPath"
}
. $smokeScriptPath

function Resolve-TalkLiveAudioQwenProbeBinaryPath {
    param(
        [string]$TalkBinaryPath,
        [string]$ReleaseDir
    )

    if (-not [string]::IsNullOrWhiteSpace($TalkBinaryPath)) {
        $resolvedTalkBinaryPath = [System.IO.Path]::GetFullPath($TalkBinaryPath)
        if (-not (Test-Path -LiteralPath $resolvedTalkBinaryPath)) {
            throw "Talk Qwen live audio probe binary does not exist: $resolvedTalkBinaryPath"
        }
        return $resolvedTalkBinaryPath
    }

    $candidateReleaseDir = if ([string]::IsNullOrWhiteSpace($ReleaseDir)) {
        Resolve-LatestTalkReleaseDir
    } else {
        [System.IO.Path]::GetFullPath($ReleaseDir)
    }
    if (-not (Test-Path -LiteralPath $candidateReleaseDir)) {
        throw "Talk Qwen live audio probe release directory does not exist: $candidateReleaseDir"
    }

    $resolvedTalkBinaryPath = Join-Path $candidateReleaseDir '.internal\talk.exe'
    if (-not (Test-Path -LiteralPath $resolvedTalkBinaryPath)) {
        throw "Talk Qwen live audio probe binary does not exist in release directory: $resolvedTalkBinaryPath"
    }

    $resolvedTalkBinaryPath
}

function Resolve-TalkLiveAudioQwenProbeApiKey {
    param(
        [string]$ApiKey,
        [string]$ApiKeyJsonPath
    )

    if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
        if ($ApiKey.Trim() -ne $ApiKey) {
            throw 'Talk live audio Qwen probe api key must not have leading or trailing whitespace'
        }
        return $ApiKey
    }

    if (-not [string]::IsNullOrWhiteSpace($ApiKeyJsonPath)) {
        $resolvedJsonPath = [System.IO.Path]::GetFullPath($ApiKeyJsonPath)
        if (-not (Test-Path -LiteralPath $resolvedJsonPath)) {
            throw "Talk live audio Qwen probe api key json does not exist: $resolvedJsonPath"
        }
        $record = Get-Content -LiteralPath $resolvedJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $jsonApiKey = [string]$record.apiKey
        if ([string]::IsNullOrWhiteSpace($jsonApiKey)) {
            throw "Talk live audio Qwen probe api key json is missing a non-empty apiKey field: $resolvedJsonPath"
        }
        if ($jsonApiKey.Trim() -ne $jsonApiKey) {
            throw "Talk live audio Qwen probe api key json apiKey field must not have leading or trailing whitespace: $resolvedJsonPath"
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

    $homeDirectory = @(
        [Environment]::GetEnvironmentVariable('USERPROFILE', 'Process')
        [Environment]::GetEnvironmentVariable('HOME', 'Process')
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } | Select-Object -First 1
    if (-not [string]::IsNullOrWhiteSpace($homeDirectory)) {
        $credentialDir = Join-Path ([System.IO.Path]::GetFullPath($homeDirectory)) '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
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
                    throw "Talk live audio Qwen probe auto-discovered api key json is missing a non-empty apiKey field: $autoDiscoveredJsonPath"
                }
                if ($jsonApiKey.Trim() -ne $jsonApiKey) {
                    throw "Talk live audio Qwen probe auto-discovered api key json apiKey field must not have leading or trailing whitespace: $autoDiscoveredJsonPath"
                }
                return $jsonApiKey
            }
        }
    }

    throw 'Talk live audio Qwen probe requires -ApiKey, -ApiKeyJsonPath, or TALK_PROVIDER_API_KEY'
}

function New-TalkLiveAudioQwenProbeCaptureConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [string]$InputDevice
    )

    $inputDeviceLine = if ([string]::IsNullOrWhiteSpace($InputDevice)) {
        ''
    } else {
        "input_device = `"$InputDevice`""
    }

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F16"

[audio]
backend = "native_windows"
$inputDeviceLine
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "mock"
mock_transcript = "capture only"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function New-TalkLiveAudioQwenProbeProviderConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus'
    )

    @"
voice_mode = "command"

[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F17"

[audio]
backend = "silent"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath (Join-Path (Split-Path -Parent $LogsDir) 'provider-audio'))"

[provider]
kind = "openai_compatible"
transcription_transport = "$ProviderTranscriptionTransport"
audio_transcriptions_endpoint = "$ProviderAudioTranscriptionsEndpoint"
chat_completions_endpoint = "$ProviderChatCompletionsEndpoint"
transcription_model = "$ProviderTranscriptionModel"
chat_model = "$ProviderChatModel"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkLiveAudioQwenProbeConfigFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, ($Content.Trim() + [Environment]::NewLine), $utf8NoBom)
}

function Convert-TalkLiveAudioQwenProbeCaptureReport {
    param([Parameter(Mandatory = $true)]$ProbeReport)

    [pscustomobject][ordered]@{
        requestedInputDevice = [string]$ProbeReport.audio.nativeWindows.requestedDeviceName
        selectedInputDevice = [string]$ProbeReport.audio.nativeWindows.deviceName
        artifactPath = [string]$ProbeReport.audio.signal.artifactPath
        mimeType = [string]$ProbeReport.audio.signal.mimeType
        sampleRateHz = [int]$ProbeReport.audio.signal.sampleRateHz
        channels = [int]$ProbeReport.audio.signal.channels
        durationSeconds = [double]$ProbeReport.audio.signal.durationSeconds
        peak = [double]$ProbeReport.audio.signal.peak
        rms = [double]$ProbeReport.audio.signal.rms
        silent = [bool]$ProbeReport.audio.signal.silent
    }
}

function Test-TalkLiveAudioQwenProbeHasSignal {
    param($ProbeSummary)

    if ($null -eq $ProbeSummary) {
        return $false
    }
    if ([bool]$ProbeSummary.silent) {
        return $false
    }
    return ([double]$ProbeSummary.peak -gt 0)
}

function New-TalkLiveAudioQwenProbeSummary {
    param(
        [Parameter(Mandatory = $true)][string]$SmokeRoot,
        [Parameter(Mandatory = $true)]$CaptureProbe,
        [Parameter(Mandatory = $true)]$Session,
        [Parameter(Mandatory = $true)][string]$ProviderConfigPath,
        [string]$LogPath,
        [Parameter(Mandatory = $true)][string]$TalkBinaryPath
    )

    $summaryPath = Join-Path $SmokeRoot 'live-audio-qwen-probe-summary.json'
    $inputDevice = if (-not [string]::IsNullOrWhiteSpace([string]$CaptureProbe.requestedInputDevice)) {
        [string]$CaptureProbe.requestedInputDevice
    } else {
        [string]$CaptureProbe.selectedInputDevice
    }

    [pscustomobject][ordered]@{
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        inputDevice = $inputDevice
        captureAudioPath = [string]$CaptureProbe.artifactPath
        capturePeak = [double]$CaptureProbe.peak
        captureRms = [double]$CaptureProbe.rms
        captureSilent = [bool]$CaptureProbe.silent
        binaryPath = $TalkBinaryPath
        providerConfigPath = $ProviderConfigPath
        logPath = [string]$LogPath
        smokeRoot = $SmokeRoot
        summaryPath = $summaryPath
    }
}

function Write-TalkLiveAudioQwenProbeSummaryFile {
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

function Invoke-TalkLiveAudioQwenProbe {
    param(
        [string]$TalkBinaryPath,
        [string]$ReleaseDir,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$SmokeRoot,
        [string]$InputDevice,
        [int]$CaptureSeconds = 3,
        [string]$ProviderAudioTranscriptionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderChatCompletionsEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$ProviderTranscriptionTransport = 'chat_completions_audio_input',
        [string]$ProviderTranscriptionModel = 'qwen3-asr-flash',
        [string]$ProviderChatModel = 'qwen3.7-plus',
        [string]$SpokenPromptText = 'What is the capital of France?',
        [string]$ExpectedText = 'Paris'
    )

    if ($CaptureSeconds -le 0) {
        throw 'Talk live audio Qwen probe capture seconds must be greater than 0'
    }

    $resolvedSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('live-audio-qwen-probe-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $resolvedSmokeRoot -Force | Out-Null

    $resolvedTalkBinaryPath = Resolve-TalkLiveAudioQwenProbeBinaryPath `
        -TalkBinaryPath $TalkBinaryPath `
        -ReleaseDir $ReleaseDir

    $captureAudioDir = Join-Path $resolvedSmokeRoot 'audio'
    $captureLogsDir = Join-Path $resolvedSmokeRoot 'capture-logs'
    $captureConfigPath = Join-Path $resolvedSmokeRoot 'capture-config.toml'
    Write-TalkLiveAudioQwenProbeConfigFile `
        -Path $captureConfigPath `
        -Content (New-TalkLiveAudioQwenProbeCaptureConfigContent `
            -AudioDir $captureAudioDir `
            -LogsDir $captureLogsDir `
            -InputDevice $InputDevice)

    Write-Host ''
    Write-Host 'Talk live audio Qwen probe capture is ready.' -ForegroundColor Green
    if (-not [string]::IsNullOrWhiteSpace($InputDevice)) {
        Write-Host ("Input device: {0}" -f $InputDevice)
    }
    Write-Host ("Speak this prompt during the capture window: {0}" -f $SpokenPromptText)
    for ($countdown = 3; $countdown -ge 1; $countdown--) {
        Write-Host ("Starting capture in {0}..." -f $countdown)
        Start-Sleep -Seconds 1
    }

    $captureOutput = & $resolvedTalkBinaryPath probe-audio --config $captureConfigPath --seconds $CaptureSeconds --json 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Talk live audio Qwen probe capture failed: $($captureOutput | Out-String)"
    }
    $captureReport = ($captureOutput -join [Environment]::NewLine) | ConvertFrom-Json
    $captureProbe = Convert-TalkLiveAudioQwenProbeCaptureReport -ProbeReport $captureReport

    $providerLogsDir = Join-Path $resolvedSmokeRoot 'provider-logs'
    $providerConfigPath = Join-Path $resolvedSmokeRoot 'provider-config.toml'
    Write-TalkLiveAudioQwenProbeConfigFile `
        -Path $providerConfigPath `
        -Content (New-TalkLiveAudioQwenProbeProviderConfigContent `
            -LogsDir $providerLogsDir `
            -ProviderAudioTranscriptionsEndpoint $ProviderAudioTranscriptionsEndpoint `
            -ProviderChatCompletionsEndpoint $ProviderChatCompletionsEndpoint `
            -ProviderTranscriptionTransport $ProviderTranscriptionTransport `
            -ProviderTranscriptionModel $ProviderTranscriptionModel `
            -ProviderChatModel $ProviderChatModel)

    if (-not (Test-TalkLiveAudioQwenProbeHasSignal -ProbeSummary $captureProbe)) {
        $summary = New-TalkLiveAudioQwenProbeSummary `
            -SmokeRoot $resolvedSmokeRoot `
            -CaptureProbe $captureProbe `
            -Session ([pscustomobject]@{
                status = 'failed'
                transcript = $null
                output_text = $null
            }) `
            -ProviderConfigPath $providerConfigPath `
            -LogPath '' `
            -TalkBinaryPath $resolvedTalkBinaryPath
        $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue 'Captured live audio was silent; provider round-trip was skipped'
        Write-TalkLiveAudioQwenProbeSummaryFile -Path $summary.summaryPath -Summary $summary
        throw 'Captured live audio was silent; provider round-trip was skipped'
    }

    $resolvedApiKey = Resolve-TalkLiveAudioQwenProbeApiKey -ApiKey $ApiKey -ApiKeyJsonPath $ApiKeyJsonPath
    $previousApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
    try {
        [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $resolvedApiKey, 'Process')
        $onceOutput = & $resolvedTalkBinaryPath once --config $providerConfigPath --audio-file $captureProbe.artifactPath 2>&1
        $onceExitCode = $LASTEXITCODE
    }
    finally {
        [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $previousApiKey, 'Process')
    }

    $logPath = ''
    $session = $null
    try {
        $log = Wait-LatestSessionLog -LogsDir $providerLogsDir -TimeoutMs 15000
        $logPath = $log.FullName
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
    }
    catch {
        if ($onceExitCode -eq 0) {
            throw
        }
        $session = [pscustomobject]@{
            status = 'failed'
            transcript = $null
            output_text = $null
        }
    }

    $summary = New-TalkLiveAudioQwenProbeSummary `
        -SmokeRoot $resolvedSmokeRoot `
        -CaptureProbe $captureProbe `
        -Session $session `
        -ProviderConfigPath $providerConfigPath `
        -LogPath $logPath `
        -TalkBinaryPath $resolvedTalkBinaryPath
    $summary | Add-Member -NotePropertyName onceExitCode -NotePropertyValue ([int]$onceExitCode)
    $summary | Add-Member -NotePropertyName onceOutputText -NotePropertyValue (($onceOutput -join [Environment]::NewLine).Trim())

    $failureReason = $null
    if ($onceExitCode -ne 0) {
        $failureReason = "Talk live audio Qwen probe once command failed with exit code $onceExitCode"
    } elseif (-not [string]::IsNullOrWhiteSpace($ExpectedText)) {
        $outputText = [string]$session.output_text
        if ($outputText -notmatch [regex]::Escape($ExpectedText)) {
            $failureReason = "Expected output text to contain [$ExpectedText], got [$outputText]"
        }
    }
    if ($failureReason) {
        $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue $failureReason
    }

    Write-TalkLiveAudioQwenProbeSummaryFile -Path $summary.summaryPath -Summary $summary
    if ($failureReason) {
        throw $failureReason
    }

    $summary
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkLiveAudioQwenProbe `
        -TalkBinaryPath $requestedTalkBinaryPath `
        -ReleaseDir $requestedReleaseDir `
        -ApiKey $requestedApiKey `
        -ApiKeyJsonPath $requestedApiKeyJsonPath `
        -SmokeRoot $requestedSmokeRoot `
        -InputDevice $requestedInputDevice `
        -CaptureSeconds $requestedCaptureSeconds `
        -ProviderAudioTranscriptionsEndpoint $requestedProviderAudioTranscriptionsEndpoint `
        -ProviderChatCompletionsEndpoint $requestedProviderChatCompletionsEndpoint `
        -ProviderTranscriptionTransport $requestedProviderTranscriptionTransport `
        -ProviderTranscriptionModel $requestedProviderTranscriptionModel `
        -ProviderChatModel $requestedProviderChatModel `
        -SpokenPromptText $requestedSpokenPromptText `
        -ExpectedText $requestedExpectedText
}
