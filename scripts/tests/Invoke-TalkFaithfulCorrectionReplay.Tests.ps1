$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkFaithfulCorrectionReplay.ps1'

Describe 'Invoke-TalkFaithfulCorrectionReplay' {
    It 'ships a replay helper for validating faithful correction against a recorded corpus' {
        (Test-Path -LiteralPath $scriptPath -PathType Leaf) | Should Be $true
    }

    It 'creates a plan-only replay matrix from the corpus manifest and local ASR reports' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-faithful-replay-plan-' + [guid]::NewGuid().ToString())
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
  "engine": "streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8",
  "sample_id": "mixed-english-001",
  "text": "打开 talk 的 rock foster a s r 测"
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'zipformer-zh-en-punct-int8-480ms-mixed-english-001.json') -Encoding UTF8

            $plan = Invoke-TalkFaithfulCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -PlanOnly

            $plan.WorkflowKind | Should Be 'talk-faithful-correction-replay-plan'
            $plan.CorpusManifest | Should Be ([System.IO.Path]::GetFullPath($manifestPath))
            $plan.ReportsRoot | Should Be ([System.IO.Path]::GetFullPath($reportsRoot))
            $plan.ModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $plan.Replays.Count | Should Be 1
            $plan.Replays[0].SampleId | Should Be 'mixed-english-001'
            $plan.Replays[0].LocalText | Should Be '打开 talk 的 rock foster a s r 测'
            $plan.Replays[0].ReferenceText | Should Be '打开 Talk 的 local first ASR 测试'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'replays faithful validation for each sample and writes an aggregate json report' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-faithful-replay-run-' + [guid]::NewGuid().ToString())
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
  "text": "打开 talk 的 rock foster a s r 测"
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-mixed-english-001.json') -Encoding UTF8

            @'
{
  "engine": "streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en",
  "sample_id": "proper-nouns-001",
  "text": "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us"
}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-proper-nouns-001.json') -Encoding UTF8

            $outputJson = Join-Path $tempRoot 'faithful-replay.json'
            $calls = New-Object System.Collections.Generic.List[string]
            $result = Invoke-TalkFaithfulCorrectionReplay `
                -CorpusManifest $manifestPath `
                -ReportsRoot $reportsRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -OutputJson $outputJson `
                -PassThru `
                -ValidatorInvoker {
                    param($InputText, $OutputText)
                    $calls.Add("$InputText -> $OutputText") | Out-Null
                    [pscustomobject]@{
                        accepted = $true
                        fallbackReason = $null
                        inputCharCount = 10
                        outputCharCount = 10
                        retentionRatio = 1.0
                        normalizedChangeRatio = 0.1
                    }
                }

            $result.WorkflowKind | Should Be 'talk-faithful-correction-replay-result'
            $result.SampleCount | Should Be 2
            $result.AcceptedCount | Should Be 2
            $result.RejectedCount | Should Be 0
            $result.AllAccepted | Should Be $true
            (Test-Path -LiteralPath $outputJson -PathType Leaf) | Should Be $true
            $calls.Count | Should Be 2
            $calls[0] | Should Match 'rock foster a s r'
            $calls[1] | Should Match '你 o talk'

            $json = Get-Content -LiteralPath $outputJson -Raw -Encoding UTF8 | ConvertFrom-Json
            $json.workflowKind | Should Be 'talk-faithful-correction-replay-result'
            $json.samples.Count | Should Be 2
            $json.acceptedCount | Should Be 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
