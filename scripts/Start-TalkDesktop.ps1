[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$ConfigPath,
    [string]$ApiKey,
    [string]$ApiKeyJsonPath,
    [string]$Hotkey,
    [string]$InputDevice,
    [switch]$ListInputDevices,
    [switch]$ProbeAudio,
    [switch]$ProbeQwenRoundTrip,
    [int]$ProbeSeconds = 3,
    [string]$SpokenPromptText = 'What is the capital of France?',
    [string]$ExpectedText = 'Paris'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-TalkDesktopLaunchHomeDirectory {
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

function Resolve-TalkDesktopLaunchApiKeyFromJsonPath {
    param(
        [Parameter(Mandatory = $true)][string]$ApiKeyJsonPath,
        [Parameter(Mandatory = $true)][string]$ContextLabel
    )

    $resolvedJsonPath = [System.IO.Path]::GetFullPath($ApiKeyJsonPath)
    if (-not (Test-Path -LiteralPath $resolvedJsonPath)) {
        throw "$ContextLabel api key json does not exist: $resolvedJsonPath"
    }
    $record = Get-Content -LiteralPath $resolvedJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $jsonApiKey = [string]$record.apiKey
    if ([string]::IsNullOrWhiteSpace($jsonApiKey)) {
        throw "$ContextLabel api key json is missing a non-empty apiKey field: $resolvedJsonPath"
    }
    if ($jsonApiKey.Trim() -ne $jsonApiKey) {
        throw "$ContextLabel api key json apiKey field must not have leading or trailing whitespace: $resolvedJsonPath"
    }
    $jsonApiKey
}

function Resolve-TalkDesktopLaunchAutoApiKeyJsonPath {
    param([string]$ConfigPath)

    if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
        return $null
    }

    $resolvedConfigPath = [System.IO.Path]::GetFullPath($ConfigPath)
    if (-not (Test-Path -LiteralPath $resolvedConfigPath)) {
        return $null
    }

    $configText = Get-Content -LiteralPath $resolvedConfigPath -Raw -Encoding UTF8
    $credentialRelativeDir = if ($configText -match 'coding\.dashscope\.aliyuncs\.com') {
        '.neuro\qwen-platform\qwen-coding-plan-openai\api-key'
    } elseif ($configText -match 'dashscope\.aliyuncs\.com/compatible-mode/') {
        '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
    } else {
        $null
    }

    if ([string]::IsNullOrWhiteSpace($credentialRelativeDir)) {
        return $null
    }

    $homeDirectory = Get-TalkDesktopLaunchHomeDirectory
    if ([string]::IsNullOrWhiteSpace($homeDirectory)) {
        return $null
    }

    $credentialDir = Join-Path $homeDirectory $credentialRelativeDir
    if (-not (Test-Path -LiteralPath $credentialDir)) {
        return $null
    }

    $preferredPath = Join-Path $credentialDir 'manual-live.json'
    if (Test-Path -LiteralPath $preferredPath) {
        return $preferredPath
    }

    Get-ChildItem -LiteralPath $credentialDir -Filter '*.json' -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -ExpandProperty FullName -First 1
}

function Resolve-TalkDesktopLaunchApiKey {
    param(
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$ConfigPath
    )

    if (-not [string]::IsNullOrWhiteSpace($ApiKey)) {
        if ($ApiKey.Trim() -ne $ApiKey) {
            throw 'Talk desktop launch api key must not have leading or trailing whitespace'
        }
        return $ApiKey
    }

    if (-not [string]::IsNullOrWhiteSpace($ApiKeyJsonPath)) {
        return Resolve-TalkDesktopLaunchApiKeyFromJsonPath `
            -ApiKeyJsonPath $ApiKeyJsonPath `
            -ContextLabel 'Talk desktop launch'
    }

    $envApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($envApiKey)) {
        if ($envApiKey.Trim() -ne $envApiKey) {
            throw 'TALK_PROVIDER_API_KEY must not have leading or trailing whitespace'
        }
        return $envApiKey
    }

    $autoDiscoveredJsonPath = Resolve-TalkDesktopLaunchAutoApiKeyJsonPath -ConfigPath $ConfigPath
    if (-not [string]::IsNullOrWhiteSpace($autoDiscoveredJsonPath)) {
        return Resolve-TalkDesktopLaunchApiKeyFromJsonPath `
            -ApiKeyJsonPath $autoDiscoveredJsonPath `
            -ContextLabel 'Talk desktop launch auto-discovered'
    }

    throw 'Talk desktop launch requires -ApiKey, -ApiKeyJsonPath, or TALK_PROVIDER_API_KEY'
}

