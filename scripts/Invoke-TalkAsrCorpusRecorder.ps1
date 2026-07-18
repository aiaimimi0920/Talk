[CmdletBinding()]
param(
    [string]$PromptManifest,
    [string]$OutputRoot,
    [string]$TalkExe,
    [string]$InputDevice,
    [int]$DefaultCaptureSeconds = 3,
    [int]$CountdownSeconds = 3,
    [switch]$AllowSilent,
    [switch]$PlanOnly,
    [switch]$PassThru,
    [scriptblock]$ProbeInvoker
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$entryPromptManifest = $PromptManifest
$entryOutputRoot = $OutputRoot
$entryTalkExe = $TalkExe
$entryInputDevice = $InputDevice
$entryDefaultCaptureSeconds = $DefaultCaptureSeconds
$entryCountdownSeconds = $CountdownSeconds
$entryAllowSilent = [bool]$AllowSilent
$entryPlanOnly = [bool]$PlanOnly
$entryPassThru = [bool]$PassThru
$entryProbeInvoker = $ProbeInvoker

function Get-TalkAsrRecorderJsonProperty {
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

function Get-TalkAsrRecorderOptionalJsonProperty {
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

function Assert-TalkAsrRecorderSafeId {
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

function Assert-TalkAsrRecorderPositiveInt {
    param(
        [Parameter(Mandatory = $true)][int]$Value,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($Value -le 0) {
        throw "$Name must be greater than 0"
    }
}

function Resolve-TalkAsrRecorderPath {
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

function Read-TalkAsrCorpusRecorderPrompts {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$PromptManifest,
        [int]$DefaultCaptureSeconds = 3
    )

    Assert-TalkAsrRecorderPositiveInt -Value $DefaultCaptureSeconds -Name 'DefaultCaptureSeconds'
    $resolvedPromptManifest = Resolve-TalkAsrRecorderPath -Path $PromptManifest
    if (-not (Test-Path -LiteralPath $resolvedPromptManifest -PathType Leaf)) {
        throw "Talk ASR corpus recorder prompt manifest does not exist: $resolvedPromptManifest"
    }

    $manifest = Get-Content -LiteralPath $resolvedPromptManifest -Raw -Encoding UTF8 | ConvertFrom-Json
    $schemaVersion = Get-TalkAsrRecorderJsonProperty -Object $manifest -Name 'schemaVersion' -Context $resolvedPromptManifest
    if ([int]$schemaVersion -ne 1) {
        throw "Unsupported Talk ASR corpus recorder prompt schemaVersion [$schemaVersion]. Expected 1."
    }

    $rawSamples = @(Get-TalkAsrRecorderJsonProperty -Object $manifest -Name 'samples' -Context $resolvedPromptManifest)
    if ($rawSamples.Count -eq 0) {
        throw "Talk ASR corpus recorder prompt manifest has no samples: $resolvedPromptManifest"
    }

    $seenSampleIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $samples = New-Object System.Collections.Generic.List[object]
    for ($index = 0; $index -lt $rawSamples.Count; $index += 1) {
        $sample = $rawSamples[$index]
        $context = "$resolvedPromptManifest samples[$index]"
        $sampleId = [string](Get-TalkAsrRecorderJsonProperty -Object $sample -Name 'sampleId' -Context $context)
        Assert-TalkAsrRecorderSafeId -Value $sampleId -Name 'sampleId'
        if (-not $seenSampleIds.Add($sampleId)) {
            throw "Talk ASR corpus recorder prompt manifest contains duplicate sampleId [$sampleId]"
        }

        $referenceText = [string](Get-TalkAsrRecorderJsonProperty -Object $sample -Name 'referenceText' -Context $context)
        if ([string]::IsNullOrWhiteSpace($referenceText)) {
            throw "$context referenceText must not be blank"
        }

        $captureSecondsProperty = Get-TalkAsrRecorderOptionalJsonProperty -Object $sample -Name 'captureSeconds'
        $captureSeconds = if ($null -eq $captureSecondsProperty) {
            $DefaultCaptureSeconds
        } else {
            [int]$captureSecondsProperty
        }
        Assert-TalkAsrRecorderPositiveInt -Value $captureSeconds -Name "$context captureSeconds"

        $samples.Add([pscustomobject]@{
            SampleId = $sampleId
            ReferenceText = $referenceText
            CaptureSeconds = $captureSeconds
        }) | Out-Null
    }

    $samples.ToArray()
}

function Resolve-TalkAsrCorpusRecorderDefaultOutputRoot {
    $baseDir = if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        Split-Path -Parent $PSScriptRoot
    } else {
        $PSScriptRoot
    }
    [System.IO.Path]::GetFullPath((Join-Path $baseDir '.runtime\asr-bench\real-mic-corpus'))
}

function Resolve-TalkAsrCorpusRecorderDefaultTalkExe {
    if ((Split-Path -Leaf $PSScriptRoot) -eq 'scripts') {
        $talkRoot = Split-Path -Parent $PSScriptRoot
        return [System.IO.Path]::GetFullPath((Join-Path $talkRoot 'target\release\talk.exe'))
    }

    [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '.internal\talk.exe'))
}

function ConvertTo-TalkAsrCorpusRecorderTomlPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $Path.Replace('\', '\\').Replace('"', '\"')
}

function New-TalkAsrCorpusRecorderConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$CaptureTempDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [string]$InputDevice,
        [int]$MaxRecordingSeconds
    )

    $inputDeviceLine = if ([string]::IsNullOrWhiteSpace($InputDevice)) {
        ''
    } else {
        'input_device = "' + $InputDevice.Replace('\', '\\').Replace('"', '\"') + '"'
    }
    $maxRecordingSecondsLine = if ($MaxRecordingSeconds -gt 0) {
        "max_recording_seconds = $MaxRecordingSeconds"
    } else {
        'max_recording_seconds = 60'
    }

    @"
voice_mode = "dictate"

[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F19"

[audio]
backend = "native_windows"
$inputDeviceLine
$maxRecordingSecondsLine
sample_rate_hz = 16000
channels = 1
temp_dir = "$(ConvertTo-TalkAsrCorpusRecorderTomlPath -Path $CaptureTempDir)"

[provider]
kind = "mock"
mock_transcript = "talk corpus recorder"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(ConvertTo-TalkAsrCorpusRecorderTomlPath -Path $LogsDir)"
"@
}

function Write-TalkAsrCorpusRecorderConfig {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$CaptureTempDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [string]$InputDevice,
        [int]$MaxRecordingSeconds
    )

    $directory = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($directory)) {
        New-Item -ItemType Directory -Path $directory -Force | Out-Null
    }
    $content = New-TalkAsrCorpusRecorderConfigContent `
        -CaptureTempDir $CaptureTempDir `
        -LogsDir $LogsDir `
        -InputDevice $InputDevice `
        -MaxRecordingSeconds $MaxRecordingSeconds
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, ($content.Trim() + [Environment]::NewLine), $utf8NoBom)
}

function New-TalkAsrCorpusRecorderPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$PromptManifest,
        [string]$OutputRoot,
        [string]$TalkExe,
        [string]$InputDevice,
        [int]$DefaultCaptureSeconds = 3,
        [int]$CountdownSeconds = 3
    )

    Assert-TalkAsrRecorderPositiveInt -Value $DefaultCaptureSeconds -Name 'DefaultCaptureSeconds'
    if ($CountdownSeconds -lt 0) {
        throw 'CountdownSeconds must not be negative'
    }
    if (-not [string]::IsNullOrWhiteSpace($InputDevice) -and $InputDevice.Trim() -ne $InputDevice) {
        throw 'InputDevice must not have leading or trailing whitespace'
    }

    $resolvedOutputRoot = if ([string]::IsNullOrWhiteSpace($OutputRoot)) {
        Resolve-TalkAsrCorpusRecorderDefaultOutputRoot
    } else {
        Resolve-TalkAsrRecorderPath -Path $OutputRoot
    }
    $resolvedTalkExe = if ([string]::IsNullOrWhiteSpace($TalkExe)) {
        Resolve-TalkAsrCorpusRecorderDefaultTalkExe
    } else {
        Resolve-TalkAsrRecorderPath -Path $TalkExe
    }
    $resolvedPromptManifest = Resolve-TalkAsrRecorderPath -Path $PromptManifest
    $samples = @(Read-TalkAsrCorpusRecorderPrompts `
        -PromptManifest $resolvedPromptManifest `
        -DefaultCaptureSeconds $DefaultCaptureSeconds)
    $maxRecordingSeconds = ($samples | Measure-Object -Property CaptureSeconds -Maximum).Maximum
    if ($null -eq $maxRecordingSeconds -or [int]$maxRecordingSeconds -le 0) {
        $maxRecordingSeconds = $DefaultCaptureSeconds
    }

    $plannedSamples = New-Object System.Collections.Generic.List[object]
    foreach ($sample in $samples) {
        $audioLeaf = "$($sample.SampleId)-16k-mono-s16.wav"
        $plannedSamples.Add([pscustomobject]@{
            SampleId = $sample.SampleId
            ReferenceText = $sample.ReferenceText
            CaptureSeconds = [int]$sample.CaptureSeconds
            AudioWav = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot $audioLeaf))
            AudioWavRelative = $audioLeaf
        }) | Out-Null
    }

    [pscustomobject]@{
        PromptManifest = $resolvedPromptManifest
        OutputRoot = $resolvedOutputRoot
        TalkExe = $resolvedTalkExe
        InputDevice = [string]$InputDevice
        DefaultCaptureSeconds = $DefaultCaptureSeconds
        CountdownSeconds = $CountdownSeconds
        ConfigPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'recording-config.toml'))
        CaptureTempDir = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot '.captures'))
        LogsDir = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'logs'))
        CorpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'corpus.json'))
        MaxRecordingSeconds = [int]$maxRecordingSeconds
        Samples = $plannedSamples.ToArray()
    }
}

