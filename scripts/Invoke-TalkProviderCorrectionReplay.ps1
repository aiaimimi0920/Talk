[CmdletBinding()]
param(
    [string]$CorpusManifest,
    [string]$ReportsRoot,
    [string]$ModelId = 'paraformer-bilingual-zh-en',
    [string]$TalkExe,
    [string]$ConfigPath,
    [string]$Mode = 'transcribe',
    [double]$MaxAutoPatchEditRatio = 0.35,
    [string]$OutputJson,
    [switch]$UseExistingReplayAsFixture,
    [switch]$PlanOnly,
    [switch]$PassThru,
    [scriptblock]$ProcessorInvoker
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$providerReplayEntryCorpusManifest = $CorpusManifest
$providerReplayEntryReportsRoot = $ReportsRoot
$providerReplayEntryModelId = $ModelId
$providerReplayEntryTalkExe = $TalkExe
$providerReplayEntryConfigPath = $ConfigPath
$providerReplayEntryMode = $Mode
$providerReplayEntryMaxAutoPatchEditRatio = $MaxAutoPatchEditRatio
$providerReplayEntryOutputJson = $OutputJson
$providerReplayEntryUseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
$providerReplayEntryPlanOnly = [bool]$PlanOnly
$providerReplayEntryPassThru = [bool]$PassThru
$providerReplayEntryProcessorInvoker = $ProcessorInvoker

function Resolve-TalkProviderCorrectionReplayPath {
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

function Resolve-TalkProviderCorrectionReplayOptionalPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    Resolve-TalkProviderCorrectionReplayPath -Path $Path
}

function Write-TalkProviderCorrectionReplayJson {
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

function Resolve-TalkProviderCorrectionReplayDefaultCorpusManifest {
    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot '.runtime\asr-bench\real-mic-corpus\corpus.json'))
}

function Resolve-TalkProviderCorrectionReplayCorpusManifest {
    param([string]$CorpusManifest)

    if ([string]::IsNullOrWhiteSpace($CorpusManifest)) {
        return Resolve-TalkProviderCorrectionReplayDefaultCorpusManifest
    }

    Resolve-TalkProviderCorrectionReplayPath -Path $CorpusManifest
}

function Resolve-TalkProviderCorrectionReplayReportsRoot {
    param(
        [string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$CorpusManifest
    )

    if ([string]::IsNullOrWhiteSpace($ReportsRoot)) {
        return [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $CorpusManifest) 'reports'))
    }

    Resolve-TalkProviderCorrectionReplayPath -Path $ReportsRoot
}

function Resolve-TalkProviderCorrectionReplayTalkExe {
    param([string]$TalkExe)

    if (-not [string]::IsNullOrWhiteSpace($TalkExe)) {
        return Resolve-TalkProviderCorrectionReplayPath -Path $TalkExe
    }

    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'target\release\talk.exe'))
}

function Resolve-TalkProviderCorrectionReplayConfigPath {
    param([string]$ConfigPath)

    if (-not [string]::IsNullOrWhiteSpace($ConfigPath)) {
        return Resolve-TalkProviderCorrectionReplayPath -Path $ConfigPath
    }

    $repoRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $repoRoot 'examples\desktop-streaming-service-speculative-config.toml'))
}

