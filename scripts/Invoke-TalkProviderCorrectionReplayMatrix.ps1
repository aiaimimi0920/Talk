[CmdletBinding()]
param(
    [string]$AsrBenchRoot,
    [string]$ModelId = 'paraformer-bilingual-zh-en',
    [string]$TalkExe,
    [string]$ConfigPath,
    [string]$Mode = 'transcribe',
    [string]$OutputJson,
    [switch]$UseExistingReplayAsFixture,
    [switch]$PlanOnly,
    [switch]$PassThru,
    [scriptblock]$ReplayInvoker
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$providerReplayMatrixEntryAsrBenchRoot = $AsrBenchRoot
$providerReplayMatrixEntryModelId = $ModelId
$providerReplayMatrixEntryTalkExe = $TalkExe
$providerReplayMatrixEntryConfigPath = $ConfigPath
$providerReplayMatrixEntryMode = $Mode
$providerReplayMatrixEntryOutputJson = $OutputJson
$providerReplayMatrixEntryUseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
$providerReplayMatrixEntryPlanOnly = [bool]$PlanOnly
$providerReplayMatrixEntryPassThru = [bool]$PassThru
$providerReplayMatrixEntryReplayInvoker = $ReplayInvoker

$providerReplayScriptPath = Join-Path $PSScriptRoot 'Invoke-TalkProviderCorrectionReplay.ps1'
if (-not (Test-Path -LiteralPath $providerReplayScriptPath -PathType Leaf)) {
    throw "Missing Talk provider correction replay dependency: $providerReplayScriptPath"
}
$script:TalkProviderCorrectionReplaySkipEntryPoint = $true
try {
    . $providerReplayScriptPath
}
finally {
    Remove-Variable -Name 'TalkProviderCorrectionReplaySkipEntryPoint' -Scope Script -ErrorAction SilentlyContinue
}

function Resolve-TalkProviderCorrectionReplayMatrixPath {
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

function Resolve-TalkProviderCorrectionReplayMatrixOptionalPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    Resolve-TalkProviderCorrectionReplayMatrixPath -Path $Path
}

function Write-TalkProviderCorrectionReplayMatrixJson {
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

function Resolve-TalkProviderCorrectionReplayMatrixAsrBenchRoot {
    param([string]$AsrBenchRoot)

    if (-not [string]::IsNullOrWhiteSpace($AsrBenchRoot)) {
        return Resolve-TalkProviderCorrectionReplayMatrixPath -Path $AsrBenchRoot
    }

    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot '.runtime\asr-bench'))
}

