$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopQwenNativeMicProbe.ps1'

. $scriptPath

function New-TalkDesktopTestWavFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$SampleRate,
        [Parameter(Mandatory = $true)][int]$Channels,
        [Parameter(Mandatory = $true)][int16[]]$Samples
    )

    $bytesPerSample = 2
    $dataSize = $Samples.Length * $bytesPerSample
    $byteRate = $SampleRate * $Channels * $bytesPerSample
    $blockAlign = $Channels * $bytesPerSample

    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    try {
        $writer = New-Object System.IO.BinaryWriter($stream)
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF'))
        $writer.Write([int] (36 + $dataSize))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('WAVE'))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('fmt '))
        $writer.Write([int]16)
        $writer.Write([int16]1)
        $writer.Write([int16]$Channels)
        $writer.Write([int]$SampleRate)
        $writer.Write([int]$byteRate)
        $writer.Write([int16]$blockAlign)
        $writer.Write([int16]16)
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('data'))
        $writer.Write([int]$dataSize)
        foreach ($sample in $Samples) {
            $writer.Write([int16]$sample)
        }
        $writer.Flush()
    }
    finally {
        $stream.Dispose()
    }
}

Describe 'Invoke-TalkDesktopQwenNativeMicProbe helpers' {
    It 'builds a native microphone probe config with push-to-talk mode and native backends' {
        $configText = New-TalkDesktopQwenNativeMicProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F14' `
            -AudioDir 'C:\Talk\.runtime\native-mic\audio' `
            -LogsDir 'C:\Talk\.runtime\native-mic\logs'

        $configText | Should Match 'mode = "push_to_talk"'
        $configText | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F14"'
        $configText | Should Match 'backend = "native_windows"'
        $configText | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $configText | Should Match 'chat_model = "qwen3\.7-plus"'
        $configText | Should Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
        $configText | Should Match 'mode = "clipboard_paste"'
        $configText | Should Match 'clipboard_backend = "native_windows"'
        $configText | Should Match 'dir = "C:\\\\Talk\\\\.runtime\\\\native-mic\\\\logs"'
    }

    It 'builds a native microphone probe config with explicit provider endpoint and model overrides' {
        $configText = New-TalkDesktopQwenNativeMicProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F14' `
            -AudioDir 'C:\Talk\.runtime\native-mic\audio' `
            -LogsDir 'C:\Talk\.runtime\native-mic\logs' `
            -ProviderAudioTranscriptionsEndpoint 'http://127.0.0.1:4200/v1/audio/transcriptions' `
            -ProviderChatCompletionsEndpoint 'http://127.0.0.1:4200/v1/chat/completions' `
            -ProviderTranscriptionTransport 'audio_transcriptions' `
            -ProviderTranscriptionModel 'gpt-4o-mini-transcribe' `
            -ProviderChatModel 'gpt-4o-mini'

        $configText | Should Match 'audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"'
        $configText | Should Match 'chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"'
        $configText | Should Match 'transcription_transport = "audio_transcriptions"'
        $configText | Should Match 'transcription_model = "gpt-4o-mini-transcribe"'
        $configText | Should Match 'chat_model = "gpt-4o-mini"'
    }

    It 'includes an explicit native input device when requested' {
        $configText = New-TalkDesktopQwenNativeMicProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F14' `
            -AudioDir 'C:\Talk\.runtime\native-mic\audio' `
            -LogsDir 'C:\Talk\.runtime\native-mic\logs' `
            -InputDevice 'Virtual Mic'

        $configText | Should Match 'input_device = "Virtual Mic"'
    }

    It 'builds a concise native microphone probe summary' {
        $summary = New-TalkDesktopQwenNativeMicProbeSummary `
            -SmokeRoot 'C:\Talk\.runtime\native-mic' `
            -Session ([pscustomobject]@{
                status = 'completed'
                transcript = 'What is the capital of France?'
                output_text = 'Paris'
            }) `
            -ConfigPath 'C:\Talk\.runtime\native-mic\config.toml' `
            -LogPath 'C:\Talk\.runtime\native-mic\logs\session.json' `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -SpokenText 'What is the capital of France?'

        $summary.status | Should Be 'completed'
        $summary.transcript | Should Be 'What is the capital of France?'
        $summary.outputText | Should Be 'Paris'
        $summary.binaryPath | Should Be 'C:\Release\talk-desktop.exe'
        $summary.spokenText | Should Be 'What is the capital of France?'
        $summary.snapshotPath | Should Be 'C:\Talk\.runtime\native-mic\text-target\snapshot.txt'
    }

    It 'matches expected text from either output text or captured inserted text' {
        $matchedOutput = Test-TalkDesktopQwenNativeMicProbeExpectedTextMatch `
            -ExpectedText 'Paris' `
            -OutputText 'Paris.' `
            -CapturedText ''
        $matchedCapture = Test-TalkDesktopQwenNativeMicProbeExpectedTextMatch `
            -ExpectedText 'Paris' `
            -OutputText '' `
            -CapturedText 'Paris.'
        $missed = Test-TalkDesktopQwenNativeMicProbeExpectedTextMatch `
            -ExpectedText 'Paris' `
            -OutputText '好的，有需要随时告诉我。' `
            -CapturedText '好的，有需要随时告诉我。'

        $matchedOutput | Should Be $true
        $matchedCapture | Should Be $true
        $missed | Should Be $false
    }

    It 'allows a blank log path when the native audio preflight fails before launch' {
        $summary = New-TalkDesktopQwenNativeMicProbeSummary `
            -SmokeRoot 'C:\Talk\.runtime\native-mic' `
            -Session ([pscustomobject]@{
                status = 'failed'
                transcript = $null
                output_text = $null
            }) `
            -ConfigPath 'C:\Talk\.runtime\native-mic\config.toml' `
            -LogPath '' `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -SpokenText 'What is the capital of France?'

        $summary.status | Should Be 'failed'
        $summary.logPath | Should Be ''
    }

    It 'prefers an explicit speaker wav path when provided' {
        $resolvedExplicit = Resolve-TalkDesktopQwenNativeMicProbeSpeakerAudioPath `
            -SpeakerWavPath 'C:\Audio\speaker.wav'
        $resolvedExplicit | Should Be 'C:\Audio\speaker.wav'
    }

    It 'materializes a fallback speaker wav inside the smoke root when no explicit path is provided' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-speaker-wav-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null

        try {
            $resolvedDefault = Resolve-TalkDesktopQwenNativeMicProbeSpeakerAudioPath `
                -SmokeRoot $tempRoot `
                -PromptText 'What is the capital of France?'

            $resolvedDefault | Should Be (Join-Path $tempRoot 'probe.wav')
            Test-Path -LiteralPath $resolvedDefault | Should Be $true
            (Get-Item -LiteralPath $resolvedDefault).Length | Should BeGreaterThan 44
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'prefers an explicit speaker output device and otherwise infers Virtual Speakers for Virtual Mic probes' {
        $explicit = Resolve-TalkDesktopQwenNativeMicProbeSpeakerOutputDevice `
            -SpeakerOutputDevice 'Headphones' `
            -InputDevice 'Virtual Mic'
        $explicit | Should Be 'Headphones'

        $inferred = Resolve-TalkDesktopQwenNativeMicProbeSpeakerOutputDevice `
            -InputDevice 'Virtual Mic'
        $inferred | Should Be 'Virtual Speakers'
    }

    It 'detects when the requested native mic route looks like an AudioRelay virtual route' {
        (Test-TalkDesktopQwenNativeMicProbeUsesAudioRelayVirtualRoute `
            -InputDevice 'Virtual Mic' `
            -SpeakerOutputDevice 'Virtual Speakers') | Should Be $true
        (Test-TalkDesktopQwenNativeMicProbeUsesAudioRelayVirtualRoute `
            -InputDevice '麦克风' `
            -SpeakerOutputDevice '') | Should Be $false
    }

    It 'extracts the latest AudioRelay startup session and detects that no remote relay was resumed' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-audiorelay-log-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'audiorelay.log'
            @'
2024-07-22 12:21:50:674 [INFO] Remotely connecting to 192.168.15.67...
2024-07-22 12:21:50:735 [INFO] Received server config, server: 0.26.1, os: ANDROID, osVersion: Android API 30
2026-07-25 02:21:00:023 [INFO] Version: 0.27.5, os: Windows 11, osVersion: 10.0
2026-07-25 02:21:02:048 [INFO] Showing main window...
2026-07-25 02:21:08:166 [INFO] Virtual mic: 1.0.2.0, Virtual speaker: 1.0.2.0
'@ | Set-Content -LiteralPath $logPath -Encoding UTF8

            $state = Get-TalkDesktopQwenNativeMicProbeAudioRelayRecentSessionState -LogPath $logPath

            $state.sessionStartedAt | Should Be '2026-07-25 02:21:00'
            $state.remoteConnectSeen | Should Be $false
            $state.remoteConnectTarget | Should Be ''
            $state.serverConfigSeen | Should Be $false
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'extracts the last observed remote AudioRelay target from historical log lines even when the current session stayed idle' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-audiorelay-history-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'audiorelay.log'
            @'
2024-07-22 12:21:50:674 [INFO] Remotely connecting to 192.168.15.67...
2024-07-22 12:21:50:735 [INFO] Received server config, server: 0.26.1, os: ANDROID, osVersion: Android API 30
2026-07-24 02:21:00:023 [INFO] Version: 0.27.5, os: Windows 11, osVersion: 10.0
2026-07-24 02:21:02:048 [INFO] Showing main window...
2026-07-24 02:21:08:166 [INFO] Virtual mic: 1.0.2.0, Virtual speaker: 1.0.2.0
'@ | Set-Content -LiteralPath $logPath -Encoding UTF8

            $state = Get-TalkDesktopQwenNativeMicProbeAudioRelayHistoricalTargetState -LogPath $logPath

            $state.lastRemoteConnectAt | Should Be '2024-07-22 12:21:50'
            $state.lastRemoteConnectTarget | Should Be '192.168.15.67'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'extracts the configured AudioRelay server address from Java preferences' {
        Mock Get-ItemProperty -ParameterFilter {
            $Path -eq 'HKCU:\Software\JavaSoft\Prefs\com\azefsw\audioconnect'
        } {
            [pscustomobject]@{
                last_server_address = '192.168.15.67'
            }
        }

        (Get-TalkDesktopQwenNativeMicProbeAudioRelayConfiguredServerAddress) | Should Be '192.168.15.67'
    }

    It 'treats TCP port 59100 as the primary configured-server reachability signal' {
        Mock Test-NetConnection -ParameterFilter {
            $ComputerName -eq '192.168.15.67' -and $Port -eq 59100 -and $InformationLevel -eq 'Quiet'
        } { $true }
        Mock Test-Connection { throw 'ping should not be needed when tcp/59100 is already open' }

        (Get-TalkDesktopQwenNativeMicProbeAudioRelayServerAddressReachability -Address '192.168.15.67') | Should Be 'tcp_59100_open'
    }

    It 'collects AudioRelay virtual-route diagnostics from local process, install, and log evidence' {
        Mock Get-Process {
            @(
                [pscustomobject]@{ ProcessName = 'AudioRelay' },
                [pscustomobject]@{ ProcessName = 'audiorelay-backend' }
            )
        }
        Mock Get-ItemProperty -ParameterFilter {
            @($Path) -contains 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
        } {
            [pscustomobject]@{
                DisplayName = 'AudioRelay version 0.27.5'
                DisplayVersion = '0.27.5'
                InstallLocation = 'C:\Program Files (x86)\AudioRelay\'
                Publisher = 'Asapha Halifa'
            }
        }
        Mock Test-Path -ParameterFilter {
            $LiteralPath -like 'C:\Program Files (x86)\AudioRelay*' -or
            $LiteralPath -like 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs*'
        } {
            return ($LiteralPath -like 'C:\Program Files (x86)\AudioRelay*' -or
                $LiteralPath -like 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs*')
        }
        Mock Get-ChildItem -ParameterFilter { $LiteralPath -eq 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs' -and $File } {
            [pscustomobject]@{
                FullName = 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.log'
                Name = 'audiorelay.log'
                LastWriteTime = [datetime]'2024-07-22T16:27:21+08:00'
            }
        }
        Mock Get-TalkDesktopQwenNativeMicProbeAudioRelayRecentSessionState {
            [pscustomobject]@{
                sessionStartedAt = '2026-07-25 02:21:00'
                remoteConnectSeen = $true
                remoteConnectTarget = '192.168.15.67'
                serverConfigSeen = $true
            }
        }
        Mock Get-TalkDesktopQwenNativeMicProbeAudioRelayHistoricalTargetState {
            [pscustomobject]@{
                lastRemoteConnectAt = '2024-07-22 12:21:50'
                lastRemoteConnectTarget = '192.168.15.67'
            }
        }
        Mock Get-TalkDesktopQwenNativeMicProbeAudioRelayConfiguredServerAddress { '192.168.15.67' }
        Mock Get-TalkDesktopQwenNativeMicProbeAudioRelayServerAddressReachability { 'tcp_59100_open' }

        $diagnostics = Get-TalkDesktopQwenNativeMicProbeVirtualRouteDiagnostics `
            -InputDevice 'Virtual Mic' `
            -SpeakerOutputDevice 'Virtual Speakers'

        $diagnostics.routeKind | Should Be 'audio_relay'
        $diagnostics.runningProcessCount | Should Be 2
        @($diagnostics.runningProcesses) | Should Be @('AudioRelay', 'audiorelay-backend')
        $diagnostics.installLocation | Should Be 'C:\Program Files (x86)\AudioRelay\'
        $diagnostics.installLocationExists | Should Be $true
        $diagnostics.displayVersion | Should Be '0.27.5'
        $diagnostics.latestLogPath | Should Be 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.log'
        $diagnostics.latestLogLastWriteTime | Should Be '2024-07-22 16:27:21'
        $diagnostics.health | Should Be 'controller_present'
        $diagnostics.recentSession.remoteConnectSeen | Should Be $true
        $diagnostics.recentSession.remoteConnectTarget | Should Be '192.168.15.67'
        $diagnostics.historicalTarget.lastRemoteConnectAt | Should Be '2024-07-22 12:21:50'
        $diagnostics.historicalTarget.lastRemoteConnectTarget | Should Be '192.168.15.67'
        $diagnostics.configuredServerAddress | Should Be '192.168.15.67'
        $diagnostics.configuredServerReachability | Should Be 'tcp_59100_open'
    }

    It 'prefers the main AudioRelay log over the backend log when deriving recent relay-session state' {
        Mock Get-Process { @([pscustomobject]@{ ProcessName = 'AudioRelay' }) }
        Mock Get-ItemProperty -ParameterFilter {
            @($Path) -contains 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
        } {
            [pscustomobject]@{
                DisplayName = 'AudioRelay version 0.27.5'
                DisplayVersion = '0.27.5'
                InstallLocation = 'C:\Program Files (x86)\AudioRelay\'
                Publisher = 'Asapha Halifa'
            }
        }
        Mock Test-Path -ParameterFilter {
            $LiteralPath -like 'C:\Program Files (x86)\AudioRelay*' -or
            $LiteralPath -like 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs*'
        } { $true }
        Mock Get-ChildItem -ParameterFilter { $LiteralPath -eq 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs' -and $File } {
            @(
                [pscustomobject]@{
                    FullName = 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.backend.log'
                    Name = 'audiorelay.backend.log'
                    LastWriteTime = [datetime]'2026-07-25T02:21:10+08:00'
                },
                [pscustomobject]@{
                    FullName = 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.log'
                    Name = 'audiorelay.log'
                    LastWriteTime = [datetime]'2026-07-25T02:21:08+08:00'
                }
            )
        }
        Mock Get-TalkDesktopQwenNativeMicProbeAudioRelayRecentSessionState -ParameterFilter {
            $LogPath -eq 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.log'
        } {
            [pscustomobject]@{
                sessionStartedAt = '2026-07-25 02:21:00'
                remoteConnectSeen = $false
                remoteConnectTarget = ''
                serverConfigSeen = $false
            }
        }

        $diagnostics = Get-TalkDesktopQwenNativeMicProbeVirtualRouteDiagnostics `
            -InputDevice 'Virtual Mic' `
            -SpeakerOutputDevice 'Virtual Speakers'

        $diagnostics.latestLogPath | Should Be 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.backend.log'
        $diagnostics.recentSession.sessionStartedAt | Should Be '2026-07-25 02:21:00'
        $diagnostics.health | Should Be 'controller_idle'
    }

    It 'tolerates a missing AudioRelay uninstall record and still returns virtual-route diagnostics' {
        Mock Get-Process { @() }
        Mock Get-ItemProperty -ParameterFilter {
            @($Path) -contains 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
        } { @() }
        Mock Test-Path -ParameterFilter { $LiteralPath -like 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs*' } { $false }

        $diagnostics = Get-TalkDesktopQwenNativeMicProbeVirtualRouteDiagnostics `
            -InputDevice 'Virtual Mic' `
            -SpeakerOutputDevice 'Virtual Speakers'

        $diagnostics.routeKind | Should Be 'audio_relay'
        $diagnostics.runningProcessCount | Should Be 0
        $diagnostics.installLocation | Should Be ''
        $diagnostics.installLocationExists | Should Be $false
        $diagnostics.health | Should Be 'controller_missing'
    }

    It 'ignores uninstall entries that do not expose DisplayName while still finding the AudioRelay record' {
        Mock Get-Process { @() }
        Mock Get-ItemProperty -ParameterFilter {
            @($Path) -contains 'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
        } {
            @(
                [pscustomobject]@{
                    InstallLocation = 'C:\NonAudioRelay'
                },
                [pscustomobject]@{
                    DisplayName = 'AudioRelay version 0.27.5'
                    DisplayVersion = '0.27.5'
                    InstallLocation = 'C:\Program Files (x86)\AudioRelay\'
                    Publisher = 'Asapha Halifa'
                }
            )
        }
        Mock Test-Path -ParameterFilter { $LiteralPath -like 'C:\Program Files (x86)\AudioRelay*' } { $false }
        Mock Test-Path -ParameterFilter { $LiteralPath -like 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs*' } { $false }

        $diagnostics = Get-TalkDesktopQwenNativeMicProbeVirtualRouteDiagnostics `
            -InputDevice 'Virtual Mic' `
            -SpeakerOutputDevice 'Virtual Speakers'

        $diagnostics.displayName | Should Be 'AudioRelay version 0.27.5'
        $diagnostics.installLocation | Should Be 'C:\Program Files (x86)\AudioRelay\'
    }

    It 'classifies a missing AudioRelay install path plus zero processes as an orphaned virtual route' {
        $health = Resolve-TalkDesktopQwenNativeMicProbeVirtualRouteHealth -VirtualRouteDiagnostics ([pscustomobject]@{
            routeKind = 'audio_relay'
            runningProcessCount = 0
            installLocation = 'C:\Program Files (x86)\AudioRelay\'
            installLocationExists = $false
        })

        $health | Should Be 'orphaned_install'
    }

    It 'classifies a running AudioRelay controller without a remote relay session as idle' {
        $health = Resolve-TalkDesktopQwenNativeMicProbeVirtualRouteHealth -VirtualRouteDiagnostics ([pscustomobject]@{
            routeKind = 'audio_relay'
            runningProcessCount = 2
            installLocation = 'C:\Program Files (x86)\AudioRelay\'
            installLocationExists = $true
            recentSession = [pscustomobject]@{
                remoteConnectSeen = $false
                serverConfigSeen = $false
            }
        })

        $health | Should Be 'controller_idle'
    }

    It 'appends an AudioRelay controller hint when a virtual route is silent and the controller app looks missing' {
        $failureReason = Add-TalkDesktopQwenNativeMicProbeVirtualRouteFailureHint `
            -FailureReason 'Native audio probe captured only silence; input device routing is unusable' `
            -VirtualRouteDiagnostics ([pscustomobject]@{
                routeKind = 'audio_relay'
                health = 'orphaned_install'
                runningProcessCount = 0
                installLocation = 'C:\Program Files (x86)\AudioRelay'
                installLocationExists = $false
                latestLogPath = 'C:\Users\vmjcv\AppData\Local\AudioRelay\Logs\audiorelay.log'
                latestLogLastWriteTime = '2024-07-22 16:27:21'
            })

        $failureReason | Should Match 'AudioRelay appears orphaned'
        $failureReason | Should Match 'processes=0'
        $failureReason | Should Match 'install path missing'
    }

    It 'appends an idle-session hint when AudioRelay is running but no remote relay session is active' {
        $failureReason = Add-TalkDesktopQwenNativeMicProbeVirtualRouteFailureHint `
            -FailureReason 'Native audio probe captured only silence; input device routing is unusable' `
            -VirtualRouteDiagnostics ([pscustomobject]@{
                routeKind = 'audio_relay'
                health = 'controller_idle'
                runningProcessCount = 2
                installLocation = 'C:\Program Files (x86)\AudioRelay\'
                installLocationExists = $true
                latestLogLastWriteTime = '2026-07-25 02:21:08'
                recentSession = [pscustomobject]@{
                    sessionStartedAt = '2026-07-25 02:21:00'
                    remoteConnectSeen = $false
                    remoteConnectTarget = ''
                    serverConfigSeen = $false
                }
                historicalTarget = [pscustomobject]@{
                    lastRemoteConnectAt = '2024-07-22 12:21:50'
                    lastRemoteConnectTarget = '192.168.15.67'
                }
                configuredServerAddress = '192.168.15.67'
                configuredServerReachability = 'tcp_59100_open'
            })

        $failureReason | Should Match 'running but no active relay session'
        $failureReason | Should Match 'session start \[2026-07-25 02:21:00\]'
        $failureReason | Should Match 'configured target \[192\.168\.15\.67; reachability=tcp_59100_open\]'
        $failureReason | Should Match 'last remote target \[192\.168\.15\.67 @ 2024-07-22 12:21:50\]'
        $failureReason | Should Match 'Player tab -> Mic mode -> click server'
    }

    It 'writes a structured summary when the release lacks the bundled internal Talk helper' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-missing-helper-' + [guid]::NewGuid().ToString())
        $releaseRoot = Join-Path $env:TEMP ('talk-native-mic-missing-helper-release-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $releaseRoot -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $releaseRoot 'talk-desktop.exe') -Value '' -Encoding ASCII

            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget { throw 'foreground target must not start without bundle helper' }
            Mock Start-TalkDesktop { throw 'desktop shell must not launch without bundle helper' }

            {
                Invoke-TalkDesktopQwenNativeMicProbe `
                    -ReleaseDir $releaseRoot `
                    -SmokeRoot $tempRoot `
                    -SpeakerWavPath (Join-Path $tempRoot 'probe.wav')
            } | Should Throw 'desktop bundle release'

            $summaryPath = Join-Path $tempRoot 'qwen-native-mic-probe-summary.json'
            Test-Path -LiteralPath $summaryPath | Should Be $true

            $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $summary.status | Should Be 'failed'
            $summary.failureReason | Should Match 'desktop bundle release'
            $summary.logPath | Should Be ''
            $summary.binaryPath | Should Be (Join-Path $releaseRoot 'talk-desktop.exe')
            $summary.speakerOutputDevice | Should Be ''
            Assert-MockCalled Start-TalkTextCaptureTarget -Times 0 -Exactly
            Assert-MockCalled Start-TalkDesktop -Times 0 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $releaseRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'formats insert target environment overrides with explicit window and child focus handles' {
        $overrides = Get-TalkDesktopQwenNativeMicProbeInsertTargetEnvironmentOverrides -Target ([pscustomobject]@{
            Hwnd = [System.IntPtr]0x500D56
            TextBoxHwnd = [System.IntPtr]0x500D88
        })

        $overrides['TALK_DESKTOP_INSERT_TARGET_WINDOW'] | Should Be '0x500D56'
        $overrides['TALK_DESKTOP_INSERT_TARGET_FOCUS'] | Should Be '0x500D88'
    }

    It 'omits the focus override when the insert target has no child textbox handle' {
        $overrides = Get-TalkDesktopQwenNativeMicProbeInsertTargetEnvironmentOverrides -Target ([pscustomobject]@{
            Hwnd = [System.IntPtr]0x500D56
            TextBoxHwnd = [System.IntPtr]::Zero
        })

        $overrides['TALK_DESKTOP_INSERT_TARGET_WINDOW'] | Should Be '0x500D56'
        ($overrides.Keys -contains 'TALK_DESKTOP_INSERT_TARGET_FOCUS') | Should Be $false
    }

    It 'clears an existing smoke root before starting a fresh probe run' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-root-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path (Join-Path $tempRoot 'logs') -Force | Out-Null
        'stale' | Set-Content -LiteralPath (Join-Path $tempRoot 'logs\stale.json') -Encoding UTF8

        try {
            Initialize-TalkDesktopQwenNativeMicProbeRoot -SmokeRoot $tempRoot

            Test-Path -LiteralPath (Join-Path $tempRoot 'logs\stale.json') | Should Be $false
            Test-Path -LiteralPath $tempRoot | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reports zero signal metrics for a silent 16-bit wav file' {
        $tempRoot = Join-Path $env:TEMP ('talk-native-mic-wav-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $wavPath = Join-Path $tempRoot 'silent.wav'
            $samples = [int16[]](0, 0, 0, 0)
            New-TalkDesktopTestWavFile `
                -Path $wavPath `
                -SampleRate 16000 `
                -Channels 1 `
                -Samples $samples

            $summary = Get-TalkDesktopProbeWavSignalSummary -AudioPath $wavPath

            $summary.sampleRate | Should Be 16000
            $summary.channels | Should Be 1
            $summary.peak | Should Be 0
            $summary.rms | Should Be 0
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'treats silent or provider-weak native audio probe summaries as unusable input' {
        $silent = Test-TalkDesktopAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0
            rms = 0
            silent = $true
        })
        $weak = Test-TalkDesktopAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0.02
            rms = 0.001
            silent = $false
        })
        $audible = Test-TalkDesktopAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0.2
            rms = 0.05
            silent = $false
        })

        $silent | Should Be $false
        $weak | Should Be $false
        $audible | Should Be $true
    }

    It 'records a provider-weak native mic failure reason instead of only a silence reason' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Get-TalkDesktopLaunchAudioProbeFailureReason'
        $scriptText | Should Match 'Native audio probe captured speech that is too weak for provider transcription; input route level is unusable'
    }

    It 'starts background speaker playback during native audio preflight when an output route is provided' {
        Mock Start-TalkDesktop {
            [pscustomobject]@{
                peak = 0.25
                silent = $false
            }
        }
        Mock Start-Job {
            [pscustomobject]@{
                State = 'Completed'
                ChildJobs = @()
            }
        }
        Mock Wait-TalkDesktopProbeSpeakerJob {}
        Mock Remove-TalkDesktopProbeSpeakerJob {}

        $summary = Invoke-TalkDesktopNativeAudioSignalProbe `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -ReleaseDir 'C:\Release' `
            -ConfigPath 'C:\Talk\.runtime\native-mic\config.toml' `
            -SpeakerWavPath 'C:\Audio\probe.wav' `
            -SpeakerOutputDevice 'Virtual Speakers' `
            -TalkBinaryPath 'C:\Release\talk.exe'

        $summary.peak | Should Be 0.25
        Assert-MockCalled Start-Job -Times 1 -Exactly
        Assert-MockCalled Wait-TalkDesktopProbeSpeakerJob -Times 1 -Exactly
        Assert-MockCalled Remove-TalkDesktopProbeSpeakerJob -Times 1 -Exactly
        Assert-MockCalled Start-TalkDesktop -Times 1 -Exactly
    }

    It 'wires the resolved virtual speaker route into the native audio preflight call' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Invoke-TalkDesktopNativeAudioSignalProbe[\s\S]*-SpeakerWavPath \$resolvedSpeakerWavPath[\s\S]*-SpeakerOutputDevice \$resolvedSpeakerOutputDevice[\s\S]*-TalkBinaryPath \$resolvedTalkBinaryPath'
    }

    It 'holds the push-to-talk hotkey while the speaker probe audio is playing' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Invoke-TalkDesktopGlobalHotkeyOperation[\s\S]*-Shortcut \$Hotkey[\s\S]*-ScriptBlock \{[\s\S]*Start-Sleep -Milliseconds 700[\s\S]*Invoke-TalkDesktopProbeSpeakerWav[\s\S]*Start-Sleep -Milliseconds 500[\s\S]*\}'
    }

    It 'keeps the text target foreground refreshed while waiting for native mic inserted text' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Set-TalkTextCaptureTargetForeground -Target \$target'
        $scriptText | Should Match 'Invoke-TalkDesktopPinnedWindowOperation -Hwnd \$target\.Hwnd -ScriptBlock'
        $scriptText | Should Match 'Wait-TalkTextCaptureContainsWithForegroundRefresh'
    }

    It 'primes and selects the text target child textbox before waiting for native mic inserted text' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Invoke-TalkTextCaptureTargetPrimer[\s\S]*-ChildHwnd \$target\.TextBoxHwnd'
        $scriptText | Should Match 'Select-TalkTextCaptureTargetChildText -ChildHwnd \$target\.TextBoxHwnd'
        $scriptText | Should Match 'Remove-TalkTextCapturePrimerPrefix'
    }

    It 'keeps the text target pinned topmost while waiting for native mic inserted text' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match '\$sessionCapture = Invoke-TalkDesktopPinnedWindowOperation[\s\S]*Wait-LatestSessionLog[\s\S]*Wait-TalkTextCaptureContainsWithForegroundRefresh'
    }
}
