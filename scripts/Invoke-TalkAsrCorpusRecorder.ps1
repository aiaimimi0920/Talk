[CmdletBinding()]
param(
    [string]$PromptManifest,
    [string]$OutputRoot,
    [string]$TalkExe,
    [string]$InputDevice,
    [int]$DefaultCaptureSeconds = 3,
    [int]$CountdownSeconds = 3,
    [string[]]$ForceRecordSampleId,
    [switch]$ResumeExisting,
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
$entryForceRecordSampleId = $ForceRecordSampleId
$entryResumeExisting = [bool]$ResumeExisting
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

function Get-TalkAsrRecorderFileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)

    (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
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

function Resolve-TalkAsrCorpusRecorderManifestAudioPath {
    param(
        [Parameter(Mandatory = $true)][string]$CorpusManifest,
        [Parameter(Mandatory = $true)][string]$AudioWav
    )

    if ([System.IO.Path]::IsPathRooted($AudioWav)) {
        return [System.IO.Path]::GetFullPath($AudioWav)
    }

    [System.IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $CorpusManifest) $AudioWav))
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

function Read-TalkAsrCorpusRecorderExistingCorpusManifest {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$CorpusManifest)

    $resolvedCorpusManifest = Resolve-TalkAsrRecorderPath -Path $CorpusManifest
    if (-not (Test-Path -LiteralPath $resolvedCorpusManifest -PathType Leaf)) {
        return @()
    }

    $manifest = Get-Content -LiteralPath $resolvedCorpusManifest -Raw -Encoding UTF8 | ConvertFrom-Json
    $schemaVersion = Get-TalkAsrRecorderJsonProperty -Object $manifest -Name 'schemaVersion' -Context $resolvedCorpusManifest
    if ([int]$schemaVersion -ne 1) {
        throw "Unsupported Talk ASR corpus manifest schemaVersion [$schemaVersion]. Expected 1."
    }

    $rawSamples = @(Get-TalkAsrRecorderJsonProperty -Object $manifest -Name 'samples' -Context $resolvedCorpusManifest)
    $seenSampleIds = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    $samples = New-Object System.Collections.Generic.List[object]
    for ($index = 0; $index -lt $rawSamples.Count; $index += 1) {
        $sample = $rawSamples[$index]
        $context = "$resolvedCorpusManifest samples[$index]"
        $sampleId = [string](Get-TalkAsrRecorderJsonProperty -Object $sample -Name 'sampleId' -Context $context)
        Assert-TalkAsrRecorderSafeId -Value $sampleId -Name 'sampleId'
        if (-not $seenSampleIds.Add($sampleId)) {
            throw "Talk ASR corpus manifest contains duplicate sampleId [$sampleId]"
        }

        $audioWav = [string](Get-TalkAsrRecorderJsonProperty -Object $sample -Name 'audioWav' -Context $context)
        if ([string]::IsNullOrWhiteSpace($audioWav)) {
            throw "$context audioWav must not be blank"
        }
        $referenceText = [string](Get-TalkAsrRecorderJsonProperty -Object $sample -Name 'referenceText' -Context $context)
        if ([string]::IsNullOrWhiteSpace($referenceText)) {
            throw "$context referenceText must not be blank"
        }

        $resolvedAudioWav = Resolve-TalkAsrCorpusRecorderManifestAudioPath `
            -CorpusManifest $resolvedCorpusManifest `
            -AudioWav $audioWav
        $declaredAudioSha256 = [string](Get-TalkAsrRecorderOptionalJsonProperty -Object $sample -Name 'audioSha256')
        $actualAudioSha256 = if (Test-Path -LiteralPath $resolvedAudioWav -PathType Leaf) {
            Get-TalkAsrRecorderFileSha256 -Path $resolvedAudioWav
        } else {
            $null
        }
        $audioIntegrityValid = `
            $declaredAudioSha256 -cmatch '^[0-9a-fA-F]{64}$' -and `
            -not [string]::IsNullOrWhiteSpace([string]$actualAudioSha256) -and `
            [string]::Equals($declaredAudioSha256, $actualAudioSha256, [System.StringComparison]::OrdinalIgnoreCase)

        $samples.Add([pscustomobject]@{
            SampleId = $sampleId
            ReferenceText = $referenceText
            AudioWav = $resolvedAudioWav
            AudioSha256 = $declaredAudioSha256
            AudioIntegrityValid = $audioIntegrityValid
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
        [int]$CountdownSeconds = 3,
        [string[]]$ForceRecordSampleId,
        [switch]$ResumeExisting
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
    $promptSampleIdSet = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($sample in $samples) {
        [void]$promptSampleIdSet.Add([string]$sample.SampleId)
    }
    $forceRecordSampleIds = New-Object System.Collections.Generic.List[string]
    $forceRecordSampleIdSet = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($rawForceRecordSampleId in @($ForceRecordSampleId)) {
        $forceRecordSampleId = [string]$rawForceRecordSampleId
        if ([string]::IsNullOrWhiteSpace($forceRecordSampleId)) {
            continue
        }
        if ($forceRecordSampleId.Trim() -ne $forceRecordSampleId) {
            throw 'ForceRecordSampleId must not have leading or trailing whitespace'
        }
        if ($forceRecordSampleId -notmatch '^[A-Za-z0-9][A-Za-z0-9_.-]*$') {
            throw "ForceRecordSampleId [$forceRecordSampleId] must use only letters, numbers, dot, underscore, or hyphen"
        }
        if (-not $promptSampleIdSet.Contains($forceRecordSampleId)) {
            throw "ForceRecordSampleId [$forceRecordSampleId] does not exist in prompt manifest [$resolvedPromptManifest]"
        }
        if ($forceRecordSampleIdSet.Add($forceRecordSampleId)) {
            $forceRecordSampleIds.Add($forceRecordSampleId) | Out-Null
        }
    }
    $corpusManifestPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'corpus.json'))
    $existingSamplesById = @{}
    if ($ResumeExisting -and (Test-Path -LiteralPath $corpusManifestPath -PathType Leaf)) {
        foreach ($existingSample in @(Read-TalkAsrCorpusRecorderExistingCorpusManifest -CorpusManifest $corpusManifestPath)) {
            $existingSamplesById[[string]$existingSample.SampleId] = $existingSample
        }
    }
    $maxRecordingSeconds = ($samples | Measure-Object -Property CaptureSeconds -Maximum).Maximum
    if ($null -eq $maxRecordingSeconds -or [int]$maxRecordingSeconds -le 0) {
        $maxRecordingSeconds = $DefaultCaptureSeconds
    }

    $plannedSamples = New-Object System.Collections.Generic.List[object]
    foreach ($sample in $samples) {
        $audioLeaf = "$($sample.SampleId)-16k-mono-s16.wav"
        $plannedAudioWav = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot $audioLeaf))
        $existingSample = $existingSamplesById[[string]$sample.SampleId]
        $existingAudioWav = $null
        $willRecord = $true
        $forceRecord = $forceRecordSampleIdSet.Contains([string]$sample.SampleId)
        if (-not $forceRecord -and $null -ne $existingSample -and
            [string]$existingSample.ReferenceText -ceq [string]$sample.ReferenceText -and
            (Test-Path -LiteralPath ([string]$existingSample.AudioWav) -PathType Leaf) -and
            [bool]$existingSample.AudioIntegrityValid) {
            $existingAudioWav = [string]$existingSample.AudioWav
            $willRecord = $false
        }
        $plannedSamples.Add([pscustomobject]@{
            SampleId = $sample.SampleId
            ReferenceText = $sample.ReferenceText
            CaptureSeconds = [int]$sample.CaptureSeconds
            AudioWav = $plannedAudioWav
            AudioWavRelative = $audioLeaf
            ExistingAudioWav = $existingAudioWav
            ForceRecord = $forceRecord
            WillRecord = $willRecord
        }) | Out-Null
    }
    $plannedSampleArray = @($plannedSamples.ToArray())
    $plannedRecordingCount = @($plannedSampleArray | Where-Object { $_.WillRecord }).Count
    $reusedSampleCount = $plannedSampleArray.Count - $plannedRecordingCount

    [pscustomobject]@{
        PromptManifest = $resolvedPromptManifest
        PromptManifestSha256 = Get-TalkAsrRecorderFileSha256 -Path $resolvedPromptManifest
        OutputRoot = $resolvedOutputRoot
        TalkExe = $resolvedTalkExe
        InputDevice = [string]$InputDevice
        DefaultCaptureSeconds = $DefaultCaptureSeconds
        CountdownSeconds = $CountdownSeconds
        ForceRecordSampleId = @($forceRecordSampleIds.ToArray())
        ResumeExisting = [bool]$ResumeExisting
        ConfigPath = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'recording-config.toml'))
        CaptureTempDir = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot '.captures'))
        LogsDir = [System.IO.Path]::GetFullPath((Join-Path $resolvedOutputRoot 'logs'))
        CorpusManifestPath = $corpusManifestPath
        MaxRecordingSeconds = [int]$maxRecordingSeconds
        PlannedRecordingCount = $plannedRecordingCount
        ReusedSampleCount = $reusedSampleCount
        Samples = $plannedSampleArray
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

function Get-TalkAsrCorpusRecorderProbeNativeWindowsMetadata {
    param([Parameter(Mandatory = $true)]$ProbeReport)

    $nativeWindows = Get-TalkAsrRecorderOptionalJsonProperty -Object $ProbeReport.audio -Name 'nativeWindows'
    if ($null -eq $nativeWindows) {
        return [pscustomobject]@{
            CapturedInputDevice = $null
            AvailableInputDevices = @()
        }
    }

    $capturedInputDevice = [string](Get-TalkAsrRecorderOptionalJsonProperty -Object $nativeWindows -Name 'deviceName')
    $availableInputDevices = @(
        @(Get-TalkAsrRecorderOptionalJsonProperty -Object $nativeWindows -Name 'availableDeviceNames') |
            ForEach-Object { [string]$_ } |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )

    [pscustomobject]@{
        CapturedInputDevice = if ([string]::IsNullOrWhiteSpace($capturedInputDevice)) { $null } else { $capturedInputDevice }
        AvailableInputDevices = $availableInputDevices
    }
}

function Test-TalkAsrCorpusRecorderProbeNeedsInputDeviceWarning {
    param(
        [Parameter(Mandatory = $true)]$Plan,
        [Parameter(Mandatory = $true)]$Recording
    )

    if (-not [string]::IsNullOrWhiteSpace([string]$Plan.InputDevice)) {
        return $false
    }

    $capturedInputDevice = [string](Get-TalkAsrRecorderOptionalJsonProperty -Object $Recording -Name 'CapturedInputDevice')
    $availableInputDevices = @(
        @(Get-TalkAsrRecorderOptionalJsonProperty -Object $Recording -Name 'AvailableInputDevices') |
            Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
    )

    (-not [string]::IsNullOrWhiteSpace($capturedInputDevice)) -and ($availableInputDevices.Count -gt 1)
}

function Read-TalkAsrCorpusRecorderPcmWavInfo {
    param([Parameter(Mandatory = $true)][string]$Path)

    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $reader = New-Object System.IO.BinaryReader($stream, [System.Text.Encoding]::ASCII)
        try {
            if ($stream.Length -lt 12) {
                throw "Talk ASR corpus recorder WAV is too short: $Path"
            }
            $riff = [System.Text.Encoding]::ASCII.GetString($reader.ReadBytes(4))
            $null = $reader.ReadUInt32()
            $wave = [System.Text.Encoding]::ASCII.GetString($reader.ReadBytes(4))
            if ($riff -ne 'RIFF' -or $wave -ne 'WAVE') {
                throw "Talk ASR corpus recorder artifact is not a RIFF/WAVE file: $Path"
            }

            $format = $null
            while (($stream.Position + 8) -le $stream.Length) {
                $chunkId = [System.Text.Encoding]::ASCII.GetString($reader.ReadBytes(4))
                $chunkSize = [uint32]$reader.ReadUInt32()
                $chunkDataStart = [int64]$stream.Position
                $chunkDataEnd = $chunkDataStart + [int64]$chunkSize
                if ($chunkDataEnd -gt $stream.Length) {
                    throw "Talk ASR corpus recorder WAV chunk [$chunkId] exceeds file length: $Path"
                }

                if ($chunkId -eq 'fmt ') {
                    if ($chunkSize -lt 16) {
                        throw "Talk ASR corpus recorder WAV fmt chunk is too short: $Path"
                    }
                    $format = [pscustomobject]@{
                        AudioFormat = [int]$reader.ReadUInt16()
                        Channels = [int]$reader.ReadUInt16()
                        SampleRateHz = [int]$reader.ReadUInt32()
                        ByteRate = [int]$reader.ReadUInt32()
                        BlockAlign = [int]$reader.ReadUInt16()
                        BitsPerSample = [int]$reader.ReadUInt16()
                    }
                }

                $stream.Position = $chunkDataEnd
                if (($chunkSize % 2) -eq 1 -and $stream.Position -lt $stream.Length) {
                    $stream.Position += 1
                }
            }

            if ($null -eq $format) {
                throw "Talk ASR corpus recorder WAV is missing a fmt chunk: $Path"
            }
            if ($format.AudioFormat -ne 1) {
                throw "Talk ASR corpus recorder expected PCM WAV, got audio format $($format.AudioFormat)"
            }
            $format
        } finally {
            $reader.Dispose()
        }
    } finally {
        $stream.Dispose()
    }
}

function Copy-TalkAsrCorpusRecorderProbeArtifact {
    param(
        [Parameter(Mandatory = $true)]$ProbeReport,
        [Parameter(Mandatory = $true)]$Sample,
        [switch]$AllowSilent
    )

    $signal = Get-TalkAsrCorpusRecorderProbeSignal -ProbeReport $ProbeReport
    $nativeWindowsMetadata = Get-TalkAsrCorpusRecorderProbeNativeWindowsMetadata -ProbeReport $ProbeReport
    $artifactPath = [string]$signal.artifactPath
    if ([string]::IsNullOrWhiteSpace($artifactPath) -or -not (Test-Path -LiteralPath $artifactPath -PathType Leaf)) {
        throw "Talk ASR corpus recorder probe artifact does not exist: $artifactPath"
    }
    $wavInfo = Read-TalkAsrCorpusRecorderPcmWavInfo -Path $artifactPath
    if ($wavInfo.SampleRateHz -ne 16000) {
        throw "Talk ASR corpus recorder expected 16kHz WAV, got $($wavInfo.SampleRateHz)"
    }
    if ($wavInfo.Channels -ne 1) {
        throw "Talk ASR corpus recorder expected mono WAV, got $($wavInfo.Channels) channels"
    }
    if ($wavInfo.BitsPerSample -ne 16) {
        throw "Talk ASR corpus recorder expected 16-bit PCM WAV, got $($wavInfo.BitsPerSample)-bit"
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
        CapturedInputDevice = $nativeWindowsMetadata.CapturedInputDevice
        AvailableInputDevices = @($nativeWindowsMetadata.AvailableInputDevices)
        Reused = $false
    }
}

function Copy-TalkAsrCorpusRecorderExistingArtifact {
    param([Parameter(Mandatory = $true)]$Sample)

    $existingAudioWav = [string]$Sample.ExistingAudioWav
    if ([string]::IsNullOrWhiteSpace($existingAudioWav) -or -not (Test-Path -LiteralPath $existingAudioWav -PathType Leaf)) {
        throw "Talk ASR corpus recorder expected an existing recording for sample [$($Sample.SampleId)]"
    }

    $wavInfo = Read-TalkAsrCorpusRecorderPcmWavInfo -Path $existingAudioWav
    if ($wavInfo.SampleRateHz -ne 16000) {
        throw "Talk ASR corpus recorder expected 16kHz WAV, got $($wavInfo.SampleRateHz)"
    }
    if ($wavInfo.Channels -ne 1) {
        throw "Talk ASR corpus recorder expected mono WAV, got $($wavInfo.Channels) channels"
    }
    if ($wavInfo.BitsPerSample -ne 16) {
        throw "Talk ASR corpus recorder expected 16-bit PCM WAV, got $($wavInfo.BitsPerSample)-bit"
    }

    if (-not [string]::Equals(
        [System.IO.Path]::GetFullPath($existingAudioWav),
        [System.IO.Path]::GetFullPath([string]$Sample.AudioWav),
        [System.StringComparison]::OrdinalIgnoreCase)) {
        Copy-Item -LiteralPath $existingAudioWav -Destination $Sample.AudioWav -Force
    }

    [pscustomobject]@{
        SampleId = $Sample.SampleId
        ReferenceText = $Sample.ReferenceText
        AudioWav = $Sample.AudioWav
        SourceArtifactPath = $existingAudioWav
        DurationSeconds = $null
        Peak = $null
        Rms = $null
        Silent = $false
        CapturedInputDevice = $null
        AvailableInputDevices = @()
        Reused = $true
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
                audioSha256 = Get-TalkAsrRecorderFileSha256 -Path ([string]$sample.AudioWav)
                referenceText = [string]$sample.ReferenceText
            }
        }
    )
    $manifest = [ordered]@{
        schemaVersion = 1
        createdAtUtc = [DateTimeOffset]::UtcNow.ToString('o')
        promptManifestSha256 = [string]$Plan.PromptManifestSha256
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
        [string[]]$ForceRecordSampleId,
        [switch]$ResumeExisting,
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
        -CountdownSeconds $CountdownSeconds `
        -ForceRecordSampleId $ForceRecordSampleId `
        -ResumeExisting:$ResumeExisting

    if ($PlanOnly) {
        return $plan
    }
    if ($plan.PlannedRecordingCount -gt 0 -and -not (Test-Path -LiteralPath $plan.TalkExe -PathType Leaf) -and $null -eq $ProbeInvoker) {
        throw "Talk executable does not exist: $($plan.TalkExe)"
    }

    New-Item -ItemType Directory -Path $plan.OutputRoot -Force | Out-Null
    if ($plan.PlannedRecordingCount -gt 0) {
        New-Item -ItemType Directory -Path $plan.CaptureTempDir -Force | Out-Null
        New-Item -ItemType Directory -Path $plan.LogsDir -Force | Out-Null
        Write-TalkAsrCorpusRecorderConfig `
            -Path $plan.ConfigPath `
            -CaptureTempDir $plan.CaptureTempDir `
            -LogsDir $plan.LogsDir `
            -InputDevice $plan.InputDevice `
            -MaxRecordingSeconds $plan.MaxRecordingSeconds
    }

    $recordings = New-Object System.Collections.Generic.List[object]
    $inputDeviceWarningEmitted = $false
    foreach ($sample in $plan.Samples) {
        if (-not [bool]$sample.WillRecord) {
            Write-Host ''
            Write-Host "Talk ASR corpus sample: $($sample.SampleId)" -ForegroundColor Green
            Write-Host "Reusing existing recording: $($sample.ExistingAudioWav)"
            $recordings.Add((Copy-TalkAsrCorpusRecorderExistingArtifact -Sample $sample)) | Out-Null
            continue
        }

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
        if (-not $inputDeviceWarningEmitted -and (Test-TalkAsrCorpusRecorderProbeNeedsInputDeviceWarning -Plan $plan -Recording $recording)) {
            $availableInputDevices = @(
                @(Get-TalkAsrRecorderOptionalJsonProperty -Object $recording -Name 'AvailableInputDevices') |
                    ForEach-Object { [string]$_ } |
                    Where-Object { -not [string]::IsNullOrWhiteSpace([string]$_) }
            )
            Write-Warning ("Talk ASR corpus recorder used default input device [{0}] while multiple input devices are available: {1}. Re-run with -InputDevice to lock the intended microphone." -f `
                ([string]$recording.CapturedInputDevice), `
                ($availableInputDevices -join ', '))
            $inputDeviceWarningEmitted = $true
        }
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
        -ForceRecordSampleId $entryForceRecordSampleId `
        -ResumeExisting:$entryResumeExisting `
        -AllowSilent:$entryAllowSilent `
        -PlanOnly:$entryPlanOnly `
        -PassThru:$entryPassThru `
        -ProbeInvoker $entryProbeInvoker
}
