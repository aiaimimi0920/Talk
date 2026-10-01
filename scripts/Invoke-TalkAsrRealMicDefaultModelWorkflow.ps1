[CmdletBinding()]
param(
    [string]$PromptManifest,
    [string]$CorpusRoot,
    [string]$TalkExe,
    [string]$InputDevice,
    [int]$DefaultCaptureSeconds = 3,
    [int]$CountdownSeconds = 3,
    [switch]$AllowSilent,
    [string[]]$ForceRecordSampleId,
    [switch]$ResumeExistingCorpus,
    [switch]$SkipRecording,
    [switch]$RecordOnly,
    [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
    [string]$ModelRoot,
    [string]$ReportsRoot,
    [string]$AsrBenchExe,
    [string]$LocalAsrDaemonExe,
    [string]$CloudOpenAiCompatibleEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
    [string]$CloudOpenAiCompatibleModel = 'qwen3-asr-flash',
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
    [string]$ConfigPath,
    [int]$MinSamples = 3,
    [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
    [switch]$AllowMissingCloudBaseline,
    [switch]$AllowSyntheticSampleIds,
    [switch]$SkipApply,
    [switch]$NoBackup,
    [switch]$PreflightOnly,
    [switch]$ProbeAudio,
    [ValidateRange(1, 60)][int]$AudioProbeSeconds = 2,
    [switch]$PlanOnly,
    [switch]$PassThru,
    [scriptblock]$ProbeInvoker,
    [scriptblock]$AudioProbeInvoker,
    [scriptblock]$ReadinessInvoker
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$realMicWorkflowEntryPromptManifest = $PromptManifest
$realMicWorkflowEntryCorpusRoot = $CorpusRoot
$realMicWorkflowEntryTalkExe = $TalkExe
$realMicWorkflowEntryInputDevice = $InputDevice
$realMicWorkflowEntryDefaultCaptureSeconds = $DefaultCaptureSeconds
$realMicWorkflowEntryCountdownSeconds = $CountdownSeconds
$realMicWorkflowEntryAllowSilent = [bool]$AllowSilent
$realMicWorkflowEntryForceRecordSampleId = $ForceRecordSampleId
$realMicWorkflowEntryResumeExistingCorpus = [bool]$ResumeExistingCorpus
$realMicWorkflowEntrySkipRecording = [bool]$SkipRecording
$realMicWorkflowEntryRecordOnly = [bool]$RecordOnly
$realMicWorkflowEntryModelId = $ModelId
$realMicWorkflowEntryModelRoot = $ModelRoot
$realMicWorkflowEntryReportsRoot = $ReportsRoot
$realMicWorkflowEntryAsrBenchExe = $AsrBenchExe
$realMicWorkflowEntryLocalAsrDaemonExe = $LocalAsrDaemonExe
$realMicWorkflowEntryCloudOpenAiCompatibleEndpoint = $CloudOpenAiCompatibleEndpoint
$realMicWorkflowEntryCloudOpenAiCompatibleModel = $CloudOpenAiCompatibleModel
$realMicWorkflowEntryCloudOpenAiCompatibleTransport = $CloudOpenAiCompatibleTransport
$realMicWorkflowEntryCloudOpenAiCompatibleApiKeyEnv = $CloudOpenAiCompatibleApiKeyEnv
$realMicWorkflowEntryBind = $Bind
$realMicWorkflowEntryChunkMs = $ChunkMs
$realMicWorkflowEntryConnectTimeoutMs = $ConnectTimeoutMs
$realMicWorkflowEntryReadyTimeoutMs = $ReadyTimeoutMs
$realMicWorkflowEntryPartialIdleTimeoutMs = $PartialIdleTimeoutMs
$realMicWorkflowEntryFinalTimeoutMs = $FinalTimeoutMs
$realMicWorkflowEntryStartupTimeoutSeconds = $StartupTimeoutSeconds
$realMicWorkflowEntrySelectionJson = $SelectionJson
$realMicWorkflowEntryConfigPath = $ConfigPath
$realMicWorkflowEntryMinSamples = $MinSamples
$realMicWorkflowEntryRequiredLocalModelId = $RequiredLocalModelId
$realMicWorkflowEntryAllowMissingCloudBaseline = [bool]$AllowMissingCloudBaseline
$realMicWorkflowEntryAllowSyntheticSampleIds = [bool]$AllowSyntheticSampleIds
$realMicWorkflowEntrySkipApply = [bool]$SkipApply
$realMicWorkflowEntryNoBackup = [bool]$NoBackup
$realMicWorkflowEntryPreflightOnly = [bool]$PreflightOnly
$realMicWorkflowEntryProbeAudio = [bool]$ProbeAudio
$realMicWorkflowEntryAudioProbeSeconds = $AudioProbeSeconds
$realMicWorkflowEntryPlanOnly = [bool]$PlanOnly
$realMicWorkflowEntryPassThru = [bool]$PassThru
$realMicWorkflowEntryProbeInvoker = $ProbeInvoker
$realMicWorkflowEntryAudioProbeInvoker = $AudioProbeInvoker
$realMicWorkflowEntryReadinessInvoker = $ReadinessInvoker

$startScriptPath = Join-Path $PSScriptRoot 'Start-TalkDesktop.ps1'
if (-not (Test-Path -LiteralPath $startScriptPath)) {
    throw "Missing Talk desktop launch script: $startScriptPath"
}
. $startScriptPath

foreach ($dependencyName in @(
    'Invoke-TalkAsrCorpusRecorder.ps1',
    'Invoke-TalkAsrDefaultModelWorkflow.ps1',
    'Invoke-TalkFaithfulCorrectionReplay.ps1',
    'Invoke-TalkProviderCorrectionReplay.ps1'
)) {
    $dependencyPath = Join-Path $PSScriptRoot $dependencyName
    if (-not (Test-Path -LiteralPath $dependencyPath -PathType Leaf)) {
        throw "Talk real microphone ASR workflow dependency is missing: $dependencyPath"
    }
    . $dependencyPath
}

function Resolve-TalkAsrRealMicDefaultWorkflowPath {
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

function Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        return $null
    }

    Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $Path
}

function Get-TalkAsrRealMicDefaultWorkflowOptionalProperty {
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

function Test-TalkAsrRealMicDefaultWorkflowHasProperty {
    param(
        $Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $false
    }

    $null -ne $Object.PSObject.Properties[$Name]
}

function Resolve-TalkAsrRealMicDefaultWorkflowDefaultPromptManifest {
    $baseDir = if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        Join-Path (Split-Path -Parent $PSScriptRoot) 'examples'
    } else {
        $PSScriptRoot
    }

    [System.IO.Path]::GetFullPath((Join-Path $baseDir 'asr-real-mic-prompts.json'))
}

function Resolve-TalkAsrRealMicDefaultWorkflowDefaultCorpusRoot {
    $baseDir = if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        Split-Path -Parent $PSScriptRoot
    } else {
        $PSScriptRoot
    }

    [System.IO.Path]::GetFullPath((Join-Path $baseDir '.runtime\asr-bench\real-mic-corpus'))
}

function Resolve-TalkAsrRealMicDefaultWorkflowCorpusRoot {
    param([string]$CorpusRoot)

    if ([string]::IsNullOrWhiteSpace($CorpusRoot)) {
        return Resolve-TalkAsrRealMicDefaultWorkflowDefaultCorpusRoot
    }

    Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $CorpusRoot
}

function Resolve-TalkAsrRealMicDefaultWorkflowReportsRoot {
    param(
        [string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$CorpusRoot
    )

    if ([string]::IsNullOrWhiteSpace($ReportsRoot)) {
        return [System.IO.Path]::GetFullPath((Join-Path $CorpusRoot 'reports'))
    }

    Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $ReportsRoot
}

function Resolve-TalkAsrRealMicDefaultWorkflowPromptManifest {
    param([string]$PromptManifest)

    if ([string]::IsNullOrWhiteSpace($PromptManifest)) {
        return Resolve-TalkAsrRealMicDefaultWorkflowDefaultPromptManifest
    }

    Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $PromptManifest
}

function New-TalkAsrRealMicDefaultWorkflowPreflightCheck {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][ValidateSet('ready', 'missing', 'failed', 'planned', 'skipped')][string]$Status,
        [string]$Path,
        [string]$Message,
        [string]$RemediationCommand,
        [string]$RemediationHint,
        [string]$RemediationArtifactPath
    )

    [pscustomobject]@{
        Name = $Name
        Status = $Status
        Path = $Path
        Message = $Message
        RemediationCommand = $RemediationCommand
        RemediationHint = $RemediationHint
        RemediationArtifactPath = $RemediationArtifactPath
    }
}

function Format-TalkAsrRealMicDefaultWorkflowBlockingCheckFailure {
    param([Parameter(Mandatory = $true)]$Check)

    $parts = New-Object System.Collections.Generic.List[string]
    $message = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Check -Name 'Message')
    if (-not [string]::IsNullOrWhiteSpace($message)) {
        $parts.Add($message) | Out-Null
    }

    $remediationCommand = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Check -Name 'RemediationCommand')
    if (-not [string]::IsNullOrWhiteSpace($remediationCommand)) {
        $parts.Add(("remediation={0}" -f $remediationCommand)) | Out-Null
    }

    $remediationHint = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Check -Name 'RemediationHint')
    if (-not [string]::IsNullOrWhiteSpace($remediationHint)) {
        $parts.Add(("hint={0}" -f $remediationHint)) | Out-Null
    }

    $remediationArtifactPath = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Check -Name 'RemediationArtifactPath')
    if (-not [string]::IsNullOrWhiteSpace($remediationArtifactPath)) {
        $parts.Add(("artifact={0}" -f $remediationArtifactPath)) | Out-Null
    }

    $parts.ToArray() -join '; '
}

function Write-TalkAsrRealMicDefaultWorkflowJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, (($Value | ConvertTo-Json -Depth 8) + [Environment]::NewLine), $utf8NoBom)
}

function ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral {
    param([string]$Value)

    "'{0}'" -f (($Value -replace "'", "''"))
}

function New-TalkAsrRealMicDefaultWorkflowSherpaInstallCommand {
    param(
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][string]$DestinationRoot
    )

    '.\Install-TalkSherpaModel.ps1 -ModelId {0} -DestinationRoot {1}' -f `
        $ModelId, `
        (ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value $DestinationRoot)
}

function New-TalkAsrRealMicDefaultWorkflowApiKeyCommand {
    param([Parameter(Mandatory = $true)][string]$EnvironmentVariableName)

    '$env:{0} = ''<redacted>''' -f $EnvironmentVariableName
}

function New-TalkAsrRealMicDefaultWorkflowResumeCommand {
    param([Parameter(Mandatory = $true)]$Plan)

    $parts = New-Object System.Collections.Generic.List[string]
    $parts.Add('.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1') | Out-Null
    $parts.Add('-SkipRecording') | Out-Null
    $parts.Add('-CorpusRoot') | Out-Null
    $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$Plan.CorpusRoot))) | Out-Null

    if (-not [string]::IsNullOrWhiteSpace([string]$Plan.ModelRoot)) {
        $parts.Add('-ModelRoot') | Out-Null
        $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$Plan.ModelRoot))) | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace([string]$Plan.ConfigPath)) {
        $parts.Add('-ConfigPath') | Out-Null
        $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$Plan.ConfigPath))) | Out-Null
    }

    $parts.ToArray() -join ' '
}

function New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [string[]]$ForceRecordSampleId,
        [string]$InputDevice,
        [switch]$ResumeExistingCorpus
    )

    $parts = New-Object System.Collections.Generic.List[string]
    $parts.Add('.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1') | Out-Null
    $parts.Add('-PromptManifest') | Out-Null
    $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$Plan.PromptManifest))) | Out-Null
    $parts.Add('-CorpusRoot') | Out-Null
    $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$Plan.CorpusRoot))) | Out-Null
    $parts.Add('-RecordOnly') | Out-Null
    if ($ResumeExistingCorpus) {
        $parts.Add('-ResumeExistingCorpus') | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($InputDevice)) {
        $parts.Add('-InputDevice') | Out-Null
        $parts.Add((ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$InputDevice))) | Out-Null
    }
    $resolvedForceRecordSampleIds = @(
        @($ForceRecordSampleId) |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )
    if ($resolvedForceRecordSampleIds.Count -gt 0) {
        $parts.Add('-ForceRecordSampleId') | Out-Null
        $parts.Add((@(
                    $resolvedForceRecordSampleIds |
                        ForEach-Object {
                            ConvertTo-TalkAsrRealMicDefaultWorkflowPowerShellSingleQuotedLiteral -Value ([string]$_)
                        }
                ) -join ',')) | Out-Null
    }

    $parts.ToArray() -join ' '
}

function Resolve-TalkAsrRealMicDefaultWorkflowManifestAudioPath {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string]$AudioWav
    )

    if ([System.IO.Path]::IsPathRooted($AudioWav)) {
        return [System.IO.Path]::GetFullPath($AudioWav)
    }

    [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $CorpusManifest) $AudioWav))
}

function ConvertTo-TalkAsrRealMicDefaultWorkflowArray {
    param($Value)

    if ($null -eq $Value) {
        return @()
    }

    @($Value)
}

function Resolve-TalkAsrRealMicDefaultWorkflowRerecordPromptManifestPath {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)][string]$ReasonSlug
    )

    if ([string]::IsNullOrWhiteSpace($ReasonSlug) -or $ReasonSlug -notmatch '^[A-Za-z0-9][A-Za-z0-9-]*$') {
        throw "ReasonSlug [$ReasonSlug] must use only letters, numbers, and hyphen"
    }

    [System.IO.Path]::GetFullPath((Join-Path ([string]$Plan.CorpusRoot) ("prompts-rerecord-{0}.json" -f $ReasonSlug)))
}

