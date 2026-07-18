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
    [string]$SelectionJson,
    [string]$EvidenceStatusJson,
    [string]$ConfigPath,
    [int]$MinSamples = 3,
    [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
    [switch]$AllowMissingCloudBaseline,
    [switch]$AllowSyntheticSampleIds,
    [switch]$SkipApply,
    [switch]$NoBackup,
    [switch]$PlanOnly,
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$workflowEntryCorpusManifest = $CorpusManifest
$workflowEntryModelId = $ModelId
$workflowEntryModelRoot = $ModelRoot
$workflowEntryOutputRoot = $OutputRoot
$workflowEntryAsrBenchExe = $AsrBenchExe
$workflowEntryLocalAsrDaemonExe = $LocalAsrDaemonExe
$workflowEntryCloudOpenAiCompatibleEndpoint = $CloudOpenAiCompatibleEndpoint
$workflowEntryCloudOpenAiCompatibleModel = $CloudOpenAiCompatibleModel
$workflowEntryCloudOpenAiCompatibleTransport = $CloudOpenAiCompatibleTransport
$workflowEntryCloudOpenAiCompatibleApiKeyEnv = $CloudOpenAiCompatibleApiKeyEnv
$workflowEntryBind = $Bind
$workflowEntryChunkMs = $ChunkMs
$workflowEntryConnectTimeoutMs = $ConnectTimeoutMs
$workflowEntryReadyTimeoutMs = $ReadyTimeoutMs
$workflowEntryPartialIdleTimeoutMs = $PartialIdleTimeoutMs
$workflowEntryFinalTimeoutMs = $FinalTimeoutMs
$workflowEntryStartupTimeoutSeconds = $StartupTimeoutSeconds
$workflowEntrySelectionJson = $SelectionJson
$workflowEntryEvidenceStatusJson = $EvidenceStatusJson
$workflowEntryConfigPath = $ConfigPath
$workflowEntryMinSamples = $MinSamples
$workflowEntryRequiredLocalModelId = $RequiredLocalModelId
$workflowEntryAllowMissingCloudBaseline = [bool]$AllowMissingCloudBaseline
$workflowEntryAllowSyntheticSampleIds = [bool]$AllowSyntheticSampleIds
$workflowEntrySkipApply = [bool]$SkipApply
$workflowEntryNoBackup = [bool]$NoBackup
$workflowEntryPlanOnly = [bool]$PlanOnly
$workflowEntryPassThru = [bool]$PassThru

foreach ($dependencyName in @(
    'Invoke-TalkAsrCorpusBenchmark.ps1',
    'Select-TalkDefaultAsrModel.ps1',
    'Set-TalkDefaultAsrModel.ps1'
)) {
    $dependencyPath = Join-Path $PSScriptRoot $dependencyName
    if (-not (Test-Path -LiteralPath $dependencyPath -PathType Leaf)) {
        throw "Talk ASR default workflow dependency is missing: $dependencyPath"
    }
    . $dependencyPath
}

function Resolve-TalkAsrDefaultWorkflowPath {
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

function Resolve-TalkAsrDefaultWorkflowOptionalPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    Resolve-TalkAsrDefaultWorkflowPath -Path $Path
}

function Get-TalkAsrDefaultWorkflowOptionalProperty {
    param(
        $Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }

    $property.Value
}

function Resolve-TalkAsrDefaultWorkflowSelectionJson {
    param(
        [Parameter(Mandatory = $true)]$BenchmarkPlanOrResult,
        [string]$SelectionJson
    )

    if (-not [string]::IsNullOrWhiteSpace($SelectionJson)) {
        return Resolve-TalkAsrDefaultWorkflowPath -Path $SelectionJson
    }

    $plan = Get-TalkAsrDefaultWorkflowOptionalProperty -Object $BenchmarkPlanOrResult -Name 'Plan'
    if ($null -eq $plan) {
        $plan = $BenchmarkPlanOrResult
    }

    $outputRoot = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $plan -Name 'OutputRoot')
    if ([string]::IsNullOrWhiteSpace($outputRoot)) {
        $comparisonPath = Resolve-TalkAsrDefaultWorkflowComparisonJson -BenchmarkPlanOrResult $BenchmarkPlanOrResult
        if ([string]::IsNullOrWhiteSpace($comparisonPath)) {
            throw 'Cannot resolve selected-default-asr-model.json path because benchmark output root and comparison path are unavailable'
        }
        $outputRoot = Split-Path -Parent $comparisonPath
    }

    [System.IO.Path]::GetFullPath((Join-Path $outputRoot 'selected-default-asr-model.json'))
}

