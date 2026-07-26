$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'

. $scriptPath

function New-TalkDesktopLiveHotkeyTestWavFile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][int]$SampleRate,
        [Parameter(Mandatory = $true)][int]$Channels,
        [Parameter(Mandatory = $true)][int16[]]$Samples
    )

    $bitsPerSample = 16
    $dataSize = $Samples.Count * 2
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    $writer = New-Object System.IO.BinaryWriter($stream)
    try {
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF'))
        $writer.Write([int] (36 + $dataSize))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('WAVE'))
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('fmt '))
        $writer.Write([int]16)
        $writer.Write([int16]1)
        $writer.Write([int16]$Channels)
        $writer.Write([int]$SampleRate)
        $writer.Write([int]($SampleRate * $Channels * ($bitsPerSample / 8)))
        $writer.Write([int16]($Channels * ($bitsPerSample / 8)))
        $writer.Write([int16]$bitsPerSample)
        $writer.Write([System.Text.Encoding]::ASCII.GetBytes('data'))
        $writer.Write([int]$dataSize)
        foreach ($sample in $Samples) {
            $writer.Write([int16]$sample)
        }
    }
    finally {
        $writer.Dispose()
        $stream.Dispose()
    }
}

Describe 'Invoke-TalkDesktopLiveHotkeyProbe helpers' {
    It 'writes a toggle-mode native mic config with the requested hotkey and input device' {
        $content = New-TalkDesktopLiveHotkeyProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F20' `
            -AudioDir 'C:\Talk\.runtime\live-hotkey\audio' `
            -LogsDir 'C:\Talk\.runtime\live-hotkey\logs' `
            -InputDevice '麦克风'

        $content | Should Match 'mode = "toggle"'
        $content | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F20"'
        $content | Should Match 'backend = "native_windows"'
        $content | Should Match 'input_device = "麦克风"'
        $content | Should Match 'voice_mode = "transcribe"'
        $content | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $content | Should Match 'mode = "clipboard_paste"'
    }
    It 'allows explicit command mode for command-routing diagnostics without changing the default insertion probe mode' {
        $content = New-TalkDesktopLiveHotkeyProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F20' `
            -AudioDir 'C:\Talk\.runtime\live-hotkey\audio' `
            -LogsDir 'C:\Talk\.runtime\live-hotkey\logs' `
            -InputDevice '麦克风' `
            -VoiceMode 'command'

        $content | Should Match 'voice_mode = "command"'
        $content | Should Match 'mode = "toggle"'
    }

    It 'writes a live hotkey config with explicit provider endpoint and model overrides' {
        $content = New-TalkDesktopLiveHotkeyProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F20' `
            -AudioDir 'C:\Talk\.runtime\live-hotkey\audio' `
            -LogsDir 'C:\Talk\.runtime\live-hotkey\logs' `
            -ProviderAudioTranscriptionsEndpoint 'http://127.0.0.1:4200/v1/audio/transcriptions' `
            -ProviderChatCompletionsEndpoint 'http://127.0.0.1:4200/v1/chat/completions' `
            -ProviderTranscriptionTransport 'audio_transcriptions' `
            -ProviderTranscriptionModel 'gpt-4o-mini-transcribe' `
            -ProviderChatModel 'gpt-4o-mini'

        $content | Should Match 'audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"'
        $content | Should Match 'chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"'
        $content | Should Match 'transcription_transport = "audio_transcriptions"'
        $content | Should Match 'transcription_model = "gpt-4o-mini-transcribe"'
        $content | Should Match 'chat_model = "gpt-4o-mini"'
    }

    It 'matches expected text from either output text or captured foreground text' {
        $matchedOutput = Test-TalkDesktopLiveHotkeyProbeExpectedTextMatch `
            -ExpectedText '测试成功' `
            -OutputText '测试成功' `
            -CapturedText ''
        $matchedCapture = Test-TalkDesktopLiveHotkeyProbeExpectedTextMatch `
            -ExpectedText '测试成功' `
            -OutputText '' `
            -CapturedText '测试成功'
        $missed = Test-TalkDesktopLiveHotkeyProbeExpectedTextMatch `
            -ExpectedText '测试成功' `
            -OutputText '别的文本' `
            -CapturedText '仍然不匹配'

        $matchedOutput | Should Be $true
        $matchedCapture | Should Be $true
        $missed | Should Be $false
    }

    It 'builds a concise live hotkey probe summary' {
        $summary = New-TalkDesktopLiveHotkeyProbeSummary `
            -ScenarioRoot 'C:\Talk\.runtime\live-hotkey' `
            -Session ([pscustomobject]@{
                status = 'completed'
                transcript = '请回复测试成功。'
                output_text = '测试成功'
                error = $null
            }) `
            -CapturedText '测试成功' `
            -ExpectedText '测试成功' `
            -PromptText '请回复测试成功。请回复测试成功。' `
            -Hotkey 'Ctrl+Alt+F20' `
            -InputDevice '麦克风' `
            -LogPath 'C:\Talk\.runtime\live-hotkey\logs\session.json' `
            -SnapshotPath 'C:\Talk\.runtime\live-hotkey\text-target\snapshot.txt' `
            -AudioPath 'C:\Talk\.runtime\live-hotkey\audio\session.wav' `
            -ConfigPath 'C:\Talk\.runtime\live-hotkey\config.toml' `
            -ProcessId 12345 `
            -AudioProbe ([pscustomobject]@{
                requestedInputDevice = '麦克风'
                selectedInputDevice = '麦克风'
                peak = 0.3
                rms = 0.08
                silent = $false
            }) `
            -AudioSignal ([pscustomobject]@{
                sampleRate = 16000
                channels = 1
                bitsPerSample = 16
                durationSeconds = 1.25
                peak = 0.5
                rms = 0.1
            })

        $summary.status | Should Be 'completed'
        $summary.transcript | Should Be '请回复测试成功。'
        $summary.outputText | Should Be '测试成功'
        $summary.capturedText | Should Be '测试成功'
        $summary.expectedText | Should Be '测试成功'
        $summary.promptText | Should Be '请回复测试成功。请回复测试成功。'
        $summary.matchedExpected | Should Be $true
        $summary.hotkey | Should Be 'Ctrl+Alt+F20'
        $summary.inputDevice | Should Be '麦克风'
        $summary.audioPath | Should Be 'C:\Talk\.runtime\live-hotkey\audio\session.wav'
        $summary.audioProbe.selectedInputDevice | Should Be '麦克风'
        $summary.audioProbe.peak | Should Be 0.3
        $summary.audioSignal.durationSeconds | Should Be 1.25
        $summary.audioSignal.peak | Should Be 0.5
        $summary.processId | Should Be 12345
    }

    It 'treats silence as unusable but lets weak nonzero live-hotkey preflight continue' {
        $silent = Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0
            rms = 0
            silent = $true
        })
        $weak = Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0.02
            rms = 0.001
            silent = $false
        })
        $audible = Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            durationSeconds = 3
            peak = 0.2
            rms = 0.1
            silent = $false
        })

        $silent | Should Be $false
        $weak | Should Be $true
        $audible | Should Be $true
    }

    It 'builds the real-mic preflight config with a CLI-compatible recording limit for release readiness' {
        Mock Resolve-TalkDesktopLaunchReleaseDir { 'C:\Release' }
        Mock Resolve-TalkDesktopLaunchTalkBinaryPath { 'C:\Release\.internal\talk.exe' }
        Mock Resolve-TalkDesktopLaunchConfigPath { 'C:\Release\talk-desktop.toml' }
        Mock New-TalkDesktopLaunchEffectiveConfig -ParameterFilter {
            $BaseConfigPath -eq 'C:\Release\talk-desktop.toml' -and
            $Hotkey -eq 'Ctrl+Alt+F20' -and
            $InputDevice -eq '麦克风' -and
            $ForceRuntimeLaunchConfig -and
            $CliCompatibleMaxRecordingSeconds -eq 5
        } { 'C:\Release\talk-desktop.runtime-launch.toml' }
        Mock Invoke-TalkDesktopLaunchReadiness {
            [pscustomobject]@{
                audio = [pscustomobject]@{ nativeWindows = [pscustomobject]@{} }
                clipboard = [pscustomobject]@{ nativeWindows = [pscustomobject]@{} }
            }
        }
        Mock New-TalkDesktopLaunchInputDeviceInventory {
            [pscustomobject]@{
                audioStatus = 'ready'
                audioReason = ''
                requestedInputDevice = '麦克风'
                selectedInputDevice = '麦克风'
                availableInputDevices = @('麦克风', 'Virtual Mic')
            }
        }
        Mock Invoke-TalkDesktopLaunchAudioProbe {
            [pscustomobject]@{
                requestedDurationSeconds = 3
                audio = [pscustomobject]@{
                    configuredBackend = 'native_windows'
                    nativeWindows = [pscustomobject]@{}
                    signal = [pscustomobject]@{
                        artifactPath = 'C:\Talk\.runtime\audio.wav'
                        mimeType = 'audio/wav'
                        sampleRateHz = 48000
                        channels = 2
                        durationSeconds = 3
                        peak = 0.2
                        rms = 0.05
                        silent = $false
                    }
                }
            }
        }

        $summary = Invoke-TalkDesktopLiveHotkeyAudioProbe `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -ReleaseDir 'C:\Release' `
            -Hotkey 'Ctrl+Alt+F20' `
            -InputDevice '麦克风' `
            -AudioProbeSeconds 3

        $summary.selectedInputDevice | Should Be '麦克风'
        Assert-MockCalled New-TalkDesktopLaunchEffectiveConfig -Times 1 -Exactly
    }

    It 'fails before launching the desktop shell when preflight audio is silent' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-preflight-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-Sleep {}
            Mock Invoke-TalkDesktopLiveHotkeyAudioProbe {
                [pscustomobject]@{
                    requestedInputDevice = '麦克风'
                    selectedInputDevice = '麦克风'
                    durationSeconds = 3
                    peak = 0
                    rms = 0
                    silent = $true
                }
            }
            Mock Start-TalkTextCaptureTarget { throw 'foreground target must not start on silent preflight' }
            Mock Start-TalkDesktop { throw 'desktop shell must not launch on silent preflight' }

            {
                Invoke-TalkDesktopLiveHotkeyProbe `
                    -BinaryPath 'C:\Release\talk-desktop.exe' `
                    -ReleaseDir 'C:\Release' `
                    -SmokeRoot $tempRoot `
                    -InputDevice '麦克风' `
                    -AudioProbeSeconds 3
            } | Should Throw 'Live hotkey audio probe captured only silence; speak louder or fix the selected input device'

            $summaryPath = Join-Path $tempRoot 'live-hotkey-probe-summary.json'
            Test-Path -LiteralPath $summaryPath | Should Be $true

            $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $summary.status | Should Be 'failed'
            $summary.failureReason | Should Match 'captured only silence'
            $summary.audioProbe.selectedInputDevice | Should Be '麦克风'
            $summary.audioProbe.silent | Should Be $true
            $summary.processId | Should Be 0

            Assert-MockCalled Start-TalkTextCaptureTarget -Times 0 -Exactly -Scope It
            Assert-MockCalled Start-TalkDesktop -Times 0 -Exactly -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'continues launching when preflight audio is weak and records a preflight warning in the final summary' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-weak-preflight-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot 'logs'
            New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            '{"status":"completed","transcript":"请回复测试成功。","output_text":"测试成功。","error":null}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            Mock Ensure-TalkDesktopSmokeWin32Type {} -Scope It
            Mock Start-Sleep {} -Scope It
            Mock Write-Warning {} -Scope It
            Mock Invoke-TalkDesktopLiveHotkeyAudioProbe {
                [pscustomobject]@{
                    requestedInputDevice = '麦克风'
                    selectedInputDevice = '麦克风'
                    durationSeconds = 3
                    peak = 0.02
                    rms = 0.001
                    silent = $false
                }
            } -Scope It
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            } -Scope It
            Mock Set-TalkDesktopForegroundWindow {} -Scope It
            Mock Start-TalkDesktop { [pscustomobject]@{ processId = 12345 } } -Scope It
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } } -Scope It
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) } -Scope It
            Mock Assert-TalkTextCaptureTargetForeground {} -Scope It
            Mock Invoke-TalkDesktopPinnedWindowOperation {
                & $ScriptBlock
            } -Scope It
            Mock Send-TalkDesktopGlobalHotkeyChord {} -Scope It
            Mock Wait-TalkDesktopLiveHotkeySessionLogWithForegroundRefresh { Get-Item -LiteralPath $logPath } -Scope It
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { '测试成功。' } -Scope It
            Mock Stop-TalkDesktopSmokeInstance {} -Scope It
            Mock Stop-TalkTextCaptureTarget {} -Scope It

            $summary = Invoke-TalkDesktopLiveHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -InitialDelaySeconds 0 `
                -RecordingSeconds 1 `
                -InputDevice '麦克风' `
                -AudioProbeSeconds 3 `
                -TimeoutSeconds 5 `
                -ExpectedText '测试成功'

            $summary.status | Should Be 'completed'
            $summary.preflightWarning | Should Match 'weak for provider transcription'
            $summary.audioProbe.selectedInputDevice | Should Be '麦克风'
            $summary.audioProbe.silent | Should Be $false
            $summary.processId | Should Be 12345

            $summaryPath = Join-Path $tempRoot 'live-hotkey-probe-summary.json'
            Test-Path -LiteralPath $summaryPath | Should Be $true
            $written = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $written.preflightWarning | Should Match 'weak for provider transcription'

            Assert-MockCalled Start-TalkTextCaptureTarget -Times 1 -Exactly -Scope It
            Assert-MockCalled Start-TalkDesktop -Times 1 -Exactly -Scope It
            Assert-MockCalled Send-TalkDesktopGlobalHotkeyChord -Times 2 -Exactly -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reads wav signal metadata for a recorded artifact' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-wav-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $wavPath = Join-Path $tempRoot 'sample.wav'
            $sampleRate = 16000
            $channels = 1
            $samples = for ($index = 0; $index -lt 320; $index++) {
                switch ($index % 4) {
                    0 { [int16]0 }
                    1 { [int16]16383 }
                    2 { [int16]-16383 }
                    default { [int16]0 }
                }
            }
            New-TalkDesktopLiveHotkeyTestWavFile `
                -Path $wavPath `
                -SampleRate $sampleRate `
                -Channels $channels `
                -Samples ([int16[]]$samples)

            $signal = Get-TalkDesktopLiveHotkeyProbeWavSignalSummary -AudioPath $wavPath

            $signal.sampleRate | Should Be 16000
            $signal.channels | Should Be 1
            $signal.bitsPerSample | Should Be 16
            $signal.durationSeconds | Should BeGreaterThan 0
            $signal.peak | Should BeGreaterThan 0
            $signal.rms | Should BeGreaterThan 0
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reports active audio coverage for a recorded artifact' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-wav-active-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $wavPath = Join-Path $tempRoot 'active.wav'
            $sampleRate = 16000
            $channels = 1
            $quiet = [int16[]](1..1600 | ForEach-Object { 0 })
            $active = [int16[]](1..1600 | ForEach-Object { if ($_ % 2 -eq 0) { 6000 } else { -6000 } })
            $samples = [int16[]](@($quiet) + @($active) + @($quiet))
            New-TalkDesktopLiveHotkeyTestWavFile `
                -Path $wavPath `
                -SampleRate $sampleRate `
                -Channels $channels `
                -Samples $samples

            $signal = Get-TalkDesktopLiveHotkeyProbeWavSignalSummary -AudioPath $wavPath

            $signal.durationSeconds | Should Be 0.3
            $signal.activeDurationSeconds | Should BeGreaterThan 0.08
            $signal.activeDurationSeconds | Should BeLessThan 0.12
            $signal.activeCoverageRatio | Should BeGreaterThan 0.3
            $signal.activeCoverageRatio | Should BeLessThan 0.35
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'waits for the completed session output text to land in the foreground target' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-insert-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot 'logs'
            New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            '{"status":"completed","transcript":"请回复测试成功。","output_text":"测试成功。","error":null}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            $waitedExpectedText = $null

            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Write-TalkDesktopLiveHotkeyProbeConfig {}
            Mock Start-Sleep {}
            Mock Invoke-TalkDesktopLiveHotkeyAudioProbe {
                [pscustomobject]@{
                    requestedInputDevice = '麦克风'
                    selectedInputDevice = '麦克风'
                    durationSeconds = 3
                    peak = 0.25
                    rms = 0.1
                    silent = $false
                }
            }
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            } -Scope It
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Start-TalkDesktop {
                [pscustomobject]@{
                    processId = 12345
                }
            }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Assert-TalkTextCaptureTargetForeground {} -Scope It
            Mock Invoke-TalkDesktopPinnedWindowOperation {
                & $ScriptBlock
            } -Scope It
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Wait-TalkDesktopLiveHotkeySessionLogWithForegroundRefresh { Get-Item -LiteralPath $logPath }
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
                $script:waitedExpectedText = $ExpectedText
                '测试成功。'
            }
            Mock Get-TalkDesktopLiveHotkeyProbeWavSignalSummary { $null }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopLiveHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -InitialDelaySeconds 0 `
                -RecordingSeconds 1 `
                -AudioProbeSeconds 3 `
                -TimeoutSeconds 5 `
                -ExpectedText '测试成功'

            $summary.status | Should Be 'completed'
            $summary.capturedText | Should Be '测试成功。'
            $script:waitedExpectedText | Should Be '测试成功。'
            Assert-MockCalled Invoke-TalkDesktopPinnedWindowOperation -Times 1 -Exactly -Scope It
            Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly -Scope It
            Assert-MockCalled Send-TalkDesktopGlobalHotkeyChord -Times 2 -Exactly -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'skips native preflight and launches the desktop with an audio override environment value' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-audio-override-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot 'logs'
            New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            '{"status":"completed","transcript":"请回复测试成功。","output_text":"测试成功。","error":null}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            $overridePath = Join-Path $tempRoot 'override.wav'
            $samples = [int16[]](1..1600 | ForEach-Object { if ($_ % 2 -eq 0) { 6000 } else { -6000 } })
            New-TalkDesktopLiveHotkeyTestWavFile `
                -Path $overridePath `
                -SampleRate 16000 `
                -Channels 1 `
                -Samples $samples
            $resolvedOverridePath = [System.IO.Path]::GetFullPath($overridePath)
            $script:startEnvironmentOverrides = $null
            $script:signalAudioPath = $null

            Mock Ensure-TalkDesktopSmokeWin32Type {} -Scope It
            Mock Write-TalkDesktopLiveHotkeyProbeConfig {} -Scope It
            Mock Start-Sleep {} -Scope It
            Mock Invoke-TalkDesktopLiveHotkeyAudioProbe { throw 'native preflight must be skipped when audio override is provided' } -Scope It
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path (Join-Path $tempRoot 'text-target') 'snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            } -Scope It
            Mock Set-TalkDesktopForegroundWindow {} -Scope It
            Mock Start-TalkDesktop {
                $script:startEnvironmentOverrides = $EnvironmentOverrides
                [pscustomobject]@{ processId = 12345 }
            } -Scope It
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } } -Scope It
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) } -Scope It
            Mock Assert-TalkTextCaptureTargetForeground {} -Scope It
            Mock Invoke-TalkDesktopPinnedWindowOperation {
                & $ScriptBlock
            } -Scope It
            Mock Send-TalkDesktopGlobalHotkeyChord {} -Scope It
            Mock Wait-TalkDesktopLiveHotkeySessionLogWithForegroundRefresh { Get-Item -LiteralPath $logPath } -Scope It
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { '测试成功。' } -Scope It
            Mock Get-TalkDesktopLiveHotkeyProbeWavSignalSummary {
                $script:signalAudioPath = $AudioPath
                [pscustomobject]@{
                    sampleRate = 16000
                    channels = 1
                    bitsPerSample = 16
                    durationSeconds = 0.1
                    peak = 0.183111
                    rms = 0.183111
                }
            } -Scope It
            Mock Stop-TalkDesktopSmokeInstance {} -Scope It
            Mock Stop-TalkTextCaptureTarget {} -Scope It

            $summary = Invoke-TalkDesktopLiveHotkeyProbe `
                -BinaryPath 'C:/release/talk-desktop.exe' `
                -ReleaseDir 'C:/release' `
                -SmokeRoot $tempRoot `
                -AudioOverridePath $overridePath `
                -InitialDelaySeconds 0 `
                -RecordingSeconds 1 `
                -AudioProbeSeconds 3 `
                -TimeoutSeconds 5 `
                -ExpectedText '测试成功'

            $summary.status | Should Be 'completed'
            $summary.audioOverridePath | Should Be $resolvedOverridePath
            $summary.audioPath | Should Be $resolvedOverridePath
            $summary.audioProbe.configuredBackend | Should Be 'audio_override'
            $summary.audioProbe.artifactPath | Should Be $resolvedOverridePath
            $script:signalAudioPath | Should Be $resolvedOverridePath
            $script:startEnvironmentOverrides['TALK_DESKTOP_AUDIO_FILE_OVERRIDE'] | Should Be $resolvedOverridePath

            Assert-MockCalled Invoke-TalkDesktopLiveHotkeyAudioProbe -Times 0 -Exactly -Scope It
            Assert-MockCalled Start-TalkDesktop -Times 1 -Exactly -Scope It
            Assert-MockCalled Send-TalkDesktopGlobalHotkeyChord -Times 2 -Exactly -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    It 'attributes provider blank text to weak audio in the actual hotkey recording window' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-hotkey-recording-weak-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot 'logs'
            $audioDir = Join-Path $tempRoot 'audio'
            New-Item -ItemType Directory -Path $logsDir, $audioDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            $wavPath = Join-Path $audioDir 'session.wav'
            '{"status":"failed","transcript":null,"output_text":null,"error":"provider error: openai-compatible transcriber returned blank text"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8
            'not-a-real-wav' | Set-Content -LiteralPath $wavPath -Encoding ASCII

            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Write-TalkDesktopLiveHotkeyProbeConfig {}
            Mock Start-Sleep {}
            Mock Invoke-TalkDesktopLiveHotkeyAudioProbe {
                [pscustomobject]@{
                    requestedInputDevice = '麦克风'
                    selectedInputDevice = '麦克风'
                    durationSeconds = 3
                    peak = 0.2
                    rms = 0.02
                    silent = $false
                }
            }
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            } -Scope It
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Start-TalkDesktop { [pscustomobject]@{ processId = 12345 } }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Assert-TalkTextCaptureTargetForeground {}
            Mock Invoke-TalkDesktopPinnedWindowOperation {
                & $ScriptBlock
            }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Wait-TalkDesktopLiveHotkeySessionLogWithForegroundRefresh { Get-Item -LiteralPath $logPath }
            Mock Get-TalkDesktopLiveHotkeyProbeWavSignalSummary {
                [pscustomobject]@{
                    sampleRate = 16000
                    channels = 1
                    bitsPerSample = 16
                    durationSeconds = 4.33
                    peak = 0.016083
                    rms = 0.000494
                }
            }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopLiveHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -InitialDelaySeconds 0 `
                -RecordingSeconds 1 `
                -AudioProbeSeconds 3 `
                -TimeoutSeconds 5 `
                -ExpectedText ''

            $summary.status | Should Be 'failed'
            $summary.error | Should Match 'blank text'
            $summary.failureReason | Should Match 'actual hotkey recording audio was too weak for provider transcription'
            $summary.failureReason | Should Match 'recording_peak=0.016083'
            $summary.audioSignal.rms | Should Be 0.000494

            $summaryPath = Join-Path $tempRoot 'live-hotkey-probe-summary.json'
            $written = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $written.failureReason | Should Match 'actual hotkey recording audio was too weak'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'attributes provider blank text to unrecognized speech when the actual recording signal is strong' {
        $session = [pscustomobject]@{
            status = 'failed'
            transcript = $null
            output_text = $null
            error = 'provider error: openai-compatible transcriber returned blank text'
        }
        $reason = Get-TalkDesktopLiveHotkeyActualRecordingFailureReason `
            -Session $session `
            -AudioSignal ([pscustomobject]@{
                durationSeconds = 4.37
                peak = 0.185675
                rms = 0.020111
            })

        $reason | Should Match 'actual hotkey recording had non-weak audio but provider returned blank transcription'
        $reason | Should Match 'recording_peak=0.185675'
        $reason | Should Match 'recording_rms=0.020111'
    }

    It 'attributes provider blank text to a short active burst when activity covers too little of the recording window' {
        $session = [pscustomobject]@{
            status = 'failed'
            transcript = $null
            output_text = $null
            error = 'provider error: openai-compatible transcriber returned blank text'
        }
        $reason = Get-TalkDesktopLiveHotkeyActualRecordingFailureReason `
            -Session $session `
            -AudioSignal ([pscustomobject]@{
                durationSeconds = 4.37
                peak = 0.185675
                rms = 0.020111
                activeDurationSeconds = 0.4
                activeCoverageRatio = 0.092
                firstActiveSecond = 2.24
                lastActiveSecond = 2.64
                longestActiveRunSeconds = 0.4
            })

        $reason | Should Match 'actual hotkey recording had only a short active burst'
        $reason | Should Match 'active_duration_seconds=0.4'
        $reason | Should Match 'active_window_seconds=2.24-2.64'
        $reason | Should Match 'active_coverage_ratio=0.092'
    }

    It 'toggles the desktop hotkey on and off around the requested recording duration' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Send-TalkDesktopGlobalHotkeyChord -Shortcut \$Hotkey[\s\S]*Start-Sleep -Seconds \$RecordingSeconds[\s\S]*Send-TalkDesktopGlobalHotkeyChord -Shortcut \$Hotkey'
    }
}