function Resolve-TalkProviderCorrectionReplayOutputJson {
    param(
        [string]$OutputJson,
        [Parameter(Mandatory = $true)][string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    if (-not [string]::IsNullOrWhiteSpace($OutputJson)) {
        return Resolve-TalkProviderCorrectionReplayPath -Path $OutputJson
    }

    [System.IO.Path]::GetFullPath((Join-Path $ReportsRoot ("provider-correction-replay-{0}.json" -f $ModelId)))
}

function Get-TalkProviderCorrectionReplayOptionalProperty {
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

function ConvertTo-TalkProviderCorrectionReplayWindowsArgument {
    param([AllowNull()][string]$Value)

    if ($null -eq $Value -or $Value.Length -eq 0) {
        return '""'
    }
    if ($Value -notmatch '[\s"]') {
        return $Value
    }

    $quoted = '"'
    $backslashCount = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') {
            $backslashCount += 1
            continue
        }

        if ($character -eq '"') {
            $quoted += ('\' * ($backslashCount * 2 + 1))
            $quoted += '"'
            $backslashCount = 0
            continue
        }

        if ($backslashCount -gt 0) {
            $quoted += ('\' * $backslashCount)
            $backslashCount = 0
        }
        $quoted += [string]$character
    }

    if ($backslashCount -gt 0) {
        $quoted += ('\' * ($backslashCount * 2))
    }
    $quoted += '"'
    $quoted
}

function Join-TalkProviderCorrectionReplayWindowsArguments {
    param([Parameter(Mandatory = $true)][string[]]$Arguments)

    ($Arguments | ForEach-Object {
            ConvertTo-TalkProviderCorrectionReplayWindowsArgument -Value $_
        }) -join ' '
}

function Read-TalkProviderCorrectionReplayCorpusManifest {
    param([Parameter(Mandatory = $true)][string]$CorpusManifest)

    if (-not (Test-Path -LiteralPath $CorpusManifest -PathType Leaf)) {
        throw "Talk provider correction replay corpus manifest does not exist: $CorpusManifest"
    }

    $manifest = Get-Content -LiteralPath $CorpusManifest -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($null -eq $manifest.samples) {
        throw "Talk provider correction replay corpus manifest is missing samples: $CorpusManifest"
    }

    $samples = @($manifest.samples)
    $seenSampleIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($sample in $samples) {
        $sampleId = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $sample -Name 'sampleId')
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            throw "Talk provider correction replay corpus manifest contains a sample without sampleId: $CorpusManifest"
        }
        if (-not $seenSampleIds.Add($sampleId)) {
            throw "Talk provider correction replay corpus manifest contains duplicate sampleId [$sampleId]: $CorpusManifest"
        }
    }

    $samples
}

function Read-TalkProviderCorrectionReplayReport {
    param(
        [Parameter(Mandatory = $true)][string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][string]$SampleId
    )

    $reportPath = [System.IO.Path]::GetFullPath((Join-Path $ReportsRoot ("{0}-{1}.json" -f $ModelId, $SampleId)))
    if (-not (Test-Path -LiteralPath $reportPath -PathType Leaf)) {
        throw "Talk provider correction replay report is missing: $reportPath"
    }

    $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
    if ([string]::IsNullOrWhiteSpace([string]$report.text)) {
        throw "Talk provider correction replay report [$reportPath] does not contain a non-empty text field"
    }

    [pscustomobject]@{
        ReportPath = $reportPath
        Engine = [string]$report.engine
        SampleId = [string]$report.sample_id
        LocalText = [string]$report.text
        Cer = if ($null -ne (Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'cer')) { [double](Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'cer') } else { $null }
        FirstPartialMs = if ($null -ne (Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'first_partial_ms')) { [int](Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'first_partial_ms') } else { $null }
        FinalLatencyMs = if ($null -ne (Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'final_latency_ms')) { [int](Get-TalkProviderCorrectionReplayOptionalProperty -Object $report -Name 'final_latency_ms') } else { $null }
    }
}

function Read-TalkProviderCorrectionReplayFixtureMap {
    param([Parameter(Mandatory = $true)][string]$FixtureJsonPath)

    if (-not (Test-Path -LiteralPath $FixtureJsonPath -PathType Leaf)) {
        throw "Talk provider correction replay fixture json does not exist: $FixtureJsonPath"
    }

    $fixture = Get-Content -LiteralPath $FixtureJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $fixtureSamples = @($fixture.samples)
    if ($fixtureSamples.Count -eq 0) {
        throw "Talk provider correction replay fixture json does not contain samples: $FixtureJsonPath"
    }

    $map = @{}
    foreach ($fixtureSample in $fixtureSamples) {
        $sampleId = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $fixtureSample -Name 'sampleId')
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            throw "Talk provider correction replay fixture json contains a sample without sampleId: $FixtureJsonPath"
        }

        $providerOutputText = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $fixtureSample -Name 'providerOutputText')
        if ([string]::IsNullOrWhiteSpace($providerOutputText)) {
            throw "Talk provider correction replay fixture sample [$sampleId] is missing providerOutputText: $FixtureJsonPath"
        }
        if ($map.ContainsKey($sampleId)) {
            throw "Talk provider correction replay fixture json contains duplicate sampleId [$sampleId]: $FixtureJsonPath"
        }

        $map[$sampleId] = $providerOutputText
    }

    $map
}