function Resolve-TalkAsrDefaultWorkflowEvidenceStatusJson {
    param(
        [Parameter(Mandatory = $true)]$BenchmarkPlanOrResult,
        [string]$EvidenceStatusJson
    )

    if (-not [string]::IsNullOrWhiteSpace($EvidenceStatusJson)) {
        return Resolve-TalkAsrDefaultWorkflowPath -Path $EvidenceStatusJson
    }

    $plan = Get-TalkAsrDefaultWorkflowOptionalProperty -Object $BenchmarkPlanOrResult -Name 'Plan'
    if ($null -eq $plan) {
        $plan = $BenchmarkPlanOrResult
    }

    $outputRoot = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $plan -Name 'OutputRoot')
    if ([string]::IsNullOrWhiteSpace($outputRoot)) {
        $comparisonPath = Resolve-TalkAsrDefaultWorkflowComparisonJson -BenchmarkPlanOrResult $BenchmarkPlanOrResult
        if ([string]::IsNullOrWhiteSpace($comparisonPath)) {
            throw 'Cannot resolve asr-model-evidence-status.json path because benchmark output root and comparison path are unavailable'
        }
        $outputRoot = Split-Path -Parent $comparisonPath
    }

    [System.IO.Path]::GetFullPath((Join-Path $outputRoot 'asr-model-evidence-status.json'))
}

function Write-TalkAsrDefaultWorkflowJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, (($Value | ConvertTo-Json -Depth 10) + [Environment]::NewLine), $utf8NoBom)
}

function Resolve-TalkAsrDefaultWorkflowComparisonJson {
    param([Parameter(Mandatory = $true)]$BenchmarkPlanOrResult)

    $comparisonPath = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $BenchmarkPlanOrResult -Name 'ComparisonPath')
    if (-not [string]::IsNullOrWhiteSpace($comparisonPath)) {
        return [System.IO.Path]::GetFullPath($comparisonPath)
    }

    $plan = Get-TalkAsrDefaultWorkflowOptionalProperty -Object $BenchmarkPlanOrResult -Name 'Plan'
    $comparisonPath = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $plan -Name 'ComparisonPath')
    if (-not [string]::IsNullOrWhiteSpace($comparisonPath)) {
        return [System.IO.Path]::GetFullPath($comparisonPath)
    }

    $null
}

function Resolve-TalkAsrDefaultWorkflowConfigPath {
    param([string]$ConfigPath)

    if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
        return Resolve-TalkAsrDefaultWorkflowPath -Path $ConfigPath
    }

    Resolve-TalkDefaultModelConfigPath -ConfigPath $null
}

function Resolve-TalkAsrDefaultWorkflowApplyModelRoot {
    param(
        [string]$ModelRoot,
        $BenchmarkResult
    )

    if (-not [string]::IsNullOrWhiteSpace($ModelRoot)) {
        return Resolve-TalkAsrDefaultWorkflowPath -Path $ModelRoot
    }

    $plan = Get-TalkAsrDefaultWorkflowOptionalProperty -Object $BenchmarkResult -Name 'Plan'
    $planModelRoot = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $plan -Name 'ModelRoot')
    if (-not [string]::IsNullOrWhiteSpace($planModelRoot)) {
        return [System.IO.Path]::GetFullPath($planModelRoot)
    }

    $null
}

