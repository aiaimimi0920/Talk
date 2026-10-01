[CmdletBinding()]
param(
    [string]$CorpusManifest,
    [string]$ReportsRoot,
    [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
    [string]$TalkExe,
    [string]$OutputJson,
    [switch]$PlanOnly,
    [switch]$PassThru,
    [scriptblock]$ValidatorInvoker
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$entryCorpusManifest = $CorpusManifest
$entryReportsRoot = $ReportsRoot
$entryModelId = $ModelId
$entryTalkExe = $TalkExe
$entryOutputJson = $OutputJson
$entryPlanOnly = [bool]$PlanOnly
$entryPassThru = [bool]$PassThru
$entryValidatorInvoker = $ValidatorInvoker

function Resolve-TalkFaithfulReplayPath {
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

function Resolve-TalkFaithfulReplayOptionalPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    Resolve-TalkFaithfulReplayPath -Path $Path
}

function Write-TalkFaithfulReplayJson {
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

function Resolve-TalkFaithfulReplayDefaultCorpusManifest {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot '.runtime\asr-bench\real-mic-corpus\corpus.json'))
}

function Resolve-TalkFaithfulReplayCorpusManifest {
    param([string]$CorpusManifest)

    if ([string]::IsNullOrWhiteSpace($CorpusManifest)) {
        return Resolve-TalkFaithfulReplayDefaultCorpusManifest
    }

    Resolve-TalkFaithfulReplayPath -Path $CorpusManifest
}

function Resolve-TalkFaithfulReplayReportsRoot {
    param(
        [string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$CorpusManifest
    )

    if ([string]::IsNullOrWhiteSpace($ReportsRoot)) {
        return [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $CorpusManifest) 'reports'))
    }

    Resolve-TalkFaithfulReplayPath -Path $ReportsRoot
}

function Resolve-TalkFaithfulReplayTalkExe {
    param([string]$TalkExe)

    if (-not [string]::IsNullOrWhiteSpace($TalkExe)) {
        return Resolve-TalkFaithfulReplayPath -Path $TalkExe
    }

    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target\release\talk.exe'))
}

function Resolve-TalkFaithfulReplayOutputJson {
    param(
        [string]$OutputJson,
        [Parameter(Mandatory = $true)][string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    if (-not [string]::IsNullOrWhiteSpace($OutputJson)) {
        return Resolve-TalkFaithfulReplayPath -Path $OutputJson
    }

    [System.IO.Path]::GetFullPath((Join-Path $ReportsRoot ("faithful-correction-replay-{0}.json" -f $ModelId)))
}

function Read-TalkFaithfulReplayCorpusManifest {
    param([Parameter(Mandatory = $true)][string]$CorpusManifest)

    if (-not (Test-Path -LiteralPath $CorpusManifest -PathType Leaf)) {
        throw "Talk faithful replay corpus manifest does not exist: $CorpusManifest"
    }

    $manifest = Get-Content -LiteralPath $CorpusManifest -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($null -eq $manifest.samples) {
        throw "Talk faithful replay corpus manifest is missing samples: $CorpusManifest"
    }

    $manifest.samples
}

function Read-TalkFaithfulReplayReport {
    param(
        [Parameter(Mandatory = $true)][string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][string]$SampleId
    )

    $reportPath = [System.IO.Path]::GetFullPath((Join-Path $ReportsRoot ("{0}-{1}.json" -f $ModelId, $SampleId)))
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        throw "Talk faithful replay report is missing: $reportPath"
    }

    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace([string]$report.text)) {
        throw "Talk faithful replay report [$reportPath] does not contain a non-empty text field"
    }

    [pscustomobject]@{
        ReportPath = $reportPath
        Engine = [string]$report.engine
        SampleId = [string]$report.sample_id
        LocalText = [string]$report.text
        Cer = if ($null -ne (Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'cer')) { [double](Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'cer') } else { $null }
        FirstPartialMs = if ($null -ne (Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'first_partial_ms')) { [int](Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'first_partial_ms') } else { $null }
        FinalLatencyMs = if ($null -ne (Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'final_latency_ms')) { [int](Get-TalkFaithfulReplayOptionalProperty -Object $report -Name 'final_latency_ms') } else { $null }
    }
}

