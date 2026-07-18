$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Select-TalkDefaultAsrModel.ps1'

. $scriptPath

function New-TestTalkDefaultAsrCandidate {
    param(
        [Parameter(Mandatory = $true)][string]$Engine,
        [Parameter(Mandatory = $true)][string[]]$SampleIds,
        [double]$Cer = 0.0,
        [int]$FirstPartialMs = 180,
        [int]$FinalLatencyMs = 320,
        [double]$Rtf = 0.2,
        [int]$ModelSizeMb = 512,
        [string[]]$Sources = @()
    )

    if ($Sources.Count -eq 0) {
        $safeEngine = $Engine -replace '[^A-Za-z0-9_.-]', '-'
        $Sources = @(
            foreach ($sampleId in $SampleIds) {
                ".\reports\$safeEngine-$sampleId.json"
            }
        )
    }

    [pscustomobject]@{
        engine = $Engine
        sources = $Sources
        sample_count = $SampleIds.Count
        sample_ids = $SampleIds
        score = 100.0
        cer = $Cer
        first_partial_ms = $FirstPartialMs
        final_latency_ms = $FinalLatencyMs
        rtf = $Rtf
        peak_rss_mb = 0
        model_size_mb = $ModelSizeMb
        text = 'sample transcript'
    }
}

function Write-TestTalkDefaultAsrComparison {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object[]]$Candidates,
        [string]$SelectedEngine
    )

    $selected = if ([string]::IsNullOrWhiteSpace($SelectedEngine)) {
        [string]$Candidates[0].engine
    } else {
        $SelectedEngine
    }
    $comparison = [ordered]@{
        selected_engine = $selected
        candidates = $Candidates
    }
    $comparison | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Path -Encoding UTF8
}