function New-TalkAsrDefaultModelWorkflowPlan {
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
        [string]$SelectionJson,
        [string]$EvidenceStatusJson,
        [string]$ConfigPath,
        [switch]$SkipApply
    )

    $benchmarkPlan = Invoke-TalkAsrCorpusBenchmark `
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
        -FinalTimeoutMs $FinalTimeoutMs `
        -PlanOnly

    $comparisonJson = Resolve-TalkAsrDefaultWorkflowComparisonJson -BenchmarkPlanOrResult $benchmarkPlan
    $selectionJsonPath = Resolve-TalkAsrDefaultWorkflowSelectionJson `
        -BenchmarkPlanOrResult $benchmarkPlan `
        -SelectionJson $SelectionJson
    $evidenceStatusJsonPath = Resolve-TalkAsrDefaultWorkflowEvidenceStatusJson `
        -BenchmarkPlanOrResult $benchmarkPlan `
        -EvidenceStatusJson $EvidenceStatusJson
    $configPathValue = if ($SkipApply) {
        Resolve-TalkAsrDefaultWorkflowOptionalPath -Path $ConfigPath
    } else {
        Resolve-TalkAsrDefaultWorkflowConfigPath -ConfigPath $ConfigPath
    }

    [pscustomobject]@{
        WorkflowKind = 'talk-asr-default-model-workflow-plan'
        BenchmarkPlan = $benchmarkPlan
        ComparisonJson = $comparisonJson
        SelectionJson = $selectionJsonPath
        EvidenceStatusJson = $evidenceStatusJsonPath
        ConfigPath = $configPathValue
        WillApply = (-not $SkipApply.IsPresent)
    }
}