function Resolve-TalkDesktopLaunchReleaseDir {
    param(
        [string]$ReleaseDir,
        [string]$BinaryPath
    )

    if (-not [string]::IsNullOrWhiteSpace($ReleaseDir)) {
        return [System.IO.Path]::GetFullPath($ReleaseDir)
    }
    if (-not [string]::IsNullOrWhiteSpace($BinaryPath)) {
        return Split-Path -Parent ([System.IO.Path]::GetFullPath($BinaryPath))
    }
    [System.IO.Path]::GetFullPath($PSScriptRoot)
}

function Resolve-TalkDesktopLaunchBinaryPath {
    param(
        [string]$BinaryPath,
        [Parameter(Mandatory = $true)][string]$ReleaseDir
    )

    $resolvedBinaryPath = if (-not [string]::IsNullOrWhiteSpace($BinaryPath)) {
        [System.IO.Path]::GetFullPath($BinaryPath)
    } else {
        Join-Path $ReleaseDir 'talk-desktop.exe'
    }
    if (-not (Test-Path -LiteralPath $resolvedBinaryPath)) {
        throw "Talk desktop launch binary does not exist: $resolvedBinaryPath"
    }
    $resolvedBinaryPath
}

function Resolve-TalkDesktopLaunchTalkBinaryPath {
    param([Parameter(Mandatory = $true)][string]$ReleaseDir)

    $resolvedTalkBinaryPath = Join-Path $ReleaseDir '.internal\talk.exe'
    if (-not (Test-Path -LiteralPath $resolvedTalkBinaryPath)) {
        throw "Talk desktop launch readiness binary does not exist: $resolvedTalkBinaryPath"
    }
    $resolvedTalkBinaryPath
}

function Resolve-TalkDesktopLaunchConfigPath {
    param(
        [string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$ReleaseDir
    )

    $resolvedConfigPath = if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
        [System.IO.Path]::GetFullPath($ConfigPath)
    } else {
        Join-Path $ReleaseDir 'talk-desktop.toml'
    }
    if (-not (Test-Path -LiteralPath $resolvedConfigPath)) {
        throw "Talk desktop launch config does not exist: $resolvedConfigPath"
    }
    $resolvedConfigPath
}

function Escape-TomlPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $Path.Replace('\', '\\')
}

