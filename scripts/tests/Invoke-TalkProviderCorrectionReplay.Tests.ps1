$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkProviderCorrectionReplay.ps1'

Describe 'Invoke-TalkProviderCorrectionReplay' {
    It 'ships a replay helper for validating provider correction against a recorded corpus' {
        (Test-Path -LiteralPath $scriptPath -PathType Leaf) | Should Be $true
    }

    It 'counts supplementary Unicode characters as one scalar like the Rust patch gate' {
        . $scriptPath

        $ratio = Get-TalkProviderCorrectionReplayCharErrorRate `
            -ReferenceText 'A😀B' `
            -CandidateText 'A😁B'

        ([Math]::Abs($ratio - (1.0 / 3.0)) -lt 0.0000001) | Should Be $true
    }

    It 'rejects duplicate sample ids in deterministic provider fixtures' {
        . $scriptPath

        $fixturePath = Join-Path $env:TEMP ('talk-provider-replay-duplicate-fixture-' + [guid]::NewGuid().ToString() + '.json')
        try {
            @'
{
  "samples": [
    { "sampleId": "duplicate-001", "providerOutputText": "first" },
    { "sampleId": "duplicate-001", "providerOutputText": "second" }
  ]
}
'@ | Set-Content -LiteralPath $fixturePath -Encoding UTF8

            { Read-TalkProviderCorrectionReplayFixtureMap -FixtureJsonPath $fixturePath } |
                Should Throw "Talk provider correction replay fixture json contains duplicate sampleId [duplicate-001]: $fixturePath"
        }
        finally {
            Remove-Item -LiteralPath $fixturePath -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects duplicate sample ids in the replay corpus manifest' {
        . $scriptPath

        $manifestPath = Join-Path $env:TEMP ('talk-provider-replay-duplicate-corpus-' + [guid]::NewGuid().ToString() + '.json')
        try {
            @'
{
  "samples": [
    { "sampleId": "duplicate-001", "referenceText": "first" },
    { "sampleId": "duplicate-001", "referenceText": "second" }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            { Read-TalkProviderCorrectionReplayCorpusManifest -CorpusManifest $manifestPath } |
                Should Throw "Talk provider correction replay corpus manifest contains duplicate sampleId [duplicate-001]: $manifestPath"
        }
        finally {
            Remove-Item -LiteralPath $manifestPath -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a deterministic fixture that is missing a corpus sample' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-missing-fixture-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "samples": [
    { "sampleId": "required-001", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8
            @'
{
  "engine": "streaming_service:test",
  "sample_id": "required-001",
  "text": "你好"
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'test-model-required-001.json') -Encoding UTF8
            $fixturePath = Join-Path $tempRoot 'fixture.json'
            @'
{
  "samples": [
    { "sampleId": "other-001", "providerOutputText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath $fixturePath -Encoding UTF8

            { New-TalkProviderCorrectionReplayPlan `
                    -CorpusManifest $manifestPath `
                    -ReportsRoot $reportsRoot `
                    -ModelId 'test-model' `
                    -TalkExe $manifestPath `
                    -ConfigPath $manifestPath `
                    -OutputJson $fixturePath `
                    -UseExistingReplayAsFixture } |
                Should Throw 'Talk provider correction replay fixture is missing sampleId [required-001]'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'creates a plan-only replay matrix from the corpus manifest and local ASR reports' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-plan-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "mixed-english-001",
      "audioWav": "mixed-english-001-16k-mono-s16.wav",
      "referenceText": "打开 Talk 的 local first ASR 测试"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "mixed-english-001",
  "text": "打开 talk 的 rock foster a s r 测",
  "cer": 0.5
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-mixed-english-001.json') -Encoding UTF8

            $plan = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath $manifestPath `
                -PlanOnly

            $plan.WorkflowKind | Should Be 'talk-provider-correction-replay-plan'
            $plan.Replays.Count | Should Be 1
            $plan.Replays[0].SampleId | Should Be 'mixed-english-001'
            $plan.Replays[0].LocalText | Should Be '打开 talk 的 rock foster a s r 测'
            $plan.Replays[0].ReferenceText | Should Be '打开 Talk 的 local first ASR 测试'
            $plan.Replays[0].LocalCer | Should Be 0.5
            $plan.MaxAutoPatchEditRatio | Should Be 0.35
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'replays provider correction for each sample and writes an aggregate json report' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-run-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "mixed-english-001",
      "audioWav": "mixed-english-001-16k-mono-s16.wav",
      "referenceText": "打开 Talk 的 local first ASR 测试"
    },
    {
      "sampleId": "proper-nouns-001",
      "audioWav": "proper-nouns-001-16k-mono-s16.wav",
      "referenceText": "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "mixed-english-001",
  "text": "打开 talk 的 rock foster a s r 测",
  "cer": 0.5
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-mixed-english-001.json') -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "proper-nouns-001",
  "text": "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us",
  "cer": 0.59375
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-proper-nouns-001.json') -Encoding UTF8

            $outputJson = Join-Path $tempRoot 'provider-replay.json'
            $calls = New-Object System.Collections.Generic.List[string]
            $result = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath $manifestPath `
                -OutputJson $outputJson `
                -PassThru `
                -ProcessorInvoker {
                    param($Transcript, $Mode, $ProviderOutputText)
                    $calls.Add("$Mode => $Transcript") | Out-Null
                    if ($Transcript -like '*rock foster*') {
                        [pscustomobject]@{
                            outputText = '打开 Talk 的 local first ASR 测试'
                            providerOutputText = '打开 Talk 的 local first ASR 测试'
                            faithfulValidation = [pscustomobject]@{
                                accepted = $true
                                fallbackReason = $null
                                inputCharCount = 21
                                outputCharCount = 22
                                retentionRatio = 1.0
                                normalizedChangeRatio = 0.05
                            }
                        }
                    }
                    else {
                        [pscustomobject]@{
                            outputText = '请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\Users\Public\Talk\logs。'
                            providerOutputText = '请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\Users\Public\Talk\logs。'
                            faithfulValidation = [pscustomobject]@{
                                accepted = $true
                                fallbackReason = $null
                                inputCharCount = 35
                                outputCharCount = 51
                                retentionRatio = 1.45
                                normalizedChangeRatio = 0.34
                            }
                        }
                    }
                }

            $result.WorkflowKind | Should Be 'talk-provider-correction-replay-result'
            $result.SampleCount | Should Be 2
            $result.ExactMatchCount | Should Be 2
            $result.ImprovedCount | Should Be 2
            $result.MaxAutoPatchEditRatio | Should Be 0.35
            ($result.MeanProcessedCer -lt $result.MeanLocalCer) | Should Be $true
            (Test-Path -LiteralPath $outputJson -PathType Leaf) | Should Be $true
            $calls.Count | Should Be 2
            $calls[0] | Should Match 'transcribe'

            $json = Get-Content -LiteralPath $outputJson -Raw -Encoding UTF8 | ConvertFrom-Json
            $json.workflowKind | Should Be 'talk-provider-correction-replay-result'
            $json.samples.Count | Should Be 2
            $json.exactMatchCount | Should Be 2
            $json.improvedCount | Should Be 2
            $json.maxAutoPatchEditRatio | Should Be 0.35
            $json.samples[0].providerOutputText | Should Be '打开 Talk 的 local first ASR 测试'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can reuse an existing replay output as a deterministic provider-output fixture' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-fixture-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "noise-realistic-001",
      "audioWav": "noise-realistic-001-16k-mono-s16.wav",
      "referenceText": "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "noise-realistic-001",
  "text": "有现在办公室里有一点空调和键盘声请继续记录 talk 的多语言识别测试结",
  "cer": 0.1315789474
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-noise-realistic-001.json') -Encoding UTF8

            $outputJson = Join-Path $tempRoot 'provider-replay.json'
            @'
{
  "workflowKind": "talk-provider-correction-replay-result",
  "samples": [
    {
      "sampleId": "noise-realistic-001",
      "providerOutputText": "有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结"
    }
  ]
}
'@ | Set-Content -LiteralPath $outputJson -Encoding UTF8

            $calls = New-Object System.Collections.Generic.List[string]
            $result = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath $manifestPath `
                -OutputJson $outputJson `
                -UseExistingReplayAsFixture `
                -PassThru `
                -ProcessorInvoker {
                    param($Transcript, $Mode, $ProviderOutputText)
                    $calls.Add("$Mode => $Transcript => $ProviderOutputText") | Out-Null
                    [pscustomobject]@{
                        outputText = '现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。'
                        providerOutputText = $ProviderOutputText
                        faithfulValidation = [pscustomobject]@{
                            accepted = $true
                            fallbackReason = $null
                            inputCharCount = 34
                            outputCharCount = 34
                            retentionRatio = 1.0
                            normalizedChangeRatio = 0.0588235294
                        }
                    }
                }

            $result.SampleCount | Should Be 1
            $result.ExactMatchCount | Should Be 1
            $result.Samples[0].UsedProviderOutputFixture | Should Be $true
            $result.EditRatioEligibleCount | Should Be 1
            $result.Samples[0].EditRatioEligible | Should Be $true
            ($result.Samples[0].PatchEditRatio -lt 0.35) | Should Be $true
            $result.Samples[0].ProviderOutputText | Should Be '有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结'
            $calls.Count | Should Be 1
            $calls[0] | Should Match '有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can reuse a corpus-root replay fixture when direct replay omits OutputJson' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-fixture-fallback-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "noise-realistic-001",
      "audioWav": "noise-realistic-001-16k-mono-s16.wav",
      "referenceText": "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "noise-realistic-001",
  "text": "有现在办公室里有一点空调和键盘声请继续记录 talk 的多语言识别测试结",
  "cer": 0.1315789474
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-noise-realistic-001.json') -Encoding UTF8

            @'
{
  "workflowKind": "talk-provider-correction-replay-result",
  "samples": [
    {
      "sampleId": "noise-realistic-001",
      "providerOutputText": "有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结"
    }
  ]
}
'@ | Set-Content -LiteralPath (Join-Path $tempRoot 'provider-correction-replay-paraformer-bilingual-zh-en.json') -Encoding UTF8

            $result = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath $manifestPath `
                -UseExistingReplayAsFixture `
                -PassThru `
                -ProcessorInvoker {
                    param($Transcript, $Mode, $ProviderOutputText)
                    [pscustomobject]@{
                        outputText = '现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。'
                        providerOutputText = $ProviderOutputText
                        faithfulValidation = [pscustomobject]@{
                            accepted = $true
                            fallbackReason = $null
                            inputCharCount = 34
                            outputCharCount = 34
                            retentionRatio = 1.0
                            normalizedChangeRatio = 0.0588235294
                        }
                    }
                }

            $result.SampleCount | Should Be 1
            $result.Samples[0].UsedProviderOutputFixture | Should Be $true
            $result.Samples[0].ProviderOutputText | Should Be '有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'accepts process-transcript json even when the Talk executable writes diagnostics to stderr' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-stderr-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "short-search-001",
      "audioWav": "short-search-001-16k-mono-s16.wav",
      "referenceText": "你好呀"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "short-search-001",
  "text": "我你好",
  "cer": 0.6666666666666666
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8

            $fakeTalkScript = Join-Path $tempRoot 'fake-talk.ps1'
            @'
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
[Console]::Error.WriteLine("Talk faithful output fallback mode=Transcribe reason=protected_token_mismatch")
[Console]::Out.WriteLine('{"app":"talk","kind":"processed_transcript","transcript":"我你好","requestedMode":"transcribe","outputText":"你好呀","providerOutputText":"你好呀","faithfulValidation":{"accepted":true,"fallbackReason":null,"inputCharCount":3,"outputCharCount":3,"retentionRatio":1.0,"normalizedChangeRatio":0.6666666667}}')
'@ | Set-Content -LiteralPath $fakeTalkScript -Encoding UTF8

            $fakeTalkCmd = Join-Path $tempRoot 'fake-talk.cmd'
            @"
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "$fakeTalkScript" %*
"@ | Set-Content -LiteralPath $fakeTalkCmd -Encoding ASCII

            $outputJson = Join-Path $tempRoot 'provider-replay.json'
            $result = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -TalkExe $fakeTalkCmd `
                -ConfigPath $manifestPath `
                -OutputJson $outputJson `
                -PassThru

            $result.SampleCount | Should Be 1
            $result.ExactMatchCount | Should Be 1
            $result.ImprovedCount | Should Be 1
            $result.Samples[0].OutputText | Should Be '你好呀'
            $result.Samples[0].ProcessedCer | Should Be 0
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'passes the full transcript as one process argument even when it contains spaces' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-replay-args-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "mixed-english-001",
      "audioWav": "mixed-english-001-16k-mono-s16.wav",
      "referenceText": "打开 Talk 的 local first ASR 测试"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "mixed-english-001",
  "text": "打开 talk 的 rock foster a s r 测",
  "cer": 0.5
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-mixed-english-001.json') -Encoding UTF8

            $fakeTalkScript = Join-Path $tempRoot 'fake-talk-args.ps1'
            @'
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Args)
if ($Args.Count -ne 8) {
    [Console]::Error.WriteLine("expected 8 args but got $($Args.Count): $($Args -join '|')")
    exit 12
}
if ($Args[0] -ne 'process-transcript' -or $Args[1] -ne '--config' -or $Args[3] -ne '--transcript' -or $Args[5] -ne '--mode' -or $Args[7] -ne '--json') {
    [Console]::Error.WriteLine("unexpected fixed args: $($Args -join '|')")
    exit 13
}
if ($Args[4] -ne '打开 talk 的 rock foster a s r 测') {
    [Console]::Error.WriteLine("transcript arg was split or altered: $($Args[4])")
    exit 14
}
[Console]::Out.WriteLine('{"app":"talk","kind":"processed_transcript","transcript":"打开 talk 的 rock foster a s r 测","requestedMode":"transcribe","outputText":"打开 Talk 的 local first ASR 测试","providerOutputText":"打开 Talk 的 local first ASR 测试","faithfulValidation":{"accepted":true,"fallbackReason":null,"inputCharCount":21,"outputCharCount":22,"retentionRatio":1.0476190476,"normalizedChangeRatio":0.0454545455}}')
'@ | Set-Content -LiteralPath $fakeTalkScript -Encoding UTF8

            $fakeTalkCmd = Join-Path $tempRoot 'fake-talk-args.cmd'
            @"
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "$fakeTalkScript" %*
"@ | Set-Content -LiteralPath $fakeTalkCmd -Encoding ASCII

            $outputJson = Join-Path $tempRoot 'provider-replay.json'
            $result = Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -TalkExe $fakeTalkCmd `
                -ConfigPath $manifestPath `
                -OutputJson $outputJson `
                -PassThru

            $result.SampleCount | Should Be 1
            $result.ExactMatchCount | Should Be 1
            $result.Samples[0].OutputText | Should Be '打开 Talk 的 local first ASR 测试'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects an invalid patch edit-ratio threshold before resolving replay paths' {
        . $scriptPath

        { New-TalkProviderCorrectionReplayPlan -MaxAutoPatchEditRatio 1.01 } |
            Should Throw 'Talk provider correction replay MaxAutoPatchEditRatio must be between 0 and 1'
    }
}