function Invoke-TalkAsrDefaultModelWorkflow {
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
        [string]$SelectionJson,
        [string]$EvidenceStatusJson,
        [string]$ConfigPath,
        [int]$MinSamples = 3,
        [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
        [switch]$AllowMissingCloudBaseline,
        [switch]$AllowSyntheticSampleIds,
        [switch]$SkipApply,
        [switch]$NoBackup,
        [switch]$PlanOnly,
        [switch]$PassThru
    )

    if ($PlanOnly) {
        return New-TalkAsrDefaultModelWorkflowPlan `
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
            -FinalTimeoutMs $FinalTimeoutMs `
            -SelectionJson $SelectionJson `
            -EvidenceStatusJson $EvidenceStatusJson `
            -ConfigPath $ConfigPath `
            -SkipApply:$SkipApply
    }

    $benchmarkResult = Invoke-TalkAsrCorpusBenchmark `
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
        -FinalTimeoutMs $FinalTimeoutMs `
        -StartupTimeoutSeconds $StartupTimeoutSeconds `
        -PassThru

    $comparisonJson = Resolve-TalkAsrDefaultWorkflowComparisonJson -BenchmarkPlanOrResult $benchmarkResult
    if ([string]::IsNullOrWhiteSpace($comparisonJson)) {
        throw 'Talk ASR default workflow requires an asr-model-comparison.json output from Invoke-TalkAsrCorpusBenchmark'
    }

    $selectionJsonPath = Resolve-TalkAsrDefaultWorkflowSelectionJson `
        -BenchmarkPlanOrResult $benchmarkResult `
        -SelectionJson $SelectionJson
    $evidenceStatusJsonPath = Resolve-TalkAsrDefaultWorkflowEvidenceStatusJson `
        -BenchmarkPlanOrResult $benchmarkResult `
        -EvidenceStatusJson $EvidenceStatusJson
    $evidenceStatus = Select-TalkDefaultAsrModel `
        -ComparisonJson $comparisonJson `
        -MinSamples $MinSamples `
        -RequiredLocalModelId $RequiredLocalModelId `
        -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$AllowSyntheticSampleIds `
        -StatusOnly
    Write-TalkAsrDefaultWorkflowJson -Path $evidenceStatusJsonPath -Value $evidenceStatus

    $selection = Select-TalkDefaultAsrModel `
        -ComparisonJson $comparisonJson `
        -OutputJson $selectionJsonPath `
        -MinSamples $MinSamples `
        -RequiredLocalModelId $RequiredLocalModelId `
        -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$AllowSyntheticSampleIds `
        -PassThru

    $selectionOutputJson = [string](Get-TalkAsrDefaultWorkflowOptionalProperty -Object $selection -Name 'outputJson')
    if ([string]::IsNullOrWhiteSpace($selectionOutputJson)) {
        $selectionOutputJson = $selectionJsonPath
    }
    $selectionOutputJson = [System.IO.Path]::GetFullPath($selectionOutputJson)

    $applyResult = $null
    $resolvedConfigPath = if ($SkipApply) {
        Resolve-TalkAsrDefaultWorkflowOptionalPath -Path $ConfigPath
    } else {
        Resolve-TalkAsrDefaultWorkflowConfigPath -ConfigPath $ConfigPath
    }
    if (-not $SkipApply) {
        $applyModelRoot = Resolve-TalkAsrDefaultWorkflowApplyModelRoot `
            -ModelRoot $ModelRoot `
            -BenchmarkResult $benchmarkResult
        $applyResult = Set-TalkDefaultAsrModel `
            -SelectionJson $selectionOutputJson `
            -ConfigPath $resolvedConfigPath `
            -ModelRoot $applyModelRoot `
            -NoBackup:$NoBackup `
            -PassThru
    }

    $result = [pscustomobject]@{
        WorkflowKind = 'talk-asr-default-model-workflow-result'
        BenchmarkResult = $benchmarkResult
        ComparisonJson = $comparisonJson
        EvidenceStatusJson = $evidenceStatusJsonPath
        EvidenceStatus = $evidenceStatus
        SelectionJson = $selectionOutputJson
        Selection = $selection
        Applied = (-not $SkipApply.IsPresent)
        ApplyResult = $applyResult
        ConfigPath = $resolvedConfigPath
    }

    if ($PassThru) {
        return $result
    }

    $result
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkAsrDefaultModelWorkflow `
        -CorpusManifest $workflowEntryCorpusManifest `
        -ModelId $workflowEntryModelId `
        -ModelRoot $workflowEntryModelRoot `
        -OutputRoot $workflowEntryOutputRoot `
        -AsrBenchExe $workflowEntryAsrBenchExe `
        -LocalAsrDaemonExe $workflowEntryLocalAsrDaemonExe `
        -CloudOpenAiCompatibleEndpoint $workflowEntryCloudOpenAiCompatibleEndpoint `
        -CloudOpenAiCompatibleModel $workflowEntryCloudOpenAiCompatibleModel `
        -CloudOpenAiCompatibleTransport $workflowEntryCloudOpenAiCompatibleTransport `
        -CloudOpenAiCompatibleApiKeyEnv $workflowEntryCloudOpenAiCompatibleApiKeyEnv `
        -Bind $workflowEntryBind `
        -ChunkMs $workflowEntryChunkMs `
        -ConnectTimeoutMs $workflowEntryConnectTimeoutMs `
        -ReadyTimeoutMs $workflowEntryReadyTimeoutMs `
        -PartialIdleTimeoutMs $workflowEntryPartialIdleTimeoutMs `
        -FinalTimeoutMs $workflowEntryFinalTimeoutMs `
        -StartupTimeoutSeconds $workflowEntryStartupTimeoutSeconds `
        -SelectionJson $workflowEntrySelectionJson `
        -EvidenceStatusJson $workflowEntryEvidenceStatusJson `
        -ConfigPath $workflowEntryConfigPath `
        -MinSamples $workflowEntryMinSamples `
        -RequiredLocalModelId $workflowEntryRequiredLocalModelId `
        -AllowMissingCloudBaseline:$workflowEntryAllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$workflowEntryAllowSyntheticSampleIds `
        -SkipApply:$workflowEntrySkipApply `
        -NoBackup:$workflowEntryNoBackup `
        -PlanOnly:$workflowEntryPlanOnly `
        -PassThru:$workflowEntryPassThru
}