function New-TalkDesktopLaunchEffectiveConfig {
    param(
        [Parameter(Mandatory = $true)][string]$BaseConfigPath,
        [string]$Hotkey,
        [string]$InputDevice
    )

    if ([string]::IsNullOrWhiteSpace($Hotkey) -and [string]::IsNullOrWhiteSpace($InputDevice)) {
        return $BaseConfigPath
    }
    if ($Hotkey.Trim() -ne $Hotkey) {
        throw 'Talk desktop launch hotkey must not have leading or trailing whitespace'
    }
    if (-not [string]::IsNullOrWhiteSpace($InputDevice) -and $InputDevice.Trim() -ne $InputDevice) {
        throw 'Talk desktop launch input device must not have leading or trailing whitespace'
    }

    $baseConfigText = Get-Content -LiteralPath $BaseConfigPath -Raw -Encoding UTF8
    $updatedConfigText = $baseConfigText
    if (-not [string]::IsNullOrWhiteSpace($Hotkey)) {
        $updatedConfigText = [System.Text.RegularExpressions.Regex]::Replace(
            $updatedConfigText,
            '^toggle_shortcut\s*=\s*".*"\r?$',
            ('toggle_shortcut = "{0}"' -f $Hotkey.Replace('\', '\\').Replace('"', '\"')),
            [System.Text.RegularExpressions.RegexOptions]::Multiline
        )
    }
    if (-not [string]::IsNullOrWhiteSpace($InputDevice)) {
        $escapedInputDevice = $InputDevice.Replace('\', '\\').Replace('"', '\"')
        if ($updatedConfigText -match '(?m)^input_device\s*=') {
            $updatedConfigText = [System.Text.RegularExpressions.Regex]::Replace(
                $updatedConfigText,
                '^input_device\s*=\s*".*"\r?$',
                ('input_device = "{0}"' -f $escapedInputDevice),
                [System.Text.RegularExpressions.RegexOptions]::Multiline
            )
        }
        else {
            $updatedConfigText = [System.Text.RegularExpressions.Regex]::Replace(
                $updatedConfigText,
                '(^backend\s*=\s*".*"\r?$)',
                ('$1' + [Environment]::NewLine + ('input_device = "{0}"' -f $escapedInputDevice)),
                [System.Text.RegularExpressions.RegexOptions]::Multiline
            )
        }
    }
    $baseConfigDirectory = Split-Path -Parent $BaseConfigPath
    $baseConfigName = [System.IO.Path]::GetFileNameWithoutExtension($BaseConfigPath)
    $effectiveConfigPath = Join-Path `
        $baseConfigDirectory `
        ('{0}.runtime-launch-{1}.toml' -f $baseConfigName, [guid]::NewGuid().ToString())
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $effectiveConfigPath,
        $updatedConfigText,
        $utf8NoBom
    )
    $effectiveConfigPath
}

function Remove-TalkDesktopLaunchTemporaryConfig {
    param(
        [Parameter(Mandatory = $true)][string]$BaseConfigPath,
        [Parameter(Mandatory = $true)][string]$EffectiveConfigPath
    )

    $resolvedBaseConfigPath = [System.IO.Path]::GetFullPath($BaseConfigPath)
    $resolvedEffectiveConfigPath = [System.IO.Path]::GetFullPath($EffectiveConfigPath)
    if ($resolvedBaseConfigPath -eq $resolvedEffectiveConfigPath) {
        return
    }

    $baseConfigDirectory = Split-Path -Parent $resolvedBaseConfigPath
    $effectiveConfigDirectory = Split-Path -Parent $resolvedEffectiveConfigPath
    if ($baseConfigDirectory -ne $effectiveConfigDirectory) {
        return
    }

    $baseConfigName = [System.IO.Path]::GetFileNameWithoutExtension($resolvedBaseConfigPath)
    $effectiveConfigLeaf = [System.IO.Path]::GetFileName($resolvedEffectiveConfigPath)
    if ($effectiveConfigLeaf -notlike ($baseConfigName + '.runtime-launch-*.toml')) {
        return
    }

    if (Test-Path -LiteralPath $resolvedEffectiveConfigPath) {
        Remove-Item -LiteralPath $resolvedEffectiveConfigPath -Force
    }
}

function New-TalkDesktopLaunchSummary {
    param(
        [Parameter(Mandatory = $true)][string]$ReleaseDir,
        [Parameter(Mandatory = $true)][string]$BinaryPath,
        [Parameter(Mandatory = $true)][string]$BaseConfigPath,
        [Parameter(Mandatory = $true)][string]$EffectiveConfigPath,
        [Parameter(Mandatory = $true)][int]$ProcessId
    )

    [pscustomobject][ordered]@{
        releaseDir = $ReleaseDir
        binaryPath = $BinaryPath
        baseConfigPath = $BaseConfigPath
        effectiveConfigPath = $EffectiveConfigPath
        processId = $ProcessId
    }
}

function Get-TalkDesktopLaunchOptionalPropertyValue {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    $property.Value
}

function Invoke-TalkDesktopLaunchReadiness {
    param(
        [Parameter(Mandatory = $true)][string]$TalkBinaryPath,
        [Parameter(Mandatory = $true)][string]$EffectiveConfigPath,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $processResult = & $TalkBinaryPath readiness --config $EffectiveConfigPath --json 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Talk desktop launch readiness probe failed: $($processResult | Out-String)"
    }

    ($processResult -join [Environment]::NewLine) | ConvertFrom-Json
}

function New-TalkDesktopLaunchInputDeviceInventory {
    param([Parameter(Mandatory = $true)]$ReadinessReport)

    $audioNative = $ReadinessReport.audio.nativeWindows
    $clipboardNative = $ReadinessReport.clipboard.nativeWindows
    $availableInputDevices = Get-TalkDesktopLaunchOptionalPropertyValue -Object $audioNative -Name 'availableDeviceNames'
    $availableInputDevicesList = if ($null -eq $availableInputDevices) {
        [object[]]@()
    } else {
        [object[]]@($availableInputDevices)
    }

    [pscustomobject][ordered]@{
        audioStatus = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $audioNative -Name 'status')
        audioReason = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $audioNative -Name 'reason')
        requestedInputDevice = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $audioNative -Name 'requestedDeviceName')
        selectedInputDevice = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $audioNative -Name 'deviceName')
        availableInputDevices = $availableInputDevicesList
        clipboardStatus = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $clipboardNative -Name 'status')
        clipboardReason = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $clipboardNative -Name 'reason')
    }
}