function Write-TalkAsrRealMicDefaultWorkflowRerecordPromptManifest {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)][string[]]$SampleIds,
        [Parameter(Mandatory = $true)][string]$ReasonSlug,
        $SourceSamples
    )

    $resolvedSampleIds = @(
        @($SampleIds) |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
            Select-Object -Unique
    )
    if ($resolvedSampleIds.Count -eq 0) {
        return $null
    }

    $sourceSampleArray = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value $SourceSamples)
    if ($sourceSampleArray.Count -eq 0) {
        $promptManifestPath = [string]$Plan.PromptManifest
        if (-not [string]::IsNullOrWhiteSpace($promptManifestPath) -and (Test-Path -LiteralPath $promptManifestPath -PathType Leaf)) {
            try {
                $sourceSampleArray = @(Read-TalkAsrRealMicDefaultWorkflowPromptSamples -PromptManifest $promptManifestPath)
            }
            catch {
                $sourceSampleArray = @()
            }
        }
    }
    if ($sourceSampleArray.Count -eq 0) {
        $corpusManifestPath = [string]$Plan.CorpusManifest
        if (-not [string]::IsNullOrWhiteSpace($corpusManifestPath) -and (Test-Path -LiteralPath $corpusManifestPath -PathType Leaf)) {
            try {
                $sourceSampleArray = @(Read-TalkAsrRealMicDefaultWorkflowCorpusSamples -CorpusManifest $corpusManifestPath)
            }
            catch {
                $sourceSampleArray = @()
            }
        }
    }

    $selectedIdSet = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($sampleId in $resolvedSampleIds) {
        [void]$selectedIdSet.Add([string]$sampleId)
    }

    $samples = New-Object System.Collections.Generic.List[object]
    foreach ($sample in $sourceSampleArray) {
        $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'SampleId')
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'sampleId')
        }
        if ([string]::IsNullOrWhiteSpace($sampleId) -or -not $selectedIdSet.Contains($sampleId)) {
            continue
        }

        $referenceText = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'ReferenceText')
        if ([string]::IsNullOrWhiteSpace($referenceText)) {
            $referenceText = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'referenceText')
        }
        $sampleDocument = [ordered]@{
            sampleId = $sampleId
        }
        if (-not [string]::IsNullOrWhiteSpace($referenceText)) {
            $sampleDocument.referenceText = $referenceText
        }

        $captureSeconds = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'CaptureSeconds'
        if ($null -eq $captureSeconds) {
            $captureSeconds = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'captureSeconds'
        }
        if ($null -ne $captureSeconds) {
            $sampleDocument.captureSeconds = [int]$captureSeconds
        }

        $samples.Add([pscustomobject]$sampleDocument) | Out-Null
    }

    if ($samples.Count -eq 0) {
        return $null
    }

    $outputPath = Resolve-TalkAsrRealMicDefaultWorkflowRerecordPromptManifestPath -Plan $Plan -ReasonSlug $ReasonSlug
    $document = [ordered]@{
        schemaVersion = 1
        samples = @($samples.ToArray())
    }
    Write-TalkAsrRealMicDefaultWorkflowJson -Path $outputPath -Value $document
    $outputPath
}

function Read-TalkAsrRealMicDefaultWorkflowPromptSamples {
    param([Parameter(Mandatory = $true)][string]$PromptManifest)

    if (-not (Test-Path -LiteralPath $PromptManifest -PathType Leaf)) {
        throw "prompt manifest is missing: $PromptManifest"
    }

    @(Read-TalkAsrCorpusRecorderPrompts -PromptManifest $PromptManifest)
}

function Read-TalkAsrRealMicDefaultWorkflowCorpusSamples {
    param([Parameter(Mandatory = $true)][string]$CorpusManifest)

    if (-not (Test-Path -LiteralPath $CorpusManifest -PathType Leaf)) {
        throw "corpus manifest is missing: $CorpusManifest"
    }

    @(Read-TalkAsrCorpusManifest -CorpusManifest $CorpusManifest)
}

function Get-TalkAsrRealMicDefaultWorkflowSampleIds {
    param($Samples)

    $sampleIds = New-Object System.Collections.Generic.List[string]
    foreach ($sample in @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value $Samples)) {
        $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'SampleId')
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'sampleId')
        }
        if (-not [string]::IsNullOrWhiteSpace($sampleId)) {
            $sampleIds.Add($sampleId) | Out-Null
        }
    }

    @($sampleIds.ToArray())
}

function Compare-TalkAsrRealMicDefaultWorkflowPromptAndCorpusSampleIds {
    param(
        [string[]]$PromptSampleIds,
        [string[]]$CorpusSampleIds
    )

    $resolvedPromptSampleIds = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value $PromptSampleIds)
    $resolvedCorpusSampleIds = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value $CorpusSampleIds)

    $promptSet = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($sampleId in $resolvedPromptSampleIds) {
        [void]$promptSet.Add([string]$sampleId)
    }

    $corpusSet = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($sampleId in $resolvedCorpusSampleIds) {
        [void]$corpusSet.Add([string]$sampleId)
    }

    $missingFromCorpusSampleIds = New-Object System.Collections.Generic.List[string]
    foreach ($sampleId in $resolvedPromptSampleIds) {
        $resolvedSampleId = [string]$sampleId
        if (-not $corpusSet.Contains($resolvedSampleId)) {
            $missingFromCorpusSampleIds.Add($resolvedSampleId) | Out-Null
        }
    }

    $extraInCorpusSampleIds = New-Object System.Collections.Generic.List[string]
    foreach ($sampleId in $resolvedCorpusSampleIds) {
        $resolvedSampleId = [string]$sampleId
        if (-not $promptSet.Contains($resolvedSampleId)) {
            $extraInCorpusSampleIds.Add($resolvedSampleId) | Out-Null
        }
    }

    [pscustomobject]@{
        PromptSampleCount = $resolvedPromptSampleIds.Count
        CorpusSampleCount = $resolvedCorpusSampleIds.Count
        MissingFromCorpusSampleIds = @($missingFromCorpusSampleIds.ToArray())
        ExtraInCorpusSampleIds = @($extraInCorpusSampleIds.ToArray())
        Matches = ($missingFromCorpusSampleIds.Count -eq 0 -and $extraInCorpusSampleIds.Count -eq 0)
    }
}

function New-TalkAsrRealMicDefaultWorkflowCorpusPromptAlignmentCheck {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        $PromptSamples,
        [string[]]$PromptSampleIds,
        [string[]]$CorpusSampleIds,
        [switch]$ResumeExistingCorpus
    )

    $comparison = Compare-TalkAsrRealMicDefaultWorkflowPromptAndCorpusSampleIds `
        -PromptSampleIds $PromptSampleIds `
        -CorpusSampleIds $CorpusSampleIds

    if ($comparison.Matches) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_prompt_alignment' `
            -Status 'ready' `
            -Path ([string]$Plan.CorpusManifest) `
            -Message ("existing corpus matches current prompt manifest: promptSampleCount={0}; corpusSampleCount={1}" -f $comparison.PromptSampleCount, $comparison.CorpusSampleCount)
    }

    $detailParts = New-Object System.Collections.Generic.List[string]
    if ($comparison.MissingFromCorpusSampleIds.Count -gt 0) {
        $detailParts.Add(("missingFromCorpus={0}" -f ($comparison.MissingFromCorpusSampleIds -join ', '))) | Out-Null
    }
    if ($comparison.ExtraInCorpusSampleIds.Count -gt 0) {
        $detailParts.Add(("extraInCorpus={0}" -f ($comparison.ExtraInCorpusSampleIds -join ', '))) | Out-Null
    }
    $remediationArtifactPath = $null
    if ($comparison.MissingFromCorpusSampleIds.Count -gt 0) {
        $remediationArtifactPath = Write-TalkAsrRealMicDefaultWorkflowRerecordPromptManifest `
            -Plan $Plan `
            -SourceSamples $PromptSamples `
            -SampleIds $comparison.MissingFromCorpusSampleIds `
            -ReasonSlug 'corpus-prompt-alignment'
    }

    New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
        -Name 'corpus_prompt_alignment' `
        -Status 'failed' `
        -Path ([string]$Plan.CorpusManifest) `
        -Message ("existing corpus manifest does not match current prompt manifest: promptSampleCount={0}; corpusSampleCount={1}; {2}" -f $comparison.PromptSampleCount, $comparison.CorpusSampleCount, ($detailParts.ToArray() -join '; ')) `
        -RemediationCommand (New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand -Plan $Plan -ResumeExistingCorpus) `
        -RemediationHint 'Re-record the current prompt manifest before benchmarking this corpus.' `
        -RemediationArtifactPath $remediationArtifactPath
}

function New-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        $RecorderResult,
        [Parameter(Mandatory = $true)][string]$CorpusManifestPath
    )

    $resolvedCorpusManifest = [System.IO.Path]::GetFullPath($CorpusManifestPath)
    $validationErrors = New-Object System.Collections.Generic.List[string]
    $missingAudio = New-Object System.Collections.Generic.List[string]
    $sampleCount = 0
    $audioFileCount = 0

    if (-not (Test-Path -LiteralPath $resolvedCorpusManifest -PathType Leaf)) {
        $validationErrors.Add("corpus manifest does not exist: $resolvedCorpusManifest") | Out-Null
    } else {
        try {
            $manifest = Get-Content -LiteralPath $resolvedCorpusManifest -Raw -Encoding UTF8 | ConvertFrom-Json
            $rawSamples = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $manifest -Name 'samples'))
            $sampleCount = $rawSamples.Count
            foreach ($sample in $rawSamples) {
                $audioWavValue = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'audioWav')
                if ([string]::IsNullOrWhiteSpace($audioWavValue)) {
                    $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'sampleId')
                    $missingAudio.Add("missing audioWav for sampleId=$sampleId") | Out-Null
                    continue
                }

                $resolvedAudioWav = Resolve-TalkAsrRealMicDefaultWorkflowManifestAudioPath `
                    -CorpusManifest $resolvedCorpusManifest `
                    -AudioWav $audioWavValue
                if (Test-Path -LiteralPath $resolvedAudioWav -PathType Leaf) {
                    $audioFileCount += 1
                } else {
                    $missingAudio.Add($resolvedAudioWav) | Out-Null
                }
            }
        }
        catch {
            $validationErrors.Add($_.Exception.Message) | Out-Null
        }
    }

    $recordings = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $RecorderResult -Name 'Recordings'))
    $plannedSamples = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan.RecorderPlan -Name 'Samples'))
    $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
    $configuredInputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'InputDevice')
    if ([string]::IsNullOrWhiteSpace($configuredInputDevice)) {
        $configuredInputDevice = $null
    }

    $capturedInputDevices = @(
        $recordings |
            ForEach-Object {
                [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $_ -Name 'CapturedInputDevice')
            } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
            Select-Object -Unique
    )
    $availableInputDevices = @(
        $recordings |
            ForEach-Object {
                @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $_ -Name 'AvailableInputDevices'))
            } |
            ForEach-Object { [string]$_ } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
            Select-Object -Unique
    )
    $inputDeviceSelectionWarning = $null
    if ([string]::IsNullOrWhiteSpace($configuredInputDevice)) {
        if ($capturedInputDevices.Count -gt 1) {
            $inputDeviceSelectionWarning = ("recorded across multiple default input devices [{0}] while available input devices were: {1}. Re-record with -InputDevice to lock the intended microphone." -f `
                ($capturedInputDevices -join ', '), `
                ($availableInputDevices -join ', '))
        } elseif ($capturedInputDevices.Count -eq 1 -and $availableInputDevices.Count -gt 1) {
            $inputDeviceSelectionWarning = ("recorded with default input device [{0}] while multiple input devices are available: {1}. Re-record with -InputDevice to lock the intended microphone." -f `
                $capturedInputDevices[0], `
                ($availableInputDevices -join ', '))
        }
    }
    $ready = (
        $validationErrors.Count -eq 0 -and
        $missingAudio.Count -eq 0 -and
        $sampleCount -gt 0 -and
        $audioFileCount -eq $sampleCount
    )

    [ordered]@{
        schemaVersion = 1
        workflowKind = 'talk-asr-real-mic-default-model-record-only-status'
        createdAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
        ready = $ready
        promptManifest = [string]$Plan.PromptManifest
        corpusRoot = [string]$Plan.CorpusRoot
        corpusManifest = $resolvedCorpusManifest
        reportsRoot = [string]$Plan.ReportsRoot
        configPath = [string]$Plan.ConfigPath
        configuredInputDevice = $configuredInputDevice
        capturedInputDevices = @($capturedInputDevices)
        availableInputDevices = @($availableInputDevices)
        inputDeviceSelectionWarning = $inputDeviceSelectionWarning
        plannedSampleCount = $plannedSamples.Count
        sampleCount = $sampleCount
        recordingCount = $recordings.Count
        audioFileCount = $audioFileCount
        missingAudioWav = @($missingAudio.ToArray())
        validationErrors = @($validationErrors.ToArray())
        nextCommand = (New-TalkAsrRealMicDefaultWorkflowResumeCommand -Plan $Plan)
    }
}

function Write-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Status
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, (($Status | ConvertTo-Json -Depth 8) + [Environment]::NewLine), $utf8NoBom)
}

function Resolve-TalkAsrRealMicDefaultWorkflowRecordOnlyStatusJson {
    param([Parameter(Mandatory = $true)]$Plan)

    [System.IO.Path]::GetFullPath((Join-Path ([string]$Plan.CorpusRoot) 'record-only-status.json'))
}

function Ensure-TalkAsrRealMicDefaultWorkflowRecorderConfig {
    param([Parameter(Mandatory = $true)]$Plan)

    $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
    if ($null -eq $recorderPlan) {
        $configPath = [System.IO.Path]::GetFullPath((Join-Path ([string]$Plan.CorpusRoot) 'recording-config.toml'))
        $captureTempDir = [System.IO.Path]::GetFullPath((Join-Path ([string]$Plan.CorpusRoot) '.captures'))
        $logsDir = [System.IO.Path]::GetFullPath((Join-Path ([string]$Plan.CorpusRoot) '.readiness-logs'))
        $inputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'InputDevice')
        if ([string]::IsNullOrWhiteSpace($inputDevice)) {
            $inputDevice = $null
        }
        $maxRecordingSeconds = [int](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'DefaultCaptureSeconds')
        if ($maxRecordingSeconds -le 0) {
            $maxRecordingSeconds = 3
        }

        Write-TalkAsrCorpusRecorderConfig `
            -Path $configPath `
            -CaptureTempDir $captureTempDir `
            -LogsDir $logsDir `
            -InputDevice $inputDevice `
            -MaxRecordingSeconds $maxRecordingSeconds

        return $configPath
    }

    $configPath = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'ConfigPath')
    if ([string]::IsNullOrWhiteSpace($configPath)) {
        return $null
    }

    $captureTempDir = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'CaptureTempDir')
    $logsDir = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'LogsDir')
    $inputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'InputDevice')
    $maxRecordingSeconds = [int](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'MaxRecordingSeconds')

    Write-TalkAsrCorpusRecorderConfig `
        -Path $configPath `
        -CaptureTempDir $captureTempDir `
        -LogsDir $logsDir `
        -InputDevice $inputDevice `
        -MaxRecordingSeconds $maxRecordingSeconds

    $configPath
}

