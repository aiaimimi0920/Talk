$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptRoot = Split-Path $here -Parent
$scriptPath = Join-Path $scriptRoot 'Invoke-TalkAsrCorpusBenchmark.ps1'

. $scriptPath

function New-TestTalkSherpaModelDir {
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

Describe 'Invoke-TalkAsrCorpusBenchmark helpers' {
    It 'loads a corpus manifest and resolves sample WAV paths relative to the manifest' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $samplePath = Join-Path $tempRoot 'short-search.wav'
            Set-Content -LiteralPath $samplePath -Value 'not a real wav for plan-only tests' -Encoding ASCII
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @'
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "short-search-001",
      "audioWav": "short-search.wav",
      "referenceText": "你好呀"
    }
  ]
}
'@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            $samples = @(Read-TalkAsrCorpusManifest -CorpusManifest $manifestPath)

            $samples.Count | Should Be 1
            $samples[0].SampleId | Should Be 'short-search-001'
            $samples[0].AudioWav | Should Be ([System.IO.Path]::GetFullPath($samplePath))
            $samples[0].ReferenceText | Should Be '你好呀'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects duplicate sample ids before any benchmark process is started' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-duplicate-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $samplePath = Join-Path $tempRoot 'sample.wav'
            Set-Content -LiteralPath $samplePath -Value 'not a real wav for validation tests' -Encoding ASCII
            $manifestPath = Join-Path $tempRoot 'corpus.json'
            @"
{
  "schemaVersion": 1,
  "samples": [
    { "sampleId": "dup", "audioWav": "$($samplePath.Replace('\', '\\'))", "referenceText": "你好" },
    { "sampleId": "dup", "audioWav": "$($samplePath.Replace('\', '\\'))", "referenceText": "你好" }
  ]
}
"@ | Set-Content -LiteralPath $manifestPath -Encoding UTF8

            { Read-TalkAsrCorpusManifest -CorpusManifest $manifestPath } | Should Throw 'duplicate sampleId'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'creates a plan-only benchmark matrix for Zipformer and Paraformer using the same corpus samples' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-plan-test-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $outputRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $modelRoot -Force | Out-Null
        try {
            New-TestTalkSherpaModelDir -Root $modelRoot -ModelId 'zipformer-zh-en-punct-int8-480ms' -Family 'transducer'
            New-TestTalkSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

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

            $plan = Invoke-TalkAsrCorpusBenchmark `
                -CorpusManifest $manifestPath `
                -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
                -ModelRoot $modelRoot `
                -OutputRoot $outputRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -PlanOnly

            $plan.Candidates.Count | Should Be 2
            $plan.Samples.Count | Should Be 1
            $plan.ReportPaths.Count | Should Be 2
            $plan.ComparisonPath | Should Be ([System.IO.Path]::GetFullPath((Join-Path $outputRoot 'asr-model-comparison.json')))
            $plan.Candidates[0].Runs[0].SampleId | Should Be 'short-search-001'
            ($plan.Candidates[0].Runs[0].AsrBenchArguments -contains '--sample-id') | Should Be $true
            ($plan.Candidates[0].Runs[0].AsrBenchArguments -contains 'short-search-001') | Should Be $true
            ($plan.Candidates[0].Runs[0].AsrBenchArguments -contains '--model-size-mb') | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'resolves explicit relative paths against the current PowerShell location instead of the process cwd' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-benchmark-relative-' + [guid]::NewGuid().ToString())
        $releaseDir = Join-Path $tempRoot 'release'
        $processCwd = Join-Path $tempRoot 'process-cwd'
        $modelRoot = Join-Path $releaseDir '.runtime\models\sherpa-onnx'
        $corpusRoot = Join-Path $releaseDir '.runtime\asr-bench\real-mic-corpus'
        New-Item -ItemType Directory -Path $corpusRoot -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $releaseDir '.internal') -Force | Out-Null
        New-Item -ItemType Directory -Path $processCwd -Force | Out-Null
        $originalDotNetCurrentDirectory = [Environment]::CurrentDirectory
        try {
            New-TestTalkSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'
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
                $plan = Invoke-TalkAsrCorpusBenchmark `
                    -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
                    -ModelId @('paraformer-bilingual-zh-en') `
                    -ModelRoot .\.runtime\models\sherpa-onnx `
                    -OutputRoot .\.runtime\asr-bench\real-mic-corpus\reports `
                    -AsrBenchExe .\.internal\asr-bench.exe `
                    -LocalAsrDaemonExe .\.internal\talk-local-asr-sherpa.exe `
                    -PlanOnly
            }
            finally {
                Pop-Location
            }

            $plan.CorpusManifest | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'corpus.json')))
            $plan.ModelRoot | Should Be ([System.IO.Path]::GetFullPath($modelRoot))
            $plan.OutputRoot | Should Be ([System.IO.Path]::GetFullPath((Join-Path $corpusRoot 'reports')))
            $plan.AsrBenchExe | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir '.internal\asr-bench.exe')))
            $plan.LocalAsrDaemonExe | Should Be ([System.IO.Path]::GetFullPath((Join-Path $releaseDir '.internal\talk-local-asr-sherpa.exe')))
        }
        finally {
            [Environment]::CurrentDirectory = $originalDotNetCurrentDirectory
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'adds an optional cloud OpenAI-compatible baseline to the same corpus comparison plan' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-cloud-plan-test-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $outputRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $modelRoot -Force | Out-Null
        try {
            New-TestTalkSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

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

            $plan = Invoke-TalkAsrCorpusBenchmark `
                -CorpusManifest $manifestPath `
                -ModelId @('paraformer-bilingual-zh-en') `
                -ModelRoot $modelRoot `
                -OutputRoot $outputRoot `
                -AsrBenchExe (Join-Path $tempRoot 'asr-bench.exe') `
                -LocalAsrDaemonExe (Join-Path $tempRoot 'talk-local-asr-sherpa.exe') `
                -CloudOpenAiCompatibleEndpoint 'http://127.0.0.1:18080/v1/chat/completions' `
                -CloudOpenAiCompatibleModel 'qwen-audio-test' `
                -CloudOpenAiCompatibleTransport 'chat_completions_audio_input' `
                -CloudOpenAiCompatibleApiKeyEnv 'TALK_TEST_PROVIDER_API_KEY' `
                -PlanOnly

            $plan.Candidates.Count | Should Be 1
            $plan.CloudOpenAiCompatibleBaseline.Model | Should Be 'qwen-audio-test'
            $plan.CloudOpenAiCompatibleBaseline.Runs.Count | Should Be 1
            $plan.ReportPaths.Count | Should Be 2
            $cloudRun = $plan.CloudOpenAiCompatibleBaseline.Runs[0]
            $cloudRun.SampleId | Should Be 'short-search-001'
            ($cloudRun.AsrBenchArguments -contains '--cloud-openai-compatible-endpoint') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains 'http://127.0.0.1:18080/v1/chat/completions') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains '--cloud-openai-compatible-model') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains 'qwen-audio-test') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains '--cloud-openai-compatible-transport') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains 'chat_completions_audio_input') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains '--cloud-openai-compatible-api-key-env') | Should Be $true
            ($cloudRun.AsrBenchArguments -contains 'TALK_TEST_PROVIDER_API_KEY') | Should Be $true
            ($plan.ComparisonArguments -contains $cloudRun.OutputJson) | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'preserves an explicitly supplied ModelId when the script is invoked directly' {
        $tempRoot = Join-Path $env:TEMP ('talk-asr-corpus-direct-test-' + [guid]::NewGuid().ToString())
        $modelRoot = Join-Path $tempRoot 'models'
        $outputRoot = Join-Path $tempRoot 'reports'
        New-Item -ItemType Directory -Path $modelRoot -Force | Out-Null
        try {
            New-TestTalkSherpaModelDir -Root $modelRoot -ModelId 'paraformer-bilingual-zh-en' -Family 'paraformer'

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
& '$scriptPath' -CorpusManifest '$manifestPath' -ModelId 'paraformer-bilingual-zh-en' -ModelRoot '$modelRoot' -OutputRoot '$outputRoot' -AsrBenchExe '$tempRoot\asr-bench.exe' -LocalAsrDaemonExe '$tempRoot\talk-local-asr-sherpa.exe' -PlanOnly | ConvertTo-Json -Depth 8
"@
            $output = powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $command 2>&1

            $LASTEXITCODE | Should Be 0
            $json = ($output | Out-String) | ConvertFrom-Json
            $json.Candidates.Count | Should Be 1
            $json.Candidates[0].ModelId | Should Be 'paraformer-bilingual-zh-en'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'resolves source checkout benchmark tools under Talk target release' {
        $expectedBench = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot '..\target\release\asr-bench.exe'))
        $expectedDaemon = [System.IO.Path]::GetFullPath((Join-Path $scriptRoot '..\target\release\talk-local-asr-sherpa.exe'))

        (Resolve-TalkAsrDefaultToolPath -Tool 'asr-bench') | Should Be $expectedBench
        (Resolve-TalkAsrDefaultToolPath -Tool 'local-daemon') | Should Be $expectedDaemon
    }

    It 'normalizes process record lists in benchmark result objects' {
        $records = New-Object System.Collections.Generic.List[object]
        $records.Add([pscustomobject]@{ ExitCode = 0; OutputText = 'ok' }) | Out-Null
        $plan = [pscustomobject]@{
            ReportPaths = @('report.json')
            ComparisonPath = 'asr-model-comparison.json'
        }

        $result = New-TalkAsrCorpusBenchmarkResult `
            -Plan $plan `
            -ProcessRecords $records `
            -ComparisonRecord ([pscustomobject]@{ ExitCode = 0 }) `
            -SkipCompare:$false

        $result.ProcessRecords.Count | Should Be 1
        $result.ProcessRecords[0].ExitCode | Should Be 0
        $result.ComparisonPath | Should Be 'asr-model-comparison.json'
    }
}
