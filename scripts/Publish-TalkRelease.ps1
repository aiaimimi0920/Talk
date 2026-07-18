[CmdletBinding()]
param(
    [string]$VersionId,
    [string]$ReleaseRoot,
    [string]$SmokeRoot,
    [string]$PackagedApiKey,
    [string]$PackagedApiKeyJsonPath,
    [switch]$DisablePackagedApiKeyDiscovery,
    [switch]$SkipVerification,
    [switch]$SkipBuild,
    [switch]$SkipSmoke,
    [switch]$SkipNativePreflight,
    [switch]$SkipNativeReadiness
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$requestedSmokeRoot = $SmokeRoot
$smokeScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkDesktopReleaseSmoke.ps1'
if (-not (Test-Path -LiteralPath $smokeScriptPath)) {
    throw "Missing Talk desktop smoke script: $smokeScriptPath"
}
. $smokeScriptPath
$manifestValidatorScriptPath = Join-Path $PSScriptRoot 'Test-TalkReleaseManifest.ps1'
if (-not (Test-Path -LiteralPath $manifestValidatorScriptPath)) {
    throw "Missing Talk release manifest validator script: $manifestValidatorScriptPath"
}
. $manifestValidatorScriptPath
$summaryScriptPath = Join-Path $PSScriptRoot 'Get-TalkReleaseSummary.ps1'
if (-not (Test-Path -LiteralPath $summaryScriptPath)) {
    throw "Missing Talk release summary script: $summaryScriptPath"
}
. $summaryScriptPath
$summaryValidatorScriptPath = Join-Path $PSScriptRoot 'Test-TalkReleaseSummary.ps1'
if (-not (Test-Path -LiteralPath $summaryValidatorScriptPath)) {
    throw "Missing Talk release summary validator script: $summaryValidatorScriptPath"
}
. $summaryValidatorScriptPath
$SmokeRoot = $requestedSmokeRoot

function Get-TalkRepoRoot {
    Split-Path -Parent $PSScriptRoot
}

function Resolve-TalkReleaseRepositoryContext {
    param([string]$TalkRepoRoot = (Get-TalkRepoRoot))

    $resolvedTalkRepoRoot = [System.IO.Path]::GetFullPath($TalkRepoRoot)
    $parentRoot = Split-Path -Parent $resolvedTalkRepoRoot
    $parentGitPath = Join-Path $parentRoot '.git'
    $parentTalkManifestPath = Join-Path $parentRoot 'Talk\Cargo.toml'
    $usesMonorepoLayout = (Test-Path -LiteralPath $parentGitPath) `
        -and (Test-Path -LiteralPath $parentTalkManifestPath)

    if ($usesMonorepoLayout) {
        return [pscustomobject]@{
            RepositoryRoot = [System.IO.Path]::GetFullPath($parentRoot)
            WorkingDirectory = [System.IO.Path]::GetFullPath($parentRoot)
            ManifestPath = 'Talk/Cargo.toml'
        }
    }

    [pscustomobject]@{
        RepositoryRoot = $resolvedTalkRepoRoot
        WorkingDirectory = $resolvedTalkRepoRoot
        ManifestPath = 'Cargo.toml'
    }
}

function Get-NeuroRoot {
    (Resolve-TalkReleaseRepositoryContext).RepositoryRoot
}

function Get-DefaultTalkReleaseRoot {
    Join-Path (Get-NeuroRoot) 'release\Talk'
}

function Resolve-TalkReleaseRuntimeDllSources {
    param(
        [string]$TalkRepoRoot = (Get-TalkRepoRoot),
        [Parameter(Mandatory = $true)][string[]]$DllNames
    )

    $resolvedTalkRepoRoot = [System.IO.Path]::GetFullPath($TalkRepoRoot)
    $releaseDir = Join-Path $resolvedTalkRepoRoot 'target\release'
    $prebuiltRoot = Join-Path $resolvedTalkRepoRoot 'target\sherpa-onnx-prebuilt'
    $sources = New-Object System.Collections.Generic.List[string]

    foreach ($dllName in $DllNames) {
        $releasePath = Join-Path $releaseDir $dllName
        if (Test-Path -LiteralPath $releasePath -PathType Leaf) {
            $sources.Add($releasePath) | Out-Null
            continue
        }

        $prebuiltCandidates = if (Test-Path -LiteralPath $prebuiltRoot -PathType Container) {
            @(
                Get-ChildItem -LiteralPath $prebuiltRoot -Recurse -File -Filter $dllName -ErrorAction SilentlyContinue |
                    Where-Object {
                        $_.Directory.Name -eq 'lib' -and
                        $_.FullName -match '(?i)shared'
                    }
            )
        } else {
            @()
        }
        $prebuiltCandidates = @($prebuiltCandidates)

        if ($prebuiltCandidates.Count -eq 0) {
            throw "Missing Talk release runtime DLL '$dllName'. Searched release path '$releasePath' and shared sherpa prebuilt cache '$prebuiltRoot'."
        }
        if ($prebuiltCandidates.Count -gt 1) {
            $candidatePaths = $prebuiltCandidates.FullName -join ', '
            throw "Ambiguous Talk release runtime DLL '$dllName' in shared sherpa prebuilt cache: $candidatePaths"
        }

        $sources.Add($prebuiltCandidates[0].FullName) | Out-Null
    }

    $sources.ToArray()
}

function Resolve-TalkReleaseRoot {
    param([string]$ReleaseRoot)

    if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
        return Get-DefaultTalkReleaseRoot
    }

    [System.IO.Path]::GetFullPath($ReleaseRoot)
}

function Resolve-TalkReleaseVersionId {
    param([string]$VersionId)

    if (-not [string]::IsNullOrWhiteSpace($VersionId)) {
        return $VersionId
    }

    'desktop-shell-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
}

function Write-Utf8NoBomText {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Content
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Content, $utf8NoBom)
}

function Get-TalkReleaseHomeDirectory {
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

function Resolve-TalkReleaseApiKeyFromJsonPath {
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

function Resolve-TalkReleaseAutoApiKeyJsonPath {
    param([Parameter(Mandatory = $true)][string]$ConfigText)

    $credentialRelativeDir = if ($ConfigText -match 'coding\.dashscope\.aliyuncs\.com') {
        '.neuro\qwen-platform\qwen-coding-plan-openai\api-key'
    } elseif ($ConfigText -match 'dashscope\.aliyuncs\.com/compatible-mode/') {
        '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
    } else {
        $null
    }

    if ([string]::IsNullOrWhiteSpace($credentialRelativeDir)) {
        return $null
    }

    $homeDirectory = Get-TalkReleaseHomeDirectory
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

function Resolve-TalkReleasePackagedApiKey {
    param(
        [string]$PackagedApiKey,
        [string]$PackagedApiKeyJsonPath,
        [Parameter(Mandatory = $true)][string]$ConfigText,
        [switch]$DisableAutoDiscovery
    )

    if (-not [string]::IsNullOrWhiteSpace($PackagedApiKey)) {
        if ($PackagedApiKey.Trim() -ne $PackagedApiKey) {
            throw 'Talk release packaged api key must not have leading or trailing whitespace'
        }
        return $PackagedApiKey
    }

    if (-not [string]::IsNullOrWhiteSpace($PackagedApiKeyJsonPath)) {
        return Resolve-TalkReleaseApiKeyFromJsonPath `
            -ApiKeyJsonPath $PackagedApiKeyJsonPath `
            -ContextLabel 'Talk release packaged'
    }

    if ($DisableAutoDiscovery) {
        return $null
    }

    $envApiKey = [Environment]::GetEnvironmentVariable('TALK_PROVIDER_API_KEY', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($envApiKey)) {
        if ($envApiKey.Trim() -ne $envApiKey) {
            throw 'TALK_PROVIDER_API_KEY must not have leading or trailing whitespace'
        }
        return $envApiKey
    }

    $autoDiscoveredJsonPath = Resolve-TalkReleaseAutoApiKeyJsonPath -ConfigText $ConfigText
    if (-not [string]::IsNullOrWhiteSpace($autoDiscoveredJsonPath)) {
        return Resolve-TalkReleaseApiKeyFromJsonPath `
            -ApiKeyJsonPath $autoDiscoveredJsonPath `
            -ContextLabel 'Talk release packaged auto-discovered'
    }

    $null
}

function ConvertTo-TalkReleaseTomlBasicString {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value -match "[`r`n]") {
        throw 'Talk release packaged api key must not contain newlines'
    }

    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function New-TalkReleaseDesktopConfigContent {
    param([string]$PackagedApiKey)

    $packagedApiKeyComment = if ([string]::IsNullOrWhiteSpace($PackagedApiKey)) {
        '# Set TALK_PROVIDER_API_KEY before launching talk-desktop.exe.'
    } else {
        '# Qwen provider api key is packaged into this release for direct desktop launch.'
    }
    $providerApiKeyLine = if ([string]::IsNullOrWhiteSpace($PackagedApiKey)) {
        'api_key_env = "TALK_PROVIDER_API_KEY"'
    } else {
        'api_key = ' + (ConvertTo-TalkReleaseTomlBasicString -Value $PackagedApiKey)
    }

    @"
# Default Talk desktop config packaged with the Windows release build.
$packagedApiKeyComment

voice_mode = "smart"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[desktop.shortcuts]
transcribe_shortcut = "RightCtrl+1"
document_shortcut = "RightCtrl+2"
command_shortcut = "RightCtrl+3"
generate_shortcut = "RightCtrl+4"
smart_shortcut = "RightCtrl+5"
translate_shortcut = "RightAlt+/"
ask_shortcut = "RightAlt+Space"

# Optional: override the paste shortcut for specific hosts or control signatures.
# Matchers are case-insensitive. The first matching rule wins.
#
# [[desktop.paste.shortcut_overrides]]
# process_name = "tabby"
# paste_shortcut = "ctrl_shift_v"
#
# [[desktop.paste.shortcut_overrides]]
# automation_framework_id = "WinUI"
# automation_control_type = "custom"
# paste_shortcut = "shift_insert"

[audio]
backend = "native_windows"
# Optional: pin a specific Windows input endpoint instead of the current default device.
# input_device = "Virtual Mic"
max_recording_seconds = 300
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk-desktop/audio"

[provider]
kind = "openai_compatible"
transcription_transport = "chat_completions_audio_input"
audio_transcriptions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
chat_completions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
transcription_model = "qwen3-asr-flash"
chat_model = "qwen3.7-plus"
$providerApiKeyLine

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "provider_text_processor"
max_patch_age_ms = 2000
max_auto_patch_edit_ratio = 0.25

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000

# Optional: uncomment this block to override the packaged model auto-discovery.
# If .runtime/models/sherpa-onnx/zipformer-zh-en-punct-int8-480ms is installed,
# Talk auto-starts the packaged daemon in sherpa-online mode; otherwise it keeps
# the daemon's dry-run fallback for model-less smoke tests.
#
# [speculative.streaming_service.local_daemon]
# mode = "sherpa-online"
# model_family = "transducer"
# model = "zipformer-zh-en-punct-int8-480ms"
# tokens = ".runtime/models/sherpa-onnx/zipformer-zh-en-punct-int8-480ms/tokens.txt"
# encoder = ".runtime/models/sherpa-onnx/zipformer-zh-en-punct-int8-480ms/encoder.int8.onnx"
# decoder = ".runtime/models/sherpa-onnx/zipformer-zh-en-punct-int8-480ms/decoder.onnx"
# joiner = ".runtime/models/sherpa-onnx/zipformer-zh-en-punct-int8-480ms/joiner.int8.onnx"
# provider = "cpu"
# num_threads = 2
# sample_rate_hz = 16000
# decoding_method = "greedy_search"

[output]
mode = "clipboard_paste"
restore_clipboard = true
clipboard_backend = "native_windows"

[logging]
dir = ".runtime/talk-desktop/logs"
"@
}

function Get-VerificationSteps {
    param(
        [bool]$Skipped,
        [string]$ManifestPath = 'Talk/Cargo.toml'
    )

    if ($Skipped) {
        return @('verification: skipped')
    }

    @(
        "cargo fmt --manifest-path $ManifestPath --all -- --check",
        "cargo check --manifest-path $ManifestPath --workspace --all-targets",
        "cargo test --manifest-path $ManifestPath --workspace"
    )
}

function New-TalkReleaseBuildInfoText {
    param(
        [Parameter(Mandatory = $true)][string]$VersionId,
        [Parameter(Mandatory = $true)][string]$BuiltAt,
        [Parameter(Mandatory = $true)][string]$SourceWorkspace,
        [Parameter(Mandatory = $true)][string[]]$ArtifactNames,
        [Parameter(Mandatory = $true)][string[]]$VerificationSteps,
        [Parameter()][object[]]$SmokeResults = @(),
        [Parameter()][object[]]$NativePreflightResults = @(),
        [Parameter()]$NativeReadinessResult = $null
    )

    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add("version_id: $VersionId")
    $lines.Add("build_date: $BuiltAt")
    $lines.Add("source_workspace: $SourceWorkspace")
    $lines.Add('artifacts:')
    foreach ($artifactName in $ArtifactNames) {
        $lines.Add("  - $artifactName")
    }
    $lines.Add('verification:')
    foreach ($verificationStep in $VerificationSteps) {
        $lines.Add("  - $verificationStep")
    }

    if ($null -eq $SmokeResults -or $SmokeResults.Count -eq 0) {
        $lines.Add('  - desktop smoke: skipped')
    } else {
        $lines.Add('  - desktop smoke: executed')
        $smokeArtifactLines = New-Object System.Collections.Generic.List[string]
        $smokeSummaryLines = New-Object System.Collections.Generic.List[string]
        $smokeFailureLines = New-Object System.Collections.Generic.List[string]
        $smokeFailureArtifactLines = New-Object System.Collections.Generic.List[string]
        $smokeRetryLines = New-Object System.Collections.Generic.List[string]
        foreach ($smokeResult in $SmokeResults) {
            $scenarioProperty = $smokeResult.PSObject.Properties['Scenario']
            if ($null -eq $scenarioProperty) {
                continue
            }
            $scenarioName = [string]$scenarioProperty.Value
            $lines.Add("  - desktop smoke: $scenarioName")

            $statusKind = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'StatusKind')
            $statusSummary = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'StatusSummary')
            if (-not [string]::IsNullOrWhiteSpace($statusKind) -or -not [string]::IsNullOrWhiteSpace($statusSummary)) {
                $summaryText = if (-not [string]::IsNullOrWhiteSpace($statusKind)) {
                    "$scenarioName [$statusKind]"
                } else {
                    $scenarioName
                }
                if (-not [string]::IsNullOrWhiteSpace($statusSummary)) {
                    $summaryText += ": $statusSummary"
                }
                $smokeSummaryLines.Add("  - $summaryText") | Out-Null
            }

            $beforeReloadStatusKind = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'BeforeReloadStatusKind')
            $beforeReloadStatusSummary = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'BeforeReloadStatusSummary')
            if (-not [string]::IsNullOrWhiteSpace($beforeReloadStatusKind) -or -not [string]::IsNullOrWhiteSpace($beforeReloadStatusSummary)) {
                $summaryText = "$scenarioName before_reload"
                if (-not [string]::IsNullOrWhiteSpace($beforeReloadStatusKind)) {
                    $summaryText += " [$beforeReloadStatusKind]"
                }
                if (-not [string]::IsNullOrWhiteSpace($beforeReloadStatusSummary)) {
                    $summaryText += ": $beforeReloadStatusSummary"
                }
                $smokeSummaryLines.Add("  - $summaryText") | Out-Null
            }

            $afterReloadStatusKind = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'AfterReloadStatusKind')
            $afterReloadStatusSummary = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'AfterReloadStatusSummary')
            if (-not [string]::IsNullOrWhiteSpace($afterReloadStatusKind) -or -not [string]::IsNullOrWhiteSpace($afterReloadStatusSummary)) {
                $summaryText = "$scenarioName after_reload"
                if (-not [string]::IsNullOrWhiteSpace($afterReloadStatusKind)) {
                    $summaryText += " [$afterReloadStatusKind]"
                }
                if (-not [string]::IsNullOrWhiteSpace($afterReloadStatusSummary)) {
                    $summaryText += ": $afterReloadStatusSummary"
                }
                $smokeSummaryLines.Add("  - $summaryText") | Out-Null
            }

            $logPathProperty = $smokeResult.PSObject.Properties['LogPath']
            if ($null -ne $logPathProperty -and -not [string]::IsNullOrWhiteSpace([string]$logPathProperty.Value)) {
                $smokeArtifactLines.Add("  - $scenarioName log: $($logPathProperty.Value)") | Out-Null
            }

            $insertTargetDiagnosticPath = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'InsertTargetDiagnosticPath')
            if (-not [string]::IsNullOrWhiteSpace($insertTargetDiagnosticPath)) {
                $smokeArtifactLines.Add("  - $scenarioName insert_target: $insertTargetDiagnosticPath") | Out-Null
            }

            $failureKind = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'FailureKind')
            $failureSummary = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'FailureSummary')
            if (-not [string]::IsNullOrWhiteSpace($failureKind) -or -not [string]::IsNullOrWhiteSpace($failureSummary)) {
                $summaryText = if (-not [string]::IsNullOrWhiteSpace($failureKind)) {
                    "$scenarioName [$failureKind]"
                } else {
                    $scenarioName
                }
                if (-not [string]::IsNullOrWhiteSpace($failureSummary)) {
                    $summaryText += ": $failureSummary"
                }
                $smokeFailureLines.Add("  - $summaryText") | Out-Null
            }

            $failureEvidencePath = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'FailureEvidencePath')
            if (-not [string]::IsNullOrWhiteSpace($failureEvidencePath)) {
                $smokeFailureArtifactLines.Add("  - $scenarioName evidence: $failureEvidencePath") | Out-Null
            }

            $retryCount = Get-OptionalPsPropertyValue -Object $smokeResult -Name 'RetryCount'
            $retryReason = [string](Get-OptionalPsPropertyValue -Object $smokeResult -Name 'RetryReason')
            if ((($retryCount -is [int] -or $retryCount -is [long]) -and $retryCount -gt 0) -or -not [string]::IsNullOrWhiteSpace($retryReason)) {
                $retrySummary = $scenarioName
                if (($retryCount -is [int] -or $retryCount -is [long]) -and $retryCount -gt 0) {
                    $retrySummary += " retry_count=$retryCount"
                }
                if (-not [string]::IsNullOrWhiteSpace($retryReason)) {
                    $retrySummary += " retry_reason=$retryReason"
                }
                $smokeRetryLines.Add("  - $retrySummary") | Out-Null
            }
        }

        if ($smokeSummaryLines.Count -gt 0) {
            $lines.Add('desktop_smoke_status:')
            foreach ($smokeSummaryLine in $smokeSummaryLines) {
                $lines.Add($smokeSummaryLine)
            }
        }

        if ($smokeFailureLines.Count -gt 0) {
            $lines.Add('desktop_smoke_failures:')
            foreach ($smokeFailureLine in $smokeFailureLines) {
                $lines.Add($smokeFailureLine)
            }
        }

        if ($smokeRetryLines.Count -gt 0) {
            $lines.Add('desktop_smoke_retries:')
            foreach ($smokeRetryLine in $smokeRetryLines) {
                $lines.Add($smokeRetryLine)
            }
        }

        if ($smokeArtifactLines.Count -gt 0) {
            $lines.Add('smoke_artifacts:')
            foreach ($smokeArtifactLine in $smokeArtifactLines) {
                $lines.Add($smokeArtifactLine)
            }
        }

        if ($smokeFailureArtifactLines.Count -gt 0) {
            $lines.Add('desktop_smoke_failure_artifacts:')
            foreach ($smokeFailureArtifactLine in $smokeFailureArtifactLines) {
                $lines.Add($smokeFailureArtifactLine)
            }
        }
    }

    if ($null -ne $NativePreflightResults -and $NativePreflightResults.Count -gt 0) {
        $lines.Add('native_preflight:')
        foreach ($preflightResult in $NativePreflightResults) {
            $name = [string]$preflightResult.Name
            $expectedError = [string]$preflightResult.ExpectedError
            $evidencePath = [string]$preflightResult.EvidencePath
            $lines.Add("  - $name")
            $lines.Add("    expected_error: $expectedError")
            $lines.Add("    evidence: $evidencePath")
        }
    }

    if ($null -eq $NativeReadinessResult) {
        $lines.Add('native_readiness: skipped')
    } else {
        $lines.Add('native_readiness:')
        $lines.Add("  - evidence: $([string]$NativeReadinessResult.EvidencePath)")
        $lines.Add("  - audio_status: $([string]$NativeReadinessResult.AudioStatus)")
        $audioReason = [string](Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioReason')
        if (-not [string]::IsNullOrWhiteSpace($audioReason)) {
            $lines.Add("  - audio_reason: $audioReason")
        }
        $audioDeviceName = [string](Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDeviceName')
        if (-not [string]::IsNullOrWhiteSpace($audioDeviceName)) {
            $lines.Add("  - audio_device: $audioDeviceName")
        }
        $audioSampleRate = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDefaultSampleRateHz'
        if ($null -ne $audioSampleRate) {
            $lines.Add("  - audio_sample_rate_hz: $audioSampleRate")
        }
        $audioChannels = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDefaultChannels'
        if ($null -ne $audioChannels) {
            $lines.Add("  - audio_channels: $audioChannels")
        }
        $audioSampleFormat = [string](Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioSampleFormat')
        if (-not [string]::IsNullOrWhiteSpace($audioSampleFormat)) {
            $lines.Add("  - audio_sample_format: $audioSampleFormat")
        }
        $lines.Add("  - clipboard_status: $([string]$NativeReadinessResult.ClipboardStatus)")
        $clipboardReason = [string](Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'ClipboardReason')
        if (-not [string]::IsNullOrWhiteSpace($clipboardReason)) {
            $lines.Add("  - clipboard_reason: $clipboardReason")
        }
    }

    ($lines -join [Environment]::NewLine)
}

function New-TalkNativePreflightConfigContent {
    param(
        [Parameter(Mandatory = $true)][ValidateSet('audio-native-disabled', 'clipboard-native-disabled')][string]$Kind,
        [Parameter(Mandatory = $true)][string]$SessionRoot
    )

    $audioDir = Join-Path $SessionRoot 'audio'
    $logsDir = Join-Path $SessionRoot 'logs'
    $outputMode = 'dry_run'
    $clipboardBackend = 'fallback'
    $audioBackend = 'silent'
    $mockTranscript = 'native preflight transcript'

    switch ($Kind) {
        'audio-native-disabled' {
            $audioBackend = 'native_windows'
            $mockTranscript = 'native audio preflight'
        }
        'clipboard-native-disabled' {
            $outputMode = 'clipboard_paste'
            $clipboardBackend = 'native_windows'
            $mockTranscript = 'native clipboard preflight'
        }
    }

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F24"

[audio]
backend = "$audioBackend"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $audioDir)"

[provider]
kind = "mock"
mock_transcript = "$mockTranscript"

[output]
mode = "$outputMode"
restore_clipboard = true
clipboard_backend = "$clipboardBackend"

[logging]
dir = "$(Escape-TomlPath $logsDir)"
"@
}

function Write-TalkNativePreflightConfig {
    param(
        [Parameter(Mandatory = $true)][ValidateSet('audio-native-disabled', 'clipboard-native-disabled')][string]$Kind,
        [Parameter(Mandatory = $true)][string]$SessionRoot
    )

    New-Item -ItemType Directory -Path $SessionRoot -Force | Out-Null
    $configPath = Join-Path $SessionRoot 'config.toml'
    $configText = New-TalkNativePreflightConfigContent -Kind $Kind -SessionRoot $SessionRoot
    Write-Utf8NoBomText -Path $configPath -Content ($configText + [Environment]::NewLine)
    $configPath
}

function New-TalkNativeReadinessConfigContent {
    param([Parameter(Mandatory = $true)][string]$SessionRoot)

    $audioDir = Join-Path $SessionRoot 'audio'
    $logsDir = Join-Path $SessionRoot 'logs'

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F24"

[audio]
backend = "native_windows"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $audioDir)"

[provider]
kind = "mock"
mock_transcript = "native readiness transcript"

[output]
mode = "clipboard_paste"
restore_clipboard = true
clipboard_backend = "native_windows"

[logging]
dir = "$(Escape-TomlPath $logsDir)"
"@
}

function Write-TalkNativeReadinessConfig {
    param([Parameter(Mandatory = $true)][string]$SessionRoot)

    New-Item -ItemType Directory -Path $SessionRoot -Force | Out-Null
    $configPath = Join-Path $SessionRoot 'config.toml'
    $configText = New-TalkNativeReadinessConfigContent -SessionRoot $SessionRoot
    Write-Utf8NoBomText -Path $configPath -Content ($configText + [Environment]::NewLine)
    $configPath
}

function Write-TalkReleaseChecksums {
    param([Parameter(Mandatory = $true)][string]$DestinationDir)

    $checksumPath = Join-Path $DestinationDir 'checksums.sha256'
    $records = Get-ChildItem -LiteralPath $DestinationDir -File -Recurse |
        Where-Object { $_.FullName -ne $checksumPath } |
        Sort-Object FullName |
        ForEach-Object {
            $relativePath = $_.FullName.Substring($DestinationDir.Length).TrimStart('\')
            $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant()
            "$relativePath  $hash"
        }

    Write-Utf8NoBomText -Path $checksumPath -Content (($records -join [Environment]::NewLine) + [Environment]::NewLine)
}

function New-TalkReleaseFileRecord {
    param(
        [Parameter(Mandatory = $true)][string]$BasePath,
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Kind,
        [string]$Name
    )

    $resolvedBasePath = [System.IO.Path]::GetFullPath($BasePath)
    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    $fileInfo = Get-Item -LiteralPath $resolvedPath
    $relativePath = $resolvedPath.Substring($resolvedBasePath.Length).TrimStart('\')

    [pscustomobject][ordered]@{
        kind = $Kind
        name = if ([string]::IsNullOrWhiteSpace($Name)) { $fileInfo.Name } else { $Name }
        path = $relativePath
        bytes = $fileInfo.Length
        sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $resolvedPath).Hash.ToLowerInvariant()
    }
}

function Write-TalkReleaseCommandLogs {
    param(
        [Parameter(Mandatory = $true)][string]$DestinationDir,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$CommandRecords
    )

    $buildLogRecords = New-Object System.Collections.Generic.List[object]
    $index = 1
    foreach ($commandRecord in $CommandRecords) {
        $relativePath = 'logs\talk-release-{0:D2}.log' -f $index
        $fullPath = Join-Path $DestinationDir $relativePath
        $outputText = if ([string]::IsNullOrWhiteSpace([string]$commandRecord.OutputText)) {
            '<empty>'
        } else {
            [string]$commandRecord.OutputText
        }
        $content = @(
            "display: $($commandRecord.Display)"
            "working_directory: $($commandRecord.WorkingDirectory)"
            "exit_code: $($commandRecord.ExitCode)"
            'output:'
            $outputText
        ) -join [Environment]::NewLine
        Write-Utf8NoBomText -Path $fullPath -Content ($content + [Environment]::NewLine)
        $buildLogRecords.Add([pscustomobject][ordered]@{
            kind = 'build-log'
            path = $relativePath
        }) | Out-Null
        $index += 1
    }

    $buildLogRecords.ToArray()
}

function New-TalkReleaseManifestObject {
    param(
        [Parameter(Mandatory = $true)][string]$VersionId,
        [Parameter(Mandatory = $true)][string]$BuiltAt,
        [Parameter(Mandatory = $true)][string]$RepoRoot,
        [Parameter(Mandatory = $true)][string]$ReleaseRoot,
        [Parameter(Mandatory = $true)][string]$DestinationDir,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$CommandRecords,
        [Parameter(Mandatory = $true)][object[]]$ExeRecords,
        [Parameter()][AllowEmptyCollection()][object[]]$SupportFileRecords = @(),
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$BuildLogRecords,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][object[]]$NativePreflightRecords,
        [Parameter()][AllowEmptyCollection()][object[]]$SmokeResults = @(),
        [Parameter()]$NativeReadinessResult = $null
    )

    $manifest = [ordered]@{
        schemaVersion = 2
        app = 'Talk'
        sourceProject = 'Talk'
        versionId = $VersionId
        builtAt = $BuiltAt
        profile = 'release'
        target = 'windows-x64'
        repoRoot = $RepoRoot
        releaseRoot = $ReleaseRoot
        destination = $DestinationDir
        commands = @(
            foreach ($commandRecord in $CommandRecords) {
                [ordered]@{
                    display = $commandRecord.Display
                    workingDirectory = $commandRecord.WorkingDirectory
                }
            }
        )
        exes = $ExeRecords
        supportFiles = @(
            [ordered]@{
                kind = 'release-summary'
                path = 'release-summary.json'
            }
            foreach ($record in $SupportFileRecords) {
                [ordered]@{
                    kind = $record.kind
                    path = $record.path
                }
            }
        )
        buildInfo = [ordered]@{
            kind = 'build-info'
            path = 'BUILD_INFO.txt'
        }
        buildLogs = $BuildLogRecords
        desktopSmoke = $null
        nativePreflight = @(
            foreach ($record in $NativePreflightRecords) {
                [ordered]@{
                    name = $record.Name
                    configPath = Get-OptionalPsPropertyValue -Object $record -Name 'ConfigPath'
                    evidencePath = Get-OptionalPsPropertyValue -Object $record -Name 'EvidencePath'
                    expectedError = Get-OptionalPsPropertyValue -Object $record -Name 'ExpectedError'
                    exitCode = Get-OptionalPsPropertyValue -Object $record -Name 'ExitCode'
                    outputText = Get-OptionalPsPropertyValue -Object $record -Name 'OutputText'
                }
            }
        )
        nativeReadiness = if ($null -eq $NativeReadinessResult) {
            $null
        } else {
            [ordered]@{
                configPath = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'ConfigPath'
                evidencePath = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'EvidencePath'
                audio = [ordered]@{
                    status = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioStatus'
                    reason = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioReason'
                    deviceName = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDeviceName'
                    defaultSampleRateHz = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDefaultSampleRateHz'
                    defaultChannels = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioDefaultChannels'
                    sampleFormat = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'AudioSampleFormat'
                }
                clipboard = [ordered]@{
                    status = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'ClipboardStatus'
                    reason = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'ClipboardReason'
                }
                outputText = Get-OptionalPsPropertyValue -Object $NativeReadinessResult -Name 'OutputText'
            }
        }
        artifacts = @()
        checksums = 'checksums.sha256'
    }

    if ($null -ne $SmokeResults -and $SmokeResults.Count -gt 0) {
        $desktopSmokeRecords = New-Object System.Collections.Generic.List[object]
        foreach ($record in $SmokeResults) {
            $desktopSmokeRecords.Add((Convert-TalkDesktopSmokeManifestRecord -Record $record)) | Out-Null
        }
        $manifest.desktopSmoke = $desktopSmokeRecords.ToArray()
    }

    $manifest
}

function Convert-TalkDesktopSmokeManifestRecord {
    param([Parameter(Mandatory = $true)]$Record)

    $manifestRecord = [ordered]@{
        scenario = Get-OptionalPsPropertyValue -Object $Record -Name 'Scenario'
        binaryPath = Get-OptionalPsPropertyValue -Object $Record -Name 'BinaryPath'
        configPath = Get-OptionalPsPropertyValue -Object $Record -Name 'ConfigPath'
        primaryConfigPath = Get-OptionalPsPropertyValue -Object $Record -Name 'PrimaryConfigPath'
        secondaryConfigPath = Get-OptionalPsPropertyValue -Object $Record -Name 'SecondaryConfigPath'
        logPath = Get-OptionalPsPropertyValue -Object $Record -Name 'LogPath'
        status = Get-OptionalPsPropertyValue -Object $Record -Name 'Status'
        failureKind = Get-OptionalPsPropertyValue -Object $Record -Name 'FailureKind'
        failureSummary = Get-OptionalPsPropertyValue -Object $Record -Name 'FailureSummary'
        failureEvidencePath = Get-OptionalPsPropertyValue -Object $Record -Name 'FailureEvidencePath'
        insertTargetDiagnosticPath = Get-OptionalPsPropertyValue -Object $Record -Name 'InsertTargetDiagnosticPath'
        dialogText = Get-OptionalPsPropertyValue -Object $Record -Name 'DialogText'
        statusKind = Get-OptionalPsPropertyValue -Object $Record -Name 'StatusKind'
        statusSummary = Get-OptionalPsPropertyValue -Object $Record -Name 'StatusSummary'
        statusFields = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'StatusFields')
        statusSnapshot = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'StatusSnapshot')
        beforeReloadDialogText = Get-OptionalPsPropertyValue -Object $Record -Name 'BeforeReloadDialogText'
        beforeReloadStatusKind = Get-OptionalPsPropertyValue -Object $Record -Name 'BeforeReloadStatusKind'
        beforeReloadStatusSummary = Get-OptionalPsPropertyValue -Object $Record -Name 'BeforeReloadStatusSummary'
        beforeReloadStatusFields = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'BeforeReloadStatusFields')
        beforeReloadStatusSnapshot = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'BeforeReloadStatusSnapshot')
        afterReloadDialogText = Get-OptionalPsPropertyValue -Object $Record -Name 'AfterReloadDialogText'
        afterReloadStatusKind = Get-OptionalPsPropertyValue -Object $Record -Name 'AfterReloadStatusKind'
        afterReloadStatusSummary = Get-OptionalPsPropertyValue -Object $Record -Name 'AfterReloadStatusSummary'
        afterReloadStatusFields = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'AfterReloadStatusFields')
        afterReloadStatusSnapshot = Convert-TalkDesktopSmokeObjectToManifestValue -Value (Get-OptionalPsPropertyValue -Object $Record -Name 'AfterReloadStatusSnapshot')
    }

    $retryCount = Get-OptionalPsPropertyValue -Object $Record -Name 'RetryCount'
    if ($retryCount -is [int] -or $retryCount -is [long]) {
        $manifestRecord.retryCount = $retryCount
    }

    $retryReason = Get-OptionalPsPropertyValue -Object $Record -Name 'RetryReason'
    if ($retryReason -is [string] -and -not [string]::IsNullOrWhiteSpace($retryReason)) {
        $manifestRecord.retryReason = $retryReason
    }

    [pscustomobject]$manifestRecord
}

function Convert-TalkDesktopSmokeObjectToManifestValue {
    param($Value)

    if ($null -eq $Value) {
        return $null
    }

    $result = [ordered]@{}
    if ($Value -is [System.Collections.IDictionary]) {
        foreach ($entry in $Value.GetEnumerator()) {
            $result[[string]$entry.Key] = [string]$entry.Value
        }
    } else {
        foreach ($property in $Value.PSObject.Properties) {
            $result[[string]$property.Name] = [string]$property.Value
        }
    }
    [pscustomobject]$result
}

function Get-OptionalPsPropertyValue {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    $property.Value
}

function Get-TalkReleaseSmokeFailureRecords {
    param([AllowEmptyCollection()][object[]]$SmokeResults)

    @(
        @($SmokeResults) | Where-Object {
            -not [string]::IsNullOrWhiteSpace([string](Get-OptionalPsPropertyValue -Object $_ -Name 'FailureKind'))
        }
    )
}

function Get-TalkReleaseSmokeRetryRecords {
    param([AllowEmptyCollection()][object[]]$SmokeResults)

    @(
        @($SmokeResults) | Where-Object {
            $retryCount = Get-OptionalPsPropertyValue -Object $_ -Name 'RetryCount'
            $retryReason = [string](Get-OptionalPsPropertyValue -Object $_ -Name 'RetryReason')
            (($retryCount -is [int] -or $retryCount -is [long]) -and $retryCount -gt 0) -or
            (-not [string]::IsNullOrWhiteSpace($retryReason))
        }
    )
}

function Add-OrUpdatePsProperty {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        $Value
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        $Object | Add-Member -NotePropertyName $Name -NotePropertyValue $Value
    } else {
        $property.Value = $Value
    }
}

function Resolve-TalkReleaseSmokeRetryRoot {
    param(
        [string]$SmokeRoot,
        [int]$Attempt
    )

    $suffix = ('retry-{0:D2}' -f $Attempt)
    if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        return Join-Path (Get-TalkRepoRoot) ('.runtime\desktop-release-smoke-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + $suffix)
    }

    ([System.IO.Path]::GetFullPath($SmokeRoot) + '-' + $suffix)
}

function Invoke-TalkDesktopReleaseSmokeWithHostileForegroundRetry {
    param(
        [Parameter(Mandatory = $true)][string]$ReleaseDir,
        [string]$SmokeRoot,
        [int]$MaxHostileForegroundRetries = 1
    )

    $results = @(
        Invoke-TalkDesktopReleaseSmoke `
            -ReleaseDir $ReleaseDir `
            -SmokeRoot $SmokeRoot `
            -ContinueOnFailure
    )
    if ($MaxHostileForegroundRetries -le 0) {
        return $results
    }

    $scenarioIndex = @{}
    for ($index = 0; $index -lt $results.Count; $index++) {
        $scenarioName = [string](Get-OptionalPsPropertyValue -Object $results[$index] -Name 'Scenario')
        if (-not [string]::IsNullOrWhiteSpace($scenarioName)) {
            $scenarioIndex[$scenarioName] = $index
        }
    }

    for ($attempt = 1; $attempt -le $MaxHostileForegroundRetries; $attempt++) {
        $retryScenarios = @(
            @($results) | Where-Object {
                [string](Get-OptionalPsPropertyValue -Object $_ -Name 'FailureKind') -eq 'hostile_foreground_environment'
            } | ForEach-Object {
                [string](Get-OptionalPsPropertyValue -Object $_ -Name 'Scenario')
            } | Where-Object {
                -not [string]::IsNullOrWhiteSpace($_)
            } | Select-Object -Unique
        )
        if ($retryScenarios.Count -eq 0) {
            break
        }

        $retrySmokeRoot = Resolve-TalkReleaseSmokeRetryRoot -SmokeRoot $SmokeRoot -Attempt $attempt
        $retryResults = @(
            Invoke-TalkDesktopReleaseSmoke `
                -ReleaseDir $ReleaseDir `
                -SmokeRoot $retrySmokeRoot `
                -Scenario $retryScenarios `
                -ContinueOnFailure
        )

        foreach ($retryResult in $retryResults) {
            Add-OrUpdatePsProperty -Object $retryResult -Name 'RetryCount' -Value $attempt
            Add-OrUpdatePsProperty -Object $retryResult -Name 'RetryReason' -Value 'hostile_foreground_environment'

            $scenarioName = [string](Get-OptionalPsPropertyValue -Object $retryResult -Name 'Scenario')
            if ([string]::IsNullOrWhiteSpace($scenarioName)) {
                continue
            }

            if ($scenarioIndex.ContainsKey($scenarioName)) {
                $results[$scenarioIndex[$scenarioName]] = $retryResult
            } else {
                $scenarioIndex[$scenarioName] = $results.Count
                $results += $retryResult
            }
        }
    }

    $results
}