function Invoke-TalkAsrRealMicDefaultWorkflowReadiness {
    param(
        [Parameter(Mandatory = $true)][string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [scriptblock]$ReadinessInvoker
    )

    if ($null -ne $ReadinessInvoker) {
        return & $ReadinessInvoker $TalkExe $ConfigPath
    }

    Invoke-TalkDesktopLaunchReadiness `
        -TalkBinaryPath $TalkExe `
        -EffectiveConfigPath $ConfigPath `
        -WorkingDirectory (Split-Path -Parent $ConfigPath)
}

function New-TalkAsrRealMicDefaultWorkflowInputDeviceSelectionCheck {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [scriptblock]$ReadinessInvoker
    )

    $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
    if ($null -eq $recorderPlan) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'skipped' `
            -Path $null `
            -Message 'input device selection applies only when the workflow records audio'
    }

    $talkExe = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'TalkExe')
    if ([string]::IsNullOrWhiteSpace($talkExe) -or -not (Test-Path -LiteralPath $talkExe -PathType Leaf)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'skipped' `
            -Path $talkExe `
            -Message 'input device selection requires talk.exe readiness support'
    }
    if ($null -eq $ReadinessInvoker -and -not (Test-TalkAsrRealMicDefaultWorkflowReplayTalkExeSupportsCommand -TalkExe $talkExe -CommandName 'readiness')) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'skipped' `
            -Path $talkExe `
            -Message 'input device selection check skipped because talk.exe readiness support is unavailable'
    }

    $configPath = Ensure-TalkAsrRealMicDefaultWorkflowRecorderConfig -Plan $Plan
    if ([string]::IsNullOrWhiteSpace($configPath) -or -not (Test-Path -LiteralPath $configPath -PathType Leaf)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'failed' `
            -Path $configPath `
            -Message 'input device selection check could not prepare the recorder config'
    }

    try {
        $readinessReport = Invoke-TalkAsrRealMicDefaultWorkflowReadiness `
            -TalkExe $talkExe `
            -ConfigPath $configPath `
            -ReadinessInvoker $ReadinessInvoker
        $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
    }
    catch {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'failed' `
            -Path $configPath `
            -Message $_.Exception.Message `
            -RemediationHint 'Verify talk.exe readiness, Windows microphone permission, and the intended input device before recording the corpus.'
    }

    $configuredInputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'InputDevice')
    if (-not [string]::IsNullOrWhiteSpace($configuredInputDevice)) {
        if ([string]$inventory.audioStatus -ne 'ready') {
            return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'input_device_selection' `
                -Status 'failed' `
                -Path $configPath `
                -Message ("configured input device [{0}] is not ready: {1}" -f $configuredInputDevice, [string]$inventory.audioReason) `
                -RemediationHint 'Fix the requested input device or choose another available microphone before recording the corpus.'
        }

        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'ready' `
            -Path $configPath `
            -Message ("configured input device locked: {0}" -f $configuredInputDevice)
    }

    $selectedInputDevice = [string]$inventory.selectedInputDevice
    $availableInputDevices = @(
        @(Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $inventory -Name 'availableInputDevices') |
            ForEach-Object { [string]$_ } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )

    if ([string]$inventory.audioStatus -ne 'ready') {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'failed' `
            -Path $configPath `
            -Message ("default input device is not ready: {0}" -f [string]$inventory.audioReason) `
            -RemediationHint 'Fix the Windows default microphone or pass -InputDevice to the intended ready device before recording the corpus.'
    }

    if ($availableInputDevices.Count -gt 1) {
        $recommendedInputDevice = if (-not [string]::IsNullOrWhiteSpace($selectedInputDevice)) {
            $selectedInputDevice
        } else {
            $availableInputDevices[0]
        }
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'input_device_selection' `
            -Status 'failed' `
            -Path $configPath `
            -Message ("multiple input devices are available and no explicit InputDevice was provided: selectedDefault={0}; available={1}" -f `
                $recommendedInputDevice, `
                ($availableInputDevices -join ', ')) `
            -RemediationCommand (New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand -Plan $Plan -ResumeExistingCorpus:$Plan.ResumeExistingCorpus -InputDevice $recommendedInputDevice) `
            -RemediationHint 'Pass -InputDevice to lock the intended microphone before recording this authoritative real-mic corpus.'
    }

    New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
        -Name 'input_device_selection' `
        -Status 'ready' `
        -Path $configPath `
        -Message ("single default input device ready: {0}" -f $selectedInputDevice)
}

function Get-TalkAsrRealMicDefaultWorkflowAverage {
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

function Get-TalkAsrRealMicDefaultWorkflowCloudBaselineSemanticValidityBudget {
    [pscustomobject]@{
        MaxMeanCloudBaselineCer = 0.60
        MaxCatastrophicSampleCer = 0.85
        MaxCatastrophicSampleCount = 1
    }
}

function Read-TalkAsrRealMicDefaultWorkflowExistingCloudBaselineReport {
    param(
        [Parameter(Mandatory = $true)][string]$ReportsRoot,
        [Parameter(Mandatory = $true)][string]$SampleId,
        [Parameter(Mandatory = $true)][string]$ExpectedCorpusManifestSha256,
        [Parameter(Mandatory = $true)][string]$ExpectedAudioSha256
    )

    if (-not (Test-Path -LiteralPath $ReportsRoot -PathType Container)) {
        return $null
    }

    $candidates = @(
        Get-ChildItem -LiteralPath $ReportsRoot -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like ("cloud-openai-compatible-*-$SampleId.json") } |
            Sort-Object Name
    )
    if ($candidates.Count -eq 0) {
        return $null
    }

    $parsedReports = @(
        foreach ($candidate in $candidates) {
            $reportPath = [System.IO.Path]::GetFullPath($candidate.FullName)
            $report = Get-Content -LiteralPath $reportPath -Raw -Encoding UTF8 | ConvertFrom-Json
            $corpusManifestSha256 = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'corpus_manifest_sha256')
            $audioSha256 = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'audio_sha256')
            $provenanceStatus = if (
                [string]::IsNullOrWhiteSpace($corpusManifestSha256) -and
                [string]::IsNullOrWhiteSpace($audioSha256)
            ) {
                'legacy'
            } elseif (
                [string]::Equals($corpusManifestSha256, $ExpectedCorpusManifestSha256, [System.StringComparison]::OrdinalIgnoreCase) -and
                [string]::Equals($audioSha256, $ExpectedAudioSha256, [System.StringComparison]::OrdinalIgnoreCase)
            ) {
                'match'
            } else {
                'mismatch'
            }

            [pscustomobject]@{
                ReportPath = $reportPath
                SampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'sample_id')
                Text = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'text')
                Cer = if ($null -ne (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'cer')) {
                    [double](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $report -Name 'cer')
                } else {
                    $null
                }
                CorpusManifestSha256 = $corpusManifestSha256
                AudioSha256 = $audioSha256
                ProvenanceStatus = $provenanceStatus
            }
        }
    )

    $matchingReport = $parsedReports |
        Where-Object { $_.ProvenanceStatus -eq 'match' } |
        Select-Object -First 1
    if ($null -ne $matchingReport) {
        return $matchingReport
    }
    $mismatchedReport = $parsedReports |
        Where-Object { $_.ProvenanceStatus -eq 'mismatch' } |
        Select-Object -First 1
    if ($null -ne $mismatchedReport) {
        return $mismatchedReport
    }
    $parsedReports[0]
}

function New-TalkAsrRealMicDefaultWorkflowCorpusSemanticValidityCheck {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        $CorpusSamples,
        [switch]$ResumeExistingCorpus
    )

    $reportsRoot = [string]$Plan.ReportsRoot
    if ([string]::IsNullOrWhiteSpace($reportsRoot) -or -not (Test-Path -LiteralPath $reportsRoot -PathType Container)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message 'semantic validity requires an existing reports root with cloud baseline reports'
    }

    $resolvedCorpusSamples = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value $CorpusSamples)
    if ($resolvedCorpusSamples.Count -eq 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message 'semantic validity requires readable corpus samples'
    }

    $corpusManifestPath = [string]$Plan.CorpusManifest
    if ([string]::IsNullOrWhiteSpace($corpusManifestPath) -or -not (Test-Path -LiteralPath $corpusManifestPath -PathType Leaf)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $corpusManifestPath `
            -Message 'semantic validity requires a readable current corpus manifest for provenance validation'
    }
    $corpusManifestSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $corpusManifestPath).Hash.ToLowerInvariant()

    $reports = New-Object System.Collections.Generic.List[object]
    $missingSampleIds = New-Object System.Collections.Generic.List[string]
    foreach ($sample in $resolvedCorpusSamples) {
        $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'SampleId')
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            $sampleId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'sampleId')
        }
        if ([string]::IsNullOrWhiteSpace($sampleId)) {
            continue
        }
        $audioSha256 = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'AudioSha256')
        if ([string]::IsNullOrWhiteSpace($audioSha256)) {
            $audioSha256 = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'audioSha256')
        }
        if ([string]::IsNullOrWhiteSpace($audioSha256)) {
            $audioWav = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'AudioWav')
            if ([string]::IsNullOrWhiteSpace($audioWav)) {
                $audioWav = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $sample -Name 'audioWav')
            }
            if (-not [string]::IsNullOrWhiteSpace($audioWav) -and (Test-Path -LiteralPath $audioWav -PathType Leaf)) {
                $audioSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $audioWav).Hash.ToLowerInvariant()
            }
        }
        if ([string]::IsNullOrWhiteSpace($audioSha256)) {
            $missingSampleIds.Add($sampleId) | Out-Null
            continue
        }

        $report = Read-TalkAsrRealMicDefaultWorkflowExistingCloudBaselineReport `
            -ReportsRoot $reportsRoot `
            -SampleId $sampleId `
            -ExpectedCorpusManifestSha256 $corpusManifestSha256 `
            -ExpectedAudioSha256 $audioSha256
        if ($null -eq $report) {
            $missingSampleIds.Add($sampleId) | Out-Null
            continue
        }

        $reports.Add($report) | Out-Null
    }

    if ($reports.Count -eq 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message 'semantic validity requires existing cloud baseline reports; none were found for this staged corpus'
    }
    $mismatchedProvenanceReports = @($reports.ToArray() | Where-Object { $_.ProvenanceStatus -eq 'mismatch' })
    if ($mismatchedProvenanceReports.Count -gt 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'failed' `
            -Path $reportsRoot `
            -Message ("cloud baseline report provenance does not match the current corpus; mismatchedSampleIds={0}; reportPaths={1}" -f `
                ((@($mismatchedProvenanceReports | ForEach-Object { $_.SampleId })) -join ', '), `
                ((@($mismatchedProvenanceReports | ForEach-Object { $_.ReportPath })) -join ', ')) `
            -RemediationHint 'Regenerate cloud baseline reports from the current corpus before default-model selection.'
    }
    $legacyProvenanceReports = @($reports.ToArray() | Where-Object { $_.ProvenanceStatus -eq 'legacy' })
    if ($legacyProvenanceReports.Count -gt 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message ("semantic validity ignores legacy cloud baseline reports without corpus/audio provenance; legacySampleIds={0}" -f `
                ((@($legacyProvenanceReports | ForEach-Object { $_.SampleId })) -join ', ')) `
            -RemediationHint 'Regenerate cloud baseline reports from the current corpus to enable semantic validity checks.'
    }
    if ($missingSampleIds.Count -gt 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message ("semantic validity requires one cloud baseline report per corpus sample; missingSampleIds={0}" -f ($missingSampleIds.ToArray() -join ', '))
    }

    $reportsWithMissingCer = @($reports.ToArray() | Where-Object { $null -eq $_.Cer })
    if ($reportsWithMissingCer.Count -gt 0) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'skipped' `
            -Path $reportsRoot `
            -Message ("semantic validity requires cloud baseline CER values; missingCerSampleIds={0}" -f ((@($reportsWithMissingCer | ForEach-Object { $_.SampleId })) -join ', '))
    }

    $budget = Get-TalkAsrRealMicDefaultWorkflowCloudBaselineSemanticValidityBudget
    $meanCloudBaselineCer = Get-TalkAsrRealMicDefaultWorkflowAverage `
        -Values ($reports.ToArray() | ForEach-Object { $_.Cer })
    $catastrophicReports = @(
        $reports.ToArray() |
            Where-Object { [double]$_.Cer -gt [double]$budget.MaxCatastrophicSampleCer }
    )
    $catastrophicSampleIds = @($catastrophicReports | ForEach-Object { [string]$_.SampleId })
    $catastrophicSampleCount = $catastrophicSampleIds.Count
    $suggestedRerecordSampleIds = @(
        $reports.ToArray() |
            Where-Object { [double]$_.Cer -gt [double]$budget.MaxMeanCloudBaselineCer } |
            ForEach-Object { [string]$_.SampleId } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
            Select-Object -Unique
    )
    if ($suggestedRerecordSampleIds.Count -eq 0) {
        $suggestedRerecordSampleIds = @($catastrophicSampleIds | Select-Object -Unique)
    }

    if ([double]$meanCloudBaselineCer -gt [double]$budget.MaxMeanCloudBaselineCer -or
        $catastrophicSampleCount -gt [int]$budget.MaxCatastrophicSampleCount) {
        $remediationArtifactPath = Write-TalkAsrRealMicDefaultWorkflowRerecordPromptManifest `
            -Plan $Plan `
            -SampleIds $suggestedRerecordSampleIds `
            -ReasonSlug 'semantic-drift'
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_semantic_validity' `
            -Status 'failed' `
            -Path $reportsRoot `
            -Message ("existing cloud baseline reports suggest this staged corpus no longer matches the current prompt references: meanCloudBaselineCer={0}; catastrophicSampleCount={1}; catastrophicSampleIds={2}" -f `
                ([Math]::Round([double]$meanCloudBaselineCer, 3)), `
                $catastrophicSampleCount, `
                (($catastrophicSampleIds | Select-Object -Unique) -join ', ')) `
            -RemediationCommand (New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand -Plan $Plan -ResumeExistingCorpus -ForceRecordSampleId $suggestedRerecordSampleIds) `
            -RemediationHint 'Re-record the staged corpus before reusing it for default model selection.' `
            -RemediationArtifactPath $remediationArtifactPath
    }

    New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
        -Name 'corpus_semantic_validity' `
        -Status 'ready' `
        -Path $reportsRoot `
        -Message ("existing cloud baseline reports remain semantically consistent: meanCloudBaselineCer={0}; catastrophicSampleCount={1}" -f `
            ([Math]::Round([double]$meanCloudBaselineCer, 3)), `
            $catastrophicSampleCount)
}

function Get-TalkAsrRealMicDefaultWorkflowSelectedModelId {
    param($DefaultModelWorkflowResult)

    $selection = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
        -Object $DefaultModelWorkflowResult `
        -Name 'Selection'
    $selectedModelId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
            -Object $selection `
            -Name 'selectedModelId')
    if (-not [string]::IsNullOrWhiteSpace($selectedModelId)) {
        return $selectedModelId
    }

    $selectionJson = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
            -Object $DefaultModelWorkflowResult `
            -Name 'SelectionJson')
    if ([string]::IsNullOrWhiteSpace($selectionJson) -or -not (Test-Path -LiteralPath $selectionJson -PathType Leaf)) {
        return $null
    }

    try {
        $selectionDocument = Get-Content -LiteralPath $selectionJson -Raw -Encoding UTF8 | ConvertFrom-Json
        $selectionJsonModelId = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
                -Object $selectionDocument `
                -Name 'selectedModelId')
        if (-not [string]::IsNullOrWhiteSpace($selectionJsonModelId)) {
            return $selectionJsonModelId
        }
    }
    catch {
        return $null
    }

    $null
}

function Test-TalkAsrRealMicDefaultWorkflowReplayTalkExeSupportsCommand {
    param(
        [string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$CommandName
    )

    if ([string]::IsNullOrWhiteSpace($TalkExe)) {
        return $false
    }

    $resolvedTalkExe = try {
        Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $TalkExe
    }
    catch {
        return $false
    }

    if (-not (Test-Path -LiteralPath $resolvedTalkExe -PathType Leaf)) {
        return $false
    }

    try {
        $output = & $resolvedTalkExe --help 2>&1
        if ($LASTEXITCODE -ne 0) {
            return $false
        }

        $outputText = (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine)
        $outputText -match [Regex]::Escape($CommandName)
    }
    catch {
        $false
    }
}

function Resolve-TalkAsrRealMicDefaultWorkflowDefaultReplayTalkExe {
    if ((Split-Path -Leaf $PSScriptRoot) -ne 'scripts') {
        return $null
    }

    $talkRoot = Split-Path -Parent $PSScriptRoot
    [System.IO.Path]::GetFullPath((Join-Path $talkRoot 'target\release\talk.exe'))
}

function Resolve-TalkAsrRealMicDefaultWorkflowReplayTalkExe {
    param(
        [string]$TalkExe,
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)][string]$RequiredCommandName
    )

    $candidatePaths = New-Object System.Collections.Generic.List[string]
    $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
    $recorderPlanTalkExe = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
            -Object $recorderPlan `
            -Name 'TalkExe')
    if (-not [string]::IsNullOrWhiteSpace($recorderPlanTalkExe)) {
        $candidatePaths.Add([System.IO.Path]::GetFullPath($recorderPlanTalkExe)) | Out-Null
    }

    if (-not [string]::IsNullOrWhiteSpace($TalkExe)) {
        $resolvedInputTalkExe = Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $TalkExe
        if (-not ($candidatePaths -contains $resolvedInputTalkExe)) {
            $candidatePaths.Add($resolvedInputTalkExe) | Out-Null
        }
    }

    $defaultReplayTalkExe = Resolve-TalkAsrRealMicDefaultWorkflowDefaultReplayTalkExe
    if (-not [string]::IsNullOrWhiteSpace($defaultReplayTalkExe) -and -not ($candidatePaths -contains $defaultReplayTalkExe)) {
        $candidatePaths.Add($defaultReplayTalkExe) | Out-Null
    }

    foreach ($candidatePath in $candidatePaths) {
        if (Test-TalkAsrRealMicDefaultWorkflowReplayTalkExeSupportsCommand -TalkExe $candidatePath -CommandName $RequiredCommandName) {
            return $candidatePath
        }
    }

    $null
}