function Invoke-TalkDesktopLaunchAudioProbe {
    param(
        [Parameter(Mandatory = $true)][string]$TalkBinaryPath,
        [Parameter(Mandatory = $true)][string]$EffectiveConfigPath,
        [Parameter(Mandatory = $true)][int]$ProbeSeconds
    )

    if ($ProbeSeconds -le 0) {
        throw 'Talk desktop launch audio probe seconds must be greater than 0'
    }

    $processResult = & $TalkBinaryPath probe-audio --config $EffectiveConfigPath --seconds $ProbeSeconds --json 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Talk desktop launch audio probe failed: $($processResult | Out-String)"
    }

    ($processResult -join [Environment]::NewLine) | ConvertFrom-Json
}

function New-TalkDesktopLaunchAudioProbeSummary {
    param(
        [Parameter(Mandatory = $true)]$ProbeReport,
        $Inventory
    )

    $nativeWindows = Get-TalkDesktopLaunchOptionalPropertyValue `
        -Object $ProbeReport.audio `
        -Name 'nativeWindows'
    $signal = $ProbeReport.audio.signal
    $requestedInputDevice = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $nativeWindows -Name 'requestedDeviceName')
    if ([string]::IsNullOrWhiteSpace($requestedInputDevice) -and $null -ne $Inventory) {
        $requestedInputDevice = [string]$Inventory.requestedInputDevice
    }

    $selectedInputDevice = [string](Get-TalkDesktopLaunchOptionalPropertyValue -Object $nativeWindows -Name 'deviceName')
    if ([string]::IsNullOrWhiteSpace($selectedInputDevice) -and $null -ne $Inventory) {
        $selectedInputDevice = [string]$Inventory.selectedInputDevice
    }

    $availableInputDevices = if ($null -eq $Inventory) {
        [object[]]@()
    } else {
        [object[]]@($Inventory.availableInputDevices)
    }

    [pscustomobject][ordered]@{
        configuredBackend = [string]$ProbeReport.audio.configuredBackend
        requestedDurationSeconds = [int]$ProbeReport.requestedDurationSeconds
        requestedInputDevice = $requestedInputDevice
        selectedInputDevice = $selectedInputDevice
        audioStatus = if ($null -eq $Inventory) { '' } else { [string]$Inventory.audioStatus }
        audioReason = if ($null -eq $Inventory) { '' } else { [string]$Inventory.audioReason }
        availableInputDevices = $availableInputDevices
        artifactPath = [string]$signal.artifactPath
        mimeType = [string]$signal.mimeType
        sampleRateHz = [int]$signal.sampleRateHz
        channels = [int]$signal.channels
        durationSeconds = [double]$signal.durationSeconds
        peak = [double]$signal.peak
        rms = [double]$signal.rms
        silent = [bool]$signal.silent
    }
}

function Test-TalkDesktopLaunchAudioProbeHasSignal {
    param($ProbeSummary)

    if ($null -eq $ProbeSummary) {
        return $false
    }
    if ([bool]$ProbeSummary.silent) {
        return $false
    }
    return ([double]$ProbeSummary.peak -gt 0)
}

