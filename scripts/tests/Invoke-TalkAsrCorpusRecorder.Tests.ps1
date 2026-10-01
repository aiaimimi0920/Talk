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

    It 'can resume an existing corpus in plan-only mode and mark only missing samples for recording' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-resume-plan-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            $shortHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "audioSha256": "$shortHash", "referenceText": "你好呀" }
  ]
}
"@ | Set-Content -LiteralPath (Join-Path $outputRoot 'corpus.json') -Encoding UTF8

            $plan = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ResumeExisting `
                -PlanOnly

            $plan.ReusedSampleCount | Should Be 1
            $plan.PlannedRecordingCount | Should Be 1
            $plan.Samples.Count | Should Be 2
            $plan.Samples[0].WillRecord | Should Be $false
            $plan.Samples[0].ExistingAudioWav | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')))
            $plan.Samples[1].WillRecord | Should Be $true
            $plan.Samples[1].ExistingAudioWav | Should Be $null
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rerecords a sample when the existing WAV no longer matches its manifest hash' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-resume-hash-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            $wavPath = Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav'
            Write-TestTalkPcmWav -Path $wavPath
            $originalHash = Get-TalkAsrRecorderFileSha256 -Path $wavPath
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "audioSha256": "$originalHash", "referenceText": "你好呀" }
  ]
}
"@ | Set-Content -LiteralPath (Join-Path $outputRoot 'corpus.json') -Encoding UTF8
            Add-Content -LiteralPath $wavPath -Value 'changed' -Encoding ASCII

            $plan = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ResumeExisting `
                -PlanOnly

            $plan.ReusedSampleCount | Should Be 0
            $plan.PlannedRecordingCount | Should Be 1
            $plan.Samples[0].WillRecord | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can force rerecord specific sample ids while still reusing other aligned corpus audio in plan-only mode' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-force-plan-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'mixed-english-japanese-001-16k-mono-s16.wav')
            $shortHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            $mixedHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'mixed-english-japanese-001-16k-mono-s16.wav')
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "audioSha256": "$shortHash", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "audioWav": "mixed-english-japanese-001-16k-mono-s16.wav", "audioSha256": "$mixedHash", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
"@ | Set-Content -LiteralPath (Join-Path $outputRoot 'corpus.json') -Encoding UTF8

            $plan = Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ResumeExisting `
                -ForceRecordSampleId 'mixed-english-japanese-001' `
                -PlanOnly

            $plan.ReusedSampleCount | Should Be 1
            $plan.PlannedRecordingCount | Should Be 1
            $plan.Samples[0].SampleId | Should Be 'short-search-001'
            $plan.Samples[0].WillRecord | Should Be $false
            $plan.Samples[1].SampleId | Should Be 'mixed-english-japanese-001'
            $plan.Samples[1].WillRecord | Should Be $true
            $plan.Samples[1].ExistingAudioWav | Should Be $null
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
            [string]::IsNullOrWhiteSpace([string]$manifest.createdAtUtc) | Should Be $false
            $manifest.promptManifestSha256 | Should Be (Get-TalkAsrRecorderFileSha256 -Path $promptPath)
            $manifest.samples.Count | Should Be 1
            $manifest.samples[0].sampleId | Should Be 'short-search-001'
            $manifest.samples[0].audioWav | Should Be 'short-search-001-16k-mono-s16.wav'
            $manifest.samples[0].audioSha256 | Should Be (Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav'))
            $manifest.samples[0].referenceText | Should Be '你好呀'
            $result.Recordings[0].Peak | Should Be 0.25
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'records the captured native_windows input device and warns when multiple devices are available without an explicit InputDevice' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-device-warning-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀","captureSeconds":1}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav
            $probeInvoker = {
                param($Plan, $Sample)
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
                        nativeWindows = [pscustomobject]@{
                            deviceName = '麦克风'
                            availableDeviceNames = @('麦克风', 'Virtual Mic')
                        }
                    }
                }
            }

            $output = @(Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -CountdownSeconds 0 `
                -ProbeInvoker $probeInvoker `
                -PassThru 3>&1)
            $warnings = @(
                $output |
                    Where-Object { $_ -is [System.Management.Automation.WarningRecord] } |
                    ForEach-Object { $_.Message }
            )
            $result = @(
                $output |
                    Where-Object { $_ -isnot [System.Management.Automation.WarningRecord] }
            )[0]

            $warnings.Count | Should Be 1
            $warnings[0] | Should Match 'multiple input devices'
            $warnings[0] | Should Match 'Virtual Mic'
            $warnings[0] | Should Match '-InputDevice'
            $result.Recordings[0].CapturedInputDevice | Should Be '麦克风'
            (@($result.Recordings[0].AvailableInputDevices) -join '|') | Should Be '麦克风|Virtual Mic'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'does not warn about multiple devices when InputDevice is explicit' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-device-explicit-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀","captureSeconds":1}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav
            $probeInvoker = {
                param($Plan, $Sample)
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
                        nativeWindows = [pscustomobject]@{
                            deviceName = 'Virtual Mic'
                            availableDeviceNames = @('麦克风', 'Virtual Mic')
                        }
                    }
                }
            }

            $output = @(Invoke-TalkAsrCorpusRecorder `
                -PromptManifest $promptPath `
                -OutputRoot $outputRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -InputDevice 'Virtual Mic' `
                -CountdownSeconds 0 `
                -ProbeInvoker $probeInvoker `
                -PassThru 3>&1)
            $warnings = @(
                $output |
                    Where-Object { $_ -is [System.Management.Automation.WarningRecord] } |
                    ForEach-Object { $_.Message }
            )
            $result = @(
                $output |
                    Where-Object { $_ -isnot [System.Management.Automation.WarningRecord] }
            )[0]

            $warnings.Count | Should Be 0
            $result.Recordings[0].CapturedInputDevice | Should Be 'Virtual Mic'
            (@($result.Recordings[0].AvailableInputDevices) -join '|') | Should Be '麦克风|Virtual Mic'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can resume an existing corpus and only probe missing samples while rewriting a full manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-resume-run-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test", "captureSeconds": 1 }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            $shortHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "audioSha256": "$shortHash", "referenceText": "你好呀" }
  ]
}
"@ | Set-Content -LiteralPath (Join-Path $outputRoot 'corpus.json') -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav
            $probeCalls = New-Object System.Collections.Generic.List[string]
            $probeInvoker = {
                param($Plan, $Sample)
                $probeCalls.Add([string]$Sample.SampleId) | Out-Null
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
                -ResumeExisting `
                -ProbeInvoker $probeInvoker `
                -PassThru

            $probeCalls.Count | Should Be 1
            $probeCalls[0] | Should Be 'mixed-english-japanese-001'
            $result.Recordings.Count | Should Be 2
            $result.Recordings[0].SampleId | Should Be 'short-search-001'
            $result.Recordings[0].Reused | Should Be $true
            $result.Recordings[1].SampleId | Should Be 'mixed-english-japanese-001'
            $result.Recordings[1].Reused | Should Be $false
            $manifest = Get-Content -LiteralPath $result.CorpusManifestPath -Raw | ConvertFrom-Json
            $manifest.samples.Count | Should Be 2
            $manifest.samples[0].sampleId | Should Be 'short-search-001'
            $manifest.samples[1].sampleId | Should Be 'mixed-english-japanese-001'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can force rerecord drifted sample ids while reusing the rest of an aligned corpus' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-recorder-force-run-' + [guid]::NewGuid().ToString())
        $outputRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $outputRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test", "captureSeconds": 1 }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            Write-TestTalkPcmWav -Path (Join-Path $outputRoot 'mixed-english-japanese-001-16k-mono-s16.wav')
            $shortHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'short-search-001-16k-mono-s16.wav')
            $mixedHash = Get-TalkAsrRecorderFileSha256 -Path (Join-Path $outputRoot 'mixed-english-japanese-001-16k-mono-s16.wav')
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "audioSha256": "$shortHash", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "audioWav": "mixed-english-japanese-001-16k-mono-s16.wav", "audioSha256": "$mixedHash", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
"@ | Set-Content -LiteralPath (Join-Path $outputRoot 'corpus.json') -Encoding UTF8

            $sourceWav = Join-Path $tempRoot 'captured.wav'
            Write-TestTalkPcmWav -Path $sourceWav
            $probeCalls = New-Object System.Collections.Generic.List[string]
            $probeInvoker = {
                param($Plan, $Sample)
                $probeCalls.Add([string]$Sample.SampleId) | Out-Null
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
                -ResumeExisting `
                -ForceRecordSampleId 'mixed-english-japanese-001' `
                -ProbeInvoker $probeInvoker `
                -PassThru

            $probeCalls.Count | Should Be 1
            $probeCalls[0] | Should Be 'mixed-english-japanese-001'
            $result.Recordings.Count | Should Be 2
            $result.Recordings[0].SampleId | Should Be 'short-search-001'
            $result.Recordings[0].Reused | Should Be $true
            $result.Recordings[1].SampleId | Should Be 'mixed-english-japanese-001'
            $result.Recordings[1].Reused | Should Be $false
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
