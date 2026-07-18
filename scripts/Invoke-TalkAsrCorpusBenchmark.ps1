[CmdletBinding()]
param(
    [string]$CorpusManifest,
    [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
    [string]$ModelRoot,
    [string]$OutputRoot,
    [string]$AsrBenchExe,
    [string]$LocalAsrDaemonExe,
    [string]$CloudOpenAiCompatibleEndpoint,
    [string]$CloudOpenAiCompatibleModel,
    [string]$CloudOpenAiCompatibleTransport = 'chat_completions_audio_input',
    [string]$CloudOpenAiCompatibleApiKeyEnv = 'TALK_PROVIDER_API_KEY',
    [string]$Bind = '127.0.0.1:53171',
    [int]$ChunkMs = 80,
    [int]$ConnectTimeoutMs = 1000,
    [int]$ReadyTimeoutMs = 1000,
    [int]$PartialIdleTimeoutMs = 10,
    [int]$FinalTimeoutMs = 7000,
    [int]$StartupTimeoutSeconds = 20,
    [switch]$SkipCompare,
    [switch]$PlanOnly,
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$entryCorpusManifest = $CorpusManifest
$entryModelId = $ModelId
$entryModelRoot = $ModelRoot
$entryOutputRoot = $OutputRoot
$entryAsrBenchExe = $AsrBenchExe
$entryLocalAsrDaemonExe = $LocalAsrDaemonExe
$entryCloudOpenAiCompatibleEndpoint = $CloudOpenAiCompatibleEndpoint
$entryCloudOpenAiCompatibleModel = $CloudOpenAiCompatibleModel
$entryCloudOpenAiCompatibleTransport = $CloudOpenAiCompatibleTransport
$entryCloudOpenAiCompatibleApiKeyEnv = $CloudOpenAiCompatibleApiKeyEnv
$entryBind = $Bind
$entryChunkMs = $ChunkMs
$entryConnectTimeoutMs = $ConnectTimeoutMs
$entryReadyTimeoutMs = $ReadyTimeoutMs
$entryPartialIdleTimeoutMs = $PartialIdleTimeoutMs
$entryFinalTimeoutMs = $FinalTimeoutMs
$entryStartupTimeoutSeconds = $StartupTimeoutSeconds
$entrySkipCompare = [bool]$SkipCompare
$entryPlanOnly = [bool]$PlanOnly
$entryPassThru = [bool]$PassThru

$installerScriptPath = Join-Path $PSScriptRoot 'Install-TalkSherpaModel.ps1'
if (-not (Test-Path -LiteralPath $installerScriptPath -PathType Leaf)) {
    throw "Talk sherpa model installer script is missing: $installerScriptPath"
}
. $installerScriptPath

function Get-TalkAsrJsonProperty {
    param(
        [Parameter(Mandatory = $true)]$Object,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Context
    )

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        throw "$Context is missing required property [$Name]"
    }

    $property.Value
}

function Assert-TalkAsrSafeId {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Name must not be blank"
    }
    if ($Value.Trim() -ne $Value) {
        throw "$Name must not have leading or trailing whitespace"
    }
    if ($Value -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') {
        throw "$Name [$Value] must use only letters, numbers, dot, underscore, or hyphen"
    }
}

function Assert-TalkAsrTrimmedNonBlank {
    param(
        [Parameter(Mandatory = $true)][string]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ([string]::IsNullOrWhiteSpace($Value)) {
        throw "$Name must not be blank"
    }
    if ($Value.Trim() -ne $Value) {
        throw "$Name must not have leading or trailing whitespace"
    }
}

function Resolve-TalkAsrCorpusBenchmarkPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }

    $currentFileSystemLocation = (Get-Location -PSProvider FileSystem).ProviderPath
    if ([string]::IsNullOrWhiteSpace($currentFileSystemLocation)) {
        $currentFileSystemLocation = [Environment]::CurrentDirectory
    }

    [System.IO.Path]::GetFullPath((Join-Path $currentFileSystemLocation $Path))
}

