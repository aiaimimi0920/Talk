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
            $plan.RecorderPlan.Samples.Count | Should Be 1
            $plan.WillRecord | Should Be $true
            $plan.WillApply | Should Be $true
        }
        finally {
            [Environment]::CurrentDirectory = $originalDotNetCurrentDirectory
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'records the corpus before benchmarking, selecting, and applying the default model' {
        $script:realMicWorkflowCalls = @()
        try {
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
                param([string]$CorpusManifest, [string]$OutputRoot, [string]$ConfigPath)
                $script:realMicWorkflowCalls += ('default:{0}:{1}:{2}' -f $CorpusManifest, $OutputRoot, $ConfigPath)
                [pscustomobject]@{
                    SelectionJson = 'C:\talk-reports\selected-default-asr-model.json'
                    ConfigPath = $ConfigPath
                    Applied = $true
                }
            }

            $result = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -PromptManifest 'C:\talk-prompts\prompts.json' `
                -CorpusRoot 'C:\talk-corpus' `
                -ReportsRoot 'C:\talk-reports' `
                -ConfigPath 'C:\talk-release\talk-desktop.toml' `
                -PassThru

            $script:realMicWorkflowCalls.Count | Should Be 3
            $script:realMicWorkflowCalls[0] | Should Be 'record-plan'
            $script:realMicWorkflowCalls[1] | Should Be 'record-run'
            $script:realMicWorkflowCalls[2] | Should Be 'default:C:\talk-corpus\corpus.json:C:\talk-reports:C:\talk-release\talk-desktop.toml'
            $result.WorkflowKind | Should Be 'talk-asr-real-mic-default-model-workflow-result'
            $result.RecorderResult.Recordings[0].SampleId | Should Be 'short-search-001'
            $result.Applied | Should Be $true
        }
        finally {
            Remove-Variable -Name realMicWorkflowCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'can skip recording and reuse an existing corpus manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-existing-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        $script:realMicWorkflowCalls = @()
        try {
            New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
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

    It 'preflight reports a ready record-only status when reusing a staged corpus' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-skip-status-ready-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $modelRoot = Join-Path $tempRoot 'models'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY
        try {
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
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkRealMicWorkflowSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
            $env:TALK_TEST_REAL_MIC_SKIP_STATUS_KEY = 'test-key'

            $preflight = Invoke-TalkAsrRealMicDefaultModelWorkflow `
                -CorpusRoot $corpusRoot `
                -ModelRoot $modelRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -ConfigPath (Join-Path $tempRoot 'talk-desktop.toml') `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_REAL_MIC_SKIP_STATUS_KEY' `
                -SkipRecording `
                -PreflightOnly

            $preflight.Ready | Should Be $true
            $recordOnlyStatusCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'record_only_status' })[0]
            $recordOnlyStatusCheck.Status | Should Be 'ready'
            $recordOnlyStatusCheck.Message | Should Match 'sampleCount=1'
            $recordOnlyStatusCheck.Path | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'record-only-status.json')))
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
            $expectedZipformerInstall = ".\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms -DestinationRoot '$resolvedModelRoot'"
            $expectedParaformerInstall = ".\Install-TalkSherpaModel.ps1 -ModelId paraformer-bilingual-zh-en -DestinationRoot '$resolvedModelRoot'"
            $expectedApiKeyCommand = '$env:TALK_TEST_REAL_MIC_MISSING_KEY = ''<redacted>'''
            $zipformerCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'model:zipformer-zh-en-punct-int8-480ms' })[0]
            $paraformerCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'model:paraformer-bilingual-zh-en' })[0]
            $cloudKeyCheck = @($preflight.Checks | Where-Object { $_.Name -eq 'cloud_baseline_api_key' })[0]
            $zipformerCheck.RemediationCommand | Should Be $expectedZipformerInstall
            $paraformerCheck.RemediationCommand | Should Be $expectedParaformerInstall
            $cloudKeyCheck.RemediationCommand | Should Be $expectedApiKeyCommand
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

    It 'temporarily provides a packaged desktop provider api key to the default model workflow' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-real-mic-workflow-run-config-key-' + [guid]::NewGuid().ToString())
        $corpusRoot = Join-Path $tempRoot 'corpus'
        $reportsRoot = Join-Path $corpusRoot 'reports'
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        $originalApiKey = $env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY
        try {
            Remove-Item Env:TALK_TEST_REAL_MIC_RUN_CONFIG_KEY -ErrorAction SilentlyContinue
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
}
