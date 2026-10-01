[CmdletBinding()]
param(
    [string]$ComparisonJson,
    [string]$OutputJson,
    [int]$MinSamples = 3,
    [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
    [switch]$AllowMissingCloudBaseline,
    [switch]$AllowSyntheticSampleIds,
    [switch]$StatusOnly,
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$entryComparisonJson = $ComparisonJson
$entryOutputJson = $OutputJson
$entryMinSamples = $MinSamples
$entryRequiredLocalModelId = $RequiredLocalModelId
$entryAllowMissingCloudBaseline = [bool]$AllowMissingCloudBaseline
$entryAllowSyntheticSampleIds = [bool]$AllowSyntheticSampleIds
$entryStatusOnly = [bool]$StatusOnly
$entryPassThru = [bool]$PassThru

function Get-TalkDefaultAsrJsonProperty {
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

function Get-TalkDefaultAsrOptionalJsonProperty {
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

function Assert-TalkDefaultAsrSafeId {
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

function Test-TalkDefaultAsrCloudCandidate {
    param([Parameter(Mandatory = $true)]$Candidate)

    ([string]$Candidate.Engine).StartsWith('cloud_openai_compatible:', [System.StringComparison]::OrdinalIgnoreCase)
}

function Resolve-TalkDefaultAsrCandidateModelId {
    param([Parameter(Mandatory = $true)]$Candidate)

    if (Test-TalkDefaultAsrCloudCandidate -Candidate $Candidate) {
        return $null
    }

    $fingerprintParts = New-Object System.Collections.Generic.List[string]
    $fingerprintParts.Add([string]$Candidate.Engine) | Out-Null
    foreach ($source in @($Candidate.Sources)) {
        $sourceText = [string]$source
        if ([string]::IsNullOrWhiteSpace($sourceText)) {
            continue
        }
        $fingerprintParts.Add($sourceText) | Out-Null
        $sourceLeaf = [System.IO.Path]::GetFileName($sourceText)
        if (-not [string]::IsNullOrWhiteSpace($sourceLeaf) -and $sourceLeaf -cne $sourceText) {
            $fingerprintParts.Add($sourceLeaf) | Out-Null
        }
    }
    $fingerprint = (($fingerprintParts.ToArray() -join ' ').ToLowerInvariant())

    if ($fingerprint.Contains('sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10')) {
        return 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10'
    }
    if ($fingerprint -match 'paraformer-bilingual-zh-en|streaming-paraformer-bilingual-zh-en') {
        return 'paraformer-bilingual-zh-en'
    }
    if ($fingerprint -match 'zipformer-zh-en-punct-int8-480ms|480ms-streaming-zipformer|x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8') {
        return 'zipformer-zh-en-punct-int8-480ms'
    }
    if ($fingerprint -match 'streaming-paraformer|paraformer') {
        return 'paraformer-bilingual-zh-en'
    }
    if ($fingerprint -match 'zipformer') {
        return 'zipformer-zh-en-punct-int8-480ms'
    }

    $null
}

function ConvertTo-TalkDefaultAsrCandidate {
    param(
        [Parameter(Mandatory = $true)]$RawCandidate,
        [Parameter(Mandatory = $true)][string]$Context
    )

    $engine = [string](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'engine' -Context $Context)
    if ([string]::IsNullOrWhiteSpace($engine)) {
        throw "$Context engine must not be blank"
    }
    if ($engine.Trim() -ne $engine) {
        throw "$Context engine must not have leading or trailing whitespace"
    }

    $sampleCount = [int](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'sample_count' -Context $Context)
    $sampleIds = @(
        Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'sample_ids' -Context $Context
    ) | ForEach-Object { [string]$_ }
    foreach ($sampleId in $sampleIds) {
        Assert-TalkDefaultAsrSafeId -Value $sampleId -Name "$Context sample_id"
    }

    $sources = @(
        Get-TalkDefaultAsrOptionalJsonProperty -Object $RawCandidate -Name 'sources'
    ) | Where-Object { $null -ne $_ } | ForEach-Object { [string]$_ }
    $candidate = [pscustomobject]@{
        Engine = $engine
        Sources = $sources
        SampleCount = $sampleCount
        SampleIds = $sampleIds
        Cer = [double](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'cer' -Context $Context)
        FirstPartialMs = [int](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'first_partial_ms' -Context $Context)
        FinalLatencyMs = [int](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'final_latency_ms' -Context $Context)
        Rtf = [double](Get-TalkDefaultAsrJsonProperty -Object $RawCandidate -Name 'rtf' -Context $Context)
        PeakRssMb = [int](Get-TalkDefaultAsrOptionalJsonProperty -Object $RawCandidate -Name 'peak_rss_mb')
        ModelSizeMb = Get-TalkDefaultAsrOptionalJsonProperty -Object $RawCandidate -Name 'model_size_mb'
    }
    $candidate | Add-Member -NotePropertyName IsCloudBaseline -NotePropertyValue (Test-TalkDefaultAsrCloudCandidate -Candidate $candidate)
    $candidate | Add-Member -NotePropertyName ModelId -NotePropertyValue (Resolve-TalkDefaultAsrCandidateModelId -Candidate $candidate)
    $candidate
}

function Read-TalkDefaultAsrComparison {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$ComparisonJson)

    $resolvedComparisonJson = [System.IO.Path]::GetFullPath($ComparisonJson)
    if (-not (Test-Path -LiteralPath $resolvedComparisonJson -PathType Leaf)) {
        throw "Talk ASR comparison json does not exist: $resolvedComparisonJson"
    }

    $comparison = Get-Content -LiteralPath $resolvedComparisonJson -Raw -Encoding UTF8 | ConvertFrom-Json
    $selectedEngine = [string](Get-TalkDefaultAsrJsonProperty -Object $comparison -Name 'selected_engine' -Context $resolvedComparisonJson)
    $rawCandidates = @(Get-TalkDefaultAsrJsonProperty -Object $comparison -Name 'candidates' -Context $resolvedComparisonJson)
    if ($rawCandidates.Count -eq 0) {
        throw "Talk ASR comparison json has no candidates: $resolvedComparisonJson"
    }

    $candidates = New-Object System.Collections.Generic.List[object]
    for ($index = 0; $index -lt $rawCandidates.Count; $index += 1) {
        $candidates.Add((ConvertTo-TalkDefaultAsrCandidate `
            -RawCandidate $rawCandidates[$index] `
            -Context "$resolvedComparisonJson candidates[$index]")) | Out-Null
    }

    [pscustomobject]@{
        Path = $resolvedComparisonJson
        SelectedEngine = $selectedEngine
        Candidates = $candidates.ToArray()
    }
}

function Assert-TalkDefaultAsrSelectionEvidence {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Comparison,
        [Parameter(Mandatory = $true)][int]$MinSamples,
        [Parameter(Mandatory = $true)][string[]]$RequiredLocalModelId,
        [switch]$AllowMissingCloudBaseline,
        [switch]$AllowSyntheticSampleIds
    )

    $status = Get-TalkDefaultAsrEvidenceStatus `
        -Comparison $Comparison `
        -MinSamples $MinSamples `
        -RequiredLocalModelId $RequiredLocalModelId `
        -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$AllowSyntheticSampleIds

    if (-not $status.ready) {
        throw ((@($status.blockingReasons) -join [Environment]::NewLine))
    }
}

function Get-TalkDefaultAsrDuplicateSampleIds {
    param([Parameter(Mandatory = $true)][string[]]$SampleIds)

    @(
        $SampleIds |
            Group-Object |
            Where-Object { $_.Count -gt 1 } |
            ForEach-Object { [string]$_.Name }
    )
}

function Get-TalkDefaultAsrSyntheticSampleIds {
    param([Parameter(Mandatory = $true)][string[]]$SampleIds)

    @($SampleIds | Where-Object { $_ -match '(?i)(huihui|tts|synthetic|smoke)' })
}

function Get-TalkDefaultAsrRequiredDefaultSampleIds {
    @(
        'short-search-001',
        'mixed-english-001',
        'mixed-english-japanese-001',
        'proper-nouns-001',
        'punctuation-longform-001',
        'noise-realistic-001'
    )
}

function Get-TalkDefaultAsrLatencyBudget {
    [pscustomobject]@{
        MaxFirstPartialMs = 750
        MaxRtf = 0.60
    }
}

function Get-TalkDefaultAsrSemanticValidityBudget {
    [pscustomobject]@{
        MaxCloudBaselineCer = 0.60
    }
}

function New-TalkDefaultAsrCandidateEvidenceStatus {
    param(
        [Parameter(Mandatory = $true)]$Candidate,
        [Parameter(Mandatory = $true)][int]$MinSamples,
        [string]$BaselineSampleIdSetKey,
        [string]$BaselineSampleIdSetEngine,
        [switch]$AllowSyntheticSampleIds,
        [Parameter(Mandatory = $true)]$LatencyBudget
    )

    $candidateSampleIds = @($Candidate.SampleIds | ForEach-Object { [string]$_ })
    $sortedUniqueSampleIds = @($candidateSampleIds | Sort-Object -Unique)
    $sampleIdSetKey = ($sortedUniqueSampleIds -join "`n")
    $duplicateSampleIds = @(Get-TalkDefaultAsrDuplicateSampleIds -SampleIds $candidateSampleIds)
    $syntheticSampleIds = @(Get-TalkDefaultAsrSyntheticSampleIds -SampleIds $candidateSampleIds)
    $blockingReasons = New-Object System.Collections.Generic.List[string]
    $selectionBlockingReasons = New-Object System.Collections.Generic.List[string]

    if ($Candidate.SampleCount -lt $MinSamples) {
        $blockingReasons.Add("Candidate [$($Candidate.Engine)] sample_count [$($Candidate.SampleCount)] is less than MinSamples [$MinSamples]") | Out-Null
    }
    if ($candidateSampleIds.Count -ne $Candidate.SampleCount) {
        $blockingReasons.Add("Candidate [$($Candidate.Engine)] must have one sample_id for every sample_count entry") | Out-Null
    }
    if ($duplicateSampleIds.Count -gt 0) {
        $blockingReasons.Add("Candidate [$($Candidate.Engine)] contains duplicate sample_id values; same-corpus evidence requires a unique sample id set") | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($BaselineSampleIdSetKey) -and $sampleIdSetKey -ne $BaselineSampleIdSetKey) {
        $blockingReasons.Add("Task 6 default ASR selection requires every candidate to use the same sample id set; candidate [$($Candidate.Engine)] differs from [$BaselineSampleIdSetEngine]") | Out-Null
    }
    if (-not $AllowSyntheticSampleIds) {
        foreach ($sampleId in $syntheticSampleIds) {
            $blockingReasons.Add("Candidate [$($Candidate.Engine)] uses synthetic or smoke sample_id [$sampleId]; real microphone evidence is required to lock the default model") | Out-Null
        }
    }

    $withinLatencyBudget = $true
    if (-not $Candidate.IsCloudBaseline) {
        if ($Candidate.FirstPartialMs -gt $LatencyBudget.MaxFirstPartialMs) {
            $withinLatencyBudget = $false
            $selectionBlockingReasons.Add("Candidate [$($Candidate.Engine)] first_partial_ms [$($Candidate.FirstPartialMs)] exceeds MaxFirstPartialMs [$($LatencyBudget.MaxFirstPartialMs)] for the default live dictation path") | Out-Null
        }
        # asr-bench streams corpus WAVs as fast as the loopback service can
        # consume them, so absolute final_latency_ms scales with utterance
        # length and is not a stable live-dictation gate across mixed samples.
        if ($Candidate.Rtf -gt $LatencyBudget.MaxRtf) {
            $withinLatencyBudget = $false
            $selectionBlockingReasons.Add("Candidate [$($Candidate.Engine)] rtf [$($Candidate.Rtf)] exceeds MaxRtf [$($LatencyBudget.MaxRtf)] for the default live dictation path") | Out-Null
        }
    }

    [pscustomobject]@{
        engine = [string]$Candidate.Engine
        modelId = $Candidate.ModelId
        isCloudBaseline = [bool]$Candidate.IsCloudBaseline
        sampleCount = [int]$Candidate.SampleCount
        sampleIds = @($candidateSampleIds)
        enoughSamples = ($Candidate.SampleCount -ge $MinSamples)
        sampleIdCountMatches = ($candidateSampleIds.Count -eq $Candidate.SampleCount)
        uniqueSampleIds = ($duplicateSampleIds.Count -eq 0)
        duplicateSampleIds = @($duplicateSampleIds)
        sameSampleSet = ([string]::IsNullOrWhiteSpace($BaselineSampleIdSetKey) -or $sampleIdSetKey -eq $BaselineSampleIdSetKey)
        syntheticSampleIds = @($syntheticSampleIds)
        withinLatencyBudget = $withinLatencyBudget
        ready = ($blockingReasons.Count -eq 0 -and $selectionBlockingReasons.Count -eq 0)
        blockingReasons = @($blockingReasons.ToArray())
        selectionBlockingReasons = @($selectionBlockingReasons.ToArray())
    }
}

function ConvertTo-TalkDefaultAsrOutputCandidate {
    param([Parameter(Mandatory = $true)]$Candidate)

    [ordered]@{
        modelId = $Candidate.ModelId
        engine = $Candidate.Engine
        sampleCount = $Candidate.SampleCount
        sampleIds = @($Candidate.SampleIds)
        cer = $Candidate.Cer
        firstPartialMs = $Candidate.FirstPartialMs
        finalLatencyMs = $Candidate.FinalLatencyMs
        rtf = $Candidate.Rtf
        peakRssMb = $Candidate.PeakRssMb
        modelSizeMb = $Candidate.ModelSizeMb
        sources = @($Candidate.Sources)
    }
}

function Sort-TalkDefaultAsrLocalCandidates {
    param([Parameter(Mandatory = $true)][object[]]$Candidates)

    @($Candidates) | Sort-Object -Property `
        @{ Expression = { [double]$_.Cer }; Ascending = $true }, `
        @{ Expression = { [int]$_.FirstPartialMs }; Ascending = $true }, `
        @{ Expression = { [int]$_.FinalLatencyMs }; Ascending = $true }, `
        @{ Expression = { [double]$_.Rtf }; Ascending = $true }, `
        @{ Expression = { [int]$_.PeakRssMb }; Ascending = $true }, `
        @{ Expression = {
            if ($null -eq $_.ModelSizeMb) {
                [double]::PositiveInfinity
            } else {
                [double]$_.ModelSizeMb
            }
        }; Ascending = $true }, `
        @{ Expression = { [string]$_.Engine }; Ascending = $true }
}

function Get-TalkDefaultAsrEvidenceStatus {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Comparison,
        [Parameter(Mandatory = $true)][int]$MinSamples,
        [Parameter(Mandatory = $true)][string[]]$RequiredLocalModelId,
        [switch]$AllowMissingCloudBaseline,
        [switch]$AllowSyntheticSampleIds
    )

    if ($MinSamples -le 0) {
        throw 'MinSamples must be greater than 0'
    }
    foreach ($modelId in $RequiredLocalModelId) {
        Assert-TalkDefaultAsrSafeId -Value $modelId -Name 'RequiredLocalModelId'
    }

    $cloudCandidates = @($Comparison.Candidates | Where-Object { $_.IsCloudBaseline })
    $localCandidates = @($Comparison.Candidates | Where-Object { -not $_.IsCloudBaseline })
    $blockingReasons = New-Object System.Collections.Generic.List[string]
    $latencyBudget = Get-TalkDefaultAsrLatencyBudget
    $semanticValidityBudget = Get-TalkDefaultAsrSemanticValidityBudget

    if ($cloudCandidates.Count -eq 0 -and -not $AllowMissingCloudBaseline) {
        $blockingReasons.Add('Task 6 default ASR selection requires a cloud OpenAI-compatible baseline candidate; rerun Invoke-TalkAsrCorpusBenchmark with cloud baseline flags or pass -AllowMissingCloudBaseline for diagnostics only') | Out-Null
    }
    if ($localCandidates.Count -eq 0) {
        $blockingReasons.Add('Talk ASR comparison contains no local streaming candidates') | Out-Null
    }
    $semanticallyValidCloudCandidates = @(
        $cloudCandidates |
            Where-Object { [double]$_.Cer -le [double]$semanticValidityBudget.MaxCloudBaselineCer }
    )
    if ($cloudCandidates.Count -gt 0 -and $semanticallyValidCloudCandidates.Count -eq 0) {
        $cloudCerSummary = @(
            $cloudCandidates |
                ForEach-Object { '{0}={1}' -f ([string]$_.Engine), ([double]$_.Cer) }
        ) -join ', '
        $blockingReasons.Add(
            "Talk default ASR evidence is semantically invalid: every cloud baseline CER exceeds MaxCloudBaselineCer [$($semanticValidityBudget.MaxCloudBaselineCer)]; corpus audio may not match its reference text; candidates=[$cloudCerSummary]"
        ) | Out-Null
    }

    $baselineSampleIdSetKey = $null
    $baselineSampleIdSetEngine = $null
    if ($Comparison.Candidates.Count -gt 0) {
        $firstCandidate = $Comparison.Candidates[0]
        $baselineSampleIdSetKey = (@($firstCandidate.SampleIds | ForEach-Object { [string]$_ }) | Sort-Object -Unique) -join "`n"
        $baselineSampleIdSetEngine = [string]$firstCandidate.Engine
    }

    $candidateStatuses = @(
        foreach ($candidate in @($Comparison.Candidates)) {
            New-TalkDefaultAsrCandidateEvidenceStatus `
                -Candidate $candidate `
                -MinSamples $MinSamples `
                -BaselineSampleIdSetKey $baselineSampleIdSetKey `
                -BaselineSampleIdSetEngine $baselineSampleIdSetEngine `
                -AllowSyntheticSampleIds:$AllowSyntheticSampleIds `
                -LatencyBudget $latencyBudget
        }
    )
    foreach ($candidateStatus in $candidateStatuses) {
        foreach ($reason in @($candidateStatus.blockingReasons)) {
            $blockingReasons.Add([string]$reason) | Out-Null
        }
    }

    $requiredDefaultSampleIds = @(Get-TalkDefaultAsrRequiredDefaultSampleIds)
    $baselineSampleIds = if ($Comparison.Candidates.Count -gt 0) {
        @($Comparison.Candidates[0].SampleIds | ForEach-Object { [string]$_ } | Sort-Object -Unique)
    } else {
        @()
    }
    $missingRequiredDefaultSampleIds = @(
        foreach ($sampleId in $requiredDefaultSampleIds) {
            if ($baselineSampleIds -notcontains $sampleId) {
                [string]$sampleId
            }
        }
    )
    if ($missingRequiredDefaultSampleIds.Count -gt 0) {
        $blockingReasons.Add(("Talk default ASR selection requires the shared benchmark sample id set to include the full required default sample set; missing [{0}]" -f ($missingRequiredDefaultSampleIds -join ', '))) | Out-Null
    }

    $missingLocalModelIds = @(
        foreach ($modelId in $RequiredLocalModelId) {
            $matchingCandidates = @($localCandidates | Where-Object { $_.ModelId -eq $modelId })
            if ($matchingCandidates.Count -eq 0) {
                $blockingReasons.Add("Task 6 default ASR selection requires local model evidence for [$modelId]") | Out-Null
                [string]$modelId
            }
        }
    )

    $rankedLocalCandidates = @(Sort-TalkDefaultAsrLocalCandidates -Candidates $localCandidates)
    $eligibleLocalCandidateEngines = @(
        foreach ($candidateStatus in $candidateStatuses) {
            if (-not $candidateStatus.isCloudBaseline -and $candidateStatus.ready) {
                [string]$candidateStatus.engine
            }
        }
    )
    $eligibleLocalCandidates = @(
        foreach ($candidate in $rankedLocalCandidates) {
            if ($eligibleLocalCandidateEngines -contains [string]$candidate.Engine) {
                $candidate
            }
        }
    )
    $rejectedCandidates = @(
        foreach ($candidateStatus in $candidateStatuses) {
            if ($candidateStatus.isCloudBaseline -or $candidateStatus.ready) {
                continue
            }
            [ordered]@{
                modelId = $candidateStatus.modelId
                engine = $candidateStatus.engine
                blockingReasons = @(
                    @($candidateStatus.blockingReasons) +
                    @($candidateStatus.selectionBlockingReasons)
                )
            }
        }
    )
    if ($localCandidates.Count -gt 0 -and $eligibleLocalCandidates.Count -eq 0) {
        $blockingReasons.Add('Talk default ASR selection requires at least one local candidate that satisfies the live default latency budget and evidence gates') | Out-Null
    }

    $ready = ($blockingReasons.Count -eq 0)
    $selectedLocalCandidate = if ($ready) { $eligibleLocalCandidates | Select-Object -First 1 } else { $null }

    [pscustomobject]@{
        schemaVersion = 1
        kind = 'talk-default-asr-model-evidence-status'
        ready = $ready
        comparisonJson = [string]$Comparison.Path
        minSamples = $MinSamples
        requiredLocalModelIds = @($RequiredLocalModelId)
        requiredDefaultSampleIds = @($requiredDefaultSampleIds)
        candidateCount = @($Comparison.Candidates).Count
        localCandidateCount = $localCandidates.Count
        cloudBaselinePresent = ($cloudCandidates.Count -gt 0)
        cloudBaselineEngines = @($cloudCandidates | ForEach-Object { [string]$_.Engine })
        maxCloudBaselineCer = [double]$semanticValidityBudget.MaxCloudBaselineCer
        semanticallyValidCloudBaselineEngines = @(
            $semanticallyValidCloudCandidates | ForEach-Object { [string]$_.Engine }
        )
        cloudBaselineSemanticValidity = if ($cloudCandidates.Count -eq 0) {
            'skipped'
        } elseif ($semanticallyValidCloudCandidates.Count -gt 0) {
            'ready'
        } else {
            'invalid'
        }
        missingLocalModelIds = @($missingLocalModelIds)
        missingRequiredDefaultSampleIds = @($missingRequiredDefaultSampleIds)
        sharedSampleIds = @($baselineSampleIds)
        selectedModelId = if ($null -ne $selectedLocalCandidate) { $selectedLocalCandidate.ModelId } else { $null }
        selectedEngine = if ($null -ne $selectedLocalCandidate) { [string]$selectedLocalCandidate.Engine } else { $null }
        candidateStatuses = @($candidateStatuses)
        eligibleLocalCandidateEngines = @($eligibleLocalCandidateEngines)
        rejectedCandidates = @($rejectedCandidates)
        rankedLocalCandidates = @($rankedLocalCandidates | ForEach-Object { ConvertTo-TalkDefaultAsrOutputCandidate -Candidate $_ })
        blockingReasons = @($blockingReasons.ToArray() | Select-Object -Unique)
    }
}

function Resolve-TalkDefaultAsrSelectionOutputJson {
    param(
        [Parameter(Mandatory = $true)][string]$ComparisonJson,
        [string]$OutputJson
    )

    if (-not [string]::IsNullOrWhiteSpace($OutputJson)) {
        return [System.IO.Path]::GetFullPath($OutputJson)
    }

    $comparisonDir = Split-Path -Parent ([System.IO.Path]::GetFullPath($ComparisonJson))
    [System.IO.Path]::GetFullPath((Join-Path $comparisonDir 'selected-default-asr-model.json'))
}

function Write-TalkDefaultAsrSelectionJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Selection
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, (($Selection | ConvertTo-Json -Depth 8) + [Environment]::NewLine), $utf8NoBom)
}