function Read-TalkAsrCorpusManifest {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$CorpusManifest)

    $resolvedManifestPath = Resolve-TalkAsrCorpusBenchmarkPath -Path $CorpusManifest
    if (-not (Test-Path -LiteralPath $resolvedManifestPath -PathType Leaf)) {
        throw "Talk ASR corpus manifest does not exist: $resolvedManifestPath"
    }

    $manifestRoot = Split-Path -Parent $resolvedManifestPath
    $manifest = Get-Content -LiteralPath $resolvedManifestPath -Raw | ConvertFrom-Json
    $schemaVersion = Get-TalkAsrJsonProperty -Object $manifest -Name 'schemaVersion' -Context $resolvedManifestPath
    if ([int]$schemaVersion -ne 1) {
        throw "Unsupported Talk ASR corpus manifest schemaVersion [$schemaVersion]. Expected 1."
    }

    $rawSamples = @(Get-TalkAsrJsonProperty -Object $manifest -Name 'samples' -Context $resolvedManifestPath)
    if ($rawSamples.Count -eq 0) {
        throw "Talk ASR corpus manifest has no samples: $resolvedManifestPath"
    }

    $seenSampleIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $samples = New-Object System.Collections.Generic.List[object]
    for ($index = 0; $index -lt $rawSamples.Count; $index += 1) {
        $sample = $rawSamples[$index]
        $context = "$resolvedManifestPath samples[$index]"
        $sampleId = [string](Get-TalkAsrJsonProperty -Object $sample -Name 'sampleId' -Context $context)
        Assert-TalkAsrSafeId -Value $sampleId -Name 'sampleId'
        if (-not $seenSampleIds.Add($sampleId)) {
            throw "Talk ASR corpus manifest contains duplicate sampleId [$sampleId]"
        }

        $audioWav = [string](Get-TalkAsrJsonProperty -Object $sample -Name 'audioWav' -Context $context)
        if ([string]::IsNullOrWhiteSpace($audioWav)) {
            throw "$context audioWav must not be blank"
        }
        $resolvedAudioWav = if ([System.IO.Path]::IsPathRooted($audioWav)) {
            [System.IO.Path]::GetFullPath($audioWav)
        } else {
            [System.IO.Path]::GetFullPath((Join-Path $manifestRoot $audioWav))
        }
        if (-not (Test-Path -LiteralPath $resolvedAudioWav -PathType Leaf)) {
            throw "$context audioWav does not exist: $resolvedAudioWav"
        }

        $referenceText = [string](Get-TalkAsrJsonProperty -Object $sample -Name 'referenceText' -Context $context)
        if ([string]::IsNullOrWhiteSpace($referenceText)) {
            throw "$context referenceText must not be blank"
        }

        $samples.Add([pscustomobject]@{
            SampleId = $sampleId
            AudioWav = $resolvedAudioWav
            ReferenceText = $referenceText
        }) | Out-Null
    }

    $samples.ToArray()
}

function Resolve-TalkAsrDefaultModelRoot {
    [System.IO.Path]::GetFullPath((Resolve-TalkSherpaDefaultModelRoot))
}

function Resolve-TalkAsrDefaultOutputRoot {
    $baseDir = if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        Split-Path -Parent $PSScriptRoot
    } else {
        $PSScriptRoot
    }
    [System.IO.Path]::GetFullPath((Join-Path $baseDir '.runtime\asr-bench\corpus'))
}

function Resolve-TalkAsrDefaultToolPath {
    param([Parameter(Mandatory = $true)][ValidateSet('asr-bench', 'local-daemon')][string]$Tool)

    if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        $talkRoot = Split-Path -Parent $PSScriptRoot
        $binaryName = if ($Tool -eq 'asr-bench') { 'asr-bench.exe' } else { 'talk-local-asr-sherpa.exe' }
        return [System.IO.Path]::GetFullPath((Join-Path $talkRoot "target\release\$binaryName"))
    }

    $releaseBinaryName = if ($Tool -eq 'asr-bench') { 'asr-bench.exe' } else { 'talk-local-asr-sherpa.exe' }
    [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".internal\$releaseBinaryName"))
}