function Invoke-TalkAsrCorpusRecorderDefaultProbe {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)]$Sample
    )

    $output = & $Plan.TalkExe probe-audio --config $Plan.ConfigPath --seconds ([string]$Sample.CaptureSeconds) --json 2>&1
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Talk ASR corpus recorder probe failed with exit code $exitCode`: $($output | Out-String)"
    }

    ($output -join [Environment]::NewLine) | ConvertFrom-Json
}

function Get-TalkAsrCorpusRecorderProbeSignal {
    param([Parameter(Mandatory = $true)]$ProbeReport)

    if ($null -eq $ProbeReport.audio -or $null -eq $ProbeReport.audio.signal) {
        throw 'Talk ASR corpus recorder probe report is missing audio.signal'
    }
    $ProbeReport.audio.signal
}

function Copy-TalkAsrCorpusRecorderProbeArtifact {
    param(
        [Parameter(Mandatory = $true)]$ProbeReport,
        [Parameter(Mandatory = $true)]$Sample,
        [switch]$AllowSilent
    )

    $signal = Get-TalkAsrCorpusRecorderProbeSignal -ProbeReport $ProbeReport
    $artifactPath = [string]$signal.artifactPath
    if ([string]::IsNullOrWhiteSpace($artifactPath) -or -not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        throw "Talk ASR corpus recorder probe artifact does not exist: $artifactPath"
    }
    if ([int]$signal.sampleRateHz -ne 16000) {
        throw "Talk ASR corpus recorder expected 16kHz WAV, got $($signal.sampleRateHz)"
    }
    if ([int]$signal.channels -ne 1) {
        throw "Talk ASR corpus recorder expected mono WAV, got $($signal.channels) channels"
    }
    if ((-not $AllowSilent) -and [bool]$signal.silent) {
        throw "Talk ASR corpus recorder captured silence for sample [$($Sample.SampleId)]"
    }

    Copy-Item -LiteralPath $artifactPath -Destination $Sample.AudioWav -Force
    [pscustomobject]@{
        SampleId = $Sample.SampleId
        ReferenceText = $Sample.ReferenceText
        AudioWav = $Sample.AudioWav
        SourceArtifactPath = $artifactPath
        DurationSeconds = [double]$signal.durationSeconds
        Peak = [double]$signal.peak
        Rms = [double]$signal.rms
        Silent = [bool]$signal.silent
    }
}

function Write-TalkAsrCorpusRecorderManifest {
    param(
        [Parameter(Mandatory = $true)]$Plan
    )

    $manifestSamples = @(
        foreach ($sample in $Plan.Samples) {
            [ordered]@{
                sampleId = [string]$sample.SampleId
                audioWav = [string]$sample.AudioWavRelative
                referenceText = [string]$sample.ReferenceText
            }
        }
    )
    $manifest = [ordered]@{
        schemaVersion = 1
        samples = $manifestSamples
    }
    $json = ($manifest | ConvertTo-Json -Depth 6)
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Plan.CorpusManifestPath, ($json + [Environment]::NewLine), $utf8NoBom)
}

function Invoke-TalkAsrCorpusRecorder {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$PromptManifest,
        [string]$OutputRoot,
        [string]$TalkExe,
        [string]$InputDevice,
        [int]$DefaultCaptureSeconds = 3,
        [int]$CountdownSeconds = 3,
        [switch]$AllowSilent,
        [switch]$PlanOnly,
        [switch]$PassThru,
        [scriptblock]$ProbeInvoker
    )

    $plan = New-TalkAsrCorpusRecorderPlan `
        -PromptManifest $PromptManifest `
        -OutputRoot $OutputRoot `
        -TalkExe $TalkExe `
        -InputDevice $InputDevice `
        -DefaultCaptureSeconds $DefaultCaptureSeconds `
        -CountdownSeconds $CountdownSeconds

    if ($PlanOnly) {
        return $plan
    }
    if (-not (Test-Path -LiteralPath $plan.TalkExe -PathType Leaf) -and $null -eq $ProbeInvoker) {
        throw "Talk executable does not exist: $($plan.TalkExe)"
    }

    New-Item -ItemType Directory -Path $plan.OutputRoot -Force | Out-Null
    New-Item -ItemType Directory -Path $plan.CaptureTempDir -Force | Out-Null
    New-Item -ItemType Directory -Path $plan.LogsDir -Force | Out-Null
    Write-TalkAsrCorpusRecorderConfig `
        -Path $plan.ConfigPath `
        -CaptureTempDir $plan.CaptureTempDir `
        -LogsDir $plan.LogsDir `
        -InputDevice $plan.InputDevice `
        -MaxRecordingSeconds $plan.MaxRecordingSeconds

    $recordings = New-Object System.Collections.Generic.List[object]
    foreach ($sample in $plan.Samples) {
        Write-Host ''
        Write-Host "Talk ASR corpus sample: $($sample.SampleId)" -ForegroundColor Green
        Write-Host "Read aloud: $($sample.ReferenceText)"
        Write-Host "Capture length: $($sample.CaptureSeconds)s"
        for ($countdown = [int]$plan.CountdownSeconds; $countdown -ge 1; $countdown -= 1) {
            Write-Host "Recording starts in $countdown..."
            Start-Sleep -Seconds 1
        }

        $probeReport = if ($null -ne $ProbeInvoker) {
            & $ProbeInvoker $plan $sample
        } else {
            Invoke-TalkAsrCorpusRecorderDefaultProbe -Plan $plan -Sample $sample
        }
        $recording = Copy-TalkAsrCorpusRecorderProbeArtifact `
            -ProbeReport $probeReport `
            -Sample $sample `
            -AllowSilent:$AllowSilent
        $recordings.Add($recording) | Out-Null
        Write-Host "Recorded: $($recording.AudioWav)"
    }

    Write-TalkAsrCorpusRecorderManifest -Plan $plan
    $result = [pscustomobject]@{
        Plan = $plan
        Recordings = $recordings.ToArray()
        CorpusManifestPath = $plan.CorpusManifestPath
    }

    if ($PassThru) {
        return $result
    }

    $result
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkAsrCorpusRecorder `
        -PromptManifest $entryPromptManifest `
        -OutputRoot $entryOutputRoot `
        -TalkExe $entryTalkExe `
        -InputDevice $entryInputDevice `
        -DefaultCaptureSeconds $entryDefaultCaptureSeconds `
        -CountdownSeconds $entryCountdownSeconds `
        -AllowSilent:$entryAllowSilent `
        -PlanOnly:$entryPlanOnly `
        -PassThru:$entryPassThru `
        -ProbeInvoker $entryProbeInvoker
}