function Resolve-TalkProviderCorrectionReplayFixtureJsonPath {
    param(
        [Parameter(Mandatory = $true)][string]$ResolvedOutputJson,
        [Parameter(Mandatory = $true)][string]$ResolvedCorpusManifest,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    if (Test-Path -LiteralPath $ResolvedOutputJson -PathType Leaf) {
        return $ResolvedOutputJson
    }

    $corpusRootFixtureJson = [System.IO.Path]::GetFullPath(
        (Join-Path (Split-Path -Parent $ResolvedCorpusManifest) ("provider-correction-replay-{0}.json" -f $ModelId))
    )
    if (Test-Path -LiteralPath $corpusRootFixtureJson -PathType Leaf) {
        return $corpusRootFixtureJson
    }

    $ResolvedOutputJson
}

function Invoke-TalkProviderCorrectionReplayProcessor {
    param(
        [Parameter(Mandatory = $true)][string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Transcript,
        [Parameter(Mandatory = $true)][string]$Mode,
        [string]$ProviderOutputText
    )

    if (-not (Test-Path -LiteralPath $TalkExe -PathType Leaf)) {
        throw "Talk provider correction replay executable does not exist: $TalkExe"
    }
    if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
        throw "Talk provider correction replay config does not exist: $ConfigPath"
    }

    $stdoutPath = Join-Path ([System.IO.Path]::GetTempPath()) ("talk-provider-replay-stdout-{0}.json" -f ([guid]::NewGuid().ToString()))
    $stderrPath = Join-Path ([System.IO.Path]::GetTempPath()) ("talk-provider-replay-stderr-{0}.log" -f ([guid]::NewGuid().ToString()))
    try {
        $arguments = New-Object System.Collections.Generic.List[string]
        foreach ($argument in @(
            'process-transcript',
            '--config', $ConfigPath,
            '--transcript', $Transcript
        )) {
            $arguments.Add([string]$argument) | Out-Null
        }
        if (-not [string]::IsNullOrWhiteSpace($ProviderOutputText)) {
            $arguments.Add('--provider-output-text') | Out-Null
            $arguments.Add([string]$ProviderOutputText) | Out-Null
        }
        foreach ($argument in @(
            '--mode', $Mode,
            '--json'
        )) {
            $arguments.Add([string]$argument) | Out-Null
        }
        $argumentString = Join-TalkProviderCorrectionReplayWindowsArguments -Arguments $arguments.ToArray()
        $process = Start-Process `
            -FilePath $TalkExe `
            -ArgumentList $argumentString `
            -NoNewWindow `
            -PassThru `
            -Wait `
            -RedirectStandardOutput $stdoutPath `
            -RedirectStandardError $stderrPath

        $stdout = if (Test-Path -LiteralPath $stdoutPath -PathType Leaf) {
            Get-Content -LiteralPath $stdoutPath -Raw -Encoding UTF8
        } else {
            ''
        }
        $stderr = if (Test-Path -LiteralPath $stderrPath -PathType Leaf) {
            Get-Content -LiteralPath $stderrPath -Raw -Encoding UTF8
        } else {
            ''
        }

        if ($process.ExitCode -ne 0) {
            throw "Talk provider correction replay processor failed with exit code $($process.ExitCode)`: $TalkExe process-transcript --json`nSTDERR:`n$stderr`nSTDOUT:`n$stdout"
        }
        if ([string]::IsNullOrWhiteSpace($stdout)) {
            throw "Talk provider correction replay processor produced empty stdout: $TalkExe process-transcript --json`nSTDERR:`n$stderr"
        }

        $json = $stdout | ConvertFrom-Json
        [pscustomobject]@{
            outputText = [string]$json.outputText
            providerOutputText = [string]$json.providerOutputText
            faithfulValidation = Get-TalkProviderCorrectionReplayOptionalProperty -Object $json -Name 'faithfulValidation'
            diagnostics = if ([string]::IsNullOrWhiteSpace($stderr)) { $null } else { $stderr.Trim() }
        }
    }
    finally {
        Remove-Item -LiteralPath $stdoutPath -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath $stderrPath -Force -ErrorAction SilentlyContinue
    }
}

