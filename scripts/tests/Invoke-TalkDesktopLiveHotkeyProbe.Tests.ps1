$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopLiveHotkeyProbe.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopLiveHotkeyProbe helpers' {
    It 'writes a push-to-talk native mic config with the requested hotkey and input device' {
        $content = New-TalkDesktopLiveHotkeyProbeConfigContent `
            -Hotkey 'Ctrl+Alt+F20' `
            -AudioDir 'C:\Talk\.runtime\live-hotkey\audio' `
            -LogsDir 'C:\Talk\.runtime\live-hotkey\logs' `
            -InputDevice '麦克风'

        $content | Should Match 'mode = "push_to_talk"'
        $content | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F20"'
        $content | Should Match 'backend = "native_windows"'
        $content | Should Match 'input_device = "麦克风"'
        $content | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $content | Should Match 'mode = "clipboard_paste"'
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

    It 'treats silent live-hotkey audio probe summaries as unusable input' {
        $silent = Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0
            silent = $true
        })
        $audible = Test-TalkDesktopLiveHotkeyAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0.2
            silent = $false
        })

        $silent | Should Be $false
        $audible | Should Be $true
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

            Assert-MockCalled Start-TalkTextCaptureTarget -Times 0 -Exactly
            Assert-MockCalled Start-TalkDesktop -Times 0 -Exactly
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
            $bitsPerSample = 16
            $samples = for ($index = 0; $index -lt 320; $index++) {
                switch ($index % 4) {
                    0 { [int16]0 }
                    1 { [int16]16383 }
                    2 { [int16]-16383 }
                    default { [int16]0 }
                }
            }
            $dataSize = $samples.Count * 2
            $stream = [System.IO.File]::Open($wavPath, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
            $writer = New-Object System.IO.BinaryWriter($stream)
            try {
                $writer.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF'))
                $writer.Write([int] (36 + $dataSize))
                $writer.Write([System.Text.Encoding]::ASCII.GetBytes('WAVE'))
                $writer.Write([System.Text.Encoding]::ASCII.GetBytes('fmt '))
                $writer.Write([int]16)
                $writer.Write([int16]1)
                $writer.Write([int16]$channels)
                $writer.Write([int]$sampleRate)
                $writer.Write([int]($sampleRate * $channels * ($bitsPerSample / 8)))
                $writer.Write([int16]($channels * ($bitsPerSample / 8)))
                $writer.Write([int16]$bitsPerSample)
                $writer.Write([System.Text.Encoding]::ASCII.GetBytes('data'))
                $writer.Write([int]$dataSize)
                foreach ($sample in $samples) {
                    $writer.Write([int16]$sample)
                }
            }
            finally {
                $writer.Dispose()
                $stream.Dispose()
            }

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
            }
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Start-TalkDesktop {
                [pscustomobject]@{
                    processId = 12345
                }
            }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Invoke-TalkDesktopGlobalHotkeyOperation { & $ScriptBlock }
            Mock Wait-LatestSessionLog { Get-Item -LiteralPath $logPath }
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
            Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'holds the push-to-talk hotkey for the requested recording duration' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Invoke-TalkDesktopGlobalHotkeyOperation[\s\S]*-Shortcut \$Hotkey[\s\S]*-ScriptBlock \{[\s\S]*Start-Sleep -Seconds \$RecordingSeconds[\s\S]*\}'
    }
}
