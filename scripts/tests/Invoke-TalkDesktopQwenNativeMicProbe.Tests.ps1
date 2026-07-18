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

    It 'treats silent native audio probe summaries as unusable input' {
        $silent = Test-TalkDesktopAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0
            silent = $true
        })
        $audible = Test-TalkDesktopAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0.2
            silent = $false
        })

        $silent | Should Be $false
        $audible | Should Be $true
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
