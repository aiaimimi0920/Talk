$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkAsrDefaultModelWorkflow.ps1'

. $scriptPath

function New-TestTalkWorkflowSherpaModelDir {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][ValidateSet('transducer', 'paraformer')][string]$Family
    )

    $modelDir = Join-Path $Root $ModelId
    New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $modelDir 'tokens.txt') -Value '<blk>' -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $modelDir 'encoder.onnx') -Value 'encoder' -Encoding ASCII
    Set-Content -LiteralPath (Join-Path $modelDir 'decoder.onnx') -Value 'decoder' -Encoding ASCII
    if ($Family -eq 'transducer') {
        Set-Content -LiteralPath (Join-Path $modelDir 'joiner.onnx') -Value 'joiner' -Encoding ASCII
    }
}

Describe 'Invoke-TalkAsrDefaultModelWorkflow' {
    It 'creates a release-side plan-only workflow using paths relative to the current PowerShell location' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-default-workflow-plan-' + [guid]::NewGuid().ToString())
        $releaseDir = Join-Path $tempRoot 'release'
        $processCwd = Join-Path $tempRoot 'process-cwd'
        $modelRoot = Join-Path $releaseDir '.runtime\models\sherpa-onnx'
        $corpusRoot = Join-Path $releaseDir '.runtime\asr-bench\real-mic-corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $releaseDir '.internal') -Force | Out-Null
        New-Item -ItemType Directory -Path $processCwd -Force | Out-Null
        $originalDotNetCurrentDirectory = [Environment]::CurrentDirectory
        try {
            New-TestTalkWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            Set-Content -LiteralPath (Join-Path $releaseDir 'talk-desktop.toml') -Value '[speculative.streaming_service]' -Encoding UTF8
            $samplePath = Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav'
            Set-Content -LiteralPath $samplePath -Value 'not a real wav for plan-only tests' -Encoding ASCII
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8

            Push-Location $releaseDir
            try {
                [Environment]::CurrentDirectory = $processCwd
                $plan = Invoke-TalkAsrDefaultModelWorkflow `
                    -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
                    -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
                    -ModelRoot .\.runtime\models\sherpa-onnx `
                    -OutputRoot .\.runtime\asr-bench\real-mic-corpus\reports `
                    -AsrBenchExe .\.internal\asr-bench.exe `
                    -LocalAsrDaemonExe .\.internal\talk-local-asr-sherpa.exe `
                    -ConfigPath .\talk-desktop.toml `
                    -PlanOnly
            }
            finally {
                Pop-Location
            }

            $plan.WorkflowKind | Should Be 'talk-asr-default-model-workflow-plan'
            $plan.BenchmarkPlan.CorpusManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $plan.BenchmarkPlan.OutputRoot | Should Be ([System.IO.Path]::GetFullPath($reportsRoot))
            $plan.ComparisonJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $reportsRoot 'asr-model-comparison.json')))
            $plan.SelectionJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $reportsRoot 'selected-default-asr-model.json')))
            $plan.EvidenceStatusJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $reportsRoot 'asr-model-evidence-status.json')))
            $plan.ConfigPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir 'talk-desktop.toml')))
            $plan.WillApply | Should Be $true
        }
        finally {
            [Environment]::CurrentDirectory = $originalDotNetCurrentDirectory
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'runs benchmark, selector, and applier in order' {
        $script:workflowCalls = @()
        try {
            Mock Invoke-TalkAsrCorpusBenchmark {
                $script:workflowCalls += 'benchmark'
                [pscustomobject]@{
                    ComparisonPath = 'C:\talk-reports\asr-model-comparison.json'
                    Plan = [pscustomobject]@{ OutputRoot = 'C:\talk-reports' }
                    ProcessRecords = @([pscustomobject]@{ ExitCode = 0 })
                    ComparisonRecord = [pscustomobject]@{ ExitCode = 0 }
                    ReportPaths = @('C:\talk-reports\zipformer-short-search-001.json')
                }
            }
            Mock Select-TalkDefaultAsrModel {
                param([string]$ComparisonJson, [string]$OutputJson, [switch]$StatusOnly)
                if ($StatusOnly) {
                    $script:workflowCalls += ('status:{0}' -f $ComparisonJson)
                    return [pscustomobject]@{
                        kind = 'talk-default-asr-model-evidence-status'
                        ready = $true
                        blockingReasons = @()
                    }
                }
                $script:workflowCalls += ('select:{0}:{1}' -f $ComparisonJson, $OutputJson)
                [pscustomobject]@{
                    outputJson = $OutputJson
                    selectedModelId = 'paraformer-bilingual-zh-en'
                    evidenceReady = $true
                }
            }
            Mock Set-TalkDefaultAsrModel {
                param([string]$SelectionJson, [string]$ConfigPath)
                $script:workflowCalls += ('apply:{0}:{1}' -f $SelectionJson, $ConfigPath)
                [pscustomobject]@{
                    Applied = $true
                    SelectionJson = $SelectionJson
                    ConfigPath = $ConfigPath
                    SelectedModelId = 'paraformer-bilingual-zh-en'
                }
            }

            $result = Invoke-TalkAsrDefaultModelWorkflow `
                -CorpusManifest 'C:\talk-corpus\corpus.json' `
                -ModelRoot 'C:\talk-models' `
                -OutputRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -CloudOpenAiCompatibleEndpoint 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-asr-latest' `
                -PassThru

            $script:workflowCalls.Count | Should Be 4
            $script:workflowCalls[0] | Should Be 'benchmark'
            $script:workflowCalls[1] | Should Match '^status:C:\\talk-reports\\asr-model-comparison\.json$'
            $script:workflowCalls[2] | Should Match '^select:C:\\talk-reports\\asr-model-comparison\.json:C:\\talk-reports\\selected-default-asr-model\.json$'
            $script:workflowCalls[3] | Should Match '^apply:C:\\talk-reports\\selected-default-asr-model\.json:C:\\talk-release\\talk-desktop\.toml$'
            $result.WorkflowKind | Should Be 'talk-asr-default-model-workflow-result'
            $result.EvidenceStatus.ready | Should Be $true
            $result.Selection.selectedModelId | Should Be 'paraformer-bilingual-zh-en'
            $result.ApplyResult.Applied | Should Be $true
        }
        finally {
            Remove-Variable -Name workflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'writes evidence status before surfacing a strict selector failure' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-default-workflow-status-on-failure-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $comparisonPath = Join-Path $reportsRoot 'asr-model-comparison.json'
            $evidenceStatusPath = Join-Path $reportsRoot 'asr-model-evidence-status.json'
            Set-Content -LiteralPath $comparisonPath -Value '{"selected_engine":"synthetic","candidates":[]}' -Encoding UTF8

            Mock Invoke-TalkAsrCorpusBenchmark {
                [pscustomobject]@{
                    ComparisonPath = $comparisonPath
                    Plan = [pscustomobject]@{ OutputRoot = $reportsRoot }
                    ProcessRecords = @()
                    ComparisonRecord = [pscustomobject]@{ ExitCode = 0 }
                    ReportPaths = @()
                }
            }
            Mock Select-TalkDefaultAsrModel {
                param([switch]$StatusOnly)
                if ($StatusOnly) {
                    return [pscustomobject]@{
                        schemaVersion = 1
                        kind = 'talk-default-asr-model-evidence-status'
                        ready = $false
                        comparisonJson = $comparisonPath
                        blockingReasons = @('missing real microphone evidence')
                        missingLocalModelIds = @('zipformer-zh-en-punct-int8-480ms')
                    }
                }
                throw 'strict selector rejected incomplete Task 6 evidence'
            }
            Mock Set-TalkDefaultAsrModel {
                throw 'Set-TalkDefaultAsrModel should not be called when selection fails'
            }

            {
                Invoke-TalkAsrDefaultModelWorkflow `
                    -CorpusManifest 'C:\talk-corpus\corpus.json' `
                    -OutputRoot $reportsRoot `
                    -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                    -PassThru
            } | Should Throw 'strict selector rejected incomplete Task 6 evidence'

            Test-Path -LiteralPath $evidenceStatusPath | Should Be $true
            $status = Get-Content -LiteralPath $evidenceStatusPath -Raw | ConvertFrom-Json
            $status.kind | Should Be 'talk-default-asr-model-evidence-status'
            $status.ready | Should Be $false
            $status.blockingReasons[0] | Should Be 'missing real microphone evidence'
            $status.missingLocalModelIds[0] | Should Be 'zipformer-zh-en-punct-int8-480ms'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can skip applying the selected model after benchmark and selection' {
        $script:workflowCalls = @()
        try {
            Mock Invoke-TalkAsrCorpusBenchmark {
                $script:workflowCalls += 'benchmark'
                [pscustomobject]@{
                    ComparisonPath = 'C:\talk-reports\asr-model-comparison.json'
                    Plan = [pscustomobject]@{ OutputRoot = 'C:\talk-reports' }
                    ProcessRecords = @()
                    ComparisonRecord = [pscustomobject]@{ ExitCode = 0 }
                    ReportPaths = @()
                }
            }
            Mock Select-TalkDefaultAsrModel {
                param([string]$OutputJson, [switch]$StatusOnly)
                if ($StatusOnly) {
                    $script:workflowCalls += 'status'
                    return [pscustomobject]@{
                        kind = 'talk-default-asr-model-evidence-status'
                        ready = $true
                        blockingReasons = @()
                    }
                }
                $script:workflowCalls += 'select'
                [pscustomobject]@{
                    outputJson = $OutputJson
                    selectedModelId = 'zipformer-zh-en-punct-int8-480ms'
                    evidenceReady = $true
                }
            }
            Mock Set-TalkDefaultAsrModel {
                throw 'Set-TalkDefaultAsrModel should not be called when SkipApply is set'
            }

            $result = Invoke-TalkAsrDefaultModelWorkflow `
                -CorpusManifest 'C:\talk-corpus\corpus.json' `
                -ModelRoot 'C:\talk-models' `
                -OutputRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -SkipApply `
                -AllowMissingCloudBaseline `
                -PassThru

            $script:workflowCalls.Count | Should Be 3
            $script:workflowCalls[0] | Should Be 'benchmark'
            $script:workflowCalls[1] | Should Be 'status'
            $script:workflowCalls[2] | Should Be 'select'
            $result.ApplyResult | Should Be $null
            $result.Applied | Should Be $false
        }
        finally {
            Remove-Variable -Name workflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'preserves explicit parameters when the workflow script is invoked directly' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-default-workflow-direct-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $outputRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $modelRoot -Force | Out-Null
        try {
            New-TestTalkWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $samplePath = Join-Path $tempRoot 'short-search.wav'
            Set-Content -LiteralPath $samplePath -Value 'not a real wav for plan-only tests' -Encoding ASCII
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "$($samplePath.Replace('\', '\\'))", "referenceText": "你好呀" }
  ]
}
"@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            $command = @"
& '$scriptPath' -CorpusManifest '$manifestPath' -ModelId 'paraformer-bilingual-zh-en' -ModelRoot '$modelRoot' -OutputRoot '$outputRoot' -AsrBenchExe '$tempRoot\asr-bench.exe' -LocalAsrDaemonExe '$tempRoot\talk-local-asr-sherpa.exe' -ConfigPath '$tempRoot\talk-desktop.toml' -PlanOnly | ConvertTo-Json -Depth 8
"@
            $output = powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1

            $LASTEXITCODE | Should Be 0
            $json = ($output | Out-String) | ConvertFrom-Json
            $json.WorkflowKind | Should Be 'talk-asr-default-model-workflow-plan'
            $json.BenchmarkPlan.Candidates.Count | Should Be 1
            $json.BenchmarkPlan.Candidates[0].ModelId | Should Be 'paraformer-bilingual-zh-en'
            $json.SelectionJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'selected-default-asr-model.json')))
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