function Get-TalkDirectorySizeMb {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$Path)

    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    if (-not (Test-Path -LiteralPath $resolvedPath -PathType Container)) {
        throw "Directory does not exist: $resolvedPath"
    }

    $bytes = [int64]0
    Get-ChildItem -LiteralPath $resolvedPath -Recurse -File -ErrorAction SilentlyContinue |
        ForEach-Object { $bytes += [int64]$_.Length }
    if ($bytes -le 0) {
        return 0
    }

    [int][Math]::Ceiling($bytes / 1MB)
}

function New-TalkAsrDaemonArguments {
    param(
        [Parameter(Mandatory = $true)]$Validation,
        [Parameter(Mandatory = $true)][string]$Bind
    )

    $arguments = @(
        '--bind', $Bind,
        '--mode', 'sherpa-online',
        '--engine', 'sherpa-onnx',
        '--model', ([string]$Validation.ModelName),
        '--model-family', ([string]$Validation.ModelFamily),
        '--tokens', ([string]$Validation.TokensPath),
        '--encoder', ([string]$Validation.EncoderPath),
        '--decoder', ([string]$Validation.DecoderPath),
        '--provider', ([string]$Validation.Provider),
        '--num-threads', ([string]$Validation.NumThreads),
        '--sample-rate-hz', ([string]$Validation.SampleRateHz),
        '--decoding-method', ([string]$Validation.DecodingMethod)
    )
    if (-not [string]::IsNullOrWhiteSpace([string]$Validation.JoinerPath)) {
        $arguments += @('--joiner', ([string]$Validation.JoinerPath))
    }

    $arguments
}

function New-TalkAsrBenchArguments {
    param(
        [Parameter(Mandatory = $true)]$Sample,
        [Parameter(Mandatory = $true)][string]$Endpoint,
        [Parameter(Mandatory = $true)][string]$OutputJson,
        [Parameter(Mandatory = $true)][int]$ModelSizeMb,
        [Parameter(Mandatory = $true)][int]$ChunkMs,
        [Parameter(Mandatory = $true)][int]$ConnectTimeoutMs,
        [Parameter(Mandatory = $true)][int]$ReadyTimeoutMs,
        [Parameter(Mandatory = $true)][int]$PartialIdleTimeoutMs,
        [Parameter(Mandatory = $true)][int]$FinalTimeoutMs
    )

    @(
        '--engine', 'streaming_service',
        '--streaming-endpoint', $Endpoint,
        '--audio-wav', ([string]$Sample.AudioWav),
        '--reference-text', ([string]$Sample.ReferenceText),
        '--sample-id', ([string]$Sample.SampleId),
        '--model-size-mb', ([string]$ModelSizeMb),
        '--chunk-ms', ([string]$ChunkMs),
        '--connect-timeout-ms', ([string]$ConnectTimeoutMs),
        '--ready-timeout-ms', ([string]$ReadyTimeoutMs),
        '--partial-idle-timeout-ms', ([string]$PartialIdleTimeoutMs),
        '--final-timeout-ms', ([string]$FinalTimeoutMs),
        '--output-json', $OutputJson
    )
}

function New-TalkAsrCloudOpenAiCompatibleBenchArguments {
    param(
        [Parameter(Mandatory = $true)]$Sample,
        [Parameter(Mandatory = $true)][string]$Endpoint,
        [Parameter(Mandatory = $true)][string]$Model,
        [Parameter(Mandatory = $true)][string]$Transport,
        [Parameter(Mandatory = $true)][string]$ApiKeyEnv,
        [Parameter(Mandatory = $true)][string]$OutputJson
    )

    @(
        '--cloud-openai-compatible-endpoint', $Endpoint,
        '--cloud-openai-compatible-model', $Model,
        '--cloud-openai-compatible-transport', $Transport,
        '--cloud-openai-compatible-api-key-env', $ApiKeyEnv,
        '--audio-wav', ([string]$Sample.AudioWav),
        '--reference-text', ([string]$Sample.ReferenceText),
        '--sample-id', ([string]$Sample.SampleId),
        '--output-json', $OutputJson
    )
}

function New-TalkAsrCorpusBenchmarkPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string[]]$ModelId,
        [string]$ModelRoot,
        [string]$OutputRoot,
        [string]$AsrBenchExe,
        [string]$LocalAsrDaemonExe,
        [string]$CloudOpenAiCompatibleEndpoint,
        [string]$CloudOpenAiCompatibleModel,
        [string]$CloudOpenAiCompatibleTransport = 'chat_completions_audio_input',
        [string]$CloudOpenAiCompatibleApiKeyEnv = 'TALK_PROVIDER_API_KEY',
        [string]$Bind = '127.0.0.1:53171',
        [int]$ChunkMs = 80,
        [int]$ConnectTimeoutMs = 1000,
        [int]$ReadyTimeoutMs = 1000,
        [int]$PartialIdleTimeoutMs = 10,
        [int]$FinalTimeoutMs = 7000
    )

    if ($ChunkMs -le 0) { throw 'ChunkMs must be greater than 0' }
    if ($ConnectTimeoutMs -le 0) { throw 'ConnectTimeoutMs must be greater than 0' }
    if ($ReadyTimeoutMs -le 0) { throw 'ReadyTimeoutMs must be greater than 0' }
    if ($PartialIdleTimeoutMs -le 0) { throw 'PartialIdleTimeoutMs must be greater than 0' }
    if ($FinalTimeoutMs -le 0) { throw 'FinalTimeoutMs must be greater than 0' }
    $hasCloudEndpoint = -not [string]::IsNullOrWhiteSpace($CloudOpenAiCompatibleEndpoint)
    $hasCloudModel = -not [string]::IsNullOrWhiteSpace($CloudOpenAiCompatibleModel)
    if ($hasCloudEndpoint -xor $hasCloudModel) {
        throw 'CloudOpenAiCompatibleEndpoint and CloudOpenAiCompatibleModel must be supplied together'
    }
    if ($hasCloudEndpoint -and $hasCloudModel) {
        Assert-TalkAsrTrimmedNonBlank -Value $CloudOpenAiCompatibleEndpoint -Name 'CloudOpenAiCompatibleEndpoint'
        Assert-TalkAsrTrimmedNonBlank -Value $CloudOpenAiCompatibleModel -Name 'CloudOpenAiCompatibleModel'
        Assert-TalkAsrTrimmedNonBlank -Value $CloudOpenAiCompatibleTransport -Name 'CloudOpenAiCompatibleTransport'
        Assert-TalkAsrTrimmedNonBlank -Value $CloudOpenAiCompatibleApiKeyEnv -Name 'CloudOpenAiCompatibleApiKeyEnv'
        if ($CloudOpenAiCompatibleTransport -notin @('audio_transcriptions', 'chat_completions_audio_input')) {
            throw 'CloudOpenAiCompatibleTransport must be audio_transcriptions or chat_completions_audio_input'
        }
        Assert-TalkAsrSafeId -Value $CloudOpenAiCompatibleTransport -Name 'CloudOpenAiCompatibleTransport'
    }
    if ($Bind -notmatch '^127\.0\.0\.1:\d{1,5}$') {
        throw "Talk ASR corpus benchmark bind must be loopback 127.0.0.1:<port>, got [$Bind]"
    }
    $port = [int]($Bind.Split(':')[-1])
    if ($port -lt 1 -or $port -gt 65535) {
        throw "Talk ASR corpus benchmark bind port must be between 1 and 65535, got [$port]"
    }

    $resolvedModelRoot = if ([string]::IsNullOrWhiteSpace($ModelRoot)) {
        Resolve-TalkAsrDefaultModelRoot
    } else {
        Resolve-TalkAsrCorpusBenchmarkPath -Path $ModelRoot
    }
    $resolvedOutputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
        Resolve-TalkAsrDefaultOutputRoot
    } else {
        Resolve-TalkAsrCorpusBenchmarkPath -Path $OutputRoot
    }
    $resolvedAsrBenchExe = if ([string]::IsNullOrWhiteSpace($AsrBenchExe)) {
        Resolve-TalkAsrDefaultToolPath -Tool 'asr-bench'
    } else {
        Resolve-TalkAsrCorpusBenchmarkPath -Path $AsrBenchExe
    }
    $resolvedLocalDaemonExe = if ([string]::IsNullOrWhiteSpace($LocalAsrDaemonExe)) {
        Resolve-TalkAsrDefaultToolPath -Tool 'local-daemon'
    } else {
        Resolve-TalkAsrCorpusBenchmarkPath -Path $LocalAsrDaemonExe
    }
    $endpoint = "ws://$Bind/asr"
    $resolvedCorpusManifest = Resolve-TalkAsrCorpusBenchmarkPath -Path $CorpusManifest
    $samples = @(Read-TalkAsrCorpusManifest -CorpusManifest $resolvedCorpusManifest)
    if ($ModelId.Count -eq 0) {
        throw 'At least one ModelId is required'
    }

    $reportPaths = New-Object System.Collections.Generic.List[string]
    $candidates = New-Object System.Collections.Generic.List[object]
    foreach ($id in $ModelId) {
        Assert-TalkAsrSafeId -Value $id -Name 'ModelId'
        $modelDir = Join-Path $resolvedModelRoot $id
        $validation = Test-TalkSherpaModelInstall -ModelId $id -ModelDir $modelDir
        $modelSizeMb = Get-TalkDirectorySizeMb -Path $validation.ModelDir
        $daemonArguments = @(New-TalkAsrDaemonArguments -Validation $validation -Bind $Bind)
        $runs = New-Object System.Collections.Generic.List[object]
        foreach ($sample in $samples) {
            $reportPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot "$id-$($sample.SampleId).json"))
            $reportPaths.Add($reportPath) | Out-Null
            $runs.Add([pscustomobject]@{
                ModelId = $id
                ModelName = $validation.ModelName
                SampleId = $sample.SampleId
                AudioWav = $sample.AudioWav
                ReferenceText = $sample.ReferenceText
                OutputJson = $reportPath
                AsrBenchArguments = @(New-TalkAsrBenchArguments `
                    -Sample $sample `
                    -Endpoint $endpoint `
                    -OutputJson $reportPath `
                    -ModelSizeMb $modelSizeMb `
                    -ChunkMs $ChunkMs `
                    -ConnectTimeoutMs $ConnectTimeoutMs `
                    -ReadyTimeoutMs $ReadyTimeoutMs `
                    -PartialIdleTimeoutMs $PartialIdleTimeoutMs `
                    -FinalTimeoutMs $FinalTimeoutMs)
            }) | Out-Null
        }

        $candidates.Add([pscustomobject]@{
            ModelId = $id
            ModelName = $validation.ModelName
            ModelFamily = $validation.ModelFamily
            ModelDir = $validation.ModelDir
            ModelSizeMb = $modelSizeMb
            DaemonArguments = $daemonArguments
            Runs = $runs.ToArray()
        }) | Out-Null
    }

    $cloudOpenAiCompatibleBaseline = $null
    if ($hasCloudEndpoint -and $hasCloudModel) {
        $cloudRuns = New-Object System.Collections.Generic.List[object]
        foreach ($sample in $samples) {
            $reportPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot "cloud-openai-compatible-$CloudOpenAiCompatibleTransport-$($sample.SampleId).json"))
            $reportPaths.Add($reportPath) | Out-Null
            $cloudRuns.Add([pscustomobject]@{
                SampleId = $sample.SampleId
                AudioWav = $sample.AudioWav
                ReferenceText = $sample.ReferenceText
                OutputJson = $reportPath
                AsrBenchArguments = @(New-TalkAsrCloudOpenAiCompatibleBenchArguments `
                    -Sample $sample `
                    -Endpoint $CloudOpenAiCompatibleEndpoint `
                    -Model $CloudOpenAiCompatibleModel `
                    -Transport $CloudOpenAiCompatibleTransport `
                    -ApiKeyEnv $CloudOpenAiCompatibleApiKeyEnv `
                    -OutputJson $reportPath)
            }) | Out-Null
        }

        $cloudOpenAiCompatibleBaseline = [pscustomobject]@{
            Endpoint = $CloudOpenAiCompatibleEndpoint
            Model = $CloudOpenAiCompatibleModel
            Transport = $CloudOpenAiCompatibleTransport
            ApiKeyEnv = $CloudOpenAiCompatibleApiKeyEnv
            Runs = $cloudRuns.ToArray()
        }
    }

    $comparisonPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'asr-model-comparison.json'))
    $comparisonArguments = New-Object System.Collections.Generic.List[string]
    foreach ($reportPath in $reportPaths) {
        $comparisonArguments.Add('--compare-report') | Out-Null
        $comparisonArguments.Add($reportPath) | Out-Null
    }
    $comparisonArguments.Add('--output-json') | Out-Null
    $comparisonArguments.Add($comparisonPath) | Out-Null

    [pscustomobject]@{
        CorpusManifest = $resolvedCorpusManifest
        ModelRoot = $resolvedModelRoot
        OutputRoot = $resolvedOutputRoot
        AsrBenchExe = $resolvedAsrBenchExe
        LocalAsrDaemonExe = $resolvedLocalDaemonExe
        Bind = $Bind
        Endpoint = $endpoint
        Samples = $samples
        Candidates = $candidates.ToArray()
        CloudOpenAiCompatibleBaseline = $cloudOpenAiCompatibleBaseline
        ReportPaths = $reportPaths.ToArray()
        ComparisonPath = $comparisonPath
        ComparisonArguments = $comparisonArguments.ToArray()
    }
}

function ConvertTo-TalkWindowsCommandLineArgument {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value -notmatch '[\s"]') {
        return $Value
    }

    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Join-TalkWindowsArgumentList {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    (@($Arguments) | ForEach-Object { ConvertTo-TalkWindowsCommandLineArgument -Value $_ }) -join ' '
}

function Wait-TalkTcpEndpoint {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$Bind,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds,
        [Parameter(Mandatory = $true)]$Process
    )

    $parts = $Bind.Split(':')
    $hostName = $parts[0]
    $port = [int]$parts[1]
    $deadline = [DateTimeOffset]::Now.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::Now -lt $deadline) {
        if ($Process.HasExited) {
            throw "Talk local ASR daemon exited before listening on $Bind with exit code $($Process.ExitCode)"
        }
        $client = New-Object System.Net.Sockets.TcpClient
        try {
            $connect = $client.BeginConnect($hostName, $port, $null, $null)
            if ($connect.AsyncWaitHandle.WaitOne(200)) {
                $client.EndConnect($connect)
                return
            }
        }
        catch {
            Start-Sleep -Milliseconds 100
        }
        finally {
            $client.Close()
        }
    }

    throw "Timed out waiting for Talk local ASR daemon on $Bind"
}

function Invoke-TalkAsrCheckedProcess {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$FilePath,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$WorkingDirectory
    )

    $output = & $FilePath @Arguments 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Command failed with exit code $exitCode`: $FilePath $($Arguments -join ' ')`n$output"
    }

    [pscustomobject]@{
        FilePath = $FilePath
        Arguments = @($Arguments)
        WorkingDirectory = $WorkingDirectory
        ExitCode = $exitCode
        OutputText = ($output | Out-String)
    }
}

function Start-TalkAsrCorpusDaemon {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$DaemonExe,
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [Parameter(Mandatory = $true)][string]$OutputRoot,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    $logDir = Join-Path $OutputRoot 'logs'
    New-Item -ItemType Directory -Path $logDir -Force | Out-Null
    $stdoutPath = Join-Path $logDir "$ModelId-daemon.out.log"
    $stderrPath = Join-Path $logDir "$ModelId-daemon.err.log"
    $argumentLine = Join-TalkWindowsArgumentList -Arguments $Arguments
    Start-Process `
        -FilePath $DaemonExe `
        -ArgumentList $argumentLine `
        -WorkingDirectory $OutputRoot `
        -RedirectStandardOutput $stdoutPath `
        -RedirectStandardError $stderrPath `
        -WindowStyle Hidden `
        -PassThru
}

function Stop-TalkAsrCorpusDaemon {
    param($Process)

    if ($null -eq $Process) {
        return
    }
    if ($Process.HasExited) {
        return
    }

    Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue
    $Process.WaitForExit(5000) | Out-Null
}

function ConvertTo-TalkObjectArray {
    param([Parameter(Mandatory = $true)]$Value)

    if ($Value -is [System.Collections.Generic.List[object]]) {
        return $Value.ToArray()
    }

    @($Value)
}

function New-TalkAsrCorpusBenchmarkResult {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)]$ProcessRecords,
        $ComparisonRecord,
        [switch]$SkipCompare
    )

    [pscustomobject]@{
        Plan = $Plan
        ProcessRecords = @(ConvertTo-TalkObjectArray -Value $ProcessRecords)
        ComparisonRecord = $ComparisonRecord
        ReportPaths = $Plan.ReportPaths
        ComparisonPath = if ($SkipCompare) { $null } else { $Plan.ComparisonPath }
    }
}