function Resolve-TalkProviderCorrectionReplayMatrixOutputJson {
    param(
        [string]$OutputJson,
        [Parameter(Mandatory = $true)][string]$AsrBenchRoot,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    if (-not [string]::IsNullOrWhiteSpace($OutputJson)) {
        return Resolve-TalkProviderCorrectionReplayMatrixPath -Path $OutputJson
    }

    [System.IO.Path]::GetFullPath((Join-Path $AsrBenchRoot ("provider-correction-replay-matrix-{0}.json" -f $ModelId)))
}

function Get-TalkProviderCorrectionReplayMatrixAverage {
    param([Parameter(Mandatory = $true)]$Values)

    $numbers = @($Values | Where-Object { $null -ne $_ })
    if ($numbers.Count -eq 0) {
        return $null
    }

    $sum = 0.0
    foreach ($number in $numbers) {
        $sum += [double]$number
    }
    $sum / [double]$numbers.Count
}

function Resolve-TalkProviderCorrectionReplayMatrixReportsRoot {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusRoot,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    $candidateNames = @('reports-sherpa-paraformer', 'reports')
    foreach ($candidateName in $candidateNames) {
        $candidateRoot = Join-Path $CorpusRoot $candidateName
        if (-not (Test-Path -LiteralPath $candidateRoot -PathType Container)) {
            continue
        }

        if (Get-ChildItem -LiteralPath $candidateRoot -Filter ("{0}-*.json" -f $ModelId) -File -ErrorAction SilentlyContinue | Select-Object -First 1) {
            return [System.IO.Path]::GetFullPath($candidateRoot)
        }
    }

    $null
}

function New-TalkProviderCorrectionReplayMatrixPlan {
    [CmdletBinding()]
    param(
        [string]$AsrBenchRoot,
        [string]$ModelId = 'paraformer-bilingual-zh-en',
        [string]$TalkExe,
        [string]$ConfigPath,
        [string]$Mode = 'transcribe',
        [string]$OutputJson,
        [switch]$UseExistingReplayAsFixture
    )

    if ([string]::IsNullOrWhiteSpace($ModelId)) {
        throw 'Talk provider correction replay matrix ModelId must not be blank'
    }
    if ([string]::IsNullOrWhiteSpace($Mode)) {
        throw 'Talk provider correction replay matrix Mode must not be blank'
    }

    $resolvedAsrBenchRoot = Resolve-TalkProviderCorrectionReplayMatrixAsrBenchRoot -AsrBenchRoot $AsrBenchRoot
    if (-not (Test-Path -LiteralPath $resolvedAsrBenchRoot -PathType Container)) {
        throw "Talk provider correction replay matrix root does not exist: $resolvedAsrBenchRoot"
    }

    $resolvedTalkExe = Resolve-TalkProviderCorrectionReplayTalkExe -TalkExe $TalkExe
    $resolvedConfigPath = Resolve-TalkProviderCorrectionReplayConfigPath -ConfigPath $ConfigPath
    $resolvedOutputJson = Resolve-TalkProviderCorrectionReplayMatrixOutputJson `
        -OutputJson $OutputJson `
        -AsrBenchRoot $resolvedAsrBenchRoot `
        -ModelId $ModelId

    $entries = @()
    $corpusDirs = Get-ChildItem -LiteralPath $resolvedAsrBenchRoot -Directory |
        Where-Object { $_.Name -like 'real-mic-corpus*' } |
        Sort-Object Name
    foreach ($corpusDir in $corpusDirs) {
        $corpusManifest = Join-Path $corpusDir.FullName 'corpus.json'
        if (-not (Test-Path -LiteralPath $corpusManifest -PathType Leaf)) {
            continue
        }

        $reportsRoot = Resolve-TalkProviderCorrectionReplayMatrixReportsRoot `
            -CorpusRoot $corpusDir.FullName `
            -ModelId $ModelId
        if ($null -eq $reportsRoot) {
            continue
        }

        $entries += [pscustomobject]@{
            CorpusId = $corpusDir.Name
            CorpusManifest = [System.IO.Path]::GetFullPath($corpusManifest)
            ReportsRoot = $reportsRoot
            OutputJson = [System.IO.Path]::GetFullPath((Join-Path $corpusDir.FullName ("provider-correction-replay-{0}.json" -f $ModelId)))
        }
    }

    [pscustomobject]@{
        WorkflowKind = 'talk-provider-correction-replay-matrix-plan'
        AsrBenchRoot = $resolvedAsrBenchRoot
        ModelId = $ModelId
        TalkExe = $resolvedTalkExe
        ConfigPath = $resolvedConfigPath
        Mode = $Mode
        OutputJson = $resolvedOutputJson
        UseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
        Corpora = @($entries)
    }
}

function Invoke-TalkProviderCorrectionReplayMatrix {
    [CmdletBinding()]
    param(
        [string]$AsrBenchRoot,
        [string]$ModelId = 'paraformer-bilingual-zh-en',
        [string]$TalkExe,
        [string]$ConfigPath,
        [string]$Mode = 'transcribe',
        [string]$OutputJson,
        [switch]$UseExistingReplayAsFixture,
        [switch]$PlanOnly,
        [switch]$PassThru,
        [scriptblock]$ReplayInvoker
    )

    $plan = New-TalkProviderCorrectionReplayMatrixPlan `
        -AsrBenchRoot $AsrBenchRoot `
        -ModelId $ModelId `
        -TalkExe $TalkExe `
        -ConfigPath $ConfigPath `
        -Mode $Mode `
        -OutputJson $OutputJson `
        -UseExistingReplayAsFixture:$UseExistingReplayAsFixture

    if ($PlanOnly) {
        return $plan
    }

    $results = @()
    foreach ($corpus in $plan.Corpora) {
        $replay = if ($null -ne $ReplayInvoker) {
            & $ReplayInvoker $corpus $plan
        } else {
            Invoke-TalkProviderCorrectionReplay `
                -CorpusManifest $corpus.CorpusManifest `
                -ReportsRoot $corpus.ReportsRoot `
                -ModelId $plan.ModelId `
                -TalkExe $plan.TalkExe `
                -ConfigPath $plan.ConfigPath `
                -Mode $plan.Mode `
                -OutputJson $corpus.OutputJson `
                -UseExistingReplayAsFixture:$plan.UseExistingReplayAsFixture `
                -PassThru
        }

        $results += [pscustomobject]@{
            corpusId = [string]$corpus.CorpusId
            corpusManifest = [string]$corpus.CorpusManifest
            reportsRoot = [string]$corpus.ReportsRoot
            outputJson = [string]$corpus.OutputJson
            sampleCount = $replay.SampleCount
            exactMatchCount = $replay.ExactMatchCount
            improvedCount = $replay.ImprovedCount
            meanLocalCer = $replay.MeanLocalCer
            meanProcessedCer = $replay.MeanProcessedCer
        }
    }

    $result = [pscustomobject]@{
        workflowKind = 'talk-provider-correction-replay-matrix-result'
        asrBenchRoot = $plan.AsrBenchRoot
        modelId = $plan.ModelId
        talkExe = $plan.TalkExe
        configPath = $plan.ConfigPath
        mode = $plan.Mode
        outputJson = $plan.OutputJson
        corpusCount = $results.Count
        totalSampleCount = @($results | Measure-Object -Property sampleCount -Sum).Sum
        totalExactMatchCount = @($results | Measure-Object -Property exactMatchCount -Sum).Sum
        totalImprovedCount = @($results | Measure-Object -Property improvedCount -Sum).Sum
        meanLocalCer = Get-TalkProviderCorrectionReplayMatrixAverage -Values ($results | ForEach-Object { $_.meanLocalCer })
        meanProcessedCer = Get-TalkProviderCorrectionReplayMatrixAverage -Values ($results | ForEach-Object { $_.meanProcessedCer })
        corpora = @($results)
    }

    Write-TalkProviderCorrectionReplayMatrixJson -Path $plan.OutputJson -Value $result

    $passThruResult = [pscustomobject]@{
        WorkflowKind = 'talk-provider-correction-replay-matrix-result'
        AsrBenchRoot = $result.asrBenchRoot
        ModelId = $result.modelId
        TalkExe = $result.talkExe
        ConfigPath = $result.configPath
        Mode = $result.mode
        OutputJson = $result.outputJson
        UseExistingReplayAsFixture = $plan.UseExistingReplayAsFixture
        CorpusCount = $result.corpusCount
        TotalSampleCount = $result.totalSampleCount
        TotalExactMatchCount = $result.totalExactMatchCount
        TotalImprovedCount = $result.totalImprovedCount
        MeanLocalCer = $result.meanLocalCer
        MeanProcessedCer = $result.meanProcessedCer
        Corpora = $result.corpora
    }

    if ($PassThru) {
        return $passThruResult
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    $invokeResult = Invoke-TalkProviderCorrectionReplayMatrix `
        -AsrBenchRoot $providerReplayMatrixEntryAsrBenchRoot `
        -ModelId $providerReplayMatrixEntryModelId `
        -TalkExe $providerReplayMatrixEntryTalkExe `
        -ConfigPath $providerReplayMatrixEntryConfigPath `
        -Mode $providerReplayMatrixEntryMode `
        -OutputJson $providerReplayMatrixEntryOutputJson `
        -UseExistingReplayAsFixture:$providerReplayMatrixEntryUseExistingReplayAsFixture `
        -PlanOnly:$providerReplayMatrixEntryPlanOnly `
        -PassThru:$providerReplayMatrixEntryPassThru `
        -ReplayInvoker $providerReplayMatrixEntryReplayInvoker

    if ($providerReplayMatrixEntryPlanOnly -or $providerReplayMatrixEntryPassThru) {
        $invokeResult
    }
}
