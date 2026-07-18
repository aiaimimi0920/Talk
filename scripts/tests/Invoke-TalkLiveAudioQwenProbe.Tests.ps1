$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkLiveAudioQwenProbe.ps1'

. $scriptPath

Describe 'Invoke-TalkLiveAudioQwenProbe helpers' {
    It 'auto-discovers a local qwen dashscope key when no explicit key is provided' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-audio-qwen-autokey-' + [guid]::NewGuid().ToString())
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

            $resolved = Resolve-TalkLiveAudioQwenProbeApiKey

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

    It 'builds a native capture config with the requested input device' {
        $configText = New-TalkLiveAudioQwenProbeCaptureConfigContent `
            -AudioDir 'C:\Talk\.runtime\live-audio-qwen\audio' `
            -LogsDir 'C:\Talk\.runtime\live-audio-qwen\capture-logs' `
            -InputDevice 'Virtual Mic'

        $configText | Should Match 'backend = "native_windows"'
        $configText | Should Match 'input_device = "Virtual Mic"'
        $configText | Should Match 'temp_dir = "C:\\\\Talk\\\\.runtime\\\\live-audio-qwen\\\\audio"'
    }

    It 'builds a provider once config using Qwen audio-input transport and dry_run output' {
        $configText = New-TalkLiveAudioQwenProbeProviderConfigContent `
            -LogsDir 'C:\Talk\.runtime\live-audio-qwen\provider-logs'

        $configText | Should Match 'kind = "openai_compatible"'
        $configText | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $configText | Should Match 'transcription_model = "qwen3-asr-flash"'
        $configText | Should Match 'chat_model = "qwen3\.7-plus"'
        $configText | Should Match 'mode = "dry_run"'
        $configText | Should Match 'clipboard_backend = "fallback"'
    }

    It 'builds a provider once config with explicit endpoint and model overrides' {
        $configText = New-TalkLiveAudioQwenProbeProviderConfigContent `
            -LogsDir 'C:\Talk\.runtime\live-audio-qwen\provider-logs' `
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

    It 'builds a concise live audio qwen probe summary' {
        $summary = New-TalkLiveAudioQwenProbeSummary `
            -SmokeRoot 'C:\Talk\.runtime\live-audio-qwen' `
            -CaptureProbe ([pscustomobject]@{
                requestedInputDevice = 'Virtual Mic'
                selectedInputDevice = 'Virtual Mic'
                artifactPath = 'C:\Talk\.runtime\live-audio-qwen\audio\capture.wav'
                peak = 0.25
                rms = 0.125
                silent = $false
            }) `
            -Session ([pscustomobject]@{
                status = 'completed'
                transcript = 'What is the capital of France?'
                output_text = 'Paris'
            }) `
            -ProviderConfigPath 'C:\Talk\.runtime\live-audio-qwen\provider-config.toml' `
            -LogPath 'C:\Talk\.runtime\live-audio-qwen\provider-logs\session.json' `
            -TalkBinaryPath 'C:\Release\talk.exe'

        $summary.status | Should Be 'completed'
        $summary.transcript | Should Be 'What is the capital of France?'
        $summary.outputText | Should Be 'Paris'
        $summary.inputDevice | Should Be 'Virtual Mic'
        $summary.captureAudioPath | Should Be 'C:\Talk\.runtime\live-audio-qwen\audio\capture.wav'
        $summary.capturePeak | Should Be 0.25
        $summary.binaryPath | Should Be 'C:\Release\talk.exe'
        $summary.summaryPath | Should Be 'C:\Talk\.runtime\live-audio-qwen\live-audio-qwen-probe-summary.json'
    }

    It 'treats silent captured live audio as unusable input' {
        $silent = Test-TalkLiveAudioQwenProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0
            silent = $true
        })
        $audible = Test-TalkLiveAudioQwenProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0.15
            silent = $false
        })

        $silent | Should Be $false
        $audible | Should Be $true
    }
}