function New-TalkDesktopLaunchQwenRoundTripConfigContent {
    param([Parameter(Mandatory = $true)][string]$LogsDir)

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
transcription_transport = "chat_completions_audio_input"
audio_transcriptions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
chat_completions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
transcription_model = "qwen3-asr-flash"
chat_model = "qwen3.7-plus"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkDesktopLaunchConfigFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $Path,
        ($Content.Trim() + [Environment]::NewLine),
        $utf8NoBom
    )
}

function Wait-TalkDesktopLaunchLatestSessionLog {
    param(
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [int]$TimeoutMs = 15000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        if (Test-Path -LiteralPath $LogsDir) {
            $log = Get-ChildItem -LiteralPath $LogsDir -Filter '*.json' -ErrorAction SilentlyContinue |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($null -ne $log) {
                return $log
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "No Talk session log was written under $LogsDir within ${TimeoutMs}ms"
}

function New-TalkDesktopLaunchQwenRoundTripSummary {
    param(
        [Parameter(Mandatory = $true)]$AudioProbeSummary,
        [Parameter(Mandatory = $true)]$Session,
        [Parameter(Mandatory = $true)][string]$ProviderConfigPath,
        [string]$LogPath
    )

    $inputDevice = if (-not [string]::IsNullOrWhiteSpace([string]$AudioProbeSummary.requestedInputDevice)) {
        [string]$AudioProbeSummary.requestedInputDevice
    } else {
        [string]$AudioProbeSummary.selectedInputDevice
    }

    [pscustomobject][ordered]@{
        status = [string]$Session.status
        transcript = [string]$Session.transcript
        outputText = [string]$Session.output_text
        inputDevice = $inputDevice
        captureAudioPath = [string]$AudioProbeSummary.artifactPath
        capturePeak = [double]$AudioProbeSummary.peak
        captureRms = [double]$AudioProbeSummary.rms
        captureSilent = [bool]$AudioProbeSummary.silent
        providerConfigPath = $ProviderConfigPath
        logPath = [string]$LogPath
    }
}

function Start-TalkDesktop {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$ConfigPath,
        [string]$ApiKey,
        [string]$ApiKeyJsonPath,
        [string]$Hotkey,
        [string]$InputDevice,
        [switch]$ListInputDevices,
        [switch]$ProbeAudio,
        [switch]$ProbeQwenRoundTrip,
        [int]$ProbeSeconds = 3,
        [string]$SpokenPromptText = 'What is the capital of France?',
        [string]$ExpectedText = 'Paris'
    )

    if ($ProbeAudio -and $ProbeQwenRoundTrip) {
        throw 'Talk desktop launch probe modes are mutually exclusive; choose -ProbeAudio or -ProbeQwenRoundTrip'
    }

    $resolvedReleaseDir = Resolve-TalkDesktopLaunchReleaseDir -ReleaseDir $ReleaseDir -BinaryPath $BinaryPath
    $resolvedBinaryPath = Resolve-TalkDesktopLaunchBinaryPath -BinaryPath $BinaryPath -ReleaseDir $resolvedReleaseDir
    $resolvedBaseConfigPath = Resolve-TalkDesktopLaunchConfigPath -ConfigPath $ConfigPath -ReleaseDir $resolvedReleaseDir
    $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
        -BaseConfigPath $resolvedBaseConfigPath `
        -Hotkey $Hotkey `
        -InputDevice $InputDevice
    $cleanupTemporaryLaunchConfig = $ListInputDevices -or $ProbeAudio -or $ProbeQwenRoundTrip
    try {
        $resolvedTalkBinaryPath = $null
        $readinessReport = $null
        if ($ListInputDevices -or $ProbeAudio -or $ProbeQwenRoundTrip) {
            $resolvedTalkBinaryPath = Resolve-TalkDesktopLaunchTalkBinaryPath -ReleaseDir $resolvedReleaseDir
        }

        if ($ListInputDevices -or (-not [string]::IsNullOrWhiteSpace($InputDevice) -and $null -ne $resolvedTalkBinaryPath)) {
            $readinessReport = Invoke-TalkDesktopLaunchReadiness `
                -TalkBinaryPath $resolvedTalkBinaryPath `
                -EffectiveConfigPath $effectiveConfigPath `
                -WorkingDirectory $resolvedReleaseDir
            $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
            if ($ListInputDevices) {
                return $inventory
            }
            if (-not [string]::IsNullOrWhiteSpace($InputDevice) -and $inventory.audioStatus -ne 'ready') {
                $availableDevicesText = if (@($inventory.availableInputDevices).Count -gt 0) {
                    (@($inventory.availableInputDevices) -join ', ')
                } else {
                    '<none>'
                }
                throw "Talk desktop launch input device [$InputDevice] is not ready: $($inventory.audioReason). Available devices: $availableDevicesText"
            }
        }

        if ($ProbeAudio) {
            if ($null -eq $readinessReport) {
                $readinessReport = Invoke-TalkDesktopLaunchReadiness `
                    -TalkBinaryPath $resolvedTalkBinaryPath `
                    -EffectiveConfigPath $effectiveConfigPath `
                    -WorkingDirectory $resolvedReleaseDir
                $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
            }

            $probeReport = Invoke-TalkDesktopLaunchAudioProbe `
                -TalkBinaryPath $resolvedTalkBinaryPath `
                -EffectiveConfigPath $effectiveConfigPath `
                -ProbeSeconds $ProbeSeconds
            return (New-TalkDesktopLaunchAudioProbeSummary -ProbeReport $probeReport -Inventory $inventory)
        }

        if ($ProbeQwenRoundTrip) {
            Write-Host ''
            Write-Host 'Talk desktop Qwen round-trip probe is ready.' -ForegroundColor Green
            if (-not [string]::IsNullOrWhiteSpace($InputDevice)) {
                Write-Host ("Input device: {0}" -f $InputDevice)
            }
            Write-Host ("Speak this prompt during the capture window: {0}" -f $SpokenPromptText)
            for ($countdown = 3; $countdown -ge 1; $countdown--) {
                Write-Host ("Starting capture in {0}..." -f $countdown)
                Start-Sleep -Seconds 1
            }

            if ($null -eq $readinessReport) {
                $readinessReport = Invoke-TalkDesktopLaunchReadiness `
                    -TalkBinaryPath $resolvedTalkBinaryPath `
                    -EffectiveConfigPath $effectiveConfigPath `
                    -WorkingDirectory $resolvedReleaseDir
                $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
            }

            $audioProbeReport = Invoke-TalkDesktopLaunchAudioProbe `
                -TalkBinaryPath $resolvedTalkBinaryPath `
                -EffectiveConfigPath $effectiveConfigPath `
                -ProbeSeconds $ProbeSeconds
            $audioProbeSummary = New-TalkDesktopLaunchAudioProbeSummary `
                -ProbeReport $audioProbeReport `
                -Inventory $inventory
            $providerLogsDir = Join-Path $resolvedReleaseDir '.runtime\talk-desktop\qwen-probe-logs'
            $providerConfigPath = Join-Path $resolvedReleaseDir 'talk-desktop.runtime-qwen-probe.toml'
            Write-TalkDesktopLaunchConfigFile `
                -Path $providerConfigPath `
                -Content (New-TalkDesktopLaunchQwenRoundTripConfigContent -LogsDir $providerLogsDir)

            if (-not (Test-TalkDesktopLaunchAudioProbeHasSignal -ProbeSummary $audioProbeSummary)) {
                $failure = New-TalkDesktopLaunchQwenRoundTripSummary `
                    -AudioProbeSummary $audioProbeSummary `
                    -Session ([pscustomobject]@{
                        status = 'failed'
                        transcript = $null
                        output_text = $null
                    }) `
                    -ProviderConfigPath $providerConfigPath `
                    -LogPath ''
                $failure | Add-Member -NotePropertyName failureReason -NotePropertyValue 'Captured live audio was silent; provider round-trip was skipped'
                throw ($failure | ConvertTo-Json -Depth 6 -Compress)
            }

            $resolvedApiKey = Resolve-TalkDesktopLaunchApiKey `
                -ApiKey $ApiKey `
                -ApiKeyJsonPath $ApiKeyJsonPath `
                -ConfigPath $providerConfigPath
            $previousApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
            try {
                [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $resolvedApiKey, 'Process')
                $onceResult = & $resolvedTalkBinaryPath once --config $providerConfigPath --audio-file $audioProbeSummary.artifactPath 2>&1
                $onceExitCode = $LASTEXITCODE
            }
            finally {
                [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $previousApiKey, 'Process')
            }

            $sessionLogPath = ''
            $session = $null
            try {
                $sessionLog = Wait-TalkDesktopLaunchLatestSessionLog -LogsDir $providerLogsDir -TimeoutMs 15000
                $sessionLogPath = $sessionLog.FullName
                $session = Get-Content -LiteralPath $sessionLog.FullName -Raw -Encoding UTF8 | ConvertFrom-Json
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

            $summary = New-TalkDesktopLaunchQwenRoundTripSummary `
                -AudioProbeSummary $audioProbeSummary `
                -Session $session `
                -ProviderConfigPath $providerConfigPath `
                -LogPath $sessionLogPath
            $summary | Add-Member -NotePropertyName onceExitCode -NotePropertyValue ([int]$onceExitCode)
            $summary | Add-Member -NotePropertyName onceOutputText -NotePropertyValue (($onceResult -join [Environment]::NewLine).Trim())

            if ($onceExitCode -ne 0) {
                $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue "Talk desktop Qwen round-trip probe failed with exit code $onceExitCode"
                throw ($summary | ConvertTo-Json -Depth 6 -Compress)
            }
            if (-not [string]::IsNullOrWhiteSpace($ExpectedText) -and ([string]$session.output_text -notmatch [regex]::Escape($ExpectedText))) {
                $summary | Add-Member -NotePropertyName failureReason -NotePropertyValue "Expected output text to contain [$ExpectedText], got [$([string]$session.output_text)]"
                throw ($summary | ConvertTo-Json -Depth 6 -Compress)
            }

            return $summary
        }

        $resolvedApiKey = Resolve-TalkDesktopLaunchApiKey `
            -ApiKey $ApiKey `
            -ApiKeyJsonPath $ApiKeyJsonPath `
            -ConfigPath $effectiveConfigPath

        $previousApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
        try {
            [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $resolvedApiKey, 'Process')
            $process = Start-Process `
                -FilePath $resolvedBinaryPath `
                -ArgumentList @('--config', $effectiveConfigPath) `
                -WorkingDirectory $resolvedReleaseDir `
                -WindowStyle Hidden `
                -PassThru
        }
        finally {
            [Environment]::SetEnvironmentVariable('TALK_PROVIDER_API_KEY', $previousApiKey, 'Process')
        }

        New-TalkDesktopLaunchSummary `
            -ReleaseDir $resolvedReleaseDir `
            -BinaryPath $resolvedBinaryPath `
            -BaseConfigPath $resolvedBaseConfigPath `
            -EffectiveConfigPath $effectiveConfigPath `
            -ProcessId $process.Id
    }
    finally {
        if ($cleanupTemporaryLaunchConfig) {
            Remove-TalkDesktopLaunchTemporaryConfig `
                -BaseConfigPath $resolvedBaseConfigPath `
                -EffectiveConfigPath $effectiveConfigPath
        }
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Start-TalkDesktop `
        -BinaryPath $BinaryPath `
        -ReleaseDir $ReleaseDir `
        -ConfigPath $ConfigPath `
        -ApiKey $ApiKey `
        -ApiKeyJsonPath $ApiKeyJsonPath `
        -Hotkey $Hotkey `
        -InputDevice $InputDevice `
        -ListInputDevices:$ListInputDevices `
        -ProbeAudio:$ProbeAudio `
        -ProbeQwenRoundTrip:$ProbeQwenRoundTrip `
        -ProbeSeconds $ProbeSeconds `
        -SpokenPromptText $SpokenPromptText `
        -ExpectedText $ExpectedText
}