function Assert-TalkReleaseSmokeResultsPassed {
    param(
        [AllowEmptyCollection()][object[]]$SmokeResults,
        [string]$Context = 'Talk desktop release smoke'
    )

    $failures = @(Get-TalkReleaseSmokeFailureRecords -SmokeResults $SmokeResults)
    if ($failures.Count -eq 0) {
        return
    }

    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($failure in $failures) {
        $scenarioName = [string](Get-OptionalPsPropertyValue -Object $failure -Name 'Scenario')
        $failureKind = [string](Get-OptionalPsPropertyValue -Object $failure -Name 'FailureKind')
        $failureSummary = [string](Get-OptionalPsPropertyValue -Object $failure -Name 'FailureSummary')
        $failureEvidencePath = [string](Get-OptionalPsPropertyValue -Object $failure -Name 'FailureEvidencePath')

        $line = "$scenarioName [$failureKind]"
        if (-not [string]::IsNullOrWhiteSpace($failureSummary)) {
            $line += ": $failureSummary"
        }
        if (-not [string]::IsNullOrWhiteSpace($failureEvidencePath)) {
            $line += " (evidence: $failureEvidencePath)"
        }
        $lines.Add(" - $line") | Out-Null
    }

    throw ("{0} failed:`n{1}" -f $Context, ($lines -join "`n"))
}