function Invoke-TalkAsrCorpusBenchmark {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
        [string]$ModelRoot,
        [string]$OutputRoot,
        [string]$AsrBenchExe,
        [string]$LocalAsrDaemonExe,
        [string]$CloudOpenAiCompatibleEndpoint,
        [string]$CloudOpenAiCompatibleModel,
        [string]$CloudOpenAiCompatibleTransport = 'chat_completions_audio_input',
        [string]$CloudOpenAiCompatibleApiKeyEnv = 'TALK_PROVIDER_API_KEY',
        [string]$Bind = '127.0.0.1:53171',
        [int]$ChunkMs = 80,
        [int]$ConnectTimeoutMs = 1000,
        [int]$ReadyTimeoutMs = 1000,
        [int]$PartialIdleTimeoutMs = 10,
        [int]$FinalTimeoutMs = 7000,
        [int]$StartupTimeoutSeconds = 20,
        [switch]$SkipCompare,
        [switch]$PlanOnly,
        [switch]$PassThru
    )

    $plan = New-TalkAsrCorpusBenchmarkPlan `
        -CorpusManifest $CorpusManifest `
        -ModelId $ModelId `
        -ModelRoot $ModelRoot `
        -OutputRoot $OutputRoot `
        -AsrBenchExe $AsrBenchExe `
        -LocalAsrDaemonExe $LocalAsrDaemonExe `
        -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint `
        -CloudOpenAiCompatibleModel $CloudOpenAiCompatibleModel `
        -CloudOpenAiCompatibleTransport $CloudOpenAiCompatibleTransport `
        -CloudOpenAiCompatibleApiKeyEnv $CloudOpenAiCompatibleApiKeyEnv `
        -Bind $Bind `
        -ChunkMs $ChunkMs `
        -ConnectTimeoutMs $ConnectTimeoutMs `
        -ReadyTimeoutMs $ReadyTimeoutMs `
        -PartialIdleTimeoutMs $PartialIdleTimeoutMs `
        -FinalTimeoutMs $FinalTimeoutMs

    if ($PlanOnly) {
        return $plan
    }

    if (-not (Test-Path -LiteralPath $plan.AsrBenchExe -PathType Leaf)) {
        throw "asr-bench executable does not exist: $($plan.AsrBenchExe)"
    }
    if (-not (Test-Path -LiteralPath $plan.LocalAsrDaemonExe -PathType Leaf)) {
        throw "Talk local ASR daemon executable does not exist: $($plan.LocalAsrDaemonExe)"
    }

    New-Item -ItemType Directory -Path $plan.OutputRoot -Force | Out-Null
    $processRecords = New-Object System.Collections.Generic.List[object]
    foreach ($candidate in $plan.Candidates) {
        Write-Host "Starting Talk local ASR daemon for $($candidate.ModelId) ..."
        $daemonProcess = $null
        try {
            $daemonProcess = Start-TalkAsrCorpusDaemon `
                -DaemonExe $plan.LocalAsrDaemonExe `
                -Arguments $candidate.DaemonArguments `
                -OutputRoot $plan.OutputRoot `
                -ModelId $candidate.ModelId
            Wait-TalkTcpEndpoint -Bind $plan.Bind -TimeoutSeconds $StartupTimeoutSeconds -Process $daemonProcess

            foreach ($run in $candidate.Runs) {
                Write-Host "Benchmarking $($candidate.ModelId) / $($run.SampleId) ..."
                $record = Invoke-TalkAsrCheckedProcess `
                    -FilePath $plan.AsrBenchExe `
                    -Arguments $run.AsrBenchArguments `
                    -WorkingDirectory $plan.OutputRoot
                $processRecords.Add($record) | Out-Null
            }
        }
        finally {
            Stop-TalkAsrCorpusDaemon -Process $daemonProcess
        }
    }

    if ($null -ne $plan.CloudOpenAiCompatibleBaseline) {
        foreach ($run in $plan.CloudOpenAiCompatibleBaseline.Runs) {
            Write-Host "Benchmarking cloud OpenAI-compatible baseline / $($run.SampleId) ..."
            $record = Invoke-TalkAsrCheckedProcess `
                -FilePath $plan.AsrBenchExe `
                -Arguments $run.AsrBenchArguments `
                -WorkingDirectory $plan.OutputRoot
            $processRecords.Add($record) | Out-Null
        }
    }

    $comparisonRecord = $null
    if (-not $SkipCompare) {
        Write-Host "Comparing Talk ASR corpus reports ..."
        $comparisonRecord = Invoke-TalkAsrCheckedProcess `
            -FilePath $plan.AsrBenchExe `
            -Arguments $plan.ComparisonArguments `
            -WorkingDirectory $plan.OutputRoot
    }

    $result = New-TalkAsrCorpusBenchmarkResult `
        -Plan $plan `
        -ProcessRecords $processRecords `
        -ComparisonRecord $comparisonRecord `
        -SkipCompare:$SkipCompare

    if ($PassThru) {
        return $result
    }

    $result
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkAsrCorpusBenchmark `
        -CorpusManifest $entryCorpusManifest `
        -ModelId $entryModelId `
        -ModelRoot $entryModelRoot `
        -OutputRoot $entryOutputRoot `
        -AsrBenchExe $entryAsrBenchExe `
        -LocalAsrDaemonExe $entryLocalAsrDaemonExe `
        -CloudOpenAiCompatibleEndpoint $entryCloudOpenAiCompatibleEndpoint `
        -CloudOpenAiCompatibleModel $entryCloudOpenAiCompatibleModel `
        -CloudOpenAiCompatibleTransport $entryCloudOpenAiCompatibleTransport `
        -CloudOpenAiCompatibleApiKeyEnv $entryCloudOpenAiCompatibleApiKeyEnv `
        -Bind $entryBind `
        -ChunkMs $entryChunkMs `
        -ConnectTimeoutMs $entryConnectTimeoutMs `
        -ReadyTimeoutMs $entryReadyTimeoutMs `
        -PartialIdleTimeoutMs $entryPartialIdleTimeoutMs `
        -FinalTimeoutMs $entryFinalTimeoutMs `
        -StartupTimeoutSeconds $entryStartupTimeoutSeconds `
        -SkipCompare:$entrySkipCompare `
        -PlanOnly:$entryPlanOnly `
        -PassThru:$entryPassThru
}