function Normalize-TalkProviderCorrectionReplayResult {
    param([Parameter(Mandatory = $true)]$Result)

    $faithfulValidation = Get-TalkProviderCorrectionReplayOptionalProperty -Object $Result -Name 'faithfulValidation'
    if ($null -eq $faithfulValidation) {
        $faithfulValidation = Get-TalkProviderCorrectionReplayOptionalProperty -Object $Result -Name 'faithful_validation'
    }

    [pscustomobject]@{
        outputText = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $Result -Name 'outputText')
        providerOutputText = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $Result -Name 'providerOutputText')
        faithfulValidation = if ($null -eq $faithfulValidation) {
            $null
        } else {
            [pscustomobject]@{
                accepted = [bool](Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'accepted')
                fallbackReason = [string](Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'fallbackReason')
                inputCharCount = Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'inputCharCount'
                outputCharCount = Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'outputCharCount'
                retentionRatio = Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'retentionRatio'
                normalizedChangeRatio = Get-TalkProviderCorrectionReplayOptionalProperty -Object $faithfulValidation -Name 'normalizedChangeRatio'
            }
        }
    }
}

function ConvertTo-TalkProviderCorrectionReplayUnicodeScalars {
    param([Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value)

    $scalars = New-Object System.Collections.Generic.List[string]
    for ($index = 0; $index -lt $Value.Length; $index++) {
        $unit = $Value[$index]
        if ([char]::IsHighSurrogate($unit) -and
            $index + 1 -lt $Value.Length -and
            [char]::IsLowSurrogate($Value[$index + 1])) {
            $scalars.Add($Value.Substring($index, 2)) | Out-Null
            $index += 1
            continue
        }
        $scalars.Add([string]$unit) | Out-Null
    }

    $scalars.ToArray()
}

function Get-TalkProviderCorrectionReplayCharErrorRate {
    param(
        [Parameter(Mandatory = $true)][string]$ReferenceText,
        [Parameter(Mandatory = $true)][string]$CandidateText
    )

    $left = @(ConvertTo-TalkProviderCorrectionReplayUnicodeScalars -Value $ReferenceText)
    $right = @(ConvertTo-TalkProviderCorrectionReplayUnicodeScalars -Value $CandidateText)
    $m = $left.Length
    $n = $right.Length

    if ($m -eq 0) {
        if ($n -eq 0) {
            return 0.0
        }
        return 1.0
    }

    $previous = New-Object 'int[]' ($n + 1)
    $current = New-Object 'int[]' ($n + 1)
    for ($j = 0; $j -le $n; $j++) {
        $previous[$j] = $j
    }

    for ($i = 1; $i -le $m; $i++) {
        $current[0] = $i
        for ($j = 1; $j -le $n; $j++) {
            $substitutionCost = if ($left[$i - 1] -ceq $right[$j - 1]) { 0 } else { 1 }
            $deletion = $previous[$j] + 1
            $insertion = $current[$j - 1] + 1
            $substitution = $previous[$j - 1] + $substitutionCost
            $current[$j] = [Math]::Min([Math]::Min($deletion, $insertion), $substitution)
        }

        $swap = $previous
        $previous = $current
        $current = $swap
    }

    [double]$previous[$n] / [double]$m
}

function Get-TalkProviderCorrectionReplayAverage {
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

function New-TalkProviderCorrectionReplayPlan {
    [CmdletBinding()]
    param(
        [string]$CorpusManifest,
        [string]$ReportsRoot,
        [string]$ModelId = 'paraformer-bilingual-zh-en',
        [string]$TalkExe,
        [string]$ConfigPath,
        [string]$Mode = 'transcribe',
        [double]$MaxAutoPatchEditRatio = 0.35,
        [string]$OutputJson,
        [switch]$UseExistingReplayAsFixture
    )

    if ([string]::IsNullOrWhiteSpace($ModelId)) {
        throw 'Talk provider correction replay ModelId must not be blank'
    }
    if ([string]::IsNullOrWhiteSpace($Mode)) {
        throw 'Talk provider correction replay Mode must not be blank'
    }
    if ([double]::IsNaN($MaxAutoPatchEditRatio) -or
        [double]::IsInfinity($MaxAutoPatchEditRatio) -or
        $MaxAutoPatchEditRatio -lt 0.0 -or
        $MaxAutoPatchEditRatio -gt 1.0) {
        throw 'Talk provider correction replay MaxAutoPatchEditRatio must be between 0 and 1'
    }

    $resolvedCorpusManifest = Resolve-TalkProviderCorrectionReplayCorpusManifest -CorpusManifest $CorpusManifest
    $resolvedReportsRoot = Resolve-TalkProviderCorrectionReplayReportsRoot -ReportsRoot $ReportsRoot -CorpusManifest $resolvedCorpusManifest
    $resolvedTalkExe = Resolve-TalkProviderCorrectionReplayTalkExe -TalkExe $TalkExe
    $resolvedConfigPath = Resolve-TalkProviderCorrectionReplayConfigPath -ConfigPath $ConfigPath
    $resolvedOutputJson = Resolve-TalkProviderCorrectionReplayOutputJson -OutputJson $OutputJson -ReportsRoot $resolvedReportsRoot -ModelId $ModelId
    $samples = @(Read-TalkProviderCorrectionReplayCorpusManifest -CorpusManifest $resolvedCorpusManifest)
    $fixtureMap = if ($UseExistingReplayAsFixture) {
        $fixtureJsonPath = Resolve-TalkProviderCorrectionReplayFixtureJsonPath `
            -ResolvedOutputJson $resolvedOutputJson `
            -ResolvedCorpusManifest $resolvedCorpusManifest `
            -ModelId $ModelId
        Read-TalkProviderCorrectionReplayFixtureMap -FixtureJsonPath $fixtureJsonPath
    } else {
        $null
    }

    $replays = @()
    foreach ($sample in $samples) {
        $sampleId = [string]$sample.sampleId
        if ($null -ne $fixtureMap -and -not $fixtureMap.ContainsKey($sampleId)) {
            throw "Talk provider correction replay fixture is missing sampleId [$sampleId]"
        }
        $report = Read-TalkProviderCorrectionReplayReport `
            -ReportsRoot $resolvedReportsRoot `
            -ModelId $ModelId `
            -SampleId $sampleId
        $replays += [pscustomobject]@{
            SampleId = $sampleId
            ReportPath = [string]$report.ReportPath
            Engine = [string]$report.Engine
            LocalText = [string]$report.LocalText
            ReferenceText = [string]$sample.referenceText
            LocalCer = $report.Cer
            FirstPartialMs = $report.FirstPartialMs
            FinalLatencyMs = $report.FinalLatencyMs
            ProviderOutputText = if ($null -ne $fixtureMap) { [string]$fixtureMap[$sampleId] } else { $null }
        }
    }

    [pscustomobject]@{
        WorkflowKind = 'talk-provider-correction-replay-plan'
        CorpusManifest = $resolvedCorpusManifest
        ReportsRoot = $resolvedReportsRoot
        ModelId = $ModelId
        TalkExe = $resolvedTalkExe
        ConfigPath = $resolvedConfigPath
        Mode = $Mode
        MaxAutoPatchEditRatio = $MaxAutoPatchEditRatio
        OutputJson = $resolvedOutputJson
        UseExistingReplayAsFixture = [bool]$UseExistingReplayAsFixture
        Replays = @($replays)
    }
}

function Invoke-TalkProviderCorrectionReplay {
    [CmdletBinding()]
    param(
        [string]$CorpusManifest,
        [string]$ReportsRoot,
        [string]$ModelId = 'paraformer-bilingual-zh-en',
        [string]$TalkExe,
        [string]$ConfigPath,
        [string]$Mode = 'transcribe',
        [double]$MaxAutoPatchEditRatio = 0.35,
        [string]$OutputJson,
        [switch]$UseExistingReplayAsFixture,
        [switch]$PlanOnly,
        [switch]$PassThru,
        [scriptblock]$ProcessorInvoker
    )

    $plan = New-TalkProviderCorrectionReplayPlan `
        -CorpusManifest $CorpusManifest `
        -ReportsRoot $ReportsRoot `
        -ModelId $ModelId `
        -TalkExe $TalkExe `
        -ConfigPath $ConfigPath `
        -Mode $Mode `
        -MaxAutoPatchEditRatio $MaxAutoPatchEditRatio `
        -OutputJson $OutputJson `
        -UseExistingReplayAsFixture:$UseExistingReplayAsFixture

    if ($PlanOnly) {
        return $plan
    }

    $samples = @()
    foreach ($replay in $plan.Replays) {
        $rawResult = if ($null -ne $ProcessorInvoker) {
            & $ProcessorInvoker $replay.LocalText $plan.Mode $replay.ProviderOutputText
        } else {
            Invoke-TalkProviderCorrectionReplayProcessor `
                -TalkExe $plan.TalkExe `
                -ConfigPath $plan.ConfigPath `
                -Transcript $replay.LocalText `
                -Mode $plan.Mode `
                -ProviderOutputText $replay.ProviderOutputText
        }
        $result = Normalize-TalkProviderCorrectionReplayResult -Result $rawResult
        $processedCer = Get-TalkProviderCorrectionReplayCharErrorRate `
            -ReferenceText ([string]$replay.ReferenceText) `
            -CandidateText ([string]$result.outputText)
        $patchEditRatio = Get-TalkProviderCorrectionReplayCharErrorRate `
            -ReferenceText ([string]$replay.LocalText) `
            -CandidateText ([string]$result.outputText)
        $editRatioEligible = -not [string]::Equals(
            [string]$replay.LocalText,
            [string]$result.outputText,
            [System.StringComparison]::Ordinal
        ) -and $patchEditRatio -le $plan.MaxAutoPatchEditRatio

        $samples += [pscustomobject]@{
            sampleId = [string]$replay.SampleId
            reportPath = [string]$replay.ReportPath
            engine = [string]$replay.Engine
            localText = [string]$replay.LocalText
            outputText = [string]$result.outputText
            providerOutputText = [string]$result.providerOutputText
            referenceText = [string]$replay.ReferenceText
            localCer = $replay.LocalCer
            processedCer = $processedCer
            patchEditRatio = $patchEditRatio
            editRatioEligible = $editRatioEligible
            improved = if ($null -eq $replay.LocalCer) { $null } else { [double]$processedCer -lt [double]$replay.LocalCer }
            exactMatch = ([string]$result.outputText -ceq [string]$replay.ReferenceText)
            firstPartialMs = $replay.FirstPartialMs
            finalLatencyMs = $replay.FinalLatencyMs
            usedProviderOutputFixture = -not [string]::IsNullOrWhiteSpace([string]$replay.ProviderOutputText)
            faithfulValidation = $result.faithfulValidation
        }
    }

    $result = [pscustomobject]@{
        workflowKind = 'talk-provider-correction-replay-result'
        corpusManifest = $plan.CorpusManifest
        reportsRoot = $plan.ReportsRoot
        modelId = $plan.ModelId
        talkExe = $plan.TalkExe
        configPath = $plan.ConfigPath
        mode = $plan.Mode
        maxAutoPatchEditRatio = $plan.MaxAutoPatchEditRatio
        outputJson = $plan.OutputJson
        sampleCount = $samples.Count
        exactMatchCount = @($samples | Where-Object { $_.exactMatch }).Count
        improvedCount = @($samples | Where-Object { $_.improved -eq $true }).Count
        editRatioEligibleCount = @($samples | Where-Object { $_.editRatioEligible -eq $true }).Count
        meanLocalCer = Get-TalkProviderCorrectionReplayAverage -Values ($samples | ForEach-Object { $_.localCer })
        meanProcessedCer = Get-TalkProviderCorrectionReplayAverage -Values ($samples | ForEach-Object { $_.processedCer })
        samples = @($samples)
    }

    Write-TalkProviderCorrectionReplayJson -Path $plan.OutputJson -Value $result

    $passThruResult = [pscustomobject]@{
        WorkflowKind = 'talk-provider-correction-replay-result'
        CorpusManifest = $result.corpusManifest
        ReportsRoot = $result.reportsRoot
        ModelId = $result.modelId
        TalkExe = $result.talkExe
        ConfigPath = $result.configPath
        Mode = $result.mode
        MaxAutoPatchEditRatio = $result.maxAutoPatchEditRatio
        OutputJson = $result.outputJson
        UseExistingReplayAsFixture = $plan.UseExistingReplayAsFixture
        SampleCount = $result.sampleCount
        ExactMatchCount = $result.exactMatchCount
        ImprovedCount = $result.improvedCount
        EditRatioEligibleCount = $result.editRatioEligibleCount
        MeanLocalCer = $result.meanLocalCer
        MeanProcessedCer = $result.meanProcessedCer
        Samples = $result.samples
    }

    if ($PassThru) {
        return $passThruResult
    }
}