function Invoke-PowerShellCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    Push-Location $WorkingDirectory
    try {
        $commandOutput = & powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $Command
        $renderedOutput = if ($null -eq $commandOutput) {
            ''
        } else {
            (@($commandOutput) -join [Environment]::NewLine)
        }
        if ($null -ne $commandOutput) {
            foreach ($line in @($commandOutput)) {
                Write-Host $line
            }
        }
        if ($LASTEXITCODE -ne 0) {
            throw "Command failed with exit code ${LASTEXITCODE}: $Command`n$renderedOutput"
        }

        [pscustomobject]@{
            Display = $Command
            WorkingDirectory = [System.IO.Path]::GetFullPath($WorkingDirectory)
            OutputText = $renderedOutput
            ExitCode = $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
    }
}

function Invoke-TalkProcess {
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory,
        [hashtable]$EnvironmentOverrides = @{}
    )

    $previousValues = @{}
    foreach ($name in $EnvironmentOverrides.Keys) {
        $previousValues[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, [string]$EnvironmentOverrides[$name], 'Process')
    }

    try {
        $stdoutPath = Join-Path $env:TEMP ("talk-process-stdout-" + [guid]::NewGuid().ToString() + ".log")
        $stderrPath = Join-Path $env:TEMP ("talk-process-stderr-" + [guid]::NewGuid().ToString() + ".log")
        $argumentString = ($Arguments | ForEach-Object {
            if ($_ -eq '') {
                '""'
            } elseif ($_ -match '[\s"]') {
                '"' + ($_.Replace('\', '\\').Replace('"', '\"')) + '"'
            } else {
                $_
            }
        }) -join ' '
        Push-Location $WorkingDirectory
        try {
            $process = Start-Process `
                -FilePath $FilePath `
                -ArgumentList $argumentString `
                -WorkingDirectory $WorkingDirectory `
                -PassThru `
                -Wait `
                -RedirectStandardOutput $stdoutPath `
                -RedirectStandardError $stderrPath
            $stdoutText = if (Test-Path -LiteralPath $stdoutPath) {
                [System.IO.File]::ReadAllText($stdoutPath)
            } else {
                ''
            }
            $stderrText = if (Test-Path -LiteralPath $stderrPath) {
                [System.IO.File]::ReadAllText($stderrPath)
            } else {
                ''
            }
            $renderedOutputParts = @($stdoutText, $stderrText) `
                | Where-Object { -not [string]::IsNullOrWhiteSpace($_) } `
                | ForEach-Object { $_.TrimEnd("`r", "`n") }
            $renderedOutput = $renderedOutputParts -join [Environment]::NewLine
            [pscustomobject]@{
                ExitCode = $process.ExitCode
                OutputText = $renderedOutput
            }
        }
        finally {
            Pop-Location
            Remove-Item -LiteralPath $stdoutPath, $stderrPath -Force -ErrorAction SilentlyContinue
        }
    }
    finally {
        foreach ($name in $EnvironmentOverrides.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previousValues[$name], 'Process')
        }
    }
}

function Invoke-TalkNativeWindowsReadiness {
    param(
        [Parameter(Mandatory = $true)][string]$TalkBinaryPath,
        [Parameter(Mandatory = $true)][string]$SmokeRoot
    )

    $talkRepoRoot = Get-TalkRepoRoot
    $sessionRoot = Join-Path $SmokeRoot 'readiness'
    if (Test-Path -LiteralPath $sessionRoot) {
        Remove-Item -LiteralPath $sessionRoot -Recurse -Force
    }
    $configPath = Write-TalkNativeReadinessConfig -SessionRoot $sessionRoot
    $processResult = Invoke-TalkProcess `
        -FilePath $TalkBinaryPath `
        -Arguments @('readiness', '--config', $configPath, '--json') `
        -WorkingDirectory $talkRepoRoot

    if ($processResult.ExitCode -ne 0) {
        throw "native readiness probe failed with exit code $($processResult.ExitCode): $($processResult.OutputText)"
    }

    $evidencePath = Join-Path $sessionRoot 'readiness.json'
    Write-Utf8NoBomText -Path $evidencePath -Content ($processResult.OutputText.Trim() + [Environment]::NewLine)
    $report = $processResult.OutputText | ConvertFrom-Json

    if ($report.app -ne 'talk') {
        throw "native readiness probe returned unexpected app [$($report.app)]"
    }
    if ($report.audio.nativeWindows.status -ne 'ready' -or $report.clipboard.nativeWindows.status -ne 'ready') {
        $audioReason = [string]$report.audio.nativeWindows.reason
        $clipboardReason = [string]$report.clipboard.nativeWindows.reason
        throw "native readiness probe reported audio=[$($report.audio.nativeWindows.status)] clipboard=[$($report.clipboard.nativeWindows.status)] audio_reason=[$audioReason] clipboard_reason=[$clipboardReason] evidence=[$evidencePath]"
    }

    [pscustomobject]@{
        ConfigPath = $configPath
        EvidencePath = $evidencePath
        AudioStatus = [string]$report.audio.nativeWindows.status
        AudioReason = [string]$report.audio.nativeWindows.reason
        AudioDeviceName = [string]$report.audio.nativeWindows.deviceName
        AudioDefaultSampleRateHz = $report.audio.nativeWindows.defaultSampleRateHz
        AudioDefaultChannels = $report.audio.nativeWindows.defaultChannels
        AudioSampleFormat = [string]$report.audio.nativeWindows.sampleFormat
        ClipboardStatus = [string]$report.clipboard.nativeWindows.status
        ClipboardReason = [string]$report.clipboard.nativeWindows.reason
        OutputText = $processResult.OutputText
    }
}

function Invoke-TalkNativeWindowsPreflight {
    param(
        [Parameter(Mandatory = $true)][string]$TalkBinaryPath,
        [Parameter(Mandatory = $true)][string]$SmokeRoot
    )

    $talkRepoRoot = Get-TalkRepoRoot
    $results = New-Object System.Collections.Generic.List[object]

    foreach ($kind in @('audio-native-disabled', 'clipboard-native-disabled')) {
        $sessionRoot = Join-Path $SmokeRoot $kind
        if (Test-Path -LiteralPath $sessionRoot) {
            Remove-Item -LiteralPath $sessionRoot -Recurse -Force
        }
        $configPath = Write-TalkNativePreflightConfig -Kind $kind -SessionRoot $sessionRoot
        $mockText = if ($kind -eq 'audio-native-disabled') {
            'hello_native_audio'
        } else {
            'hello_native_clipboard'
        }
        $environmentOverrides = if ($kind -eq 'audio-native-disabled') {
            @{ TALK_DISABLE_NATIVE_AUDIO = '1' }
        } else {
            @{ TALK_DISABLE_NATIVE_CLIPBOARD = '1' }
        }

        $processResult = Invoke-TalkProcess `
            -FilePath $TalkBinaryPath `
            -Arguments @('once', '--config', $configPath, '--mock-text', $mockText) `
            -WorkingDirectory $talkRepoRoot `
            -EnvironmentOverrides $environmentOverrides

        if ($processResult.ExitCode -eq 0) {
            throw "native preflight '$kind' unexpectedly succeeded"
        }

        $log = Get-LatestSessionLog -LogsDir (Join-Path $sessionRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'failed') {
            throw "native preflight '$kind' expected failed status, got [$($session.status)]"
        }

        if ($kind -eq 'audio-native-disabled') {
            if ($session.transcript -ne $null -or $session.output_text -ne $null -or (Get-OptionalPsPropertyValue -Object $session -Name 'insert_outcome') -ne $null) {
                throw "native audio preflight must fail before transcript/output/insert"
            }
            $audioDir = Join-Path $sessionRoot 'audio'
            $wavFiles = if (Test-Path -LiteralPath $audioDir) {
                @(Get-ChildItem -LiteralPath $audioDir -Filter '*.wav' -ErrorAction SilentlyContinue)
            } else {
                @()
            }
            if (@($wavFiles).Count -ne 0) {
                throw "native audio preflight must not create silent wav artifacts"
            }
            $expectedError = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
        } else {
            if ($session.output_text -ne $mockText) {
                throw "native clipboard preflight expected output_text [$mockText], got [$($session.output_text)]"
            }
            if ((Get-OptionalPsPropertyValue -Object $session -Name 'insert_outcome') -ne $null) {
                throw "native clipboard preflight must not report insert_outcome on disabled backend"
            }
            $expectedError = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
        }

        if (-not ([string]$session.error).Contains($expectedError)) {
            throw "native preflight '$kind' error did not contain expected text [$expectedError]"
        }

        $results.Add([pscustomobject]@{
            Name = $kind
            ConfigPath = $configPath
            EvidencePath = $log.FullName
            ExpectedError = $expectedError
            ExitCode = $processResult.ExitCode
            OutputText = $processResult.OutputText
        }) | Out-Null
    }

    $results.ToArray()
}

function Publish-TalkRelease {
    param(
        [string]$VersionId,
        [string]$ReleaseRoot,
        [string]$SmokeRoot,
        [string]$PackagedApiKey,
        [string]$PackagedApiKeyJsonPath,
        [switch]$DisablePackagedApiKeyDiscovery,
        [switch]$SkipVerification,
        [switch]$SkipBuild,
        [switch]$SkipSmoke,
        [switch]$SkipNativePreflight,
        [switch]$SkipNativeReadiness
    )

    $talkRepoRoot = Get-TalkRepoRoot
    $repositoryContext = Resolve-TalkReleaseRepositoryContext -TalkRepoRoot $talkRepoRoot
    $neuroRoot = $repositoryContext.RepositoryRoot
    $commandWorkingDirectory = $repositoryContext.WorkingDirectory
    $manifestPath = $repositoryContext.ManifestPath
    $resolvedReleaseRoot = Resolve-TalkReleaseRoot -ReleaseRoot $ReleaseRoot
    $resolvedVersionId = Resolve-TalkReleaseVersionId -VersionId $VersionId
    $destinationDir = Join-Path $resolvedReleaseRoot $resolvedVersionId

    $verificationSteps = Get-VerificationSteps `
        -Skipped $SkipVerification.IsPresent `
        -ManifestPath $manifestPath
    $commandRecords = New-Object System.Collections.Generic.List[object]

    if (-not $SkipVerification) {
        foreach ($command in $verificationSteps) {
            $commandRecords.Add((Invoke-PowerShellCommand -Command $command -WorkingDirectory $commandWorkingDirectory)) | Out-Null
        }
    }

    if (-not $SkipBuild) {
        $commandRecords.Add((
            Invoke-PowerShellCommand `
                -Command "cargo build --manifest-path $manifestPath --release -p talk-daemon -p talk-desktop -p talk-local-asr-sherpa -p asr-bench" `
                -WorkingDirectory $commandWorkingDirectory
        )) | Out-Null
    }

    if (Test-Path -LiteralPath $destinationDir) {
        Remove-Item -LiteralPath $destinationDir -Recurse -Force
    }
    New-Item -ItemType Directory -Path $destinationDir -Force | Out-Null

    $talkExeSource = Join-Path $talkRepoRoot 'target\release\talk.exe'
    $talkDesktopExeSource = Join-Path $talkRepoRoot 'target\release\talk-desktop.exe'
    $talkLocalAsrSherpaExeSource = Join-Path $talkRepoRoot 'target\release\talk-local-asr-sherpa.exe'
    $asrBenchExeSource = Join-Path $talkRepoRoot 'target\release\asr-bench.exe'
    $talkLocalAsrSherpaRuntimeDllNames = @(
        'sherpa-onnx-c-api.dll',
        'sherpa-onnx-cxx-api.dll',
        'onnxruntime.dll',
        'onnxruntime_providers_shared.dll'
    )
    $talkLocalAsrSherpaRuntimeDllSources = @(
        Resolve-TalkReleaseRuntimeDllSources `
            -TalkRepoRoot $talkRepoRoot `
            -DllNames $talkLocalAsrSherpaRuntimeDllNames
    )
    $desktopLauncherSource = Join-Path $talkRepoRoot 'scripts\Start-TalkDesktop.ps1'
    $desktopLiveHotkeyProbeSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopLiveHotkeyProbe.ps1'
    $desktopLiveOperatorProbeSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopLiveOperatorProbe.ps1'
    $desktopQwenGlobalHotkeyProbeSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
    $desktopQwenGlobalHotkeySoakProbeSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'
    $desktopQwenNativeMicProbeSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopQwenNativeMicProbe.ps1'
    $desktopSmokeHelperSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkDesktopReleaseSmoke.ps1'
    $localAsrModelInstallerSource = Join-Path $talkRepoRoot 'scripts\Install-TalkSherpaModel.ps1'
    $asrCorpusBenchmarkHelperSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkAsrCorpusBenchmark.ps1'
    $asrCorpusRecorderHelperSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkAsrCorpusRecorder.ps1'
    $asrDefaultModelSelectorSource = Join-Path $talkRepoRoot 'scripts\Select-TalkDefaultAsrModel.ps1'
    $asrDefaultModelApplierSource = Join-Path $talkRepoRoot 'scripts\Set-TalkDefaultAsrModel.ps1'
    $asrDefaultModelWorkflowSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkAsrDefaultModelWorkflow.ps1'
    $asrRealMicDefaultModelWorkflowSource = Join-Path $talkRepoRoot 'scripts\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'
    $asrRealMicPromptTemplateSource = Join-Path $talkRepoRoot 'examples\asr-real-mic-prompts.json'
    $requiredPaths = @(
        $talkExeSource,
        $talkDesktopExeSource,
        $talkLocalAsrSherpaExeSource,
        $asrBenchExeSource
    ) + $talkLocalAsrSherpaRuntimeDllSources + @(
        $desktopLauncherSource,
        $desktopLiveHotkeyProbeSource,
        $desktopLiveOperatorProbeSource,
        $desktopQwenGlobalHotkeyProbeSource,
        $desktopQwenGlobalHotkeySoakProbeSource,
        $desktopQwenNativeMicProbeSource,
        $desktopSmokeHelperSource,
        $localAsrModelInstallerSource,
        $asrCorpusBenchmarkHelperSource,
        $asrCorpusRecorderHelperSource,
        $asrDefaultModelSelectorSource,
        $asrDefaultModelApplierSource,
        $asrDefaultModelWorkflowSource,
        $asrRealMicDefaultModelWorkflowSource,
        $asrRealMicPromptTemplateSource
    )
    foreach ($requiredPath in $requiredPaths) {
        if (-not (Test-Path -LiteralPath $requiredPath)) {
            throw "Missing Talk release artifact: $requiredPath"
        }
    }

    $internalSupportDir = Join-Path $destinationDir '.internal'
    New-Item -ItemType Directory -Path $internalSupportDir -Force | Out-Null
    Copy-Item -LiteralPath $talkExeSource -Destination (Join-Path $internalSupportDir 'talk.exe')
    Copy-Item -LiteralPath $talkLocalAsrSherpaExeSource -Destination (Join-Path $internalSupportDir 'talk-local-asr-sherpa.exe')
    Copy-Item -LiteralPath $asrBenchExeSource -Destination (Join-Path $internalSupportDir 'asr-bench.exe')
    foreach ($runtimeDllSource in $talkLocalAsrSherpaRuntimeDllSources) {
        Copy-Item -LiteralPath $runtimeDllSource -Destination (Join-Path $internalSupportDir (Split-Path -Leaf $runtimeDllSource))
    }
    Copy-Item -LiteralPath $talkDesktopExeSource -Destination (Join-Path $destinationDir 'talk-desktop.exe')
    Copy-Item -LiteralPath $desktopLauncherSource -Destination (Join-Path $destinationDir 'Start-TalkDesktop.ps1')
    Copy-Item -LiteralPath $desktopLiveHotkeyProbeSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopLiveHotkeyProbe.ps1')
    Copy-Item -LiteralPath $desktopLiveOperatorProbeSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopLiveOperatorProbe.ps1')
    Copy-Item -LiteralPath $desktopQwenGlobalHotkeyProbeSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1')
    Copy-Item -LiteralPath $desktopQwenGlobalHotkeySoakProbeSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1')
    Copy-Item -LiteralPath $desktopQwenNativeMicProbeSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopQwenNativeMicProbe.ps1')
    Copy-Item -LiteralPath $desktopSmokeHelperSource -Destination (Join-Path $destinationDir 'Invoke-TalkDesktopReleaseSmoke.ps1')
    Copy-Item -LiteralPath $localAsrModelInstallerSource -Destination (Join-Path $destinationDir 'Install-TalkSherpaModel.ps1')
    Copy-Item -LiteralPath $asrCorpusBenchmarkHelperSource -Destination (Join-Path $destinationDir 'Invoke-TalkAsrCorpusBenchmark.ps1')
    Copy-Item -LiteralPath $asrCorpusRecorderHelperSource -Destination (Join-Path $destinationDir 'Invoke-TalkAsrCorpusRecorder.ps1')
    Copy-Item -LiteralPath $asrDefaultModelSelectorSource -Destination (Join-Path $destinationDir 'Select-TalkDefaultAsrModel.ps1')
    Copy-Item -LiteralPath $asrDefaultModelApplierSource -Destination (Join-Path $destinationDir 'Set-TalkDefaultAsrModel.ps1')
    Copy-Item -LiteralPath $asrDefaultModelWorkflowSource -Destination (Join-Path $destinationDir 'Invoke-TalkAsrDefaultModelWorkflow.ps1')
    Copy-Item -LiteralPath $asrRealMicDefaultModelWorkflowSource -Destination (Join-Path $destinationDir 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1')
    Copy-Item -LiteralPath $asrRealMicPromptTemplateSource -Destination (Join-Path $destinationDir 'asr-real-mic-prompts.json')
    $desktopConfigPath = Join-Path $destinationDir 'talk-desktop.toml'
    $desktopConfigContent = New-TalkReleaseDesktopConfigContent
    $resolvedPackagedApiKey = Resolve-TalkReleasePackagedApiKey `
        -PackagedApiKey $PackagedApiKey `
        -PackagedApiKeyJsonPath $PackagedApiKeyJsonPath `
        -ConfigText $desktopConfigContent `
        -DisableAutoDiscovery:$DisablePackagedApiKeyDiscovery
    if (-not [string]::IsNullOrWhiteSpace($resolvedPackagedApiKey)) {
        $desktopConfigContent = New-TalkReleaseDesktopConfigContent -PackagedApiKey $resolvedPackagedApiKey
    }
    Write-Utf8NoBomText `
        -Path $desktopConfigPath `
        -Content ($desktopConfigContent.Trim() + [Environment]::NewLine)
    $exeRecords = @(
        New-TalkReleaseFileRecord -BasePath $destinationDir -Path (Join-Path $destinationDir 'talk-desktop.exe') -Kind 'exe' -Name 'talk-desktop.exe'
    )
    $supportFileRecords = @(
        [pscustomobject]@{
            kind = 'desktop-config'
            path = 'talk-desktop.toml'
        }
        [pscustomobject]@{
            kind = 'local-asr-daemon'
            path = '.internal/talk-local-asr-sherpa.exe'
        }
        [pscustomobject]@{
            kind = 'asr-benchmark-tool'
            path = '.internal/asr-bench.exe'
        }
        [pscustomobject]@{
            kind = 'asr-corpus-benchmark-helper'
            path = 'Invoke-TalkAsrCorpusBenchmark.ps1'
        }
        [pscustomobject]@{
            kind = 'asr-corpus-recorder-helper'
            path = 'Invoke-TalkAsrCorpusRecorder.ps1'
        }
        [pscustomobject]@{
            kind = 'asr-corpus-prompt-template'
            path = 'asr-real-mic-prompts.json'
        }
        [pscustomobject]@{
            kind = 'asr-default-model-selector'
            path = 'Select-TalkDefaultAsrModel.ps1'
        }
        [pscustomobject]@{
            kind = 'asr-default-model-applier'
            path = 'Set-TalkDefaultAsrModel.ps1'
        }
        [pscustomobject]@{
            kind = 'asr-default-model-workflow'
            path = 'Invoke-TalkAsrDefaultModelWorkflow.ps1'
        }
        [pscustomobject]@{
            kind = 'asr-real-mic-default-model-workflow'
            path = 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'
        }
        foreach ($dllName in $talkLocalAsrSherpaRuntimeDllNames) {
            [pscustomobject]@{
                kind = 'local-asr-runtime'
                path = ".internal/$dllName"
            }
        }
        [pscustomobject]@{
            kind = 'desktop-launcher'
            path = 'Start-TalkDesktop.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-live-hotkey-probe'
            path = 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-live-operator-probe'
            path = 'Invoke-TalkDesktopLiveOperatorProbe.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-qwen-global-hotkey-probe'
            path = 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-qwen-global-hotkey-soak-probe'
            path = 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-qwen-native-mic-probe'
            path = 'Invoke-TalkDesktopQwenNativeMicProbe.ps1'
        }
        [pscustomobject]@{
            kind = 'desktop-smoke-helper'
            path = 'Invoke-TalkDesktopReleaseSmoke.ps1'
        }
        [pscustomobject]@{
            kind = 'local-asr-model-installer'
            path = 'Install-TalkSherpaModel.ps1'
        }
    )
    $buildLogRecords = @(
        Write-TalkReleaseCommandLogs -DestinationDir $destinationDir -CommandRecords $commandRecords.ToArray()
    )

    $nativeReadinessResult = $null
    if (-not $SkipNativeReadiness) {
        $readinessRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
            Join-Path $talkRepoRoot '.runtime\desktop-release-native-readiness'
        } else {
            Join-Path ([System.IO.Path]::GetFullPath($SmokeRoot)) 'native-readiness'
        }
        $nativeReadinessResult = Invoke-TalkNativeWindowsReadiness `
            -TalkBinaryPath (Join-Path $internalSupportDir 'talk.exe') `
            -SmokeRoot $readinessRoot
    }

    $nativePreflightResults = @()
    if (-not $SkipNativePreflight) {
        $preflightRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
            Join-Path $talkRepoRoot '.runtime\desktop-release-native-preflight'
        } else {
            Join-Path ([System.IO.Path]::GetFullPath($SmokeRoot)) 'native-preflight'
        }
        $nativePreflightResults = @(
            Invoke-TalkNativeWindowsPreflight `
                -TalkBinaryPath (Join-Path $internalSupportDir 'talk.exe') `
                -SmokeRoot $preflightRoot
        )
    }

    $smokeResults = @()
    if (-not $SkipSmoke) {
        $smokeResults = @(
            Invoke-TalkDesktopReleaseSmokeWithHostileForegroundRetry `
                -ReleaseDir $destinationDir `
                -SmokeRoot $SmokeRoot
        )
    }

    $builtAt = Get-Date -Format o
    $buildInfoText = New-TalkReleaseBuildInfoText `
        -VersionId $resolvedVersionId `
        -BuiltAt $builtAt `
        -SourceWorkspace $talkRepoRoot `
        -ArtifactNames @(
            'talk-desktop.exe',
            '.internal/talk-local-asr-sherpa.exe',
            '.internal/asr-bench.exe',
            '.internal/sherpa-onnx-c-api.dll',
            '.internal/sherpa-onnx-cxx-api.dll',
            '.internal/onnxruntime.dll',
            '.internal/onnxruntime_providers_shared.dll'
        ) `
        -VerificationSteps $verificationSteps `
        -SmokeResults $smokeResults `
        -NativePreflightResults $nativePreflightResults `
        -NativeReadinessResult $nativeReadinessResult
    Write-Utf8NoBomText `
        -Path (Join-Path $destinationDir 'BUILD_INFO.txt') `
        -Content ($buildInfoText + [Environment]::NewLine)

    $manifest = New-TalkReleaseManifestObject `
        -VersionId $resolvedVersionId `
        -BuiltAt $builtAt `
        -RepoRoot $neuroRoot `
        -ReleaseRoot $resolvedReleaseRoot `
        -DestinationDir $destinationDir `
        -CommandRecords $commandRecords.ToArray() `
        -ExeRecords $exeRecords `
        -SupportFileRecords $supportFileRecords `
        -BuildLogRecords $buildLogRecords `
        -NativePreflightRecords $nativePreflightResults `
        -SmokeResults $smokeResults `
        -NativeReadinessResult $nativeReadinessResult
    Write-Utf8NoBomText `
        -Path (Join-Path $destinationDir 'manifest.json') `
        -Content (($manifest | ConvertTo-Json -Depth 8) + [Environment]::NewLine)
    $writtenManifest = Read-TalkReleaseManifest -ManifestPath (Join-Path $destinationDir 'manifest.json')
    Assert-TalkReleaseManifestObject -Manifest $writtenManifest -Context (Join-Path $destinationDir 'manifest.json')
    $releaseSummary = New-TalkReleaseSummaryObjectFromManifest -Manifest $writtenManifest
    Write-Utf8NoBomText `
        -Path (Join-Path $destinationDir 'release-summary.json') `
        -Content (($releaseSummary | ConvertTo-Json -Depth 8) + [Environment]::NewLine)
    $writtenSummary = Read-TalkReleaseSummary -SummaryPath (Join-Path $destinationDir 'release-summary.json')
    Assert-TalkReleaseSummaryObject -Summary $writtenSummary -Context (Join-Path $destinationDir 'release-summary.json')

    Write-TalkReleaseChecksums -DestinationDir $destinationDir
    Assert-TalkReleaseSmokeResultsPassed `
        -SmokeResults $smokeResults `
        -Context ("Talk release publish smoke for " + $resolvedVersionId)

    [PSCustomObject]@{
        VersionId = $resolvedVersionId
        DestinationDir = $destinationDir
        VerificationSkipped = $SkipVerification.IsPresent
        BuildSkipped = $SkipBuild.IsPresent
        SmokeSkipped = $SkipSmoke.IsPresent
        NativePreflightSkipped = $SkipNativePreflight.IsPresent
        NativeReadinessSkipped = $SkipNativeReadiness.IsPresent
        CommandRecords = $commandRecords.ToArray()
        BuildLogRecords = $buildLogRecords
        ExeRecords = $exeRecords
        NativeReadinessResult = $nativeReadinessResult
        NativePreflightResults = $nativePreflightResults
        SmokeResults = $smokeResults
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Publish-TalkRelease `
        -VersionId $VersionId `
        -ReleaseRoot $ReleaseRoot `
        -SmokeRoot $SmokeRoot `
        -DisablePackagedApiKeyDiscovery:$DisablePackagedApiKeyDiscovery `
        -SkipVerification:$SkipVerification `
        -SkipBuild:$SkipBuild `
        -SkipSmoke:$SkipSmoke `
        -SkipNativePreflight:$SkipNativePreflight `
        -SkipNativeReadiness:$SkipNativeReadiness
}