function Get-TalkFaithfulReplayOptionalProperty {
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

function Invoke-TalkFaithfulReplayValidator {
    param(
        [Parameter(Mandatory = $true)][string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$InputText,
        [Parameter(Mandatory = $true)][string]$OutputText
    )

    if (-not (Test-Path -LiteralPath $TalkExe -PathType Leaf)) {
        throw "Talk faithful replay validator executable does not exist: $TalkExe"
    }

    $output = & $TalkExe `
        'validate-faithful-output' `
        '--input-text' $InputText `
        '--output-text' $OutputText `
        '--json' 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Talk faithful replay validator failed with exit code $exitCode`: $TalkExe validate-faithful-output --json`n$output"
    }

    (($output | Out-String) | ConvertFrom-Json).validation
}

function Normalize-TalkFaithfulReplayValidation {
    param([Parameter(Mandatory = $true)]$Validation)

    $accepted = [bool](Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'accepted')
    $fallbackReason = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'fallbackReason'
    if ($null -eq $fallbackReason) {
        $fallbackReason = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'fallback_reason'
    }

    [pscustomobject]@{
        accepted = $accepted
        fallbackReason = if ([string]::IsNullOrWhiteSpace([string]$fallbackReason)) { $null } else { [string]$fallbackReason }
        inputCharCount = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'inputCharCount'
        outputCharCount = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'outputCharCount'
        retentionRatio = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'retentionRatio'
        normalizedChangeRatio = Get-TalkFaithfulReplayOptionalProperty -Object $Validation -Name 'normalizedChangeRatio'
    }
}

function New-TalkFaithfulCorrectionReplayPlan {
    [CmdletBinding()]
    param(
        [string]$CorpusManifest,
        [string]$ReportsRoot,
        [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
        [string]$TalkExe,
        [string]$OutputJson
    )

    if ([string]::IsNullOrWhiteSpace($ModelId)) {
        throw 'Talk faithful replay ModelId must not be blank'
    }

    $resolvedCorpusManifest = Resolve-TalkFaithfulReplayCorpusManifest -CorpusManifest $CorpusManifest
    $resolvedReportsRoot = Resolve-TalkFaithfulReplayReportsRoot -ReportsRoot $ReportsRoot -CorpusManifest $resolvedCorpusManifest
    $resolvedTalkExe = Resolve-TalkFaithfulReplayTalkExe -TalkExe $TalkExe
    $resolvedOutputJson = Resolve-TalkFaithfulReplayOutputJson -OutputJson $OutputJson -ReportsRoot $resolvedReportsRoot -ModelId $ModelId
    $samples = @(Read-TalkFaithfulReplayCorpusManifest -CorpusManifest $resolvedCorpusManifest)

    $replays = New-Object System.Collections.Generic.List[object]
    foreach ($sample in $samples) {
        $report = Read-TalkFaithfulReplayReport `
            -ReportsRoot $resolvedReportsRoot `
            -ModelId $ModelId `
            -SampleId ([string]$sample.sampleId)
        $replays.Add([pscustomobject]@{
                SampleId = [string]$sample.sampleId
                ReportPath = [string]$report.ReportPath
                Engine = [string]$report.Engine
                LocalText = [string]$report.LocalText
                ReferenceText = [string]$sample.referenceText
                Cer = $report.Cer
                FirstPartialMs = $report.FirstPartialMs
                FinalLatencyMs = $report.FinalLatencyMs
            }) | Out-Null
    }

    [pscustomobject]@{
        WorkflowKind = 'talk-faithful-correction-replay-plan'
        CorpusManifest = $resolvedCorpusManifest
        ReportsRoot = $resolvedReportsRoot
        ModelId = $ModelId
        TalkExe = $resolvedTalkExe
        OutputJson = $resolvedOutputJson
        Replays = $replays.ToArray()
    }
}

function Invoke-TalkFaithfulCorrectionReplay {
    [CmdletBinding()]
    param(
        [string]$CorpusManifest,
        [string]$ReportsRoot,
        [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
        [string]$TalkExe,
        [string]$OutputJson,
        [switch]$PlanOnly,
        [switch]$PassThru,
        [scriptblock]$ValidatorInvoker
    )

    $plan = New-TalkFaithfulCorrectionReplayPlan `
        -CorpusManifest $CorpusManifest `
        -ReportsRoot $ReportsRoot `
        -ModelId $ModelId `
        -TalkExe $TalkExe `
        -OutputJson $OutputJson

    if ($PlanOnly) {
        return $plan
    }

    $samples = New-Object System.Collections.Generic.List[object]
    $acceptedCount = 0
    foreach ($replay in $plan.Replays) {
        $validation = if ($null -ne $ValidatorInvoker) {
            & $ValidatorInvoker ([string]$replay.LocalText) ([string]$replay.ReferenceText)
        } else {
            Invoke-TalkFaithfulReplayValidator `
                -TalkExe $plan.TalkExe `
                -InputText ([string]$replay.LocalText) `
                -OutputText ([string]$replay.ReferenceText)
        }
        $normalized = Normalize-TalkFaithfulReplayValidation -Validation $validation
        if ($normalized.accepted) {
            $acceptedCount++
        }

        $samples.Add([pscustomobject]@{
                sampleId = [string]$replay.SampleId
                reportPath = [string]$replay.ReportPath
                engine = [string]$replay.Engine
                localText = [string]$replay.LocalText
                referenceText = [string]$replay.ReferenceText
                cer = $replay.Cer
                firstPartialMs = $replay.FirstPartialMs
                finalLatencyMs = $replay.FinalLatencyMs
                validation = $normalized
            }) | Out-Null
    }

    $sampleCount = $samples.Count
    $rejectedCount = $sampleCount - $acceptedCount
    $result = [pscustomobject]@{
        workflowKind = 'talk-faithful-correction-replay-result'
        corpusManifest = $plan.CorpusManifest
        reportsRoot = $plan.ReportsRoot
        modelId = $plan.ModelId
        talkExe = $plan.TalkExe
        outputJson = $plan.OutputJson
        sampleCount = $sampleCount
        acceptedCount = $acceptedCount
        rejectedCount = $rejectedCount
        allAccepted = ($sampleCount -gt 0 -and $rejectedCount -eq 0)
        samples = $samples.ToArray()
    }
    Write-TalkFaithfulReplayJson -Path $plan.OutputJson -Value $result

    $passThruResult = [pscustomobject]@{
        WorkflowKind = 'talk-faithful-correction-replay-result'
        CorpusManifest = $plan.CorpusManifest
        ReportsRoot = $plan.ReportsRoot
        ModelId = $plan.ModelId
        TalkExe = $plan.TalkExe
        OutputJson = $plan.OutputJson
        SampleCount = $sampleCount
        AcceptedCount = $acceptedCount
        RejectedCount = $rejectedCount
        AllAccepted = ($sampleCount -gt 0 -and $rejectedCount -eq 0)
        Samples = $samples.ToArray()
    }

    if ($PassThru) {
        return $passThruResult
    }

    $passThruResult
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkFaithfulCorrectionReplay `
        -CorpusManifest $entryCorpusManifest `
        -ReportsRoot $entryReportsRoot `
        -ModelId $entryModelId `
        -TalkExe $entryTalkExe `
        -OutputJson $entryOutputJson `
        -PlanOnly:$entryPlanOnly `
        -PassThru:$entryPassThru `
        -ValidatorInvoker $entryValidatorInvoker
}