$skipEntryPoint = $false
$skipEntryPointVariable = Get-Variable -Name 'TalkProviderCorrectionReplaySkipEntryPoint' -Scope Script -ErrorAction SilentlyContinue
if ($null -ne $skipEntryPointVariable) {
    $skipEntryPoint = [bool]$skipEntryPointVariable.Value
}

if (-not $skipEntryPoint -and $MyInvocation.InvocationName -ne '.') {
    $invokeResult = Invoke-TalkProviderCorrectionReplay `
        -CorpusManifest $providerReplayEntryCorpusManifest `
        -ReportsRoot $providerReplayEntryReportsRoot `
        -ModelId $providerReplayEntryModelId `
        -TalkExe $providerReplayEntryTalkExe `
        -ConfigPath $providerReplayEntryConfigPath `
        -Mode $providerReplayEntryMode `
        -MaxAutoPatchEditRatio $providerReplayEntryMaxAutoPatchEditRatio `
        -OutputJson $providerReplayEntryOutputJson `
        -UseExistingReplayAsFixture:$providerReplayEntryUseExistingReplayAsFixture `
        -PlanOnly:$providerReplayEntryPlanOnly `
        -PassThru:$providerReplayEntryPassThru `
        -ProcessorInvoker $providerReplayEntryProcessorInvoker

    if ($providerReplayEntryPlanOnly -or $providerReplayEntryPassThru) {
        $invokeResult
    }
}