function Select-TalkDefaultAsrModel {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ComparisonJson,
        [string]$OutputJson,
        [int]$MinSamples = 3,
        [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
        [switch]$AllowMissingCloudBaseline,
        [switch]$AllowSyntheticSampleIds,
        [switch]$StatusOnly,
        [switch]$PassThru
    )

    $comparison = Read-TalkDefaultAsrComparison -ComparisonJson $ComparisonJson
    if ($StatusOnly) {
        return Get-TalkDefaultAsrEvidenceStatus `
            -Comparison $comparison `
            -MinSamples $MinSamples `
            -RequiredLocalModelId $RequiredLocalModelId `
            -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
            -AllowSyntheticSampleIds:$AllowSyntheticSampleIds
    }

    Assert-TalkDefaultAsrSelectionEvidence `
        -Comparison $comparison `
        -MinSamples $MinSamples `
        -RequiredLocalModelId $RequiredLocalModelId `
        -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$AllowSyntheticSampleIds

    $status = Get-TalkDefaultAsrEvidenceStatus `
        -Comparison $comparison `
        -MinSamples $MinSamples `
        -RequiredLocalModelId $RequiredLocalModelId `
        -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$AllowSyntheticSampleIds

    $localCandidates = @(Sort-TalkDefaultAsrLocalCandidates -Candidates @(
        $comparison.Candidates | Where-Object { -not $_.IsCloudBaseline }
    ))
    $selectedLocalCandidate = $localCandidates | Where-Object { $_.ModelId -eq $status.selectedModelId } | Select-Object -First 1
    if ($null -eq $selectedLocalCandidate) {
        throw 'Talk ASR comparison contains no selectable local candidate'
    }
    if ([string]::IsNullOrWhiteSpace([string]$selectedLocalCandidate.ModelId)) {
        throw "Cannot resolve Talk model id for selected local engine [$($selectedLocalCandidate.Engine)]"
    }

    $cloudCandidates = @($comparison.Candidates | Where-Object { $_.IsCloudBaseline })
    $outputPath = Resolve-TalkDefaultAsrSelectionOutputJson `
        -ComparisonJson $comparison.Path `
        -OutputJson $OutputJson
    $selection = [ordered]@{
        schemaVersion = 1
        kind = 'talk-default-asr-model-selection'
        evidenceReady = $true
        comparisonJson = $comparison.Path
        outputJson = $outputPath
        minSamples = $MinSamples
        requiredLocalModelIds = @($RequiredLocalModelId)
        requiredDefaultSampleIds = @($status.requiredDefaultSampleIds)
        globalSelectedEngine = $comparison.SelectedEngine
        selectedModelId = $selectedLocalCandidate.ModelId
        selectedEngine = $selectedLocalCandidate.Engine
        cloudBaselinePresent = ($cloudCandidates.Count -gt 0)
        cloudBaselineEngines = @($cloudCandidates | ForEach-Object { $_.Engine })
        rejectedCandidates = @($status.rejectedCandidates)
        rankedLocalCandidates = @($localCandidates | ForEach-Object { ConvertTo-TalkDefaultAsrOutputCandidate -Candidate $_ })
    }

    Write-TalkDefaultAsrSelectionJson -Path $outputPath -Selection $selection

    if ($PassThru) {
        return [pscustomobject]$selection
    }

    [pscustomobject]$selection
}

if ($MyInvocation.InvocationName -ne '.') {
    Select-TalkDefaultAsrModel `
        -ComparisonJson $entryComparisonJson `
        -OutputJson $entryOutputJson `
        -MinSamples $entryMinSamples `
        -RequiredLocalModelId $entryRequiredLocalModelId `
        -AllowMissingCloudBaseline:$entryAllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$entryAllowSyntheticSampleIds `
        -StatusOnly:$entryStatusOnly `
        -PassThru:$entryPassThru
}
