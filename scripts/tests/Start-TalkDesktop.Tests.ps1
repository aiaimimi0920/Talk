$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Start-TalkDesktop.ps1'

. $scriptPath

Describe 'Start-TalkDesktop helpers' {
    It 'resolves the packaged internal Talk helper binary from the hidden support directory' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-internal-helper-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path (Join-Path $tempRoot '.internal') -Force | Out-Null
        try {
            $internalTalkPath = Join-Path $tempRoot '.internal\talk.exe'
            Set-Content -LiteralPath $internalTalkPath -Value '' -Encoding ASCII

            $resolved = Resolve-TalkDesktopLaunchTalkBinaryPath -ReleaseDir $tempRoot

            $resolved | Should Be $internalTalkPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'prefers an explicit api key over json file and environment fallback' {
        $env:TALK_PROVIDER_API_KEY = 'env-key'
        try {
            $tempRoot = Join-Path $env:TEMP ('talk-start-launch-key-' + [guid]::NewGuid().ToString())
            New-Item -ItemType Directory -Path $tempRoot | Out-Null
            $jsonPath = Join-Path $tempRoot 'manual-live.json'
            '{"apiKey":"json-key"}' | Set-Content -LiteralPath $jsonPath -Encoding UTF8

            $resolved = Resolve-TalkDesktopLaunchApiKey `
                -ApiKey 'direct-key' `
                -ApiKeyJsonPath $jsonPath

            $resolved | Should Be 'direct-key'
        }
        finally {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reads api key from json file when direct key is omitted' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-key-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $jsonPath = Join-Path $tempRoot 'manual-live.json'
            '{"apiKey":"json-key"}' | Set-Content -LiteralPath $jsonPath -Encoding UTF8

            $resolved = Resolve-TalkDesktopLaunchApiKey -ApiKeyJsonPath $jsonPath

            $resolved | Should Be 'json-key'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'auto-discovers a local qwen dashscope key for qwen official configs when no explicit key is provided' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-autokey-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        $originalTalkProviderApiKey = $env:TALK_PROVIDER_API_KEY
        try {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot

            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"auto-json-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8

            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
chat_completions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8

            $resolved = Resolve-TalkDesktopLaunchApiKey -ConfigPath $configPath

            $resolved | Should Be 'auto-json-key'
        }
        finally {
            if ($null -eq $originalTalkProviderApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $originalTalkProviderApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'does not auto-discover a qwen key for non-qwen configs' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-nonqwen-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        $originalTalkProviderApiKey = $env:TALK_PROVIDER_API_KEY
        try {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot

            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"auto-json-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8

            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"
chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8

            { Resolve-TalkDesktopLaunchApiKey -ConfigPath $configPath } |
                Should Throw 'Talk desktop launch requires -ApiKey, -ApiKeyJsonPath, or TALK_PROVIDER_API_KEY'
        }
        finally {
            if ($null -eq $originalTalkProviderApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $originalTalkProviderApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes a same-directory hotkey override config when a launch override is requested' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-config-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[trigger]
mode = "push_to_talk"
toggle_shortcut = "Ctrl+Alt+Space"
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8

            $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -Hotkey 'Ctrl+Alt+F18'

            $effectiveConfigPath | Should Not Be $baseConfigPath
            (Split-Path -Parent $effectiveConfigPath) | Should Be $tempRoot
            $effectiveConfigText = Get-Content -LiteralPath $effectiveConfigPath -Raw
            $effectiveConfigText | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F18"'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes a same-directory input-device override config when a launch override is requested' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-input-device-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[audio]
backend = "native_windows"
max_recording_seconds = 15
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8

            $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -InputDevice 'Virtual Mic'

            $effectiveConfigPath | Should Not Be $baseConfigPath
            $effectiveConfigText = Get-Content -LiteralPath $effectiveConfigPath -Raw
            $effectiveConfigText | Should Match 'input_device = "Virtual Mic"'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes a unique same-directory override config for each launch override invocation' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-config-unique-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[trigger]
mode = "push_to_talk"
toggle_shortcut = "Ctrl+Alt+Space"
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8

            $firstEffectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -Hotkey 'Ctrl+Alt+F18'
            $secondEffectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -Hotkey 'Ctrl+Alt+F19'

            $firstEffectiveConfigPath | Should Not Be $secondEffectiveConfigPath
            (Split-Path -Parent $firstEffectiveConfigPath) | Should Be $tempRoot
            (Split-Path -Parent $secondEffectiveConfigPath) | Should Be $tempRoot
            (Split-Path -Leaf $firstEffectiveConfigPath) | Should Match '^talk-desktop\.runtime-launch[-\.]'
            (Split-Path -Leaf $secondEffectiveConfigPath) | Should Match '^talk-desktop\.runtime-launch[-\.]'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'removes a generated runtime-launch override config after one-shot probe flows complete' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-config-cleanup-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[trigger]
mode = "push_to_talk"
toggle_shortcut = "Ctrl+Alt+Space"
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8

            $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -Hotkey 'Ctrl+Alt+F18'
            Test-Path -LiteralPath $effectiveConfigPath | Should Be $true

            Remove-TalkDesktopLaunchTemporaryConfig `
                -BaseConfigPath $baseConfigPath `
                -EffectiveConfigPath $effectiveConfigPath

            Test-Path -LiteralPath $effectiveConfigPath | Should Be $false
            Test-Path -LiteralPath $baseConfigPath | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'does not remove unrelated non-generated config files during launch cleanup' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-config-keep-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            $otherPath = Join-Path $tempRoot 'talk-desktop.runtime-qwen-probe.toml'
            @'
[trigger]
mode = "push_to_talk"
toggle_shortcut = "Ctrl+Alt+Space"
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8
            'probe config' | Set-Content -LiteralPath $otherPath -Encoding UTF8

            Remove-TalkDesktopLaunchTemporaryConfig `
                -BaseConfigPath $baseConfigPath `
                -EffectiveConfigPath $otherPath

            Test-Path -LiteralPath $baseConfigPath | Should Be $true
            Test-Path -LiteralPath $otherPath | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'replaces an existing input-device line when a new launch override is requested' {
        $tempRoot = Join-Path $env:TEMP ('talk-start-launch-input-device-replace-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $baseConfigPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[audio]
backend = "native_windows"
input_device = "Old Mic"
max_recording_seconds = 15
'@ | Set-Content -LiteralPath $baseConfigPath -Encoding UTF8

            $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig `
                -BaseConfigPath $baseConfigPath `
                -InputDevice 'Virtual Mic'

            $effectiveConfigText = Get-Content -LiteralPath $effectiveConfigPath -Raw
            $effectiveConfigText | Should Match 'input_device = "Virtual Mic"'
            $effectiveConfigText | Should Not Match 'input_device = "Old Mic"'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'returns the base config path unchanged when no hotkey override is requested' {
        $baseConfigPath = 'C:\Release\talk-desktop.toml'

        $effectiveConfigPath = New-TalkDesktopLaunchEffectiveConfig -BaseConfigPath $baseConfigPath

        $effectiveConfigPath | Should Be $baseConfigPath
    }

    It 'builds a concise launch summary around the spawned process' {
        $summary = New-TalkDesktopLaunchSummary `
            -ReleaseDir 'C:\Release' `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -BaseConfigPath 'C:\Release\talk-desktop.toml' `
            -EffectiveConfigPath 'C:\Release\talk-desktop.runtime-launch.toml' `
            -ProcessId 45678

        $summary.releaseDir | Should Be 'C:\Release'
        $summary.binaryPath | Should Be 'C:\Release\talk-desktop.exe'
        $summary.baseConfigPath | Should Be 'C:\Release\talk-desktop.toml'
        $summary.effectiveConfigPath | Should Be 'C:\Release\talk-desktop.runtime-launch.toml'
        $summary.processId | Should Be 45678
    }

    It 'builds a concise native input-device inventory from a readiness report' {
        $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport ([pscustomobject]@{
            audio = [pscustomobject]@{
                nativeWindows = [pscustomobject]@{
                    status = 'ready'
                    reason = $null
                    requestedDeviceName = 'Virtual Mic'
                    deviceName = 'Virtual Mic'
                    availableDeviceNames = @('Virtual Mic', '麦克风')
                }
            }
            clipboard = [pscustomobject]@{
                nativeWindows = [pscustomobject]@{
                    status = 'ready'
                    reason = $null
                }
            }
        })

        $inventory.audioStatus | Should Be 'ready'
        $inventory.requestedInputDevice | Should Be 'Virtual Mic'
        $inventory.selectedInputDevice | Should Be 'Virtual Mic'
        @($inventory.availableInputDevices) | Should Be @('Virtual Mic', '麦克风')
        $inventory.clipboardStatus | Should Be 'ready'
    }

    It 'tolerates older readiness reports that do not yet expose requested or available input-device fields' {
        $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport ([pscustomobject]@{
            audio = [pscustomobject]@{
                nativeWindows = [pscustomobject]@{
                    status = 'ready'
                    reason = $null
                    deviceName = '麦克风'
                }
            }
            clipboard = [pscustomobject]@{
                nativeWindows = [pscustomobject]@{
                    status = 'ready'
                    reason = $null
                }
            }
        })

        $inventory.requestedInputDevice | Should Be ''
        $inventory.selectedInputDevice | Should Be '麦克风'
        @($inventory.availableInputDevices).Count | Should Be 0
        $inventory.audioStatus | Should Be 'ready'
    }

    It 'builds a concise audio probe summary from a talk probe report' {
        $summary = New-TalkDesktopLaunchAudioProbeSummary -ProbeReport ([pscustomobject]@{
            requestedDurationSeconds = 3
            audio = [pscustomobject]@{
                configuredBackend = 'native_windows'
                nativeWindows = [pscustomobject]@{
                    requestedDeviceName = 'Virtual Mic'
                    deviceName = 'Virtual Mic'
                }
                signal = [pscustomobject]@{
                    artifactPath = 'C:\Talk\.runtime\audio-probe.wav'
                    mimeType = 'audio/wav'
                    sampleRateHz = 48000
                    channels = 2
                    durationSeconds = 3
                    peak = 0.25
                    rms = 0.125
                    silent = $false
                }
            }
        })

        $summary.configuredBackend | Should Be 'native_windows'
        $summary.requestedDurationSeconds | Should Be 3
        $summary.requestedInputDevice | Should Be 'Virtual Mic'
        $summary.selectedInputDevice | Should Be 'Virtual Mic'
        $summary.artifactPath | Should Be 'C:\Talk\.runtime\audio-probe.wav'
        $summary.mimeType | Should Be 'audio/wav'
        $summary.sampleRateHz | Should Be 48000
        $summary.channels | Should Be 2
        $summary.durationSeconds | Should Be 3
        $summary.peak | Should Be 0.25
        $summary.rms | Should Be 0.125
        $summary.silent | Should Be $false
    }

    It 'merges readiness inventory into audio probe summaries for release-side diagnostics' {
        $summary = New-TalkDesktopLaunchAudioProbeSummary `
            -ProbeReport ([pscustomobject]@{
                requestedDurationSeconds = 3
                audio = [pscustomobject]@{
                    configuredBackend = 'native_windows'
                    nativeWindows = [pscustomobject]@{}
                    signal = [pscustomobject]@{
                        artifactPath = 'C:\Talk\.runtime\audio-probe.wav'
                        mimeType = 'audio/wav'
                        sampleRateHz = 48000
                        channels = 2
                        durationSeconds = 3
                        peak = 0
                        rms = 0
                        silent = $true
                    }
                }
            }) `
            -Inventory ([pscustomobject]@{
                audioStatus = 'ready'
                audioReason = ''
                requestedInputDevice = 'Virtual Mic'
                selectedInputDevice = 'Virtual Mic'
                availableInputDevices = @('Virtual Mic', '麦克风')
            })

        $summary.audioStatus | Should Be 'ready'
        $summary.audioReason | Should Be ''
        $summary.requestedInputDevice | Should Be 'Virtual Mic'
        $summary.selectedInputDevice | Should Be 'Virtual Mic'
        @($summary.availableInputDevices) | Should Be @('Virtual Mic', '麦克风')
        $summary.silent | Should Be $true
    }

    It 'treats silent audio probe summaries as unusable input for release-side round-trip probes' {
        $silent = Test-TalkDesktopLaunchAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0
            silent = $true
        })
        $audible = Test-TalkDesktopLaunchAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0.2
            silent = $false
        })

        $silent | Should Be $false
        $audible | Should Be $true
    }

    It 'builds a qwen round-trip provider config with audio-input transport and dry_run output' {
        $configText = New-TalkDesktopLaunchQwenRoundTripConfigContent `
            -LogsDir 'C:\Release\probe-logs'

        $configText | Should Match 'kind = "openai_compatible"'
        $configText | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $configText | Should Match 'transcription_model = "qwen3-asr-flash"'
        $configText | Should Match 'chat_model = "qwen3\.7-plus"'
        $configText | Should Match 'mode = "dry_run"'
        $configText | Should Match 'clipboard_backend = "fallback"'
        $configText | Should Match 'dir = "C:\\\\Release\\\\probe-logs"'
    }

    It 'builds a concise qwen round-trip summary from capture and provider session reports' {
        $summary = New-TalkDesktopLaunchQwenRoundTripSummary `
            -AudioProbeSummary ([pscustomobject]@{
                requestedInputDevice = 'Virtual Mic'
                selectedInputDevice = 'Virtual Mic'
                artifactPath = 'C:\Release\audio-probe.wav'
                peak = 0.25
                rms = 0.125
                silent = $false
            }) `
            -Session ([pscustomobject]@{
                status = 'completed'
                transcript = 'What is the capital of France?'
                output_text = 'Paris'
            }) `
            -ProviderConfigPath 'C:\Release\talk-desktop.runtime-qwen-probe.toml' `
            -LogPath 'C:\Release\probe-logs\session.json'

        $summary.status | Should Be 'completed'
        $summary.transcript | Should Be 'What is the capital of France?'
        $summary.outputText | Should Be 'Paris'
        $summary.inputDevice | Should Be 'Virtual Mic'
        $summary.captureAudioPath | Should Be 'C:\Release\audio-probe.wav'
        $summary.capturePeak | Should Be 0.25
        $summary.captureRms | Should Be 0.125
        $summary.captureSilent | Should Be $false
        $summary.providerConfigPath | Should Be 'C:\Release\talk-desktop.runtime-qwen-probe.toml'
        $summary.logPath | Should Be 'C:\Release\probe-logs\session.json'
    }
}