function Resolve-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplayJson {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    [System.IO.Path]::GetFullPath(
        (Join-Path (Split-Path -Parent $CorpusManifest) ("faithful-correction-replay-{0}.json" -f $ModelId))
    )
}

function Resolve-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplayLiveJson {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    [System.IO.Path]::GetFullPath(
        (Join-Path (Split-Path -Parent $CorpusManifest) ("provider-correction-replay-{0}.json" -f $ModelId))
    )
}

function Resolve-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplayDeterministicJson {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string]$ModelId
    )

    [System.IO.Path]::GetFullPath(
        (Join-Path (Split-Path -Parent $CorpusManifest) ("provider-correction-replay-{0}-deterministic.json" -f $ModelId))
    )
}

function Get-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplaySkipReason {
    param(
        [string]$SelectedModelId,
        [string]$TalkExe,
        [string]$ConfigPath
    )

    if ([string]::IsNullOrWhiteSpace($SelectedModelId)) {
        return 'provider correction replay skipped because the default selection did not expose selectedModelId'
    }
    if ([string]::IsNullOrWhiteSpace($TalkExe)) {
        return 'provider correction replay skipped because Talk.exe path is unavailable'
    }
    if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
        return 'provider correction replay skipped because config path is unavailable'
    }

    $null
}

function Get-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplaySkipReason {
    param(
        [string]$SelectedModelId,
        [string]$TalkExe
    )

    if ([string]::IsNullOrWhiteSpace($SelectedModelId)) {
        return 'faithful correction replay skipped because the default selection did not expose selectedModelId'
    }
    if ([string]::IsNullOrWhiteSpace($TalkExe)) {
        return 'faithful correction replay skipped because Talk.exe path is unavailable'
    }

    $null
}

function Get-TalkAsrRealMicDefaultWorkflowRejectedFaithfulReplaySampleIds {
    param($FaithfulCorrectionReplayResult)

    @(
        @(Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $FaithfulCorrectionReplayResult -Name 'Samples') |
            ForEach-Object {
                $validation = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $_ -Name 'validation'
                $accepted = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $validation -Name 'accepted'
                if ($accepted -eq $false) {
                    [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $_ -Name 'sampleId')
                }
            } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
            Select-Object -Unique
    )
}

