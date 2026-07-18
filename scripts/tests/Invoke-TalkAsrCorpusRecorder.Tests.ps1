$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkAsrCorpusRecorder.ps1'

. $scriptPath

function Write-TestTalkPcmWav {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [int]$SampleRateHz = 16000,
        [int]$Channels = 1,
        [int]$DurationSeconds = 1
    )

    $sampleCount = $SampleRateHz * $DurationSeconds
    $bitsPerSample = 16
    $blockAlign = $Channels * ($bitsPerSample / 8)
    $dataSize = $sampleCount * $blockAlign
    $riffSize = 36 + $dataSize
    $directory = Split-Path -Parent $Path
    New-Item -ItemType Directory -Path $directory -Force | Out-Null

    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    try {
        $writer = New-Object System.IO.BinaryWriter($stream, [System.Text.Encoding]::ASCII)
        try {
            $writer.Write([System.Text.Encoding]::ASCII.GetBytes('RIFF'))
            $writer.Write([int]$riffSize)
            $writer.Write([System.Text.Encoding]::ASCII.GetBytes('WAVE'))
            $writer.Write([System.Text.Encoding]::ASCII.GetBytes('fmt '))
            $writer.Write([int]16)
            $writer.Write([int16]1)
            $writer.Write([int16]$Channels)
            $writer.Write([int]$SampleRateHz)
            $writer.Write([int]($SampleRateHz * $blockAlign))
            $writer.Write([int16]$blockAlign)
            $writer.Write([int16]$bitsPerSample)
            $writer.Write([System.Text.Encoding]::ASCII.GetBytes('data'))
            $writer.Write([int]$dataSize)
            for ($index = 0; $index -lt ($sampleCount * $Channels); $index += 1) {
                $writer.Write([int16]0)
            }
        } finally {
            $writer.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

Describe 'Invoke-TalkAsrCorpusRecorder helpers' {
    It 'loads recording prompts with per-sample capture seconds' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-prompts-' + [guid]::NewGuid().ToString())
            New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀","captureSeconds":2}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8

            $samples = @(Read-TalkAsrCorpusRecorderPrompts -PromptManifest $promptPath -DefaultCaptureSeconds 4)

            $samples.Count | Should Be 1
            $samples[0].SampleId | Should Be 'short-search-001'
            $samples[0].ReferenceText | Should Be '你好呀'
            $samples[0].CaptureSeconds | Should Be 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'creates a plan-only recording matrix that writes benchmark-ready wav names' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-plan-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-001", "referenceText": "打开 Talk 的 local first ASR 测试", "captureSeconds": 5 }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8

            $plan = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -InputDevice 'Microphone Array' `
                -DefaultCaptureSeconds 3 `
                -PlanOnly

            $plan.Samples.Count | Should Be 2
            $plan.ConfigPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'recording-config.toml')))
            $plan.CorpusManifestPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'corpus.json')))
            $plan.Samples[0].AudioWav | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')))
            $plan.Samples[0].CaptureSeconds | Should Be 3
            $plan.Samples[1].CaptureSeconds | Should Be 5
            $plan.InputDevice | Should Be 'Microphone Array'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'resolves explicit relative paths against the current PowerShell location instead of the process cwd' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-relative-' + [guid]::NewGuid().ToString())
        $releaseDir = Join-Path $tempRoot 'release'
        $processCwd = Join-Path $tempRoot 'process-cwd'
        New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $releaseDir '.internal') -Force | Out-Null
        New-Item -ItemType Directory -Path $processCwd -Force | Out-Null
        $originalDotNetCurrentDirectory = [Environment]::CurrentDirectory
        try {
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $releaseDir 'asr-real-mic-prompts.json') -Encoding UTF8

            Push-Location $releaseDir
            try {
                [Environment]::CurrentDirectory = $processCwd
                $plan = Invoke-TalkAsrCorpusRecorder `
                    -PromptManifest .\asr-real-mic-prompts.json `
                    -OutputRoot .\.runtime\asr-bench\real-mic-corpus `
                    -TalkExe .\.internal\talk.exe `
                    -PlanOnly
            }
            finally {
                Pop-Location
            }

            $plan.PromptManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir 'asr-real-mic-prompts.json')))
            $plan.OutputRoot | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir '.runtime\asr-bench\real-mic-corpus')))
            $plan.TalkExe | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir '.internal\talk.exe')))
        }
        finally {
            [Environment]::CurrentDirectory = $originalDotNetCurrentDirectory
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'records samples through a supplied probe invoker and writes a benchmark corpus manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-run-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀","captureSeconds":1}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav
            $probeCalls = New-Object System.Collections.Generic.List[object]
            $probeInvoker = {
                param($Plan, $Sample)
                $probeCalls.Add([pscustomobject]@{
                    SampleId = $Sample.SampleId
                    CaptureSeconds = $Sample.CaptureSeconds
                    ConfigPath = $Plan.ConfigPath
                }) | Out-Null
                [pscustomobject]@{
                    audio = [pscustomobject]@{
                        signal = [pscustomobject]@{
                            artifactPath = $sourceWav
                            sampleRateHz = 16000
                            channels = 1
                            durationSeconds = 1.0
                            peak = 0.25
                            rms = 0.10
                            silent = $false
                        }
                    }
                }
            }

            $result = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -CountdownSeconds 0 `
                -ProbeInvoker $probeInvoker `
                -PassThru

            $probeCalls.Count | Should Be 1
            Test-Path -LiteralPath $result.CorpusManifestPath | Should Be $true
            Test-Path -LiteralPath (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav') | Should Be $true
            $manifest = Get-Content -LiteralPath $result.CorpusManifestPath -Raw | ConvertFrom-Json
            $manifest.schemaVersion | Should Be 1
            $manifest.samples.Count | Should Be 1
            $manifest.samples[0].sampleId | Should Be 'short-search-001'
            $manifest.samples[0].audioWav | Should Be 'short-search-001-16k-mono-s16.wav'
            $manifest.samples[0].referenceText | Should Be '你好呀'
            $result.Recordings[0].Peak | Should Be 0.25
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'validates the normalized artifact instead of native source signal metadata' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-normalized-artifact-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"native-rate-001","referenceText":"你好呀","captureSeconds":1}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav -SampleRateHz 16000 -Channels 1
            $probeInvoker = {
                param($Plan, $Sample)
                [pscustomobject]@{
                    audio = [pscustomobject]@{
                        signal = [pscustomobject]@{
                            artifactPath = $sourceWav
                            sampleRateHz = 48000
                            channels = 2
                            durationSeconds = 1.0
                            peak = 0.25
                            rms = 0.10
                            silent = $false
                        }
                    }
                }
            }

            $result = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -CountdownSeconds 0 `
                -ProbeInvoker $probeInvoker `
                -PassThru

            Test-Path -LiteralPath (Join-Path $outputRoot 'native-rate-001-16k-mono-s16.wav') | Should Be $true
            $result.Recordings[0].Peak | Should Be 0.25
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