Describe 'Select-TalkDefaultAsrModel' {
    It 'rejects synthetic smoke sample ids even when comparison metrics are otherwise present' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-synthetic-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds @('huihui-nihaoya') `
                        -Sources @('.\docs\asr-benchmarks\paraformer-bilingual-zh-en-huihui-nihaoya.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds @('huihui-nihaoya') `
                        -Sources @('.\docs\asr-benchmarks\zipformer-zh-en-punct-int8-480ms-huihui-nihaoya.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                        -SampleIds @('huihui-nihaoya') `
                        -Sources @('.\reports\cloud-openai-compatible-chat_completions_audio_input-huihui-nihaoya.json'))
                )

            {
                Select-TalkDefaultAsrModel `
                    -ComparisonJson $comparisonPath `
                    -MinSamples 1
            } | Should Throw 'synthetic'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects evidence with fewer samples than the default model selection gate requires' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-minsamples-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds @('short-search-001', 'mixed-english-001') `
                        -Sources @('.\reports\paraformer-bilingual-zh-en-short-search-001.json', '.\reports\paraformer-bilingual-zh-en-mixed-english-001.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds @('short-search-001', 'mixed-english-001') `
                        -Sources @('.\reports\zipformer-zh-en-punct-int8-480ms-short-search-001.json', '.\reports\zipformer-zh-en-punct-int8-480ms-mixed-english-001.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                        -SampleIds @('short-search-001', 'mixed-english-001'))
                )

            {
                Select-TalkDefaultAsrModel -ComparisonJson $comparisonPath
            } | Should Throw 'MinSamples'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reports evidence blockers without writing a selection when status-only is requested' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-status-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            $outputPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds @('huihui-nihaoya') `
                        -Sources @('.\docs\asr-benchmarks\paraformer-bilingual-zh-en-huihui-nihaoya.json'))
                )

            $status = Select-TalkDefaultAsrModel `
                -ComparisonJson $comparisonPath `
                -OutputJson $outputPath `
                -StatusOnly

            $status.kind | Should Be 'talk-default-asr-model-evidence-status'
            $status.ready | Should Be $false
            $status.cloudBaselinePresent | Should Be $false
            ($status.missingLocalModelIds -contains 'zipformer-zh-en-punct-int8-480ms') | Should Be $true
            ($status.blockingReasons -join "`n") | Should Match 'cloud'
            ($status.blockingReasons -join "`n") | Should Match 'MinSamples'
            ($status.blockingReasons -join "`n") | Should Match 'synthetic'
            $status.candidateStatuses[0].ready | Should Be $false
            $status.candidateStatuses[0].syntheticSampleIds[0] | Should Be 'huihui-nihaoya'
            Test-Path -LiteralPath $outputPath | Should Be $false
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'selects the best local candidate from a comparison that also includes a cloud baseline' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-ready-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            $outputPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            $sampleIds = @('short-search-001', 'mixed-english-001', 'punctuation-001')
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -SelectedEngine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                        -SampleIds $sampleIds `
                        -Cer 0.0 `
                        -FirstPartialMs 950 `
                        -FinalLatencyMs 950),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds $sampleIds `
                        -Cer 0.01 `
                        -FirstPartialMs 180 `
                        -FinalLatencyMs 330 `
                        -ModelSizeMb 1052 `
                        -Sources @('.\reports\paraformer-bilingual-zh-en-short-search-001.json', '.\reports\paraformer-bilingual-zh-en-mixed-english-001.json', '.\reports\paraformer-bilingual-zh-en-punctuation-001.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds $sampleIds `
                        -Cer 0.03 `
                        -FirstPartialMs 140 `
                        -FinalLatencyMs 280 `
                        -ModelSizeMb 162 `
                        -Sources @('.\reports\zipformer-zh-en-punct-int8-480ms-short-search-001.json', '.\reports\zipformer-zh-en-punct-int8-480ms-mixed-english-001.json', '.\reports\zipformer-zh-en-punct-int8-480ms-punctuation-001.json'))
                )

            $selection = Select-TalkDefaultAsrModel `
                -ComparisonJson $comparisonPath `
                -OutputJson $outputPath `
                -PassThru

            $selection.evidenceReady | Should Be $true
            $selection.selectedModelId | Should Be 'paraformer-bilingual-zh-en'
            $selection.selectedEngine | Should Be 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en'
            $selection.cloudBaselinePresent | Should Be $true
            $selection.globalSelectedEngine | Should Be 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest'
            Test-Path -LiteralPath $outputPath | Should Be $true

            $written = Get-Content -LiteralPath $outputPath -Raw | ConvertFrom-Json
            $written.schemaVersion | Should Be 1
            $written.kind | Should Be 'talk-default-asr-model-selection'
            $written.selectedModelId | Should Be 'paraformer-bilingual-zh-en'
            $written.rankedLocalCandidates.Count | Should Be 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'ranks local candidates by evidence metrics instead of trusting input order' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-rerank-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            $outputPath = Join-Path $tempRoot 'selected-default-asr-model.json'
            $sampleIds = @('short-search-001', 'mixed-english-001', 'punctuation-001')
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -SelectedEngine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                        -SampleIds $sampleIds `
                        -Cer 0.0 `
                        -FirstPartialMs 900 `
                        -FinalLatencyMs 900),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds $sampleIds `
                        -Cer 0.06 `
                        -FirstPartialMs 160 `
                        -FinalLatencyMs 300 `
                        -ModelSizeMb 1052),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds $sampleIds `
                        -Cer 0.02 `
                        -FirstPartialMs 240 `
                        -FinalLatencyMs 360 `
                        -ModelSizeMb 162)
                )

            $selection = Select-TalkDefaultAsrModel `
                -ComparisonJson $comparisonPath `
                -OutputJson $outputPath `
                -PassThru

            $selection.selectedModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $selection.rankedLocalCandidates[0].modelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $selection.rankedLocalCandidates[1].modelId | Should Be 'paraformer-bilingual-zh-en'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects candidates that were not benchmarked on the same sample id set' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-sample-set-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            $sharedSampleIds = @('short-search-001', 'mixed-english-001', 'punctuation-001')
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -SelectedEngine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds $sharedSampleIds),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds @('short-search-001', 'mixed-english-001', 'browser-url-001')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'cloud_openai_compatible:chat_completions_audio_input:qwen-audio-asr-latest' `
                        -SampleIds $sharedSampleIds)
                )

            {
                Select-TalkDefaultAsrModel -ComparisonJson $comparisonPath
            } | Should Throw 'same sample'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects missing cloud baseline by default because Task 6 requires that comparison' {
        $tempRoot = Join-Path $env:TEMP ('talk-default-asr-select-cloud-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $tempRoot 'asr-model-comparison.json'
            $sampleIds = @('short-search-001', 'mixed-english-001', 'punctuation-001')
            Write-TestTalkDefaultAsrComparison `
                -Path $comparisonPath `
                -Candidates @(
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:streaming-paraformer-bilingual-zh-en' `
                        -SampleIds $sampleIds `
                        -Sources @('.\reports\paraformer-bilingual-zh-en-short-search-001.json', '.\reports\paraformer-bilingual-zh-en-mixed-english-001.json', '.\reports\paraformer-bilingual-zh-en-punctuation-001.json')),
                    (New-TestTalkDefaultAsrCandidate `
                        -Engine 'streaming_service:sherpa-onnx:x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8' `
                        -SampleIds $sampleIds `
                        -Sources @('.\reports\zipformer-zh-en-punct-int8-480ms-short-search-001.json', '.\reports\zipformer-zh-en-punct-int8-480ms-mixed-english-001.json', '.\reports\zipformer-zh-en-punct-int8-480ms-punctuation-001.json'))
                )

            {
                Select-TalkDefaultAsrModel -ComparisonJson $comparisonPath
            } | Should Throw 'cloud'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
