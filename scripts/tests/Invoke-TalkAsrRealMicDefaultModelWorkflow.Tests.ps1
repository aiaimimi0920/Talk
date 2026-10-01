$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1'

. $scriptPath

function New-TestTalkRealMicWorkflowSherpaModelDir {
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

Describe 'Invoke-TalkAsrRealMicDefaultModelWorkflow' {
    It 'uses the evidence-selected zh-en punctuation model first in every default collection' {
        $expected = @(
            'zipformer-zh-en-punct-int8-480ms',
            'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10',
            'paraformer-bilingual-zh-en'
        )
        $tokens = $null
        $parseErrors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            $scriptPath,
            [ref]$tokens,
            [ref]$parseErrors)
        $defaults = @($ast.FindAll({
                    param($node)
                    $node -is [System.Management.Automation.Language.ParameterAst] -and
                    $node.Name.VariablePath.UserPath -in @('ModelId', 'RequiredLocalModelId') -and
                    $null -ne $node.DefaultValue
                }, $true))

        $parseErrors.Count | Should Be 0
        $defaults.Count | Should Be 5
        foreach ($default in $defaults) {
            (@($default.DefaultValue.SafeGetValue()) -join '|') | Should Be ($expected -join '|')
        }
    }

    It 'ships a multilingual default prompt manifest for default-model locking' {
        $promptPath = Join-Path (Split-Path $scriptRoot -Parent) 'examples\asr-real-mic-prompts.json'
        $manifest = Get-Content -LiteralPath $promptPath -Raw -Encoding UTF8 | ConvertFrom-Json
        $sampleIds = @($manifest.samples | ForEach-Object { [string]$_.sampleId })

        ($sampleIds -contains 'short-search-001') | Should Be $true
        ($sampleIds -contains 'mixed-english-001') | Should Be $true
        ($sampleIds -contains 'mixed-english-japanese-001') | Should Be $true
        ($sampleIds -contains 'proper-nouns-001') | Should Be $true
        ($sampleIds -contains 'punctuation-longform-001') | Should Be $true
        ($sampleIds -contains 'noise-realistic-001') | Should Be $true
    }

    It 'creates a release-side plan from prompt recording through default model selection' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-plan-' + [guid]::NewGuid().ToString())
        $releaseDir = Join-Path $tempRoot 'release'
        $processCwd = Join-Path $tempRoot 'process-cwd'
        $corpusRoot = Join-Path $releaseDir '.runtime\asr-bench\real-mic-corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        New-Item -ItemType Directory -Path $releaseDir -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $releaseDir '.internal') -Force | Out-Null
        New-Item -ItemType Directory -Path $processCwd -Force | Out-Null
        $originalDotNetCurrentDirectory = [Environment]::CurrentDirectory
        try {
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath (Join-Path $releaseDir 'asr-real-mic-prompts.json') -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $releaseDir 'talk-desktop.toml') -Value '[speculative.streaming_service]' -Encoding UTF8

            Push-Location $releaseDir
            try {
                [Environment]::CurrentDirectory = $processCwd
                $plan = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -PromptManifest .\asr-real-mic-prompts.json `
                    -CorpusRoot .\.runtime\asr-bench\real-mic-corpus `
                    -TalkExe .\.internal\talk.exe `
                    -ModelRoot .\.runtime\models\sherpa-onnx `
                    -ReportsRoot .\.runtime\asr-bench\real-mic-corpus\reports `
                    -AsrBenchExe .\.internal\asr-bench.exe `
                    -LocalAsrDaemonExe .\.internal\talk-local-asr-sherpa.exe `
                    -ConfigPath .\talk-desktop.toml `
                    -PlanOnly
            }
            finally {
                Pop-Location
            }

            $plan.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-plan'
            $plan.PromptManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir 'asr-real-mic-prompts.json')))
            $plan.CorpusRoot | Should Be ([System.IO.Path]::GetFullPath($corpusRoot))
            $plan.CorpusManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $plan.ReportsRoot | Should Be ([System.IO.Path]::GetFullPath($reportsRoot))
            $plan.SelectionJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $reportsRoot 'selected-default-asr-model.json')))
            $plan.ConfigPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir 'talk-desktop.toml')))
            $plan.CloudOpenAiCompatibleModel | Should Be 'qwen3-asr-flash'
            (@($plan.ModelId) -join '|') | Should Be (@(
                'zipformer-zh-en-punct-int8-480ms',
                'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10',
                'paraformer-bilingual-zh-en'
            ) -join '|')
            $plan.RecorderPlan.Samples.Count | Should Be 1
            $plan.WillRecord | Should Be $true
            $plan.WillApply | Should Be $true
        }
        finally {
            [Environment]::CurrentDirectory = $originalDotNetCurrentDirectory
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'applies the selected model only after faithful and provider replays succeed' {
        $script:realMicWorkflowCalls = @()
        try {
            $expectedCliTalkExe = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $scriptRoot -Parent) 'target\release\talk.exe'))
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [switch]$PlanOnly
                )
                if ($PlanOnly) {
                    $script:realMicWorkflowCalls += 'record-plan'
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                    }
                }

                $script:realMicWorkflowCalls += 'record-run'
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot, [string]$ConfigPath, [switch]$SkipApply)
                $script:realMicWorkflowCalls += ('default:{0}:{1}:{2}:skipapply:{3}' -f $CorpusManifest, $OutputRoot, $ConfigPath, [bool]$SkipApply)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    Selection = [pscustomobject]@{
                        selectedModelId = 'zipformer-zh-en-punct-int8-480ms'
                    }
                    ConfigPath = $ConfigPath
                    Applied = $false
                }
            }
            Mock Invoke-TalkFaithfulCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$OutputJson,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('faithful:{0}:{1}:{2}:{3}:{4}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $OutputJson)
                [pscustomobject]@{
                    WorkflowKind = 'talk-faithful-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    OutputJson = $OutputJson
                    SampleCount = 1
                    AcceptedCount = 1
                    RejectedCount = 0
                    AllAccepted = $true
                    Samples = @()
                }
            }
            Mock Invoke-TalkProviderCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$ConfigPath,
                    [string]$OutputJson,
                    [switch]$UseExistingReplayAsFixture,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('replay:{0}:{1}:{2}:{3}:{4}:{5}:{6}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $ConfigPath, $OutputJson, [bool]$UseExistingReplayAsFixture)
                [pscustomobject]@{
                    WorkflowKind = 'talk-provider-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    ConfigPath = $ConfigPath
                    OutputJson = $OutputJson
                    UseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
                    SampleCount = 1
                    ExactMatchCount = 1
                    ImprovedCount = 1
                    MeanLocalCer = 0.25
                    MeanProcessedCer = 0.0
                    Samples = @()
                }
            }
            Mock Set-TalkDefaultAsrModel {
                param([string]$SelectionJson, [string]$ConfigPath, [string]$ModelRoot, [switch]$NoBackup, [switch]$PassThru)
                $script:realMicWorkflowCalls += ('apply:{0}:{1}:{2}' -f $SelectionJson, $ConfigPath, $ModelRoot)
                [pscustomobject]@{
                    Applied = $true
                    SelectionJson = $SelectionJson
                    ConfigPath = $ConfigPath
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 7
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run'
            $script:realMicWorkflowCalls[2] | Should Be 'default:C:\talk-corpus\corpus.json:C:\talk-reports:C:\talk-release\talk-desktop.toml:skipapply:True'
            $script:realMicWorkflowCalls[3] | Should Be ('faithful:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-corpus\faithful-correction-replay-zipformer-zh-en-punct-int8-480ms.json' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[4] | Should Be ('replay:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-release\talk-desktop.toml:C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms.json:False' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[5] | Should Be ('replay:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-release\talk-desktop.toml:C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms-deterministic.json:True' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[6] | Should Be 'apply:C:\talk-reports\selected-default-asr-model.json:C:\talk-release\talk-desktop.toml:'
            $result.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-result'
            $result.RecorderResult.Recordings[0].SampleId | Should Be 'short-search-001'
            $result.Applied | Should Be $true
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'replays live and deterministic provider correction for the selected default model after benchmarking' {
        $script:realMicWorkflowCalls = @()
        try {
            $expectedCliTalkExe = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $scriptRoot -Parent) 'target\release\talk.exe'))
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [switch]$PlanOnly
                )
                if ($PlanOnly) {
                    $script:realMicWorkflowCalls += 'record-plan'
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        TalkExe = 'C:\talk-release\Talk.exe'
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                    }
                }

                $script:realMicWorkflowCalls += 'record-run'
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot, [string]$ConfigPath, [switch]$SkipApply)
                $script:realMicWorkflowCalls += ('default:{0}:{1}:{2}:skipapply:{3}' -f $CorpusManifest, $OutputRoot, $ConfigPath, [bool]$SkipApply)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    Selection = [pscustomobject]@{
                        selectedModelId = 'zipformer-zh-en-punct-int8-480ms'
                    }
                    ConfigPath = $ConfigPath
                    Applied = $false
                }
            }
            Mock Invoke-TalkFaithfulCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$OutputJson,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('faithful:{0}:{1}:{2}:{3}:{4}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $OutputJson)
                [pscustomobject]@{
                    WorkflowKind = 'talk-faithful-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    OutputJson = $OutputJson
                    SampleCount = 1
                    AcceptedCount = 1
                    RejectedCount = 0
                    AllAccepted = $true
                    Samples = @()
                }
            }
            Mock Invoke-TalkProviderCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$ConfigPath,
                    [string]$OutputJson,
                    [switch]$UseExistingReplayAsFixture,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('replay:{0}:{1}:{2}:{3}:{4}:{5}:{6}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $ConfigPath, $OutputJson, [bool]$UseExistingReplayAsFixture)
                [pscustomobject]@{
                    WorkflowKind = 'talk-provider-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    ConfigPath = $ConfigPath
                    OutputJson = $OutputJson
                    UseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
                    SampleCount = 1
                    ExactMatchCount = 1
                    ImprovedCount = 1
                    MeanLocalCer = 0.25
                    MeanProcessedCer = 0.0
                    Samples = @()
                }
            }
            Mock Set-TalkDefaultAsrModel {
                param([string]$SelectionJson, [string]$ConfigPath, [string]$ModelRoot, [switch]$NoBackup, [switch]$PassThru)
                $script:realMicWorkflowCalls += ('apply:{0}:{1}:{2}' -f $SelectionJson, $ConfigPath, $ModelRoot)
                [pscustomobject]@{
                    Applied = $true
                    SelectionJson = $SelectionJson
                    ConfigPath = $ConfigPath
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -TalkExe 'C:\talk-release\Talk.exe' `
                -ConfigPath 'C:\talk-release\talk.toml' `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 7
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run'
            $script:realMicWorkflowCalls[2] | Should Be 'default:C:\talk-corpus\corpus.json:C:\talk-reports:C:\talk-release\talk.toml:skipapply:True'
            $script:realMicWorkflowCalls[3] | Should Be ('faithful:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-corpus\faithful-correction-replay-zipformer-zh-en-punct-int8-480ms.json' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[4] | Should Be ('replay:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-release\talk.toml:C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms.json:False' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[5] | Should Be ('replay:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-release\talk.toml:C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms-deterministic.json:True' -f $expectedCliTalkExe)
            $script:realMicWorkflowCalls[6] | Should Be 'apply:C:\talk-reports\selected-default-asr-model.json:C:\talk-release\talk.toml:'
            $result.FaithfulCorrectionReplayModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $result.FaithfulCorrectionReplayResult.TalkExe | Should Be $expectedCliTalkExe
            $result.FaithfulCorrectionReplayResult.OutputJson | Should Be 'C:\talk-corpus\faithful-correction-replay-zipformer-zh-en-punct-int8-480ms.json'
            [string]::IsNullOrWhiteSpace([string]$result.FaithfulCorrectionReplaySkippedReason) | Should Be $true
            $result.ProviderCorrectionReplayModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
            $result.ProviderCorrectionReplayLiveResult.TalkExe | Should Be $expectedCliTalkExe
            $result.ProviderCorrectionReplayLiveResult.OutputJson | Should Be 'C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms.json'
            $result.ProviderCorrectionReplayDeterministicResult.TalkExe | Should Be $expectedCliTalkExe
            $result.ProviderCorrectionReplayDeterministicResult.OutputJson | Should Be 'C:\talk-corpus\provider-correction-replay-zipformer-zh-en-punct-int8-480ms-deterministic.json'
            $result.ProviderCorrectionReplayDeterministicResult.UseExistingReplayAsFixture | Should Be $true
            [string]::IsNullOrWhiteSpace([string]$result.ProviderCorrectionReplaySkippedReason) | Should Be $true
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'stops before provider replay and config apply when faithful replay rejects the recorded corpus' {
        $script:realMicWorkflowCalls = @()
        try {
            $expectedCliTalkExe = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $scriptRoot -Parent) 'target\release\talk.exe'))
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [switch]$PlanOnly
                )
                if ($PlanOnly) {
                    $script:realMicWorkflowCalls += 'record-plan'
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        TalkExe = 'C:\talk-release\Talk.exe'
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @([pscustomobject]@{ SampleId = 'mixed-english-001' })
                    }
                }

                $script:realMicWorkflowCalls += 'record-run'
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @([pscustomobject]@{ SampleId = 'mixed-english-001' })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot, [string]$ConfigPath, [switch]$SkipApply)
                $script:realMicWorkflowCalls += ('default:{0}:{1}:{2}:skipapply:{3}' -f $CorpusManifest, $OutputRoot, $ConfigPath, [bool]$SkipApply)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    Selection = [pscustomobject]@{
                        selectedModelId = 'zipformer-zh-en-punct-int8-480ms'
                    }
                    ConfigPath = $ConfigPath
                    Applied = $false
                }
            }
            Mock Invoke-TalkFaithfulCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$OutputJson,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('faithful:{0}:{1}:{2}:{3}:{4}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $OutputJson)
                [pscustomobject]@{
                    WorkflowKind = 'talk-faithful-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    OutputJson = $OutputJson
                    SampleCount = 2
                    AcceptedCount = 1
                    RejectedCount = 1
                    AllAccepted = $false
                    Samples = @(
                        [pscustomobject]@{
                            sampleId = 'mixed-english-001'
                            validation = [pscustomobject]@{
                                accepted = $false
                                fallbackReason = 'excessive_sequence_change'
                            }
                        },
                        [pscustomobject]@{
                            sampleId = 'short-search-001'
                            validation = [pscustomobject]@{
                                accepted = $true
                                fallbackReason = $null
                            }
                        }
                    )
                }
            }
            Mock Invoke-TalkProviderCorrectionReplay {
                throw 'Invoke-TalkProviderCorrectionReplay should not run when faithful replay is unhealthy'
            }
            Mock Set-TalkDefaultAsrModel {
                throw 'Set-TalkDefaultAsrModel should not run when faithful replay is unhealthy'
            }

            {
                Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -PromptManifest 'C:\talk-prompts\prompts.json' `
                    -CorpusRoot 'C:\talk-corpus' `
                    -ReportsRoot 'C:\talk-reports' `
                    -TalkExe 'C:\talk-release\Talk.exe' `
                    -ConfigPath 'C:\talk-release\talk.toml' `
                    -PassThru
            } | Should Throw 'faithful correction replay rejected sample ids'

            $script:realMicWorkflowCalls.Count | Should Be 4
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run'
            $script:realMicWorkflowCalls[2] | Should Be 'default:C:\talk-corpus\corpus.json:C:\talk-reports:C:\talk-release\talk.toml:skipapply:True'
            $script:realMicWorkflowCalls[3] | Should Be ('faithful:C:\talk-corpus\corpus.json:C:\talk-reports:zipformer-zh-en-punct-int8-480ms:{0}:C:\talk-corpus\faithful-correction-replay-zipformer-zh-en-punct-int8-480ms.json' -f $expectedCliTalkExe)
            $expectedPromptSubsetPath = 'C:\talk-corpus\prompts-rerecord-faithful-reject.json'
            (Test-Path -LiteralPath $expectedPromptSubsetPath -PathType Leaf) | Should Be $true
            $promptSubset = Get-Content -LiteralPath $expectedPromptSubsetPath -Raw -Encoding UTF8 | ConvertFrom-Json
            @($promptSubset.samples).Count | Should Be 1
            [string]$promptSubset.samples[0].sampleId | Should Be 'mixed-english-001'
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'can skip recording and reuse an existing corpus manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-existing-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        $script:realMicWorkflowCalls = @()
        try {
            New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
            New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'Invoke-TalkAsrCorpusRecorder should not be called when SkipRecording is set'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot)
                $script:realMicWorkflowCalls += ('default:{0}:{1}' -f $CorpusManifest, $OutputRoot)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-corpus\reports\selected-default-asr-model.json'
                    ConfigPath = $null
                    Applied = $false
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -SkipRecording `
                -SkipApply `
                -AllowMissingCloudBaseline `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 1
            $script:realMicWorkflowCalls[0] | Should Be ('default:{0}:{1}' -f (Join-Path $corpusRoot 'corpus.json'), $reportsRoot)
            $result.RecorderResult | Should Be $null
            $result.CorpusManifest | Should Be (Join-Path $corpusRoot 'corpus.json')
            $result.Applied | Should Be $false
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can stop after real microphone corpus recording for staged operator runs' {
        $script:realMicWorkflowCalls = @()
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [switch]$ResumeExisting,
                    [switch]$PlanOnly
                )
                if ($PlanOnly) {
                    $script:realMicWorkflowCalls += 'record-plan'
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                    }
                }

                $script:realMicWorkflowCalls += 'record-run'
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'Invoke-TalkAsrDefaultModelWorkflow should not be called when RecordOnly is set'
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -RecordOnly `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 2
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run'
            $result.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-result'
            $result.RecordOnly | Should Be $true
            $result.CorpusManifest | Should Be 'C:\talk-corpus\corpus.json'
            $result.DefaultModelWorkflowResult | Should Be $null
            $result.SelectionJson | Should Be $null
            $result.Applied | Should Be $false
            $result.FaithfulCorrectionReplaySkippedReason | Should Be 'record-only mode skips faithful correction replay'
            $result.ProviderCorrectionReplaySkippedReason | Should Be 'record-only mode skips provider correction replay'
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'skips provider correction replay when the workflow has no reusable Talk config path' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-replay-no-config-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        $script:realMicWorkflowCalls = @()
        try {
            New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
            New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'Invoke-TalkAsrCorpusRecorder should not be called when SkipRecording is set'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot)
                $script:realMicWorkflowCalls += ('default:{0}:{1}' -f $CorpusManifest, $OutputRoot)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-corpus\reports\selected-default-asr-model.json'
                    Selection = [pscustomobject]@{
                        selectedModelId = 'paraformer-bilingual-zh-en'
                    }
                    ConfigPath = $null
                    Applied = $false
                }
            }
            Mock Invoke-TalkFaithfulCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$OutputJson,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('faithful:{0}:{1}:{2}:{3}:{4}' -f $CorpusManifest, $ReportsRoot, $ModelId, $TalkExe, $OutputJson)
                [pscustomobject]@{
                    WorkflowKind = 'talk-faithful-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    OutputJson = $OutputJson
                    SampleCount = 1
                    AcceptedCount = 1
                    RejectedCount = 0
                    AllAccepted = $true
                    Samples = @()
                }
            }
            Mock Invoke-TalkProviderCorrectionReplay {
                throw 'Invoke-TalkProviderCorrectionReplay should not be called when ConfigPath is unavailable'
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -TalkExe 'C:\talk-release\Talk.exe' `
                -SkipRecording `
                -SkipApply `
                -AllowMissingCloudBaseline `
                -PassThru

            $expectedCliTalkExe = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $scriptRoot -Parent) 'target\release\talk.exe'))
            $script:realMicWorkflowCalls.Count | Should Be 2
            $script:realMicWorkflowCalls[0] | Should Be ('default:{0}:{1}' -f (Join-Path $corpusRoot 'corpus.json'), $reportsRoot)
            $script:realMicWorkflowCalls[1] | Should Be ('faithful:{0}:{1}:paraformer-bilingual-zh-en:{2}:{3}' -f (Join-Path $corpusRoot 'corpus.json'), $reportsRoot, $expectedCliTalkExe, (Join-Path $corpusRoot 'faithful-correction-replay-paraformer-bilingual-zh-en.json'))
            $result.FaithfulCorrectionReplayResult.TalkExe | Should Be $expectedCliTalkExe
            $result.FaithfulCorrectionReplayResult.OutputJson | Should Be (Join-Path $corpusRoot 'faithful-correction-replay-paraformer-bilingual-zh-en.json')
            [string]::IsNullOrWhiteSpace([string]$result.FaithfulCorrectionReplaySkippedReason) | Should Be $true
            $result.ProviderCorrectionReplayLiveResult | Should Be $null
            $result.ProviderCorrectionReplayDeterministicResult | Should Be $null
            $result.ProviderCorrectionReplaySkippedReason | Should Match 'config'
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'falls back to the repo CLI talk.exe when TalkExe points at the packaged desktop shell' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-replay-cli-fallback-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        $productTalkCmd = Join-Path $tempRoot 'Talk.cmd'
        $workflowConfigPath = Join-Path $tempRoot 'talk-r17-workflow.toml'
        $script:realMicWorkflowCalls = @()
        try {
            New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
            New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            @'
{
  "schemaVersion": 1,
  "workflowKind": "talk-asr-real-mic-default-model-record-only-status",
  "ready": true,
  "corpusManifest": "REPLACED_AT_RUNTIME",
  "sampleCount": 1,
  "recordingCount": 1,
  "audioFileCount": 1,
  "missingAudioWav": [],
  "validationErrors": []
}
'@.Replace('REPLACED_AT_RUNTIME', ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')).Replace('\', '\\'))) |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            @"
@echo off
echo Talk OpenLess/Typeless-style Windows desktop shell
echo.
echo Usage: Talk.exe [OPTIONS]
"@ | Set-Content -LiteralPath $productTalkCmd -Encoding ASCII
            Set-Content -LiteralPath $workflowConfigPath -Value '[provider]' -Encoding UTF8

            $expectedCliTalkExe = [System.IO.Path]::GetFullPath((Join-Path (Split-Path $scriptRoot -Parent) 'target\release\talk.exe'))

            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'Invoke-TalkAsrCorpusRecorder should not be called when SkipRecording is set'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CorpusManifest, [string]$OutputRoot)
                $script:realMicWorkflowCalls += ('default:{0}:{1}' -f $CorpusManifest, $OutputRoot)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-corpus\reports\selected-default-asr-model.json'
                    Selection = [pscustomobject]@{
                        selectedModelId = 'paraformer-bilingual-zh-en'
                    }
                    ConfigPath = $workflowConfigPath
                    Applied = $false
                }
            }
            Mock Invoke-TalkFaithfulCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$OutputJson,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('faithful:{0}:{1}:{2}' -f $ModelId, $TalkExe, $OutputJson)
                [pscustomobject]@{
                    WorkflowKind = 'talk-faithful-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    OutputJson = $OutputJson
                    SampleCount = 1
                    AcceptedCount = 1
                    RejectedCount = 0
                    AllAccepted = $true
                    Samples = @()
                }
            }
            Mock Invoke-TalkProviderCorrectionReplay {
                param(
                    [string]$CorpusManifest,
                    [string]$ReportsRoot,
                    [string]$ModelId,
                    [string]$TalkExe,
                    [string]$ConfigPath,
                    [string]$OutputJson,
                    [switch]$UseExistingReplayAsFixture,
                    [switch]$PassThru
                )
                $script:realMicWorkflowCalls += ('replay:{0}:{1}:{2}:{3}' -f $ModelId, $TalkExe, $ConfigPath, [bool]$UseExistingReplayAsFixture)
                [pscustomobject]@{
                    WorkflowKind = 'talk-provider-correction-replay-result'
                    CorpusManifest = $CorpusManifest
                    ReportsRoot = $ReportsRoot
                    ModelId = $ModelId
                    TalkExe = $TalkExe
                    ConfigPath = $ConfigPath
                    OutputJson = $OutputJson
                    UseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
                    SampleCount = 1
                    ExactMatchCount = 1
                    ImprovedCount = 1
                    MeanLocalCer = 0.25
                    MeanProcessedCer = 0.0
                    Samples = @()
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -TalkExe $productTalkCmd `
                -ConfigPath $workflowConfigPath `
                -SkipRecording `
                -SkipApply `
                -AllowMissingCloudBaseline `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 4
            $script:realMicWorkflowCalls[0] | Should Be ('default:{0}:{1}' -f (Join-Path $corpusRoot 'corpus.json'), $reportsRoot)
            $script:realMicWorkflowCalls[1] | Should Be ('faithful:paraformer-bilingual-zh-en:{0}:{1}' -f $expectedCliTalkExe, (Join-Path $corpusRoot 'faithful-correction-replay-paraformer-bilingual-zh-en.json'))
            $script:realMicWorkflowCalls[2] | Should Be ('replay:paraformer-bilingual-zh-en:{0}:{1}:False' -f $expectedCliTalkExe, $workflowConfigPath)
            $script:realMicWorkflowCalls[3] | Should Be ('replay:paraformer-bilingual-zh-en:{0}:{1}:True' -f $expectedCliTalkExe, $workflowConfigPath)
            $result.FaithfulCorrectionReplayResult.TalkExe | Should Be $expectedCliTalkExe
            $result.ProviderCorrectionReplayLiveResult.TalkExe | Should Be $expectedCliTalkExe
            $result.ProviderCorrectionReplayDeterministicResult.TalkExe | Should Be $expectedCliTalkExe
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can pass through resume-existing recording when refreshing only missing real microphone samples' {
        $script:realMicWorkflowCalls = @()
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string[]]$ForceRecordSampleId,
                    [switch]$ResumeExisting,
                    [switch]$PlanOnly
                )
                if ($PlanOnly) {
                    $forced = if ($null -eq $ForceRecordSampleId) { '' } else { (@($ForceRecordSampleId) -join ',') }
                    $script:realMicWorkflowCalls += ('record-plan-resume:{0}:force:{1}' -f [bool]$ResumeExisting, $forced)
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @([pscustomobject]@{
                            SampleId = 'mixed-english-japanese-001'
                            ReferenceText = '打开 Talk の local first ASR test'
                            WillRecord = $true
                        })
                    }
                }

                $forced = if ($null -eq $ForceRecordSampleId) { '' } else { (@($ForceRecordSampleId) -join ',') }
                $script:realMicWorkflowCalls += ('record-run-resume:{0}:force:{1}' -f [bool]$ResumeExisting, $forced)
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @([pscustomobject]@{
                        SampleId = 'mixed-english-japanese-001'
                        Reused = $false
                    })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'Invoke-TalkAsrDefaultModelWorkflow should not be called when RecordOnly is set'
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -RecordOnly `
                -ResumeExistingCorpus `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 2
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan-resume:True:force:'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run-resume:True:force:'
            $result.RecordOnly | Should Be $true
            $result.CorpusManifest | Should Be 'C:\talk-corpus\corpus.json'
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'can pass through forced rerecord sample ids when refreshing only semantic-drifted real microphone samples' {
        $script:realMicWorkflowCalls = @()
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string[]]$ForceRecordSampleId,
                    [switch]$ResumeExisting,
                    [switch]$PlanOnly
                )
                $forced = if ($null -eq $ForceRecordSampleId) { '' } else { (@($ForceRecordSampleId) -join ',') }
                if ($PlanOnly) {
                    $script:realMicWorkflowCalls += ('record-plan-resume:{0}:force:{1}' -f [bool]$ResumeExisting, $forced)
                    return [pscustomobject]@{
                        PromptManifest = $PromptManifest
                        OutputRoot = $OutputRoot
                        CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                        Samples = @(
                            [pscustomobject]@{
                                SampleId = 'short-search-001'
                                ReferenceText = '你好呀'
                                WillRecord = $false
                            },
                            [pscustomobject]@{
                                SampleId = 'mixed-english-japanese-001'
                                ReferenceText = '打开 Talk の local first ASR test'
                                WillRecord = $true
                            }
                        )
                    }
                }

                $script:realMicWorkflowCalls += ('record-run-resume:{0}:force:{1}' -f [bool]$ResumeExisting, $forced)
                [pscustomobject]@{
                    CorpusManifestPath = 'C:\talk-corpus\corpus.json'
                    Recordings = @(
                        [pscustomobject]@{
                            SampleId = 'short-search-001'
                            Reused = $true
                        },
                        [pscustomobject]@{
                            SampleId = 'mixed-english-japanese-001'
                            Reused = $false
                        }
                    )
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'Invoke-TalkAsrDefaultModelWorkflow should not be called when RecordOnly is set'
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -RecordOnly `
                -ResumeExistingCorpus `
                -ForceRecordSampleId 'mixed-english-japanese-001' `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 2
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan-resume:True:force:mixed-english-japanese-001'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run-resume:True:force:mixed-english-japanese-001'
            $result.RecordOnly | Should Be $true
            $result.CorpusManifest | Should Be 'C:\talk-corpus\corpus.json'
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'writes a record-only corpus readiness status after staged recording' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-record-status-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $promptManifest = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
            Set-Content -LiteralPath $promptManifest -Encoding UTF8
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [switch]$PlanOnly
                )

                $resolvedOutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                $corpusManifestPath = Join-Path $resolvedOutputRoot 'corpus.json'
                if ($PlanOnly) {
                    return [pscustomobject]@{
                        PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                        OutputRoot = $resolvedOutputRoot
                        CorpusManifestPath = $corpusManifestPath
                        Samples = @([pscustomobject]@{
                            SampleId = 'short-search-001'
                            ReferenceText = '你好呀'
                            AudioWav = (Join-Path $resolvedOutputRoot 'short-search-001-16k-mono-s16.wav')
                        })
                    }
                }

                New-Item -ItemType Directory -Path $resolvedOutputRoot -Force | Out-Null
                Set-Content -LiteralPath (Join-Path $resolvedOutputRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
                '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                    Set-Content -LiteralPath $corpusManifestPath -Encoding UTF8

                [pscustomobject]@{
                    CorpusManifestPath = $corpusManifestPath
                    Recordings = @([pscustomobject]@{
                        SampleId = 'short-search-001'
                        AudioWav = (Join-Path $resolvedOutputRoot 'short-search-001-16k-mono-s16.wav')
                        CapturedInputDevice = '麦克风'
                        AvailableInputDevices = @('麦克风', 'Virtual Mic')
                    })
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'Invoke-TalkAsrDefaultModelWorkflow should not be called when RecordOnly is set'
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptManifest `
                -CorpusRoot $corpusRoot `
                -RecordOnly `
                -PassThru

            $expectedStatusPath = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'record-only-status.json'))
            $result.RecordOnlyStatusJson | Should Be $expectedStatusPath
            Test-Path -LiteralPath $expectedStatusPath -PathType Leaf | Should Be $true
            $status = Get-Content -LiteralPath $expectedStatusPath -Raw -Encoding UTF8 | ConvertFrom-Json
            $status.workflowKind | Should Be 'talk-asr-real-mic-default-model-record-only-status'
            $status.ready | Should Be $true
            $status.corpusManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $status.sampleCount | Should Be 1
            $status.recordingCount | Should Be 1
            $status.audioFileCount | Should Be 1
            $status.configuredInputDevice | Should Be $null
            (@($status.capturedInputDevices) -join '|') | Should Be '麦克风'
            (@($status.availableInputDevices) -join '|') | Should Be '麦克风|Virtual Mic'
            [string]$status.inputDeviceSelectionWarning | Should Match 'multiple input devices'
            [string]$status.inputDeviceSelectionWarning | Should Match 'Virtual Mic'
            $status.missingAudioWav.Count | Should Be 0
            $status.nextCommand | Should Match '-SkipRecording'
            $status.nextCommand | Should Match '-CorpusRoot'
            $status.nextCommand | Should Match 'corpus'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects record-only when recording is explicitly skipped' {
        Mock Invoke-TalkAsrCorpusRecorder {
            throw 'Invoke-TalkAsrCorpusRecorder should not be called for invalid RecordOnly and SkipRecording arguments'
        }
        Mock Invoke-TalkAsrDefaultModelWorkflow {
            throw 'Invoke-TalkAsrDefaultModelWorkflow should not be called for invalid RecordOnly and SkipRecording arguments'
        }

        {
            Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -CorpusRoot 'C:\talk-corpus' `
                -RecordOnly `
                -SkipRecording `
                -PassThru
        } | Should Throw 'RecordOnly cannot be combined with SkipRecording'
    }

    It 'preflight record-only checks recording prerequisites without requiring models or cloud baseline' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-record-only-preflight-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_RECORD_ONLY_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_RECORD_ONLY_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk.exe') -Value 'talk' -Encoding ASCII

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot (Join-Path $tempRoot 'corpus') `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot (Join-Path $tempRoot 'missing-models') `
                -AsrBenchExe (Join-Path $tempRoot 'missing-asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'missing-daemon.exe') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_RECORD_ONLY_KEY' `
                -RecordOnly `
                -PreflightOnly

            $preflight.Ready | Should Be $true
            $preflight.BlockingCheckCount | Should Be 0
            @($preflight.Checks | Where-Object { $_.Name -eq 'prompt_manifest' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'talk_probe_exe' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'asr_bench_exe' -and $_.Status -eq 'skipped' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' -and $_.Status -eq 'skipped' }).Count | Should Be 1
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_RECORD_ONLY_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_RECORD_ONLY_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks record-only when multiple input devices are available without an explicit InputDevice' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-record-only-device-choice-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [string]$InputDevice,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    InputDevice = $InputDevice
                    ConfigPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'recording-config.toml'))
                    CaptureTempDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot '.captures'))
                    LogsDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'logs'))
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    MaxRecordingSeconds = 3
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            $readinessInvoker = {
                param([string]$TalkExe, [string]$ConfigPath)
                [pscustomobject]@{
                    audio = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            requestedDeviceName = $null
                            deviceName = '麦克风'
                            availableDeviceNames = @('麦克风', 'Virtual Mic')
                        }
                    }
                    clipboard = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            reason = $null
                        }
                    }
                }
            }
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk.exe') -Value 'talk' -Encoding ASCII

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot (Join-Path $tempRoot 'corpus') `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot (Join-Path $tempRoot 'missing-models') `
                -AsrBenchExe (Join-Path $tempRoot 'missing-asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'missing-daemon.exe') `
                -RecordOnly `
                -PreflightOnly `
                -ReadinessInvoker $readinessInvoker

            $preflight.Ready | Should Be $false
            $deviceCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'input_device_selection' })[0]
            $deviceCheck.Status | Should Be 'failed'
            $deviceCheck.Message | Should Match 'multiple input devices'
            $deviceCheck.Message | Should Match 'Virtual Mic'
            $deviceCheck.RemediationCommand | Should Match '-InputDevice'
            $deviceCheck.RemediationCommand | Should Match '麦克风'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can probe microphone signal during record-only preflight without requiring benchmark prerequisites' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-record-only-probe-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $script:recordOnlyProbeConfigPath = $null
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [string]$InputDevice,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    InputDevice = $InputDevice
                    ConfigPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'recording-config.toml'))
                    CaptureTempDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot '.captures'))
                    LogsDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'logs'))
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    MaxRecordingSeconds = 3
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            $audioProbeInvoker = {
                param([string]$TalkExe, [string]$ConfigPath, [int]$Seconds)
                $script:recordOnlyProbeConfigPath = $ConfigPath
                [pscustomobject]@{
                    ExitCode = 0
                    Stdout = '{"audio":{"nativeWindows":{"status":"ready","deviceName":"麦克风"},"signal":{"silent":false,"peak":0.05,"rms":0.01,"artifactPath":"record-only-probe.wav"}}}'
                    Stderr = ''
                }
            }
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk.exe') -Value 'talk' -Encoding ASCII

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot (Join-Path $tempRoot 'corpus') `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot (Join-Path $tempRoot 'missing-models') `
                -AsrBenchExe (Join-Path $tempRoot 'missing-asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'missing-daemon.exe') `
                -RecordOnly `
                -PreflightOnly `
                -ProbeAudio `
                -AudioProbeInvoker $audioProbeInvoker

            $preflight.Ready | Should Be $true
            @($preflight.Checks | Where-Object { $_.Name -eq 'asr_bench_exe' -and $_.Status -eq 'skipped' }).Count | Should Be 1
            $probeCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'microphone_signal' })[0]
            $probeCheck.Status | Should Be 'ready'
            $script:recordOnlyProbeConfigPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $tempRoot 'corpus\recording-config.toml')))
        }
        finally {
            Remove-Variable -Name recordOnlyProbeConfigPath -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'stops record-only recording before capture when multiple input devices are available without an explicit InputDevice' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-record-only-device-run-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $script:realMicRecordOnlyCalls = @()
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [string]$InputDevice,
                    [switch]$PlanOnly
                )
                $script:realMicRecordOnlyCalls += ('record:{0}:input:{1}' -f [bool]$PlanOnly, [string]$InputDevice)
                if ($PlanOnly) {
                    [pscustomobject]@{
                        PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                        OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                        TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                        InputDevice = $InputDevice
                        ConfigPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'recording-config.toml'))
                        CaptureTempDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot '.captures'))
                        LogsDir = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'logs'))
                        CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                        MaxRecordingSeconds = 3
                        Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                    }
                } else {
                    throw 'recording should be blocked before capture starts'
                }
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'benchmarking should not start when input device selection is ambiguous'
            }
            $readinessInvoker = {
                param([string]$TalkExe, [string]$ConfigPath)
                [pscustomobject]@{
                    audio = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            requestedDeviceName = $null
                            deviceName = '麦克风'
                            availableDeviceNames = @('麦克风', 'Virtual Mic')
                        }
                    }
                    clipboard = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            reason = $null
                        }
                    }
                }
            }
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk.exe') -Value 'talk' -Encoding ASCII

            {
                Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -PromptManifest $promptPath `
                    -CorpusRoot (Join-Path $tempRoot 'corpus') `
                    -TalkExe (Join-Path $tempRoot 'talk.exe') `
                    -RecordOnly `
                    -PassThru `
                    -ReadinessInvoker $readinessInvoker
            } | Should Throw 'multiple input devices'

            $script:realMicRecordOnlyCalls.Count | Should Be 1
            $script:realMicRecordOnlyCalls[0] | Should Be 'record:True:input:'
        }
        finally {
            Remove-Variable -Name realMicRecordOnlyCalls -Scope Script -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks SkipRecording when record-only status warns about ambiguous default input-device capture' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-status-ready-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $modelRoot = Join-Path $tempRoot 'models'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY
        try {
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                configuredInputDevice = $null
                capturedInputDevices = @('麦克风')
                availableInputDevices = @('麦克风', 'Virtual Mic')
                inputDeviceSelectionWarning = 'recorded with default input device [麦克风] while multiple input devices are available: 麦克风, Virtual Mic'
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 1
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            foreach ($leaf in @('asr-bench.exe', 'talk-local-asr-sherpa.exe', 'talk-desktop.toml')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            $env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY = 'test-key'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SKIP_STATUS_KEY' `
                -SkipRecording `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $recordOnlyStatusCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'record_only_status' })[0]
            $recordOnlyStatusCheck.Status | Should Be 'failed'
            $recordOnlyStatusCheck.Message | Should Match 'not authoritative'
            $recordOnlyStatusCheck.Message | Should Match 'multiple input devices'
            $recordOnlyStatusCheck.Message | Should Match 'Virtual Mic'
            $recordOnlyStatusCheck.Path | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'record-only-status.json')))
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-RecordOnly'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-ResumeExistingCorpus'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-InputDevice'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '麦克风'
            $recordOnlyStatusCheck.RemediationCommand | Should Match 'short-search-001'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks SkipRecording when record-only status predates input-device diagnostics and readiness shows multiple devices' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-status-legacy-device-diag-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $modelRoot = Join-Path $tempRoot 'models'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        $talkExe = Join-Path $tempRoot 'talk.exe'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SKIP_STATUS_LEGACY_DEVICE_KEY
        try {
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 1
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            foreach ($leaf in @('asr-bench.exe', 'talk-local-asr-sherpa.exe', 'talk-desktop.toml')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            Set-Content -LiteralPath $talkExe -Value 'talk' -Encoding ASCII
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            $env:TALK_TEST_REAL_MIC_SKIP_STATUS_LEGACY_DEVICE_KEY = 'test-key'
            $readinessInvoker = {
                param($TalkExePath, $ConfigPath)

                [pscustomobject]@{
                    audio = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            reason = $null
                            requestedDeviceName = $null
                            deviceName = '麦克风'
                            availableDeviceNames = @('麦克风', 'Virtual Mic')
                        }
                    }
                    clipboard = [pscustomobject]@{
                        nativeWindows = [pscustomobject]@{
                            status = 'ready'
                            reason = $null
                        }
                    }
                }
            }

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe $talkExe `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SKIP_STATUS_LEGACY_DEVICE_KEY' `
                -SkipRecording `
                -ReadinessInvoker $readinessInvoker `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $recordOnlyStatusCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'record_only_status' })[0]
            $recordOnlyStatusCheck.Status | Should Be 'failed'
            $recordOnlyStatusCheck.Message | Should Match 'predates input-device diagnostics'
            $recordOnlyStatusCheck.Message | Should Match 'Virtual Mic'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-RecordOnly'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-ResumeExistingCorpus'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '-InputDevice'
            $recordOnlyStatusCheck.RemediationCommand | Should Match '麦克风'
            $recordOnlyStatusCheck.RemediationCommand | Should Match 'short-search-001'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_SKIP_STATUS_LEGACY_DEVICE_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_SKIP_STATUS_LEGACY_DEVICE_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks SkipRecording when the existing corpus misses current prompt-manifest samples' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-prompt-mismatch-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $modelRoot = Join-Path $tempRoot 'models'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SKIP_MISMATCH_KEY
        try {
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 1
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            foreach ($leaf in @('asr-bench.exe', 'talk-local-asr-sherpa.exe', 'talk-desktop.toml')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            $env:TALK_TEST_REAL_MIC_SKIP_MISMATCH_KEY = 'test-key'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SKIP_MISMATCH_KEY' `
                -SkipRecording `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $alignmentCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'corpus_prompt_alignment' })[0]
            $alignmentCheck.Status | Should Be 'failed'
            $alignmentCheck.Message | Should Match 'does not match current prompt manifest'
            $alignmentCheck.Message | Should Match 'mixed-english-japanese-001'
            $alignmentCheck.RemediationCommand | Should Match '-RecordOnly'
            $alignmentCheck.RemediationCommand | Should Match '-ResumeExistingCorpus'
            $alignmentCheck.RemediationCommand | Should Match ([regex]::Escape($promptPath))
            $alignmentCheck.RemediationCommand | Should Match ([regex]::Escape($corpusRoot))
            @($preflight.Checks | Where-Object { $_.Name -eq 'record_only_status' -and $_.Status -eq 'ready' }).Count | Should Be 1
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_SKIP_MISMATCH_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_SKIP_MISMATCH_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks an invalid record-only status before reusing a staged corpus' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-status-invalid-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        try {
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $false
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 0
                missingAudioWav = @('short-search-001-16k-mono-s16.wav')
                validationErrors = @('missing audio')
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -CorpusRoot $corpusRoot `
                -AsrBenchExe (Join-Path $tempRoot 'missing-asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'missing-daemon.exe') `
                -ModelRoot (Join-Path $tempRoot 'missing-models') `
                -ConfigPath (Join-Path $tempRoot 'missing-config.toml') `
                -AllowMissingCloudBaseline `
                -SkipRecording `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $recordOnlyStatusCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'record_only_status' })[0]
            $recordOnlyStatusCheck.Status | Should Be 'failed'
            $recordOnlyStatusCheck.Message | Should Match 'not ready'
            $recordOnlyStatusCheck.Message | Should Match 'missing audio'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'stops a full skip-recording run before benchmarking when record-only status is invalid' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-status-run-invalid-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'recording should be skipped in this test'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'benchmarking should not start when record-only status is invalid'
            }
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $false
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 0
                missingAudioWav = @('short-search-001-16k-mono-s16.wav')
                validationErrors = @('missing audio')
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8

            {
                Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -CorpusRoot $corpusRoot `
                    -SkipRecording `
                    -SkipApply `
                    -AllowMissingCloudBaseline `
                    -PassThru
            } | Should Throw 'Record-only status is not ready for SkipRecording'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'stops a full SkipRecording run before benchmarking when the corpus no longer matches the current prompt manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-prompt-mismatch-run-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'recording should be skipped in this test'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'benchmarking should not start when prompt/corpus alignment fails'
            }
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-japanese-001", "referenceText": "打开 Talk の local first ASR test" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 1
                recordingCount = 1
                audioFileCount = 1
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8

            try {
                Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -PromptManifest $promptPath `
                    -CorpusRoot $corpusRoot `
                    -SkipRecording `
                    -SkipApply `
                    -AllowMissingCloudBaseline `
                    -PassThru
                throw 'expected prompt/corpus mismatch to throw'
            }
            catch {
                $_.Exception.Message | Should Match 'existing corpus manifest does not match current prompt manifest'
                $_.Exception.Message | Should Match '-RecordOnly'
                $_.Exception.Message | Should Match '-ResumeExistingCorpus'
                $_.Exception.Message | Should Match 'mixed-english-japanese-001'
                $expectedPromptSubsetPath = Join-Path $corpusRoot 'prompts-rerecord-corpus-prompt-alignment.json'
                $_.Exception.Message | Should Match ([regex]::Escape($expectedPromptSubsetPath))
                (Test-Path -LiteralPath $expectedPromptSubsetPath -PathType Leaf) | Should Be $true
                $promptSubset = Get-Content -LiteralPath $expectedPromptSubsetPath -Raw -Encoding UTF8 | ConvertFrom-Json
                @($promptSubset.samples).Count | Should Be 1
                [string]$promptSubset.samples[0].sampleId | Should Be 'mixed-english-japanese-001'
            }
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight blocks SkipRecording when existing cloud baseline reports imply semantic drift from the prompt references' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-semantic-mismatch-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $modelRoot = Join-Path $tempRoot 'models'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $corpusRoot, $reportsRoot, $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SKIP_SEMANTIC_KEY
        try {
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-001", "referenceText": "打开 Talk 的 local first ASR 测试" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $corpusRoot 'mixed-english-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            $corpusManifestPath = Join-Path $corpusRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-001", "audioWav": "mixed-english-001-16k-mono-s16.wav", "referenceText": "打开 Talk 的 local first ASR 测试" }
  ]
}
'@ | Set-Content -LiteralPath $corpusManifestPath -Encoding UTF8
            $corpusManifestSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $corpusManifestPath).Hash.ToLowerInvariant()
            $shortAudioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav')).Hash.ToLowerInvariant()
            $mixedAudioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $corpusRoot 'mixed-english-001-16k-mono-s16.wav')).Hash.ToLowerInvariant()
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 2
                recordingCount = 2
                audioFileCount = 2
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            @"
{
  "engine": "cloud_openai_compatible:chat_completions_audio_input:qwen3-asr-flash",
  "sample_id": "short-search-001",
  "corpus_manifest_sha256": "$corpusManifestSha256",
  "audio_sha256": "$shortAudioSha256",
  "text": "世界杯决赛",
  "cer": 1.0
}
"@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-chat_completions_audio_input-short-search-001.json') -Encoding UTF8
            @"
{
  "engine": "cloud_openai_compatible:chat_completions_audio_input:qwen3-asr-flash",
  "sample_id": "mixed-english-001",
  "corpus_manifest_sha256": "$corpusManifestSha256",
  "audio_sha256": "$mixedAudioSha256",
  "text": "基本上就是完全掌握，嗯，前掌打门，打的有点正。",
  "cer": 1.0
}
"@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-chat_completions_audio_input-mixed-english-001.json') -Encoding UTF8
            foreach ($leaf in @('asr-bench.exe', 'talk-local-asr-sherpa.exe', 'talk-desktop.toml')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            $env:TALK_TEST_REAL_MIC_SKIP_SEMANTIC_KEY = 'test-key'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SKIP_SEMANTIC_KEY' `
                -SkipRecording `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $semanticCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'corpus_semantic_validity' })[0]
            $semanticCheck.Status | Should Be 'failed'
            $semanticCheck.Message | Should Match 'meanCloudBaselineCer'
            $semanticCheck.Message | Should Match 'short-search-001'
            $semanticCheck.RemediationCommand | Should Match '-RecordOnly'
            $semanticCheck.RemediationCommand | Should Match '-ResumeExistingCorpus'
            $semanticCheck.RemediationCommand | Should Match '-ForceRecordSampleId'
            $semanticCheck.RemediationCommand | Should Match 'short-search-001'
            $semanticCheck.RemediationCommand | Should Match 'mixed-english-001'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_SKIP_SEMANTIC_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_SKIP_SEMANTIC_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'stops a full SkipRecording run before benchmarking when existing cloud baseline reports imply semantic drift' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-semantic-run-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $corpusRoot, $reportsRoot, $tempRoot -Force | Out-Null
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'recording should be skipped in this test'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                throw 'benchmarking should not start when semantic validity fails'
            }
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-001", "referenceText": "打开 Talk 的 local first ASR 测试" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $corpusRoot 'mixed-english-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            $corpusManifestPath = Join-Path $corpusRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "referenceText": "你好呀" },
    { "sampleId": "mixed-english-001", "audioWav": "mixed-english-001-16k-mono-s16.wav", "referenceText": "打开 Talk 的 local first ASR 测试" }
  ]
}
'@ | Set-Content -LiteralPath $corpusManifestPath -Encoding UTF8
            $corpusManifestSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $corpusManifestPath).Hash.ToLowerInvariant()
            $shortAudioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav')).Hash.ToLowerInvariant()
            $mixedAudioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $corpusRoot 'mixed-english-001-16k-mono-s16.wav')).Hash.ToLowerInvariant()
            $status = [ordered]@{
                schemaVersion = 1
                workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
                ready = $true
                corpusManifest = [System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json'))
                sampleCount = 2
                recordingCount = 2
                audioFileCount = 2
                missingAudioWav = @()
                validationErrors = @()
            }
            $status | ConvertTo-Json -Depth 6 |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'record-only-status.json') -Encoding UTF8
            @"
{
  "engine": "cloud_openai_compatible:chat_completions_audio_input:qwen3-asr-flash",
  "sample_id": "short-search-001",
  "corpus_manifest_sha256": "$corpusManifestSha256",
  "audio_sha256": "$shortAudioSha256",
  "text": "世界杯决赛",
  "cer": 1.0
}
"@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-chat_completions_audio_input-short-search-001.json') -Encoding UTF8
            @"
{
  "engine": "cloud_openai_compatible:chat_completions_audio_input:qwen3-asr-flash",
  "sample_id": "mixed-english-001",
  "corpus_manifest_sha256": "$corpusManifestSha256",
  "audio_sha256": "$mixedAudioSha256",
  "text": "基本上就是完全掌握，嗯，前掌打门，打的有点正。",
  "cer": 1.0
}
"@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-chat_completions_audio_input-mixed-english-001.json') -Encoding UTF8

            try {
                Invoke-TalkAsrRealMicDefaultModelWorkflow `
                    -PromptManifest $promptPath `
                    -CorpusRoot $corpusRoot `
                    -ReportsRoot $reportsRoot `
                    -SkipRecording `
                    -SkipApply `
                    -AllowMissingCloudBaseline `
                    -PassThru
                throw 'expected semantic validity failure to throw'
            }
            catch {
                $_.Exception.Message | Should Match 'cloud baseline reports suggest this staged corpus no longer matches the current prompt references'
                $_.Exception.Message | Should Match '-RecordOnly'
                $_.Exception.Message | Should Match '-ResumeExistingCorpus'
                $_.Exception.Message | Should Match '-ForceRecordSampleId'
                $_.Exception.Message | Should Match 'mixed-english-001'
                $_.Exception.Message | Should Match 'short-search-001'
                $expectedPromptSubsetPath = Join-Path $corpusRoot 'prompts-rerecord-semantic-drift.json'
                $_.Exception.Message | Should Match ([regex]::Escape($expectedPromptSubsetPath))
                (Test-Path -LiteralPath $expectedPromptSubsetPath -PathType Leaf) | Should Be $true
                $promptSubset = Get-Content -LiteralPath $expectedPromptSubsetPath -Raw -Encoding UTF8 | ConvertFrom-Json
                @($promptSubset.samples).Count | Should Be 2
                (@($promptSubset.samples | ForEach-Object { [string]$_.sampleId }) -join '|') | Should Be 'short-search-001|mixed-english-001'
            }
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'fails semantic validity when a cloud baseline report belongs to different corpus bytes' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-provenance-mismatch-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $audioPath = Join-Path $tempRoot 'sample.wav'
            Set-Content -LiteralPath $audioPath -Value 'wav' -Encoding ASCII
            $corpusManifest = Join-Path $tempRoot 'corpus.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"sample-001","audioWav":"sample.wav","referenceText":"hello"}]}' |
                Set-Content -LiteralPath $corpusManifest -Encoding UTF8
            $audioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $audioPath).Hash.ToLowerInvariant()
            @"
{
  "engine": "cloud_openai_compatible:chat_completions_audio_input:test",
  "sample_id": "sample-001",
  "corpus_manifest_sha256": "$('a' * 64)",
  "audio_sha256": "$audioSha256",
  "text": "hello",
  "cer": 0.0
}
"@ | Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-test-sample-001.json') -Encoding UTF8
            $plan = [pscustomobject]@{
                ReportsRoot = $reportsRoot
                CorpusManifest = $corpusManifest
            }
            $samples = @([pscustomobject]@{
                    SampleId = 'sample-001'
                    AudioWav = $audioPath
                    AudioSha256 = $audioSha256
                })

            $check = New-TalkAsrRealMicDefaultWorkflowCorpusSemanticValidityCheck -Plan $plan -CorpusSamples $samples

            $check.Status | Should Be 'failed'
            $check.Message | Should Match 'provenance does not match'
            $check.Message | Should Match 'sample-001'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'skips legacy cloud baseline reports that have no corpus or audio provenance' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-provenance-legacy-' + [guid]::NewGuid().ToString())
        $reportsRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $reportsRoot -Force | Out-Null
        try {
            $audioPath = Join-Path $tempRoot 'sample.wav'
            Set-Content -LiteralPath $audioPath -Value 'wav' -Encoding ASCII
            $corpusManifest = Join-Path $tempRoot 'corpus.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"sample-001","audioWav":"sample.wav","referenceText":"hello"}]}' |
                Set-Content -LiteralPath $corpusManifest -Encoding UTF8
            $audioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $audioPath).Hash.ToLowerInvariant()
            '{"engine":"cloud_openai_compatible:test","sample_id":"sample-001","text":"hello","cer":0.0}' |
                Set-Content -LiteralPath (Join-Path $reportsRoot 'cloud-openai-compatible-test-sample-001.json') -Encoding UTF8
            $plan = [pscustomobject]@{
                ReportsRoot = $reportsRoot
                CorpusManifest = $corpusManifest
            }
            $samples = @([pscustomobject]@{
                    SampleId = 'sample-001'
                    AudioWav = $audioPath
                    AudioSha256 = $audioSha256
                })

            $check = New-TalkAsrRealMicDefaultWorkflowCorpusSemanticValidityCheck -Plan $plan -CorpusSamples $samples

            $check.Status | Should Be 'skipped'
            $check.Message | Should Match 'legacy cloud baseline reports'
            $check.Message | Should Match 'sample-001'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflights all release-side prerequisites before recording or benchmarking' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [int]$DefaultCaptureSeconds,
                    [int]$CountdownSeconds,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001'; CaptureSeconds = $DefaultCaptureSeconds })
                    CountdownSeconds = $CountdownSeconds
                }
            }
            $env:TALK_TEST_REAL_MIC_KEY = 'test-key'
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe', 'talk-desktop.toml')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_KEY' `
                -PreflightOnly

            $preflight.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-preflight'
            $preflight.Ready | Should Be $true
            $preflight.BlockingCheckCount | Should Be 0
            @($preflight.Checks | Where-Object { $_.Name -eq 'prompt_manifest' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'talk_probe_exe' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'corpus_manifest' -and $_.Status -eq 'planned' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'model:sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'model:zipformer-zh-en-punct-int8-480ms' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'model:paraformer-bilingual-zh-en' -and $_.Status -eq 'ready' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' -and $_.Status -eq 'ready' }).Count | Should Be 1
            $preflight.RemediationCommands.Count | Should Be 0
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight reports blocking checks instead of starting the full workflow' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-missing-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_MISSING_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_MISSING_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk.exe') -Value 'talk' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $tempRoot 'talk-desktop.toml') -Value '[speculative.streaming_service]' -Encoding UTF8

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot (Join-Path $tempRoot 'corpus') `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot (Join-Path $tempRoot 'missing-models') `
                -AsrBenchExe (Join-Path $tempRoot 'missing-asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'missing-daemon.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_MISSING_KEY' `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $preflight.BlockingCheckCount | Should Not BeLessThan 4
            @($preflight.Checks | Where-Object { $_.Name -eq 'asr_bench_exe' -and $_.Status -eq 'missing' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'local_asr_daemon_exe' -and $_.Status -eq 'missing' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'model_root' -and $_.Status -eq 'missing' }).Count | Should Be 1
            @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' -and $_.Status -eq 'missing' }).Count | Should Be 1

            $resolvedModelRoot = [System.IO.Path]::GetFullPath((Join-Path $tempRoot 'missing-models'))
            $expectedMultilingualInstall = ".\Install-TalkSherpaModel.ps1 -ModelId sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10 -DestinationRoot '$resolvedModelRoot'"
            $expectedZipformerInstall = ".\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms -DestinationRoot '$resolvedModelRoot'"
            $expectedParaformerInstall = ".\Install-TalkSherpaModel.ps1 -ModelId paraformer-bilingual-zh-en -DestinationRoot '$resolvedModelRoot'"
            $expectedApiKeyCommand = '$env:TALK_TEST_REAL_MIC_MISSING_KEY = ''<redacted>'''
            $multilingualCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'model:sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' })[0]
            $zipformerCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'model:zipformer-zh-en-punct-int8-480ms' })[0]
            $paraformerCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'model:paraformer-bilingual-zh-en' })[0]
            $cloudKeyCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' })[0]
            $multilingualCheck.RemediationCommand | Should Be $expectedMultilingualInstall
            $zipformerCheck.RemediationCommand | Should Be $expectedZipformerInstall
            $paraformerCheck.RemediationCommand | Should Be $expectedParaformerInstall
            $cloudKeyCheck.RemediationCommand | Should Be $expectedApiKeyCommand
            ($preflight.RemediationCommands -contains $expectedMultilingualInstall) | Should Be $true
            ($preflight.RemediationCommands -contains $expectedZipformerInstall) | Should Be $true
            ($preflight.RemediationCommands -contains $expectedParaformerInstall) | Should Be $true
            ($preflight.RemediationCommands -contains $expectedApiKeyCommand) | Should Be $true
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_MISSING_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_MISSING_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight accepts a packaged desktop provider api key as the cloud baseline key source' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-config-key-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_CONFIG_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_CONFIG_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
api_key = "packaged-test-key"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath $configPath `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_CONFIG_KEY' `
                -PreflightOnly

            $preflight.Ready | Should Be $true
            $preflight.BlockingCheckCount | Should Be 0
            $cloudKeyCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' })[0]
            $cloudKeyCheck.Status | Should Be 'ready'
            $cloudKeyCheck.Message | Should Be 'cloud baseline API key is available from desktop config provider api_key'
            [string]::IsNullOrWhiteSpace($cloudKeyCheck.RemediationCommand) | Should Be $true
            $json = $preflight | ConvertTo-Json -Depth 8
            ($json -match 'packaged-test-key') | Should Be $false
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_CONFIG_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_CONFIG_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preflight accepts the standard DashScope credential file as the cloud baseline key source' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-legacy-key-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_LEGACY_KEY
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record audio'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_LEGACY_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot
            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"legacy-json-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -CloudOpenAiCompatibleEndpoint 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen3-asr-flash' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_LEGACY_KEY' `
                -PreflightOnly

            $preflight.Ready | Should Be $true
            $cloudKeyCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' })[0]
            $cloudKeyCheck.Status | Should Be 'ready'
            $cloudKeyCheck.Message | Should Be 'cloud baseline API key is available from the standard DashScope credential file'
            [string]::IsNullOrWhiteSpace($cloudKeyCheck.RemediationCommand) | Should Be $true
            $json = $preflight | ConvertTo-Json -Depth 8
            ($json -match 'legacy-json-key') | Should Be $false
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_LEGACY_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_LEGACY_KEY = $originalApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can include an optional real microphone signal probe in preflight' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-probe-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_PROBE_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record the full corpus'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_PROBE_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
api_key = "packaged-test-key"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $audioProbeInvoker = {
                param([string]$TalkExe, [string]$ConfigPath, [int]$Seconds)
                [pscustomobject]@{
                    ExitCode = 0
                    Stdout = '{"audio":{"nativeWindows":{"status":"ready","deviceName":"麦克风"},"signal":{"silent":false,"peak":0.12,"rms":0.02,"artifactPath":"probe.wav"}}}'
                    Stderr = ''
                }
            }

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath $configPath `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_PROBE_KEY' `
                -ProbeAudio `
                -AudioProbeSeconds 2 `
                -AudioProbeInvoker $audioProbeInvoker `
                -PreflightOnly

            $preflight.Ready | Should Be $true
            $probeCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'microphone_signal' })[0]
            $probeCheck.Status | Should Be 'ready'
            $probeCheck.Message | Should Match 'non-silent microphone signal'
            $probeCheck.Message | Should Match 'peak=0.12'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_PROBE_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_PROBE_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'blocks preflight when the optional microphone probe records silence' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-silent-probe-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SILENT_PROBE_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record the full corpus'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_SILENT_PROBE_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
api_key = "packaged-test-key"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $audioProbeInvoker = {
                param([string]$TalkExe, [string]$ConfigPath, [int]$Seconds)
                [pscustomobject]@{
                    ExitCode = 0
                    Stdout = '{"audio":{"nativeWindows":{"status":"ready","deviceName":"麦克风"},"signal":{"silent":true,"peak":0,"rms":0,"artifactPath":"probe.wav"}}}'
                    Stderr = ''
                }
            }

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath $configPath `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SILENT_PROBE_KEY' `
                -ProbeAudio `
                -AudioProbeSeconds 2 `
                -AudioProbeInvoker $audioProbeInvoker `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $probeCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'microphone_signal' })[0]
            $probeCheck.Status | Should Be 'failed'
            $probeCheck.Message | Should Match 'microphone probe recorded silence'
            $probeCheck.RemediationHint | Should Match 'microphone'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_SILENT_PROBE_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_SILENT_PROBE_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'blocks preflight when the optional microphone probe is too weak for provider transcription' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-preflight-weak-probe-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $corpusRoot = Join-Path $tempRoot 'corpus'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_WEAK_PROBE_KEY
        try {
            Mock Invoke-TalkAsrCorpusRecorder {
                param(
                    [string]$PromptManifest,
                    [string]$OutputRoot,
                    [string]$TalkExe,
                    [switch]$PlanOnly
                )
                if (-not $PlanOnly) {
                    throw 'preflight must not record the full corpus'
                }
                [pscustomobject]@{
                    PromptManifest = [System.IO.Path]::GetFullPath($PromptManifest)
                    OutputRoot = [System.IO.Path]::GetFullPath($OutputRoot)
                    TalkExe = [System.IO.Path]::GetFullPath($TalkExe)
                    CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $OutputRoot 'corpus.json'))
                    Samples = @([pscustomobject]@{ SampleId = 'short-search-001' })
                }
            }
            Remove-Item Env:TALK_TEST_REAL_MIC_WEAK_PROBE_KEY -ErrorAction SilentlyContinue
            $promptPath = Join-Path $tempRoot 'prompts.json'
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            foreach ($leaf in @('talk.exe', 'asr-bench.exe', 'talk-local-asr-sherpa.exe')) {
                Set-Content -LiteralPath (Join-Path $tempRoot $leaf) -Value $leaf -Encoding ASCII
            }
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
api_key = "packaged-test-key"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

            $audioProbeInvoker = {
                param([string]$TalkExe, [string]$ConfigPath, [int]$Seconds)
                [pscustomobject]@{
                    ExitCode = 0
                    Stdout = '{"audio":{"nativeWindows":{"status":"ready","deviceName":"麦克风"},"signal":{"durationSeconds":2,"silent":false,"peak":0.02,"rms":0.001,"artifactPath":"probe.wav"}}}'
                    Stderr = ''
                }
            }

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -TalkExe (Join-Path $tempRoot 'talk.exe') `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath $configPath `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_WEAK_PROBE_KEY' `
                -ProbeAudio `
                -AudioProbeSeconds 2 `
                -AudioProbeInvoker $audioProbeInvoker `
                -PreflightOnly

            $preflight.Ready | Should Be $false
            $probeCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'microphone_signal' })[0]
            $probeCheck.Status | Should Be 'failed'
            $probeCheck.Message | Should Match 'too weak for provider transcription'
            $probeCheck.Message | Should Match 'peak=0.02'
            $probeCheck.RemediationHint | Should Match 'microphone'
        }
        finally {
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_WEAK_PROBE_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_WEAK_PROBE_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'temporarily provides a packaged desktop provider api key to the default model workflow' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-run-config-key-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY
        try {
            Remove-Item Env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY -ErrorAction SilentlyContinue
            New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $configPath = Join-Path $tempRoot 'talk-desktop.toml'
            @'
[provider]
kind = "openai_compatible"
api_key = "packaged-run-key"
'@ | Set-Content -LiteralPath $configPath -Encoding UTF8
            $script:realMicWorkflowObservedKey = $null
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'recording should be skipped in this test'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CloudOpenAiCompatibleApiKeyEnv)
                $script:realMicWorkflowObservedKey = [Environment]::GetEnvironmentVariable($CloudOpenAiCompatibleApiKeyEnv, 'Process')
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    ConfigPath = $null
                    Applied = $false
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -ConfigPath $configPath `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_RUN_CONFIG_KEY' `
                -SkipRecording `
                -SkipApply `
                -PassThru

            $result.Applied | Should Be $false
            $script:realMicWorkflowObservedKey | Should Be 'packaged-run-key'
            [Environment]::GetEnvironmentVariable('TALK_TEST_REAL_MIC_RUN_CONFIG_KEY', 'Process') | Should Be $null
        }
        finally {
            Remove-Variable -Name realMicWorkflowObservedKey -Scope Script -ErrorAction SilentlyContinue
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY = $originalApiKey
            }
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'temporarily provides the standard DashScope credential file key to the default model workflow' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-run-legacy-key-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $promptPath = Join-Path $tempRoot 'prompts.json'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_RUN_LEGACY_KEY
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        try {
            Remove-Item Env:TALK_TEST_REAL_MIC_RUN_LEGACY_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot
            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"legacy-run-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8
            New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath $promptPath -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            '{"schemaVersion":1,"samples":[{"sampleId":"short-search-001","audioWav":"short-search-001-16k-mono-s16.wav","referenceText":"你好呀"}]}' |
                Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            $script:realMicWorkflowObservedKey = $null
            Mock Invoke-TalkAsrCorpusRecorder {
                throw 'recording should be skipped in this test'
            }
            Mock Invoke-TalkAsrDefaultModelWorkflow {
                param([string]$CloudOpenAiCompatibleApiKeyEnv)
                $script:realMicWorkflowObservedKey = [Environment]::GetEnvironmentVariable($CloudOpenAiCompatibleApiKeyEnv, 'Process')
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    ConfigPath = $null
                    Applied = $false
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest $promptPath `
                -CorpusRoot $corpusRoot `
                -ReportsRoot $reportsRoot `
                -CloudOpenAiCompatibleEndpoint 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen3-asr-flash' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_RUN_LEGACY_KEY' `
                -SkipRecording `
                -SkipApply `
                -PassThru

            $result.Applied | Should Be $false
            $script:realMicWorkflowObservedKey | Should Be 'legacy-run-key'
            [Environment]::GetEnvironmentVariable('TALK_TEST_REAL_MIC_RUN_LEGACY_KEY', 'Process') | Should Be $null
        }
        finally {
            Remove-Variable -Name realMicWorkflowObservedKey -Scope Script -ErrorAction SilentlyContinue
            if ($null -eq $originalApiKey) {
                Remove-Item Env:TALK_TEST_REAL_MIC_RUN_LEGACY_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_TEST_REAL_MIC_RUN_LEGACY_KEY = $originalApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preserves explicit parameters when invoked directly in plan-only mode' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-direct-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8

            $command = @"
& '$scriptPath' -PromptManifest '$promptPath' -CorpusRoot '$corpusRoot' -ModelId 'paraformer-bilingual-zh-en' -ReportsRoot '$reportsRoot' -ConfigPath '$tempRoot\talk-desktop.toml' -SkipApply -PlanOnly | ConvertTo-Json -Depth 8
"@
            $output = powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1

            $LASTEXITCODE | Should Be 0
            $json = ($output | Out-String) | ConvertFrom-Json
            $json.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-plan'
            $json.ModelId.Count | Should Be 1
            $json.ModelId[0] | Should Be 'paraformer-bilingual-zh-en'
            $json.CorpusManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $json.SelectionJson | Should Be ([System.IO.Path]::GetFullPath((Join-Path $reportsRoot 'selected-default-asr-model.json')))
            $json.WillApply | Should Be $false
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preserves explicit ConfigPath when the workflow script is invoked directly without SkipApply' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-direct-config-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $configPath = Join-Path $tempRoot 'talk-r17-workflow.toml'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        try {
            $promptPath = Join-Path $tempRoot 'prompts.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath $promptPath -Encoding UTF8
            @'
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "short-search-001", "audioWav": "short-search-001-16k-mono-s16.wav", "referenceText": "你好呀" }
  ]
}
'@ | Set-Content -LiteralPath (Join-Path $corpusRoot 'corpus.json') -Encoding UTF8
            Set-Content -LiteralPath (Join-Path $corpusRoot 'short-search-001-16k-mono-s16.wav') -Value 'wav' -Encoding ASCII
            Set-Content -LiteralPath $configPath -Value '[provider]' -Encoding UTF8

            $command = @"
& '$scriptPath' -PromptManifest '$promptPath' -CorpusRoot '$corpusRoot' -ReportsRoot '$reportsRoot' -ConfigPath '$configPath' -SkipRecording -PlanOnly | ConvertTo-Json -Depth 8
"@
            $output = powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1

            $LASTEXITCODE | Should Be 0
            $json = ($output | Out-String) | ConvertFrom-Json
            $json.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-plan'
            $json.ConfigPath | Should Be ([System.IO.Path]::GetFullPath($configPath))
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
