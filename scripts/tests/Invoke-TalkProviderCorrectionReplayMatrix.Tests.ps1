$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot 'Invoke-TalkProviderCorrectionReplayMatrix.ps1'))

Describe 'Invoke-TalkProviderCorrectionReplayMatrix' {
    It 'ships a matrix helper for aggregating provider correction replay across real microphone corpora' {
        (Test-Path -LiteralPath $scriptPath -PathType Leaf) | Should Be $true
    }

    It 'supports script-entry plan-only mode without accidentally invoking nested replay execution' {
        $tempRoot = Join-Path $env:TEMP ('talk-provider-matrix-entry-' + [guid]::NewGuid().ToString())
        try {
            $corpusRoot = Join-Path $tempRoot 'real-mic-corpus-r9'
            $reportsRoot = Join-Path $corpusRoot 'reports'
            $fakeTalkExe = Join-Path $tempRoot 'fake-talk.exe'
            New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null

            @'
{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            @'
{"engine":"e","sample_id":"short-search-001","text":"我你好","cer":0.6}
'@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8
            '' | Set-Content -LiteralPath $fakeTalkExe -Encoding ASCII

            $command = "& '{0}' -AsrBenchRoot '{1}' -ModelId 'paraformer-bilingual-zh-en' -TalkExe '{2}' -ConfigPath '{3}' -PlanOnly | ConvertTo-Json -Depth 8" -f `
                $scriptPath, `
                $tempRoot, `
                $fakeTalkExe, `
                (Join-Path $corpusRoot 'corpus.json')

            $output = & powershell -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1
            $lastExitCode = $LASTEXITCODE

            $lastExitCode | Should Be 0
            $json = ($output | Out-String) | ConvertFrom-Json
            $json.WorkflowKind | Should Be 'talk-provider-correction-replay-matrix-plan'
            $json.TalkExe | Should Be ([System.IO.Path]::GetFullPath($fakeTalkExe))
            $json.ConfigPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $json.Corpora.Count | Should Be 1
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'discovers compatible real microphone corpora and prefers sherpa report roots when available' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-matrix-plan-' + [guid]::NewGuid().ToString())
        try {
            $corpusA = Join-Path $tempRoot 'real-mic-corpus-r4'
            $corpusB = Join-Path $tempRoot 'real-mic-corpus-r5'
            $corpusIgnored = Join-Path $tempRoot 'real-mic-corpus-r3'
            New-Item -ItemType Directory -Path $corpusA, $corpusB, $corpusIgnored -Force | Out-Null
            New-Item -ItemType Directory -Path (Join-Path $corpusA 'reports'), (Join-Path $corpusA 'reports-sherpa-paraformer'), (Join-Path $corpusB 'reports') -Force | Out-Null

            @'
{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusA 'corpus.json') -Encoding UTF8
            @'
{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusB 'corpus.json') -Encoding UTF8
            @'
{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusIgnored 'corpus.json') -Encoding UTF8

            @'
{"engine":"e","sample_id":"short-search-001","text":"我你好","cer":0.6}
'@ | Set-Content -LiteralPath (Join-Path $corpusA 'reports-sherpa-paraformer\paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8
            @'
{"engine":"e","sample_id":"short-search-001","text":"我你好","cer":0.6}
'@ | Set-Content -LiteralPath (Join-Path $corpusA 'reports\paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8
            @'
{"engine":"e","sample_id":"short-search-001","text":"我你好","cer":0.6}
'@ | Set-Content -LiteralPath (Join-Path $corpusB 'reports\paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8

            $plan = Invoke-TalkProviderCorrectionReplayMatrix `
                -AsrBenchRoot $tempRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath (Join-Path $corpusA 'corpus.json') `
                -PlanOnly

            $plan.WorkflowKind | Should Be 'talk-provider-correction-replay-matrix-plan'
            $plan.Corpora.Count | Should Be 2
            $plan.Corpora[0].CorpusId | Should Be 'real-mic-corpus-r4'
            $plan.Corpora[0].ReportsRoot | Should Match 'reports-sherpa-paraformer'
            $plan.Corpora[1].CorpusId | Should Be 'real-mic-corpus-r5'
            $plan.Corpora[1].ReportsRoot | Should Match '\\reports$'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'aggregates replay results across discovered corpora and writes a summary json report' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-matrix-run-' + [guid]::NewGuid().ToString())
        try {
            $corpusA = Join-Path $tempRoot 'real-mic-corpus-r6'
            $corpusB = Join-Path $tempRoot 'real-mic-corpus-r7'
            New-Item -ItemType Directory -Path $corpusA, $corpusB -Force | Out-Null
            New-Item -ItemType Directory -Path (Join-Path $corpusA 'reports'), (Join-Path $corpusB 'reports-sherpa-paraformer') -Force | Out-Null

            @'
{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusA 'corpus.json') -Encoding UTF8
            @'
{"schemaVersion":1,"samples":[{"sampleId":"mixed-english-001","referenceText":"打开 Talk 的 local first ASR 测试"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusB 'corpus.json') -Encoding UTF8

            @'
{"engine":"e","sample_id":"short-search-001","text":"我你好","cer":0.6}
'@ | Set-Content -LiteralPath (Join-Path $corpusA 'reports\paraformer-bilingual-zh-en-short-search-001.json') -Encoding UTF8
            @'
{"engine":"e","sample_id":"mixed-english-001","text":"打开 talk 的 rock foster a s r 测","cer":0.5}
'@ | Set-Content -LiteralPath (Join-Path $corpusB 'reports-sherpa-paraformer\paraformer-bilingual-zh-en-mixed-english-001.json') -Encoding UTF8

            $outputJson = Join-Path $tempRoot 'matrix.json'
            $calls = New-Object System.Collections.Generic.List[string]
            $result = Invoke-TalkProviderCorrectionReplayMatrix `
                -AsrBenchRoot $tempRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath (Join-Path $corpusA 'corpus.json') `
                -OutputJson $outputJson `
                -PassThru `
                -ReplayInvoker {
                    param($Corpus, $Plan)
                    $calls.Add($Corpus.CorpusId) | Out-Null
                    if ($Corpus.CorpusId -eq 'real-mic-corpus-r6') {
                        [pscustomobject]@{
                            SampleCount = 1
                            ExactMatchCount = 1
                            ImprovedCount = 1
                            MeanLocalCer = 0.6
                            MeanProcessedCer = 0.0
                        }
                    }
                    else {
                        [pscustomobject]@{
                            SampleCount = 1
                            ExactMatchCount = 0
                            ImprovedCount = 1
                            MeanLocalCer = 0.5
                            MeanProcessedCer = 0.1
                        }
                    }
                }

            $result.WorkflowKind | Should Be 'talk-provider-correction-replay-matrix-result'
            $result.CorpusCount | Should Be 2
            $result.TotalSampleCount | Should Be 2
            $result.TotalExactMatchCount | Should Be 1
            $result.TotalImprovedCount | Should Be 2
            ($result.MeanProcessedCer -lt $result.MeanLocalCer) | Should Be $true
            $calls.Count | Should Be 2
            (Test-Path -LiteralPath $outputJson -PathType Leaf) | Should Be $true

            $json = Get-Content -LiteralPath $outputJson -Raw -Encoding UTF8 | ConvertFrom-Json
            $json.workflowKind | Should Be 'talk-provider-correction-replay-matrix-result'
            $json.corpora.Count | Should Be 2
            $json.totalExactMatchCount | Should Be 1
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'forwards deterministic fixture mode into discovered corpus replays' {
        . $scriptPath

        $tempRoot = Join-Path $env:TEMP ('talk-provider-matrix-fixture-forward-' + [guid]::NewGuid().ToString())
        try {
            $corpusRoot = Join-Path $tempRoot 'real-mic-corpus-r8'
            New-Item -ItemType Directory -Path (Join-Path $corpusRoot 'reports') -Force | Out-Null

            @'
{"schemaVersion":1,"samples":[{"sampleId":"noise-realistic-001","referenceText":"现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"}]}
'@ | Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            @'
{"engine":"e","sample_id":"noise-realistic-001","text":"有现在办公室里有一点空调和键盘声请继续记录 talk 的多语言识别测试结","cer":0.13}
'@ | Set-Content -LiteralPath (Join-Path $corpusRoot 'reports\paraformer-bilingual-zh-en-noise-realistic-001.json') -Encoding UTF8

            $flags = New-Object System.Collections.Generic.List[string]
            $result = Invoke-TalkProviderCorrectionReplayMatrix `
                -AsrBenchRoot $tempRoot `
                -ModelId 'paraformer-bilingual-zh-en' `
                -ConfigPath (Join-Path $corpusRoot 'corpus.json') `
                -UseExistingReplayAsFixture `
                -PassThru `
                -ReplayInvoker {
                    param($Corpus, $Plan)
                    $flags.Add([string]$Plan.UseExistingReplayAsFixture) | Out-Null
                    [pscustomobject]@{
                        SampleCount = 1
                        ExactMatchCount = 1
                        ImprovedCount = 1
                        MeanLocalCer = 0.13
                        MeanProcessedCer = 0.0
                    }
                }

            $result.CorpusCount | Should Be 1
            $result.UseExistingReplayAsFixture | Should Be $true
            $flags.Count | Should Be 1
            $flags[0] | Should Be 'True'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