function New-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplayFailureMessage {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)]$FaithfulCorrectionReplayResult
    )

    $rejectedSampleIds = @(Get-TalkAsrRealMicDefaultWorkflowRejectedFaithfulReplaySampleIds -FaithfulCorrectionReplayResult $FaithfulCorrectionReplayResult)
    $acceptedCount = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $FaithfulCorrectionReplayResult -Name 'AcceptedCount'
    $rejectedCount = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $FaithfulCorrectionReplayResult -Name 'RejectedCount'
    $outputJson = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $FaithfulCorrectionReplayResult -Name 'OutputJson')
    $remediationArtifactPath = Write-TalkAsrRealMicDefaultWorkflowRerecordPromptManifest `
        -Plan $Plan `
        -SampleIds $rejectedSampleIds `
        -ReasonSlug 'faithful-reject' `
        -SourceSamples (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan.RecorderPlan -Name 'Samples')
    $remediationCommand = New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand `
        -Plan $Plan `
        -ResumeExistingCorpus `
        -ForceRecordSampleId $rejectedSampleIds

    $detailParts = New-Object System.Collections.Generic.List[string]
    $detailParts.Add(("acceptedCount={0}" -f $acceptedCount)) | Out-Null
    $detailParts.Add(("rejectedCount={0}" -f $rejectedCount)) | Out-Null
    if ($rejectedSampleIds.Count -gt 0) {
        $detailParts.Add(("sampleIds={0}" -f ($rejectedSampleIds -join ', '))) | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($outputJson)) {
        $detailParts.Add(("report={0}" -f $outputJson)) | Out-Null
    }
    if (-not [string]::IsNullOrWhiteSpace($remediationArtifactPath)) {
        $detailParts.Add(("artifact={0}" -f $remediationArtifactPath)) | Out-Null
    }
    $detailParts.Add(("remediation={0}" -f $remediationCommand)) | Out-Null

    "faithful correction replay rejected sample ids: {0}" -f ($detailParts.ToArray() -join '; ')
}

function Test-TalkAsrRealMicDefaultWorkflowSamePath {
    param(
        [Parameter(Mandatory = $true)][string]$Left,
        [Parameter(Mandatory = $true)][string]$Right
    )

    $leftPath = [System.IO.Path]::GetFullPath($Left)
    $rightPath = [System.IO.Path]::GetFullPath($Right)
    [string]::Equals($leftPath, $rightPath, [System.StringComparison]::OrdinalIgnoreCase)
}

function Test-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [scriptblock]$ReadinessInvoker
    )

    $statusJson = Resolve-TalkAsrRealMicDefaultWorkflowRecordOnlyStatusJson -Plan $Plan
    if (-not (Test-Path -LiteralPath $statusJson -PathType Leaf)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'skipped' `
            -Path $statusJson `
            -Message 'record-only status is not present; using corpus manifest directly'
    }

    try {
        $status = Get-Content -LiteralPath $statusJson -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message ("record-only status JSON is invalid: {0}" -f $_.Exception.Message)
    }

    $workflowKind = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'workflowKind')
    if ($workflowKind -ne 'talk-asr-real-mic-default-model-record-only-status') {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message ("record-only status has unexpected workflowKind: {0}" -f $workflowKind)
    }

    $statusCorpusManifest = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'corpusManifest')
    if ([string]::IsNullOrWhiteSpace($statusCorpusManifest)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message 'record-only status is missing corpusManifest'
    }

    $planCorpusManifest = [string]$Plan.CorpusManifest
    if (-not (Test-TalkAsrRealMicDefaultWorkflowSamePath -Left $statusCorpusManifest -Right $planCorpusManifest)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message ("record-only status corpusManifest does not match current plan: status={0}; plan={1}" -f ([System.IO.Path]::GetFullPath($statusCorpusManifest)), ([System.IO.Path]::GetFullPath($planCorpusManifest)))
    }

    $ready = [bool](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'ready')
    $sampleCount = [int](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'sampleCount')
    $audioFileCount = [int](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'audioFileCount')
    if (-not $ready) {
        $validationErrors = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'validationErrors'))
        $missingAudio = @(ConvertTo-TalkAsrRealMicDefaultWorkflowArray -Value (Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'missingAudioWav'))
        $detailParts = New-Object System.Collections.Generic.List[string]
        if ($validationErrors.Count -gt 0) {
            $detailParts.Add(("validationErrors={0}" -f ($validationErrors -join ', '))) | Out-Null
        }
        if ($missingAudio.Count -gt 0) {
            $detailParts.Add(("missingAudioWav={0}" -f ($missingAudio -join ', '))) | Out-Null
        }
        $details = if ($detailParts.Count -gt 0) { '; {0}' -f ($detailParts.ToArray() -join '; ') } else { '' }
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message ("record-only status is not ready: sampleCount={0}; audioFileCount={1}{2}" -f $sampleCount, $audioFileCount, $details)
    }

    $resolvedRerecordSampleIds = @()
    try {
        $promptManifest = [string]$Plan.PromptManifest
        if (-not [string]::IsNullOrWhiteSpace($promptManifest) -and (Test-Path -LiteralPath $promptManifest -PathType Leaf)) {
            $resolvedRerecordSampleIds = @(Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples (Read-TalkAsrRealMicDefaultWorkflowPromptSamples -PromptManifest $promptManifest))
        }
    }
    catch {
        $resolvedRerecordSampleIds = @()
    }
    if ($resolvedRerecordSampleIds.Count -eq 0) {
        try {
            if (Test-Path -LiteralPath $planCorpusManifest -PathType Leaf) {
                $resolvedRerecordSampleIds = @(Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples (Read-TalkAsrRealMicDefaultWorkflowCorpusSamples -CorpusManifest $planCorpusManifest))
            }
        }
        catch {
            $resolvedRerecordSampleIds = @()
        }
    }

    $configuredInputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'configuredInputDevice')
    if ([string]::IsNullOrWhiteSpace($configuredInputDevice)) {
        $configuredInputDevice = $null
    }
    $capturedInputDevices = @(
        @(Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'capturedInputDevices') |
            ForEach-Object { [string]$_ } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )
    $availableInputDevices = @(
        @(Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'availableInputDevices') |
            ForEach-Object { [string]$_ } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )
    $inputDeviceSelectionWarning = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $status -Name 'inputDeviceSelectionWarning')
    $recommendedInputDevice = if (-not [string]::IsNullOrWhiteSpace($configuredInputDevice)) {
        $configuredInputDevice
    } elseif ($capturedInputDevices.Count -gt 0) {
        $capturedInputDevices[0]
    } elseif ($availableInputDevices.Count -gt 0) {
        $availableInputDevices[0]
    } else {
        $null
    }
    $rerecordCommand = New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand `
        -Plan $Plan `
        -ResumeExistingCorpus `
        -ForceRecordSampleId $resolvedRerecordSampleIds `
        -InputDevice $recommendedInputDevice

    if (-not [string]::IsNullOrWhiteSpace($inputDeviceSelectionWarning)) {
        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'record_only_status' `
            -Status 'failed' `
            -Path $statusJson `
            -Message ("record-only status is not authoritative for SkipRecording: {0}" -f $inputDeviceSelectionWarning) `
            -RemediationCommand $rerecordCommand `
            -RemediationHint 'Re-record the staged real-microphone corpus with an explicit -InputDevice before reusing it for default-model locking.'
    }

    $hasInputDeviceDiagnostics = `
        (Test-TalkAsrRealMicDefaultWorkflowHasProperty -Object $status -Name 'configuredInputDevice') -or `
        (Test-TalkAsrRealMicDefaultWorkflowHasProperty -Object $status -Name 'capturedInputDevices') -or `
        (Test-TalkAsrRealMicDefaultWorkflowHasProperty -Object $status -Name 'availableInputDevices') -or `
        (Test-TalkAsrRealMicDefaultWorkflowHasProperty -Object $status -Name 'inputDeviceSelectionWarning')
    if (-not $hasInputDeviceDiagnostics) {
        $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
        $talkExe = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'TalkExe')
        if ([string]::IsNullOrWhiteSpace($talkExe)) {
            $talkExe = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'TalkExe')
        }
        if (-not [string]::IsNullOrWhiteSpace($talkExe) -and (Test-Path -LiteralPath $talkExe -PathType Leaf)) {
            $supportsReadiness = ($null -ne $ReadinessInvoker) -or `
                (Test-TalkAsrRealMicDefaultWorkflowReplayTalkExeSupportsCommand -TalkExe $talkExe -CommandName 'readiness')
            if ($supportsReadiness) {
                try {
                    $configPath = Ensure-TalkAsrRealMicDefaultWorkflowRecorderConfig -Plan $Plan
                    $readinessReport = Invoke-TalkAsrRealMicDefaultWorkflowReadiness `
                        -TalkExe $talkExe `
                        -ConfigPath $configPath `
                        -ReadinessInvoker $ReadinessInvoker
                    $inventory = New-TalkDesktopLaunchInputDeviceInventory -ReadinessReport $readinessReport
                    $currentAvailableInputDevices = @(
                        @(Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $inventory -Name 'availableInputDevices') |
                            ForEach-Object { [string]$_ } |
                            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
                    )
                    $currentSelectedInputDevice = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $inventory -Name 'selectedInputDevice')
                    if ([string]$inventory.audioStatus -eq 'ready' -and $currentAvailableInputDevices.Count -gt 1) {
                        $recommendedLegacyInputDevice = if (-not [string]::IsNullOrWhiteSpace($currentSelectedInputDevice)) {
                            $currentSelectedInputDevice
                        } else {
                            $currentAvailableInputDevices[0]
                        }
                        $legacyRerecordCommand = New-TalkAsrRealMicDefaultWorkflowRecordOnlyCommand `
                            -Plan $Plan `
                            -ResumeExistingCorpus `
                            -ForceRecordSampleId $resolvedRerecordSampleIds `
                            -InputDevice $recommendedLegacyInputDevice
                        return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                            -Name 'record_only_status' `
                            -Status 'failed' `
                            -Path $statusJson `
                            -Message ("record-only status predates input-device diagnostics while multiple input devices are currently available: selectedDefault={0}; available={1}" -f `
                                $recommendedLegacyInputDevice, `
                                ($currentAvailableInputDevices -join ', ')) `
                            -RemediationCommand $legacyRerecordCommand `
                            -RemediationHint 'Re-record the staged real-microphone corpus with an explicit -InputDevice before reusing it for default-model locking.'
                    }
                }
                catch {
                }
            }
        }
    }

    $inputDeviceDetails = if ([string]::IsNullOrWhiteSpace($inputDeviceSelectionWarning)) {
        ''
    } else {
        '; {0}' -f $inputDeviceSelectionWarning
    }
    New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
        -Name 'record_only_status' `
        -Status 'ready' `
        -Path $statusJson `
        -Message ("record-only status ready: sampleCount={0}; audioFileCount={1}{2}" -f `
            $sampleCount, `
            $audioFileCount, `
            $inputDeviceDetails)
}

function Read-TalkAsrRealMicDefaultWorkflowDesktopProviderApiKey {
    param([string]$ConfigPath)

    if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $ConfigPath -PathType Leaf)) {
        return $null
    }

    $insideProvider = $false
    foreach ($line in Get-Content -LiteralPath $ConfigPath -Encoding UTF8) {
        $trimmed = ([string]$line).Trim()
        if ([string]::IsNullOrWhiteSpace($trimmed) -or $trimmed.StartsWith('#')) {
            continue
        }

        if ($trimmed -match '^\[(.+)\]\s*$') {
            $insideProvider = ($matches[1] -eq 'provider')
            continue
        }

        if (-not $insideProvider) {
            continue
        }

        if ($trimmed -match '^api_key\s*=\s*"([^"]*)"\s*(?:#.*)?$') {
            return $matches[1]
        }
        if ($trimmed -match "^api_key\s*=\s*'([^']*)'\s*(?:#.*)?$") {
            return $matches[1]
        }
    }

    $null
}

function Get-TalkAsrRealMicDefaultWorkflowHomeDirectory {
    $userProfile = [Environment]::GetEnvironmentVariable('USERPROFILE', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($userProfile)) {
        return $userProfile
    }

    $home = [Environment]::GetEnvironmentVariable('HOME', 'Process')
    if (-not [string]::IsNullOrWhiteSpace($home)) {
        return $home
    }

    $null
}

function Test-TalkAsrRealMicDefaultWorkflowDashScopeEndpoint {
    param([string]$Endpoint)

    if ([string]::IsNullOrWhiteSpace($Endpoint)) {
        return $false
    }

    $uri = $null
    if (-not [System.Uri]::TryCreate($Endpoint, [System.UriKind]::Absolute, [ref]$uri)) {
        return $false
    }

    ($uri.Scheme -eq 'https') -and ($uri.Host -eq 'dashscope.aliyuncs.com')
}

function Resolve-TalkAsrRealMicDefaultWorkflowLegacyDashScopeApiKeyJsonPath {
    param([string]$CloudOpenAiCompatibleEndpoint)

    if (-not (Test-TalkAsrRealMicDefaultWorkflowDashScopeEndpoint -Endpoint $CloudOpenAiCompatibleEndpoint)) {
        return $null
    }

    $homeDirectory = Get-TalkAsrRealMicDefaultWorkflowHomeDirectory
    if ([string]::IsNullOrWhiteSpace($homeDirectory)) {
        return $null
    }

    $credentialPath = Join-Path $homeDirectory '.neuro\qwen-platform\qwen-dashscope-openai\api-key\manual-live.json'
    if (Test-Path -LiteralPath $credentialPath -PathType Leaf) {
        return [System.IO.Path]::GetFullPath($credentialPath)
    }

    $null
}

function Read-TalkAsrRealMicDefaultWorkflowApiKeyFromJsonPath {
    param([string]$ApiKeyJsonPath)

    if ([string]::IsNullOrWhiteSpace($ApiKeyJsonPath)) {
        return $null
    }
    if (-not (Test-Path -LiteralPath $ApiKeyJsonPath -PathType Leaf)) {
        return $null
    }

    try {
        $json = Get-Content -LiteralPath $ApiKeyJsonPath -Raw -Encoding UTF8 | ConvertFrom-Json
    }
    catch {
        return $null
    }

    foreach ($fieldName in @('apiKey', 'api_key', 'key')) {
        $value = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $json -Name $fieldName)
        if (-not [string]::IsNullOrWhiteSpace($value) -and $value.Trim() -eq $value) {
            return $value
        }
    }

    $null
}

function Get-TalkAsrRealMicDefaultWorkflowCloudApiKeySource {
    param(
        [Parameter(Mandatory = $true)][string]$EnvironmentVariableName,
        [string]$CloudOpenAiCompatibleEndpoint,
        [string]$ConfigPath
    )

    $environmentValue = [Environment]::GetEnvironmentVariable($EnvironmentVariableName, 'Process')
    if (-not [string]::IsNullOrWhiteSpace($environmentValue)) {
        return [pscustomobject]@{
            Available = $true
            Source = 'environment'
            Value = $environmentValue
        }
    }

    $configValue = Read-TalkAsrRealMicDefaultWorkflowDesktopProviderApiKey -ConfigPath $ConfigPath
    if (-not [string]::IsNullOrWhiteSpace($configValue)) {
        return [pscustomobject]@{
            Available = $true
            Source = 'desktop_config'
            Value = $configValue
        }
    }

    $legacyJsonPath = Resolve-TalkAsrRealMicDefaultWorkflowLegacyDashScopeApiKeyJsonPath `
        -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint
    $legacyJsonValue = Read-TalkAsrRealMicDefaultWorkflowApiKeyFromJsonPath -ApiKeyJsonPath $legacyJsonPath
    if (-not [string]::IsNullOrWhiteSpace($legacyJsonValue)) {
        return [pscustomobject]@{
            Available = $true
            Source = 'legacy_json'
            Value = $legacyJsonValue
        }
    }

    [pscustomobject]@{
        Available = $false
        Source = 'missing'
        Value = $null
    }
}

function Invoke-TalkAsrRealMicDefaultWorkflowWithCloudApiKeySource {
    param(
        [Parameter(Mandatory = $true)][string]$EnvironmentVariableName,
        [string]$CloudOpenAiCompatibleEndpoint,
        [string]$ConfigPath,
        [Parameter(Mandatory = $true)][scriptblock]$ScriptBlock
    )

    $originalValue = [Environment]::GetEnvironmentVariable($EnvironmentVariableName, 'Process')
    $injected = $false
    if ([string]::IsNullOrWhiteSpace($originalValue)) {
        $configValue = Read-TalkAsrRealMicDefaultWorkflowDesktopProviderApiKey -ConfigPath $ConfigPath
        if (-not [string]::IsNullOrWhiteSpace($configValue)) {
            [Environment]::SetEnvironmentVariable($EnvironmentVariableName, $configValue, 'Process')
            $injected = $true
        } else {
            $legacyJsonPath = Resolve-TalkAsrRealMicDefaultWorkflowLegacyDashScopeApiKeyJsonPath `
                -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint
            $legacyJsonValue = Read-TalkAsrRealMicDefaultWorkflowApiKeyFromJsonPath -ApiKeyJsonPath $legacyJsonPath
            if (-not [string]::IsNullOrWhiteSpace($legacyJsonValue)) {
                [Environment]::SetEnvironmentVariable($EnvironmentVariableName, $legacyJsonValue, 'Process')
                $injected = $true
            }
        }
    }

    try {
        & $ScriptBlock
    }
    finally {
        if ($injected) {
            [Environment]::SetEnvironmentVariable($EnvironmentVariableName, $originalValue, 'Process')
        }
    }
}

function Invoke-TalkAsrRealMicDefaultWorkflowAudioProbe {
    param(
        [Parameter(Mandatory = $true)][string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][ValidateRange(1, 60)][int]$Seconds,
        [scriptblock]$AudioProbeInvoker
    )

    if ($null -ne $AudioProbeInvoker) {
        return & $AudioProbeInvoker $TalkExe $ConfigPath $Seconds
    }

    $output = & $TalkExe probe-audio --config $ConfigPath --seconds $Seconds --json 2>&1
    [pscustomobject]@{
        ExitCode = $LASTEXITCODE
        Stdout = (($output | ForEach-Object { [string]$_ }) -join [Environment]::NewLine)
        Stderr = ''
    }
}

function New-TalkAsrRealMicDefaultWorkflowAudioProbeCheck {
    param(
        [Parameter(Mandatory = $true)][string]$TalkExe,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][ValidateRange(1, 60)][int]$Seconds,
        [scriptblock]$AudioProbeInvoker
    )

    try {
        $probe = Invoke-TalkAsrRealMicDefaultWorkflowAudioProbe `
            -TalkExe $TalkExe `
            -ConfigPath $ConfigPath `
            -Seconds $Seconds `
            -AudioProbeInvoker $AudioProbeInvoker

        if ([int]$probe.ExitCode -ne 0) {
            return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'microphone_signal' `
                -Status 'failed' `
                -Path $ConfigPath `
                -Message ("talk.exe probe-audio failed with exit code {0}: {1}" -f [int]$probe.ExitCode, [string]$probe.Stdout) `
                -RemediationHint 'Check the microphone permission, selected input device, and talk-desktop.toml audio backend before recording the corpus.'
        }

        $probeJson = ([string]$probe.Stdout) | ConvertFrom-Json
        $nativeStatus = [string]$probeJson.audio.nativeWindows.status
        $deviceName = [string]$probeJson.audio.nativeWindows.deviceName
        $artifactPath = [string]$probeJson.audio.signal.artifactPath
        $silent = [bool]$probeJson.audio.signal.silent
        $durationSeconds = if ($null -ne $probeJson.audio.signal.PSObject.Properties['durationSeconds']) {
            [double]$probeJson.audio.signal.durationSeconds
        } else {
            [double]$Seconds
        }
        $peak = [double]$probeJson.audio.signal.peak
        $rms = [double]$probeJson.audio.signal.rms

        if ($nativeStatus -ne 'ready') {
            return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'microphone_signal' `
                -Status 'failed' `
                -Path $artifactPath `
                -Message ("microphone backend is not ready: {0}" -f $nativeStatus) `
                -RemediationHint 'Check Windows microphone permission and the configured input device before recording the corpus.'
        }

        $probeSummary = [pscustomobject]@{
            durationSeconds = $durationSeconds
            peak = $peak
            rms = $rms
            silent = $silent
        }

        if (-not (Test-TalkDesktopLaunchAudioProbeHasSignal -ProbeSummary $probeSummary)) {
            $failureMessage = Get-TalkDesktopLaunchAudioProbeFailureReason `
                -ProbeSummary $probeSummary `
                -SilentReason ("microphone probe recorded silence: device={0}; peak={1}; rms={2}" -f $deviceName, $peak, $rms) `
                -WeakReason ("microphone probe captured speech that is too weak for provider transcription: device={0}; peak={1}; rms={2}" -f $deviceName, $peak, $rms)
            return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'microphone_signal' `
                -Status 'failed' `
                -Path $artifactPath `
                -Message $failureMessage `
                -RemediationHint 'Speak during the probe, select the correct microphone, or check Windows microphone permissions before recording the corpus.'
        }

        New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'microphone_signal' `
            -Status 'ready' `
            -Path $artifactPath `
            -Message ("non-silent microphone signal: device={0}; seconds={1}; peak={2}; rms={3}" -f $deviceName, $Seconds, $peak, $rms)
    }
    catch {
        New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'microphone_signal' `
            -Status 'failed' `
            -Path $ConfigPath `
            -Message $_.Exception.Message `
            -RemediationHint 'Check the microphone permission, selected input device, and talk-desktop.toml audio backend before recording the corpus.'
    }
}

function Resolve-TalkAsrRealMicDefaultWorkflowToolPath {
    param(
        [string]$Path,
        [Parameter(Mandatory = $true)][ValidateSet('asr-bench', 'local-daemon')][string]$Tool
    )

    if (-not [string]::IsNullOrWhiteSpace($Path)) {
        return Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $Path
    }

    Resolve-TalkAsrDefaultToolPath -Tool $Tool
}

function Test-TalkAsrRealMicDefaultModelWorkflowPreflight {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [string]$ModelRoot,
        [string]$AsrBenchExe,
        [string]$LocalAsrDaemonExe,
        [string]$CloudOpenAiCompatibleEndpoint,
        [string]$CloudOpenAiCompatibleModel,
        [string]$CloudOpenAiCompatibleApiKeyEnv = 'TALK_PROVIDER_API_KEY',
        [switch]$ProbeAudio,
        [ValidateRange(1, 60)][int]$AudioProbeSeconds = 2,
        [scriptblock]$AudioProbeInvoker,
        [scriptblock]$ReadinessInvoker,
        [switch]$ResumeExistingCorpus,
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [switch]$SkipApply,
        [switch]$AllowMissingCloudBaseline
    )

    $checks = New-Object System.Collections.Generic.List[object]

    if ($SkipRecording) {
        $corpusManifest = [string]$Plan.CorpusManifest
        $promptManifest = [string]$Plan.PromptManifest
        $corpusSamples = $null
        $promptSamples = $null

        if (Test-Path -LiteralPath $promptManifest -PathType Leaf) {
            try {
                $promptSamples = @(Read-TalkAsrRealMicDefaultWorkflowPromptSamples -PromptManifest $promptManifest)
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'prompt_manifest' `
                    -Status 'ready' `
                    -Path $promptManifest `
                    -Message ("current prompt manifest has {0} sample(s)" -f $promptSamples.Count))) | Out-Null
            }
            catch {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'prompt_manifest' `
                    -Status 'failed' `
                    -Path $promptManifest `
                    -Message $_.Exception.Message)) | Out-Null
            }
        } else {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'prompt_manifest' `
                -Status 'missing' `
                -Path $promptManifest `
                -Message 'current prompt manifest is required when SkipRecording is set')) | Out-Null
        }

        if (Test-Path -LiteralPath $corpusManifest -PathType Leaf) {
            try {
                $corpusSamples = @(Read-TalkAsrRealMicDefaultWorkflowCorpusSamples -CorpusManifest $corpusManifest)
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'corpus_manifest' `
                    -Status 'ready' `
                    -Path $corpusManifest `
                    -Message ("existing corpus manifest has {0} sample(s)" -f $corpusSamples.Count))) | Out-Null
            }
            catch {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'corpus_manifest' `
                    -Status 'failed' `
                    -Path $corpusManifest `
                    -Message $_.Exception.Message)) | Out-Null
            }
        } else {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'corpus_manifest' `
                -Status 'missing' `
                -Path $corpusManifest `
                -Message 'existing corpus manifest is required when SkipRecording is set')) | Out-Null
        }
        $checks.Add((Test-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus -Plan $Plan -ReadinessInvoker $ReadinessInvoker)) | Out-Null
        if ($null -ne $promptSamples -and $null -ne $corpusSamples) {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowCorpusPromptAlignmentCheck `
                -Plan $Plan `
                -PromptSampleIds (Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples $promptSamples) `
                -CorpusSampleIds (Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples $corpusSamples) `
                -ResumeExistingCorpus:$ResumeExistingCorpus)) | Out-Null
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowCorpusSemanticValidityCheck `
                -Plan $Plan `
                -CorpusSamples $corpusSamples `
                -ResumeExistingCorpus:$ResumeExistingCorpus)) | Out-Null
        } else {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'corpus_prompt_alignment' `
                -Status 'skipped' `
                -Path $corpusManifest `
                -Message 'alignment requires a readable current prompt manifest and corpus manifest')) | Out-Null
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'corpus_semantic_validity' `
                -Status 'skipped' `
                -Path ([string]$Plan.ReportsRoot) `
                -Message 'semantic validity requires a readable corpus manifest and existing cloud baseline reports')) | Out-Null
        }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'talk_probe_exe' `
            -Status 'skipped' `
            -Path $null `
            -Message 'recording is skipped')) | Out-Null
    } else {
        $promptManifest = [string]$Plan.PromptManifest
        $promptManifestExists = Test-Path -LiteralPath $promptManifest -PathType Leaf
        $promptManifestStatus = if ($promptManifestExists) { 'ready' } else { 'missing' }
        $promptManifestMessage = if ($promptManifestExists) { 'prompt manifest is available' } else { 'prompt manifest is missing' }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'prompt_manifest' `
            -Status $promptManifestStatus `
            -Path $promptManifest `
            -Message $promptManifestMessage)) | Out-Null

        $talkExe = [string]$Plan.RecorderPlan.TalkExe
        $talkExeExists = Test-Path -LiteralPath $talkExe -PathType Leaf
        $talkExeStatus = if ($talkExeExists) { 'ready' } else { 'missing' }
        $talkExeMessage = if ($talkExeExists) { 'talk.exe probe-audio is available' } else { 'talk.exe is required for real microphone recording' }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'talk_probe_exe' `
            -Status $talkExeStatus `
            -Path $talkExe `
            -Message $talkExeMessage)) | Out-Null

        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'corpus_manifest' `
            -Status 'planned' `
            -Path ([string]$Plan.CorpusManifest) `
            -Message 'corpus manifest will be created by the recording step')) | Out-Null
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowInputDeviceSelectionCheck `
            -Plan $Plan `
            -ReadinessInvoker $ReadinessInvoker)) | Out-Null
    }

    if ($RecordOnly) {
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'asr_bench_exe' `
            -Status 'skipped' `
            -Path ([string]$Plan.AsrBenchExe) `
            -Message 'record-only mode stops before same-corpus benchmarking')) | Out-Null
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'local_asr_daemon_exe' `
            -Status 'skipped' `
            -Path ([string]$Plan.LocalAsrDaemonExe) `
            -Message 'record-only mode stops before local ASR benchmarking')) | Out-Null
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'model_root' `
            -Status 'skipped' `
            -Path ([string]$Plan.ModelRoot) `
            -Message 'record-only mode does not require installed sherpa models')) | Out-Null
        foreach ($id in @($Plan.ModelId)) {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name "model:$id" `
                -Status 'skipped' `
                -Path $null `
                -Message 'record-only mode does not validate benchmark models')) | Out-Null
        }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'cloud_baseline_api_key' `
            -Status 'skipped' `
            -Path $CloudOpenAiCompatibleApiKeyEnv `
            -Message 'record-only mode stops before the cloud baseline benchmark')) | Out-Null
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'desktop_config' `
            -Status 'skipped' `
            -Path ([string]$Plan.ConfigPath) `
            -Message 'record-only mode does not apply the selected model')) | Out-Null
    } else {
        $resolvedAsrBenchExe = Resolve-TalkAsrRealMicDefaultWorkflowToolPath -Path $AsrBenchExe -Tool 'asr-bench'
        $asrBenchExists = Test-Path -LiteralPath $resolvedAsrBenchExe -PathType Leaf
        $asrBenchStatus = if ($asrBenchExists) { 'ready' } else { 'missing' }
        $asrBenchMessage = if ($asrBenchExists) { 'asr-bench executable is available' } else { 'asr-bench executable is missing' }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'asr_bench_exe' `
            -Status $asrBenchStatus `
            -Path $resolvedAsrBenchExe `
            -Message $asrBenchMessage `
            -RemediationHint 'Use a packaged Talk release or pass -AsrBenchExe .\.internal\asr-bench.exe.')) | Out-Null

        $resolvedLocalAsrDaemonExe = Resolve-TalkAsrRealMicDefaultWorkflowToolPath -Path $LocalAsrDaemonExe -Tool 'local-daemon'
        $localAsrDaemonExists = Test-Path -LiteralPath $resolvedLocalAsrDaemonExe -PathType Leaf
        $localAsrDaemonStatus = if ($localAsrDaemonExists) { 'ready' } else { 'missing' }
        $localAsrDaemonMessage = if ($localAsrDaemonExists) { 'local ASR daemon executable is available' } else { 'local ASR daemon executable is missing' }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'local_asr_daemon_exe' `
            -Status $localAsrDaemonStatus `
            -Path $resolvedLocalAsrDaemonExe `
            -Message $localAsrDaemonMessage `
            -RemediationHint 'Use a packaged Talk release or pass -LocalAsrDaemonExe .\.internal\talk-local-asr-sherpa.exe.')) | Out-Null

        $resolvedModelRoot = if ([string]::IsNullOrWhiteSpace($ModelRoot)) {
            Resolve-TalkAsrDefaultModelRoot
        } else {
            Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $ModelRoot
        }
        $modelRootExists = Test-Path -LiteralPath $resolvedModelRoot -PathType Container
        $modelRootStatus = if ($modelRootExists) { 'ready' } else { 'missing' }
        $modelRootMessage = if ($modelRootExists) { 'model root exists' } else { 'model root is missing' }
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'model_root' `
            -Status $modelRootStatus `
            -Path $resolvedModelRoot `
            -Message $modelRootMessage `
            -RemediationHint 'Install the required sherpa-onnx models into this root before running the benchmark.')) | Out-Null

        foreach ($id in @($Plan.ModelId)) {
            $modelDir = Join-Path $resolvedModelRoot ([string]$id)
            $modelInstallCommand = New-TalkAsrRealMicDefaultWorkflowSherpaInstallCommand `
                -ModelId ([string]$id) `
                -DestinationRoot $resolvedModelRoot
            if (-not (Test-Path -LiteralPath $modelDir -PathType Container)) {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name "model:$id" `
                    -Status 'missing' `
                    -Path $modelDir `
                    -Message 'installed sherpa model directory is missing' `
                    -RemediationCommand $modelInstallCommand `
                    -RemediationHint 'Install this model into the selected sherpa-onnx model root.')) | Out-Null
                continue
            }

            try {
                $validation = Test-TalkSherpaModelInstall -ModelId ([string]$id) -ModelDir $modelDir
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name "model:$id" `
                    -Status 'ready' `
                    -Path ([string]$validation.ModelDir) `
                    -Message ("validated {0} {1}" -f $validation.ModelFamily, $validation.ModelName))) | Out-Null
            }
            catch {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name "model:$id" `
                    -Status 'failed' `
                    -Path $modelDir `
                    -Message $_.Exception.Message `
                    -RemediationCommand $modelInstallCommand `
                    -RemediationHint 'Reinstall this model because the existing model directory did not validate.')) | Out-Null
            }
        }

        $hasCloudBaseline = (-not [string]::IsNullOrWhiteSpace($CloudOpenAiCompatibleEndpoint)) -and
            (-not [string]::IsNullOrWhiteSpace($CloudOpenAiCompatibleModel))
        if ($AllowMissingCloudBaseline -and -not $hasCloudBaseline) {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'cloud_baseline_api_key' `
                -Status 'skipped' `
                -Path $CloudOpenAiCompatibleApiKeyEnv `
                -Message 'cloud baseline is optional for this diagnostic run')) | Out-Null
        } elseif ($hasCloudBaseline) {
            $apiKeySource = Get-TalkAsrRealMicDefaultWorkflowCloudApiKeySource `
                -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv `
                -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint `
                -ConfigPath ([string]$Plan.ConfigPath)
            $status = if ($apiKeySource.Available) { 'ready' } else { 'missing' }
            $message = if ($status -eq 'ready') {
                if ($apiKeySource.Source -eq 'desktop_config') {
                    'cloud baseline API key is available from desktop config provider api_key'
                } elseif ($apiKeySource.Source -eq 'legacy_json') {
                    'cloud baseline API key is available from the standard DashScope credential file'
                } else {
                    'cloud baseline API key environment variable is set'
                }
            } else {
                'cloud baseline API key environment variable is missing or blank'
            }
            $apiKeyRemediationCommand = $null
            if ($status -eq 'missing') {
                $apiKeyRemediationCommand = New-TalkAsrRealMicDefaultWorkflowApiKeyCommand `
                    -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv
            }
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'cloud_baseline_api_key' `
                -Status $status `
                -Path $CloudOpenAiCompatibleApiKeyEnv `
                -Message $message `
                -RemediationCommand $apiKeyRemediationCommand `
                -RemediationHint 'Set the cloud baseline API key environment variable before running production default selection.')) | Out-Null
        } else {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'cloud_baseline_api_key' `
                -Status 'missing' `
                -Path $CloudOpenAiCompatibleApiKeyEnv `
                -Message 'production default selection requires a cloud baseline' `
                -RemediationCommand (New-TalkAsrRealMicDefaultWorkflowApiKeyCommand -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv) `
                -RemediationHint 'Provide a cloud OpenAI-compatible endpoint/model/key, or use a diagnostic override only when not selecting the production default.')) | Out-Null
        }

        if ($SkipApply) {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'desktop_config' `
                -Status 'skipped' `
                -Path ([string]$Plan.ConfigPath) `
                -Message 'config apply is skipped')) | Out-Null
        } else {
            $configPath = [string]$Plan.ConfigPath
            $configExists = Test-Path -LiteralPath $configPath -PathType Leaf
            $configStatus = if ($configExists) { 'ready' } else { 'missing' }
            $configMessage = if ($configExists) { 'desktop config is available for update' } else { 'desktop config must exist before applying the selected model' }
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'desktop_config' `
                -Status $configStatus `
                -Path $configPath `
                -Message $configMessage `
                -RemediationHint 'Use the packaged talk-desktop.toml or pass -ConfigPath to an existing desktop config file.')) | Out-Null
        }
    }

    if ($ProbeAudio) {
        if ($SkipRecording) {
            $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'microphone_signal' `
                -Status 'skipped' `
                -Path ([string]$Plan.ConfigPath) `
                -Message 'recording is skipped')) | Out-Null
        } else {
            $talkProbeCheck = @($checks.ToArray() | Where-Object { $_.Name -eq 'talk_probe_exe' } | Select-Object -First 1)
            $probeConfigPath = [string]$Plan.ConfigPath
            $probeConfigReady = $false
            $probeConfigMessage = 'microphone probe requires ready talk.exe and desktop config checks'

            if ($RecordOnly) {
                $recorderPlan = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $Plan -Name 'RecorderPlan'
                $probeConfigPath = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'ConfigPath')
                if ([string]::IsNullOrWhiteSpace($probeConfigPath)) {
                    $probeConfigMessage = 'record-only microphone probe requires a recorder config path from the recording plan'
                } else {
                    try {
                        Write-TalkAsrCorpusRecorderConfig `
                            -Path $probeConfigPath `
                            -CaptureTempDir ([string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'CaptureTempDir')) `
                            -LogsDir ([string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'LogsDir')) `
                            -InputDevice ([string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'InputDevice')) `
                            -MaxRecordingSeconds ([int](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $recorderPlan -Name 'MaxRecordingSeconds'))
                        $probeConfigReady = Test-Path -LiteralPath $probeConfigPath -PathType Leaf
                        if (-not $probeConfigReady) {
                            $probeConfigMessage = 'record-only microphone probe recorder config was not created'
                        }
                    }
                    catch {
                        $probeConfigMessage = $_.Exception.Message
                    }
                }
            } else {
                $desktopConfigCheck = @($checks.ToArray() | Where-Object { $_.Name -eq 'desktop_config' } | Select-Object -First 1)
                $probeConfigReady = ($desktopConfigCheck.Count -gt 0 -and $desktopConfigCheck[0].Status -eq 'ready')
            }

            if ($talkProbeCheck.Count -gt 0 -and $talkProbeCheck[0].Status -eq 'ready' -and $probeConfigReady) {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowAudioProbeCheck `
                    -TalkExe ([string]$Plan.RecorderPlan.TalkExe) `
                    -ConfigPath $probeConfigPath `
                    -Seconds $AudioProbeSeconds `
                    -AudioProbeInvoker $AudioProbeInvoker)) | Out-Null
            } else {
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'microphone_signal' `
                    -Status 'failed' `
                    -Path $probeConfigPath `
                    -Message $probeConfigMessage `
                    -RemediationHint 'Fix the talk_probe_exe check and the probe config path before probing the microphone.')) | Out-Null
            }
        }
    }

    $blockingChecks = @($checks.ToArray() | Where-Object { $_.Status -in @('missing', 'failed') })
    $remediationCommands = @($checks.ToArray() |
        ForEach-Object { $_.RemediationCommand } |
        Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) } |
        Select-Object -Unique)
    [pscustomobject]@{
        WorkflowKind = 'talk-asr-real-mic-default-model-workflow-preflight'
        Ready = ($blockingChecks.Count -eq 0)
        BlockingCheckCount = $blockingChecks.Count
        RemediationCommands = @($remediationCommands)
        Plan = $Plan
        Checks = $checks.ToArray()
    }
}

function New-TalkAsrRealMicDefaultModelWorkflowPlan {
    [CmdletBinding()]
    param(
        [string]$PromptManifest,
        [string]$CorpusRoot,
        [string]$TalkExe,
        [string]$InputDevice,
        [int]$DefaultCaptureSeconds = 3,
        [int]$CountdownSeconds = 3,
        [string[]]$ForceRecordSampleId,
        [switch]$ResumeExistingCorpus,
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
        [string]$ModelRoot,
        [string]$ReportsRoot,
        [string]$AsrBenchExe,
        [string]$LocalAsrDaemonExe,
        [string]$CloudOpenAiCompatibleEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$CloudOpenAiCompatibleModel = 'qwen3-asr-flash',
        [string]$CloudOpenAiCompatibleTransport = 'chat_completions_audio_input',
        [string]$CloudOpenAiCompatibleApiKeyEnv = 'TALK_PROVIDER_API_KEY',
        [string]$Bind = '127.0.0.1:53171',
        [int]$ChunkMs = 80,
        [int]$ConnectTimeoutMs = 1000,
        [int]$ReadyTimeoutMs = 1000,
        [int]$PartialIdleTimeoutMs = 10,
        [int]$FinalTimeoutMs = 7000,
        [string]$SelectionJson,
        [string]$ConfigPath,
        [switch]$SkipApply
    )

    $resolvedCorpusRoot = Resolve-TalkAsrRealMicDefaultWorkflowCorpusRoot -CorpusRoot $CorpusRoot
    $resolvedReportsRoot = Resolve-TalkAsrRealMicDefaultWorkflowReportsRoot `
        -ReportsRoot $ReportsRoot `
        -CorpusRoot $resolvedCorpusRoot
    $resolvedPromptManifest = Resolve-TalkAsrRealMicDefaultWorkflowPromptManifest -PromptManifest $PromptManifest

    $recorderPlan = $null
    if (-not $SkipRecording) {
        $recorderPlanArguments = @{
            PromptManifest = $resolvedPromptManifest
            OutputRoot = $resolvedCorpusRoot
            TalkExe = $TalkExe
            InputDevice = $InputDevice
            DefaultCaptureSeconds = $DefaultCaptureSeconds
            CountdownSeconds = $CountdownSeconds
            ForceRecordSampleId = $ForceRecordSampleId
            PlanOnly = $true
        }
        if ($ResumeExistingCorpus) {
            $recorderPlanArguments.ResumeExisting = $true
        }
        $recorderPlan = Invoke-TalkAsrCorpusRecorder @recorderPlanArguments
    }

    $corpusManifestPath = if ($null -ne $recorderPlan) {
        [string]$recorderPlan.CorpusManifestPath
    } else {
        [System.IO.Path]::GetFullPath((Join-Path $resolvedCorpusRoot 'corpus.json'))
    }
    $selectionJsonPath = if ([string]::IsNullOrWhiteSpace($SelectionJson)) {
        [System.IO.Path]::GetFullPath((Join-Path $resolvedReportsRoot 'selected-default-asr-model.json'))
    } else {
        Resolve-TalkAsrRealMicDefaultWorkflowPath -Path $SelectionJson
    }
    $configPathValue = if ($SkipApply) {
        Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath -Path $ConfigPath
    } else {
        Resolve-TalkAsrDefaultWorkflowConfigPath -ConfigPath $ConfigPath
    }

    [pscustomobject]@{
        WorkflowKind = 'talk-asr-real-mic-default-model-workflow-plan'
        RecordOnly = [bool]$RecordOnly
        ResumeExistingCorpus = [bool]$ResumeExistingCorpus
        ForceRecordSampleId = @($ForceRecordSampleId)
        PromptManifest = $resolvedPromptManifest
        CorpusRoot = $resolvedCorpusRoot
        CorpusManifest = $corpusManifestPath
        ReportsRoot = $resolvedReportsRoot
        SelectionJson = $selectionJsonPath
        ConfigPath = $configPathValue
        TalkExe = Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath -Path $TalkExe
        InputDevice = if ([string]::IsNullOrWhiteSpace($InputDevice)) { $null } else { [string]$InputDevice }
        RecorderPlan = $recorderPlan
        ModelId = @($ModelId)
        ModelRoot = Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath -Path $ModelRoot
        AsrBenchExe = Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath -Path $AsrBenchExe
        LocalAsrDaemonExe = Resolve-TalkAsrRealMicDefaultWorkflowOptionalPath -Path $LocalAsrDaemonExe
        CloudOpenAiCompatibleEndpoint = $CloudOpenAiCompatibleEndpoint
        CloudOpenAiCompatibleModel = $CloudOpenAiCompatibleModel
        CloudOpenAiCompatibleTransport = $CloudOpenAiCompatibleTransport
        CloudOpenAiCompatibleApiKeyEnv = $CloudOpenAiCompatibleApiKeyEnv
        Bind = $Bind
        ChunkMs = $ChunkMs
        ConnectTimeoutMs = $ConnectTimeoutMs
        ReadyTimeoutMs = $ReadyTimeoutMs
        PartialIdleTimeoutMs = $PartialIdleTimeoutMs
        DefaultCaptureSeconds = $DefaultCaptureSeconds
        FinalTimeoutMs = $FinalTimeoutMs
        WillRecord = (-not $SkipRecording.IsPresent)
        WillBenchmark = (-not $RecordOnly.IsPresent)
        WillApply = ((-not $RecordOnly.IsPresent) -and (-not $SkipApply.IsPresent))
    }
}

function Invoke-TalkAsrRealMicDefaultModelWorkflow {
    [CmdletBinding()]
    param(
        [string]$PromptManifest,
        [string]$CorpusRoot,
        [string]$TalkExe,
        [string]$InputDevice,
        [int]$DefaultCaptureSeconds = 3,
        [int]$CountdownSeconds = 3,
        [switch]$AllowSilent,
        [string[]]$ForceRecordSampleId,
        [switch]$ResumeExistingCorpus,
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
        [string]$ModelRoot,
        [string]$ReportsRoot,
        [string]$AsrBenchExe,
        [string]$LocalAsrDaemonExe,
        [string]$CloudOpenAiCompatibleEndpoint = 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions',
        [string]$CloudOpenAiCompatibleModel = 'qwen3-asr-flash',
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
        [string]$ConfigPath,
        [int]$MinSamples = 3,
        [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en'),
        [switch]$AllowMissingCloudBaseline,
        [switch]$AllowSyntheticSampleIds,
        [switch]$SkipApply,
        [switch]$NoBackup,
        [switch]$PreflightOnly,
        [switch]$ProbeAudio,
        [ValidateRange(1, 60)][int]$AudioProbeSeconds = 2,
        [switch]$PlanOnly,
        [switch]$PassThru,
        [scriptblock]$ProbeInvoker,
        [scriptblock]$AudioProbeInvoker,
        [scriptblock]$ReadinessInvoker
    )

    if ($RecordOnly -and $SkipRecording) {
        throw 'RecordOnly cannot be combined with SkipRecording because record-only mode must capture a corpus; use SkipRecording without RecordOnly to reuse an existing corpus.'
    }

    $plan = New-TalkAsrRealMicDefaultModelWorkflowPlan `
        -PromptManifest $PromptManifest `
        -CorpusRoot $CorpusRoot `
        -TalkExe $TalkExe `
        -InputDevice $InputDevice `
        -DefaultCaptureSeconds $DefaultCaptureSeconds `
        -CountdownSeconds $CountdownSeconds `
        -ForceRecordSampleId $ForceRecordSampleId `
        -ResumeExistingCorpus:$ResumeExistingCorpus `
        -SkipRecording:$SkipRecording `
        -RecordOnly:$RecordOnly `
        -ModelId $ModelId `
        -ModelRoot $ModelRoot `
        -ReportsRoot $ReportsRoot `
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
        -ConfigPath $ConfigPath `
        -SkipApply:$SkipApply

    if ($PreflightOnly) {
        return Test-TalkAsrRealMicDefaultModelWorkflowPreflight `
            -Plan $plan `
            -ModelRoot $ModelRoot `
            -AsrBenchExe $AsrBenchExe `
            -LocalAsrDaemonExe $LocalAsrDaemonExe `
            -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint `
            -CloudOpenAiCompatibleModel $CloudOpenAiCompatibleModel `
            -CloudOpenAiCompatibleApiKeyEnv $CloudOpenAiCompatibleApiKeyEnv `
            -ProbeAudio:$ProbeAudio `
            -AudioProbeSeconds $AudioProbeSeconds `
            -AudioProbeInvoker $AudioProbeInvoker `
            -ReadinessInvoker $ReadinessInvoker `
            -ResumeExistingCorpus:$ResumeExistingCorpus `
            -SkipRecording:$SkipRecording `
            -RecordOnly:$RecordOnly `
            -SkipApply:$SkipApply `
            -AllowMissingCloudBaseline:$AllowMissingCloudBaseline
    }

    if ($PlanOnly) {
        return $plan
    }

    $recorderResult = $null
    $corpusManifestPath = [string]$plan.CorpusManifest
    if (-not $SkipRecording) {
        $inputDeviceSelectionCheck = New-TalkAsrRealMicDefaultWorkflowInputDeviceSelectionCheck `
            -Plan $plan `
            -ReadinessInvoker $ReadinessInvoker
        if ($inputDeviceSelectionCheck.Status -eq 'failed') {
            throw (Format-TalkAsrRealMicDefaultWorkflowBlockingCheckFailure -Check $inputDeviceSelectionCheck)
        }

        $recorderArguments = @{
            PromptManifest = [string]$plan.PromptManifest
            OutputRoot = [string]$plan.CorpusRoot
            TalkExe = $TalkExe
            InputDevice = $InputDevice
            DefaultCaptureSeconds = $DefaultCaptureSeconds
            CountdownSeconds = $CountdownSeconds
            ForceRecordSampleId = $ForceRecordSampleId
            AllowSilent = $AllowSilent
            PassThru = $true
        }
        if ($ResumeExistingCorpus) {
            $recorderArguments.ResumeExisting = $true
        }
        if ($null -ne $ProbeInvoker) {
            $recorderArguments.ProbeInvoker = $ProbeInvoker
        }

        $recorderResult = Invoke-TalkAsrCorpusRecorder @recorderArguments
        $corpusManifestPath = [string]$recorderResult.CorpusManifestPath
    }

    if ($RecordOnly) {
        $recordOnlyStatusJson = [System.IO.Path]::GetFullPath((Join-Path ([string]$plan.CorpusRoot) 'record-only-status.json'))
        $recordOnlyStatus = New-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus `
            -Plan $plan `
            -RecorderResult $recorderResult `
            -CorpusManifestPath $corpusManifestPath
        Write-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus `
            -Path $recordOnlyStatusJson `
            -Status $recordOnlyStatus

        $result = [pscustomobject]@{
            WorkflowKind = 'talk-asr-real-mic-default-model-workflow-result'
            RecordOnly = $true
            Plan = $plan
            RecorderResult = $recorderResult
            CorpusManifest = $corpusManifestPath
            RecordOnlyStatusJson = $recordOnlyStatusJson
            RecordOnlyStatus = [pscustomobject]$recordOnlyStatus
            DefaultModelWorkflowResult = $null
            SelectionJson = $null
            ConfigPath = [string]$plan.ConfigPath
            Applied = $false
            FaithfulCorrectionReplayModelId = $null
            FaithfulCorrectionReplayResult = $null
            FaithfulCorrectionReplaySkippedReason = 'record-only mode skips faithful correction replay'
            ProviderCorrectionReplayModelId = $null
            ProviderCorrectionReplayLiveResult = $null
            ProviderCorrectionReplayDeterministicResult = $null
            ProviderCorrectionReplaySkippedReason = 'record-only mode skips provider correction replay'
        }

        if ($PassThru) {
            return $result
        }

        return $result
    }

    if ($SkipRecording) {
        $recordOnlyStatusCheck = Test-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus -Plan $plan
        if ($recordOnlyStatusCheck.Status -eq 'failed') {
            throw ("Record-only status is not ready for SkipRecording: {0}" -f $recordOnlyStatusCheck.Message)
        }

        $promptSamples = Read-TalkAsrRealMicDefaultWorkflowPromptSamples -PromptManifest ([string]$plan.PromptManifest)
        $corpusSamples = Read-TalkAsrRealMicDefaultWorkflowCorpusSamples -CorpusManifest $corpusManifestPath
        $corpusPromptAlignmentCheck = New-TalkAsrRealMicDefaultWorkflowCorpusPromptAlignmentCheck `
            -Plan $plan `
            -PromptSamples $promptSamples `
            -PromptSampleIds (Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples $promptSamples) `
            -CorpusSampleIds (Get-TalkAsrRealMicDefaultWorkflowSampleIds -Samples $corpusSamples) `
            -ResumeExistingCorpus
        if ($corpusPromptAlignmentCheck.Status -eq 'failed') {
            throw (Format-TalkAsrRealMicDefaultWorkflowBlockingCheckFailure -Check $corpusPromptAlignmentCheck)
        }

        $corpusSemanticValidityCheck = New-TalkAsrRealMicDefaultWorkflowCorpusSemanticValidityCheck `
            -Plan $plan `
            -CorpusSamples $corpusSamples `
            -ResumeExistingCorpus
        if ($corpusSemanticValidityCheck.Status -eq 'failed') {
            throw (Format-TalkAsrRealMicDefaultWorkflowBlockingCheckFailure -Check $corpusSemanticValidityCheck)
        }
    }

    $defaultWorkflowResult = Invoke-TalkAsrRealMicDefaultWorkflowWithCloudApiKeySource `
        -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv `
        -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint `
        -ConfigPath ([string]$plan.ConfigPath) `
        -ScriptBlock {
            Invoke-TalkAsrDefaultModelWorkflow `
                -CorpusManifest $corpusManifestPath `
                -ModelId $ModelId `
                -ModelRoot $ModelRoot `
                -OutputRoot ([string]$plan.ReportsRoot) `
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
                -SelectionJson $SelectionJson `
                -ConfigPath $ConfigPath `
                -MinSamples $MinSamples `
                -RequiredLocalModelId $RequiredLocalModelId `
                -AllowMissingCloudBaseline:$AllowMissingCloudBaseline `
                -AllowSyntheticSampleIds:$AllowSyntheticSampleIds `
                -SkipApply `
                -NoBackup:$NoBackup `
                -PassThru
        }

    $providerCorrectionReplayModelId = Get-TalkAsrRealMicDefaultWorkflowSelectedModelId `
        -DefaultModelWorkflowResult $defaultWorkflowResult
    $faithfulCorrectionReplayModelId = $providerCorrectionReplayModelId
    $faithfulCorrectionReplayTalkExe = Resolve-TalkAsrRealMicDefaultWorkflowReplayTalkExe `
        -TalkExe $TalkExe `
        -Plan $plan `
        -RequiredCommandName 'validate-faithful-output'
    $faithfulCorrectionReplaySkippedReason = Get-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplaySkipReason `
        -SelectedModelId $faithfulCorrectionReplayModelId `
        -TalkExe $faithfulCorrectionReplayTalkExe
    $faithfulCorrectionReplayResult = $null
    if ([string]::IsNullOrWhiteSpace($faithfulCorrectionReplaySkippedReason)) {
        $faithfulCorrectionReplayJson = Resolve-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplayJson `
            -CorpusManifest $corpusManifestPath `
            -ModelId $faithfulCorrectionReplayModelId
        $faithfulCorrectionReplayResult = Invoke-TalkFaithfulCorrectionReplay `
            -CorpusManifest $corpusManifestPath `
            -ReportsRoot ([string]$plan.ReportsRoot) `
            -ModelId $faithfulCorrectionReplayModelId `
            -TalkExe $faithfulCorrectionReplayTalkExe `
            -OutputJson $faithfulCorrectionReplayJson `
            -PassThru
    }

    if (-not $SkipApply -and -not [string]::IsNullOrWhiteSpace($faithfulCorrectionReplaySkippedReason)) {
        throw $faithfulCorrectionReplaySkippedReason
    }
    if ($null -ne $faithfulCorrectionReplayResult) {
        $faithfulReplayAllAccepted = [bool](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $faithfulCorrectionReplayResult -Name 'AllAccepted')
        if (-not $faithfulReplayAllAccepted) {
            throw (New-TalkAsrRealMicDefaultWorkflowFaithfulCorrectionReplayFailureMessage `
                    -Plan $plan `
                    -FaithfulCorrectionReplayResult $faithfulCorrectionReplayResult)
        }
    }

    $providerCorrectionReplayTalkExe = Resolve-TalkAsrRealMicDefaultWorkflowReplayTalkExe `
        -TalkExe $TalkExe `
        -Plan $plan `
        -RequiredCommandName 'process-transcript'
    $providerCorrectionReplayConfigPath = [string]$defaultWorkflowResult.ConfigPath
    if ([string]::IsNullOrWhiteSpace($providerCorrectionReplayConfigPath)) {
        $providerCorrectionReplayConfigPath = [string]$plan.ConfigPath
    }
    $providerCorrectionReplaySkippedReason = Get-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplaySkipReason `
        -SelectedModelId $providerCorrectionReplayModelId `
        -TalkExe $providerCorrectionReplayTalkExe `
        -ConfigPath $providerCorrectionReplayConfigPath
    $providerCorrectionReplayLiveResult = $null
    $providerCorrectionReplayDeterministicResult = $null
    if ([string]::IsNullOrWhiteSpace($providerCorrectionReplaySkippedReason)) {
        $providerCorrectionReplayLiveJson = Resolve-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplayLiveJson `
            -CorpusManifest $corpusManifestPath `
            -ModelId $providerCorrectionReplayModelId
        $providerCorrectionReplayDeterministicJson = Resolve-TalkAsrRealMicDefaultWorkflowProviderCorrectionReplayDeterministicJson `
            -CorpusManifest $corpusManifestPath `
            -ModelId $providerCorrectionReplayModelId

        $providerCorrectionReplayLiveResult = Invoke-TalkAsrRealMicDefaultWorkflowWithCloudApiKeySource `
            -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv `
            -CloudOpenAiCompatibleEndpoint $CloudOpenAiCompatibleEndpoint `
            -ConfigPath $providerCorrectionReplayConfigPath `
            -ScriptBlock {
                Invoke-TalkProviderCorrectionReplay `
                    -CorpusManifest $corpusManifestPath `
                    -ReportsRoot ([string]$plan.ReportsRoot) `
                    -ModelId $providerCorrectionReplayModelId `
                    -TalkExe $providerCorrectionReplayTalkExe `
                    -ConfigPath $providerCorrectionReplayConfigPath `
                    -OutputJson $providerCorrectionReplayLiveJson `
                    -PassThru
            }
        $providerCorrectionReplayDeterministicResult = Invoke-TalkProviderCorrectionReplay `
            -CorpusManifest $corpusManifestPath `
            -ReportsRoot ([string]$plan.ReportsRoot) `
            -ModelId $providerCorrectionReplayModelId `
            -TalkExe $providerCorrectionReplayTalkExe `
            -ConfigPath $providerCorrectionReplayConfigPath `
            -OutputJson $providerCorrectionReplayDeterministicJson `
            -UseExistingReplayAsFixture `
            -PassThru
    }

    $applyResult = $null
    $resolvedConfigPath = [string]$plan.ConfigPath
    if ($SkipApply) {
        $resolvedConfigPath = [string]$defaultWorkflowResult.ConfigPath
        if ([string]::IsNullOrWhiteSpace($resolvedConfigPath)) {
            $resolvedConfigPath = [string]$plan.ConfigPath
        }
    } else {
        $defaultWorkflowBenchmarkResult = Get-TalkAsrRealMicDefaultWorkflowOptionalProperty `
            -Object $defaultWorkflowResult `
            -Name 'BenchmarkResult'
        $applyModelRoot = Resolve-TalkAsrDefaultWorkflowApplyModelRoot `
            -ModelRoot $ModelRoot `
            -BenchmarkResult $defaultWorkflowBenchmarkResult
        $applyResult = Set-TalkDefaultAsrModel `
            -SelectionJson ([string]$defaultWorkflowResult.SelectionJson) `
            -ConfigPath ([string]$plan.ConfigPath) `
            -ModelRoot $applyModelRoot `
            -NoBackup:$NoBackup `
            -PassThru
        $resolvedConfigPath = [string](Get-TalkAsrRealMicDefaultWorkflowOptionalProperty -Object $applyResult -Name 'ConfigPath')
        if ([string]::IsNullOrWhiteSpace($resolvedConfigPath)) {
            $resolvedConfigPath = [string]$plan.ConfigPath
        }
    }

    $result = [pscustomobject]@{
        WorkflowKind = 'talk-asr-real-mic-default-model-workflow-result'
        RecordOnly = $false
        Plan = $plan
        RecorderResult = $recorderResult
        CorpusManifest = $corpusManifestPath
        RecordOnlyStatusJson = $null
        RecordOnlyStatus = $null
        DefaultModelWorkflowResult = $defaultWorkflowResult
        SelectionJson = [string]$defaultWorkflowResult.SelectionJson
        ConfigPath = $resolvedConfigPath
        Applied = (-not $SkipApply.IsPresent)
        ApplyResult = $applyResult
        FaithfulCorrectionReplayModelId = $faithfulCorrectionReplayModelId
        FaithfulCorrectionReplayResult = $faithfulCorrectionReplayResult
        FaithfulCorrectionReplaySkippedReason = $faithfulCorrectionReplaySkippedReason
        ProviderCorrectionReplayModelId = $providerCorrectionReplayModelId
        ProviderCorrectionReplayLiveResult = $providerCorrectionReplayLiveResult
        ProviderCorrectionReplayDeterministicResult = $providerCorrectionReplayDeterministicResult
        ProviderCorrectionReplaySkippedReason = $providerCorrectionReplaySkippedReason
    }

    if ($PassThru) {
        return $result
    }

    $result
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkAsrRealMicDefaultModelWorkflow `
        -PromptManifest $realMicWorkflowEntryPromptManifest `
        -CorpusRoot $realMicWorkflowEntryCorpusRoot `
        -TalkExe $realMicWorkflowEntryTalkExe `
        -InputDevice $realMicWorkflowEntryInputDevice `
        -DefaultCaptureSeconds $realMicWorkflowEntryDefaultCaptureSeconds `
        -CountdownSeconds $realMicWorkflowEntryCountdownSeconds `
        -AllowSilent:$realMicWorkflowEntryAllowSilent `
        -ForceRecordSampleId $realMicWorkflowEntryForceRecordSampleId `
        -ResumeExistingCorpus:$realMicWorkflowEntryResumeExistingCorpus `
        -SkipRecording:$realMicWorkflowEntrySkipRecording `
        -RecordOnly:$realMicWorkflowEntryRecordOnly `
        -ModelId $realMicWorkflowEntryModelId `
        -ModelRoot $realMicWorkflowEntryModelRoot `
        -ReportsRoot $realMicWorkflowEntryReportsRoot `
        -AsrBenchExe $realMicWorkflowEntryAsrBenchExe `
        -LocalAsrDaemonExe $realMicWorkflowEntryLocalAsrDaemonExe `
        -CloudOpenAiCompatibleEndpoint $realMicWorkflowEntryCloudOpenAiCompatibleEndpoint `
        -CloudOpenAiCompatibleModel $realMicWorkflowEntryCloudOpenAiCompatibleModel `
        -CloudOpenAiCompatibleTransport $realMicWorkflowEntryCloudOpenAiCompatibleTransport `
        -CloudOpenAiCompatibleApiKeyEnv $realMicWorkflowEntryCloudOpenAiCompatibleApiKeyEnv `
        -Bind $realMicWorkflowEntryBind `
        -ChunkMs $realMicWorkflowEntryChunkMs `
        -ConnectTimeoutMs $realMicWorkflowEntryConnectTimeoutMs `
        -ReadyTimeoutMs $realMicWorkflowEntryReadyTimeoutMs `
        -PartialIdleTimeoutMs $realMicWorkflowEntryPartialIdleTimeoutMs `
        -FinalTimeoutMs $realMicWorkflowEntryFinalTimeoutMs `
        -StartupTimeoutSeconds $realMicWorkflowEntryStartupTimeoutSeconds `
        -SelectionJson $realMicWorkflowEntrySelectionJson `
        -ConfigPath $realMicWorkflowEntryConfigPath `
        -MinSamples $realMicWorkflowEntryMinSamples `
        -RequiredLocalModelId $realMicWorkflowEntryRequiredLocalModelId `
        -AllowMissingCloudBaseline:$realMicWorkflowEntryAllowMissingCloudBaseline `
        -AllowSyntheticSampleIds:$realMicWorkflowEntryAllowSyntheticSampleIds `
        -SkipApply:$realMicWorkflowEntrySkipApply `
        -NoBackup:$realMicWorkflowEntryNoBackup `
        -PreflightOnly:$realMicWorkflowEntryPreflightOnly `
        -ProbeAudio:$realMicWorkflowEntryProbeAudio `
        -AudioProbeSeconds $realMicWorkflowEntryAudioProbeSeconds `
        -PlanOnly:$realMicWorkflowEntryPlanOnly `
        -PassThru:$realMicWorkflowEntryPassThru `
        -ProbeInvoker $realMicWorkflowEntryProbeInvoker `
        -AudioProbeInvoker $realMicWorkflowEntryAudioProbeInvoker `
        -ReadinessInvoker $realMicWorkflowEntryReadinessInvoker
}
