[CmdletBinding()]
param(
    [string]$PromptManifest,
    [string]$CorpusRoot,
    [string]$TalkExe,
    [string]$InputDevice,
    [int]$DefaultCaptureSeconds = 3,
    [int]$CountdownSeconds = 3,
    [switch]$AllowSilent,
    [switch]$SkipRecording,
    [switch]$RecordOnly,
    [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
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
    [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
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
    [scriptblock]$AudioProbeInvoker
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

foreach ($dependencyName in @(
    'Invoke-TalkAsrCorpusRecorder.ps1',
    'Invoke-TalkAsrDefaultModelWorkflow.ps1'
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
        [string]$RemediationHint
    )

    [pscustomobject]@{
        Name = $Name
        Status = $Status
        Path = $Path
        Message = $Message
        RemediationCommand = $RemediationCommand
        RemediationHint = $RemediationHint
    }
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
    param([Parameter(Mandatory = $true)]$Plan)

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

    New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
        -Name 'record_only_status' `
        -Status 'ready' `
        -Path $statusJson `
        -Message ("record-only status ready: sampleCount={0}; audioFileCount={1}" -f $sampleCount, $audioFileCount)
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

function Get-TalkAsrRealMicDefaultWorkflowCloudApiKeySource {
    param(
        [Parameter(Mandatory = $true)][string]$EnvironmentVariableName,
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

    [pscustomobject]@{
        Available = $false
        Source = 'missing'
        Value = $null
    }
}

function Invoke-TalkAsrRealMicDefaultWorkflowWithCloudApiKeySource {
    param(
        [Parameter(Mandatory = $true)][string]$EnvironmentVariableName,
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

        if ($silent) {
            return New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                -Name 'microphone_signal' `
                -Status 'failed' `
                -Path $artifactPath `
                -Message ("microphone probe recorded silence: device={0}; peak={1}; rms={2}" -f $deviceName, $peak, $rms) `
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
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [switch]$SkipApply,
        [switch]$AllowMissingCloudBaseline
    )

    $checks = New-Object System.Collections.Generic.List[object]

    if ($SkipRecording) {
        $corpusManifest = [string]$Plan.CorpusManifest
        if (Test-Path -LiteralPath $corpusManifest -PathType Leaf) {
            try {
                $samples = @(Read-TalkAsrCorpusManifest -CorpusManifest $corpusManifest)
                $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
                    -Name 'corpus_manifest' `
                    -Status 'ready' `
                    -Path $corpusManifest `
                    -Message ("existing corpus manifest has {0} sample(s)" -f $samples.Count))) | Out-Null
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
        $checks.Add((Test-TalkAsrRealMicDefaultWorkflowRecordOnlyStatus -Plan $Plan)) | Out-Null
        $checks.Add((New-TalkAsrRealMicDefaultWorkflowPreflightCheck `
            -Name 'prompt_manifest' `
            -Status 'skipped' `
            -Path ([string]$Plan.PromptManifest) `
            -Message 'recording is skipped')) | Out-Null
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
                -ConfigPath ([string]$Plan.ConfigPath)
            $status = if ($apiKeySource.Available) { 'ready' } else { 'missing' }
            $message = if ($status -eq 'ready') {
                if ($apiKeySource.Source -eq 'desktop_config') {
                    'cloud baseline API key is available from desktop config provider api_key'
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
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
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
        $recorderPlan = Invoke-TalkAsrCorpusRecorder `
            -PromptManifest $resolvedPromptManifest `
            -OutputRoot $resolvedCorpusRoot `
            -TalkExe $TalkExe `
            -InputDevice $InputDevice `
            -DefaultCaptureSeconds $DefaultCaptureSeconds `
            -CountdownSeconds $CountdownSeconds `
            -PlanOnly
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
        PromptManifest = $resolvedPromptManifest
        CorpusRoot = $resolvedCorpusRoot
        CorpusManifest = $corpusManifestPath
        ReportsRoot = $resolvedReportsRoot
        SelectionJson = $selectionJsonPath
        ConfigPath = $configPathValue
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
        [switch]$SkipRecording,
        [switch]$RecordOnly,
        [string[]]$ModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
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
        [string[]]$RequiredLocalModelId = @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en'),
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
        [scriptblock]$AudioProbeInvoker
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
        $recorderArguments = @{
            PromptManifest = [string]$plan.PromptManifest
            OutputRoot = [string]$plan.CorpusRoot
            TalkExe = $TalkExe
            InputDevice = $InputDevice
            DefaultCaptureSeconds = $DefaultCaptureSeconds
            CountdownSeconds = $CountdownSeconds
            AllowSilent = $AllowSilent
            PassThru = $true
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
    }

    $defaultWorkflowResult = Invoke-TalkAsrRealMicDefaultWorkflowWithCloudApiKeySource `
        -EnvironmentVariableName $CloudOpenAiCompatibleApiKeyEnv `
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
                -SkipApply:$SkipApply `
                -NoBackup:$NoBackup `
                -PassThru
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
        ConfigPath = [string]$defaultWorkflowResult.ConfigPath
        Applied = [bool]$defaultWorkflowResult.Applied
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
        -AudioProbeInvoker $realMicWorkflowEntryAudioProbeInvoker
}
