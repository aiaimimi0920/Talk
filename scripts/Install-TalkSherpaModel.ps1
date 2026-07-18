[CmdletBinding()]
param(
    [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
    [string]$DestinationRoot,
    [string]$ArchivePath,
    [switch]$SkipDownload,
    [switch]$Force,
    [switch]$PassThru
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Get-TalkSherpaModelCatalog {
    @(
        [pscustomobject]@{
            Id = 'zipformer-zh-en-punct-int8-480ms'
            Recommended = $true
            Family = 'transducer'
            ModelName = 'x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8'
            ArchiveName = 'sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2'
            ArchiveUrl = 'https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2'
            SizeBytes = 133895136
            SampleRateHz = 16000
            Provider = 'cpu'
            NumThreads = 2
            DecodingMethod = 'greedy_search'
        }
        [pscustomobject]@{
            Id = 'zipformer-zh-int8-2025-06-30'
            Recommended = $false
            Family = 'transducer'
            ModelName = 'streaming-zipformer-zh-int8-2025-06-30'
            ArchiveName = 'sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30.tar.bz2'
            ArchiveUrl = 'https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-zipformer-zh-int8-2025-06-30.tar.bz2'
            SizeBytes = 132634597
            SampleRateHz = 16000
            Provider = 'cpu'
            NumThreads = 2
            DecodingMethod = 'greedy_search'
        }
        [pscustomobject]@{
            Id = 'paraformer-bilingual-zh-en'
            Recommended = $false
            Family = 'paraformer'
            ModelName = 'streaming-paraformer-bilingual-zh-en'
            ArchiveName = 'sherpa-onnx-streaming-paraformer-bilingual-zh-en.tar.bz2'
            ArchiveUrl = 'https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-streaming-paraformer-bilingual-zh-en.tar.bz2'
            SizeBytes = 1047319737
            SampleRateHz = 16000
            Provider = 'cpu'
            NumThreads = 2
            DecodingMethod = 'greedy_search'
        }
    )
}

function Get-TalkSherpaModelSpec {
    param([Parameter(Mandatory = $true)][string]$ModelId)

    $model = Get-TalkSherpaModelCatalog |
        Where-Object { $_.Id -eq $ModelId } |
        Select-Object -First 1
    if ($null -eq $model) {
        $supported = (Get-TalkSherpaModelCatalog | ForEach-Object { $_.Id }) -join ', '
        throw "Unsupported Talk sherpa model id [$ModelId]. Supported: $supported"
    }

    $model
}

function Resolve-TalkSherpaDefaultModelRoot {
    $scriptLeaf = Split-Path -Leaf $PSScriptRoot
    $baseDir = if ($scriptLeaf -eq 'scripts') {
        Split-Path -Parent $PSScriptRoot
    } else {
        $PSScriptRoot
    }

    Join-Path $baseDir '.runtime\models\sherpa-onnx'
}

function ConvertTo-TalkSherpaTomlBasicString {
    param([Parameter(Mandatory = $true)][string]$Value)

    if ($Value -match "[`r`n]") {
        throw 'Talk sherpa TOML value must not contain newlines'
    }

    '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Find-TalkSherpaModelFile {
    param(
        [Parameter(Mandatory = $true)][string]$ModelDir,
        [Parameter(Mandatory = $true)][ValidateSet('tokens', 'encoder', 'decoder', 'joiner')][string]$Kind,
        [switch]$Required
    )

    $filter = switch ($Kind) {
        'tokens' { 'tokens.txt' }
        'encoder' { 'encoder*.onnx' }
        'decoder' { 'decoder*.onnx' }
        'joiner' { 'joiner*.onnx' }
    }

    $candidate = Get-ChildItem -LiteralPath $ModelDir -Recurse -File -Filter $filter -ErrorAction SilentlyContinue |
        Sort-Object `
            @{ Expression = { $_.FullName -match '\.int8\.' }; Descending = $true },
            @{ Expression = { $_.FullName.Length }; Descending = $false },
            FullName |
        Select-Object -First 1

    if ($null -eq $candidate) {
        if ($Required) {
            throw "Talk sherpa model directory [$ModelDir] is missing required $Kind file matching [$filter]"
        }
        return ''
    }

    $candidate.FullName
}

function New-TalkSherpaModelConfigSnippet {
    param(
        [Parameter(Mandatory = $true)]$ModelSpec,
        [Parameter(Mandatory = $true)][string]$TokensPath,
        [Parameter(Mandatory = $true)][string]$EncoderPath,
        [Parameter(Mandatory = $true)][string]$DecoderPath,
        [string]$JoinerPath = ''
    )

    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add('[speculative.streaming_service.local_daemon]')
    $lines.Add('mode = "sherpa-online"')
    $lines.Add('model_family = ' + (ConvertTo-TalkSherpaTomlBasicString -Value ([string]$ModelSpec.Family)))
    $lines.Add('model = ' + (ConvertTo-TalkSherpaTomlBasicString -Value ([string]$ModelSpec.ModelName)))
    $lines.Add('tokens = ' + (ConvertTo-TalkSherpaTomlBasicString -Value $TokensPath))
    $lines.Add('encoder = ' + (ConvertTo-TalkSherpaTomlBasicString -Value $EncoderPath))
    $lines.Add('decoder = ' + (ConvertTo-TalkSherpaTomlBasicString -Value $DecoderPath))
    if (-not [string]::IsNullOrWhiteSpace($JoinerPath)) {
        $lines.Add('joiner = ' + (ConvertTo-TalkSherpaTomlBasicString -Value $JoinerPath))
    }
    $lines.Add('provider = ' + (ConvertTo-TalkSherpaTomlBasicString -Value ([string]$ModelSpec.Provider)))
    $lines.Add('num_threads = ' + [string]$ModelSpec.NumThreads)
    $lines.Add('sample_rate_hz = ' + [string]$ModelSpec.SampleRateHz)
    $lines.Add('decoding_method = ' + (ConvertTo-TalkSherpaTomlBasicString -Value ([string]$ModelSpec.DecodingMethod)))

    $lines -join [Environment]::NewLine
}

function Test-TalkSherpaModelInstall {
    param(
        [Parameter(Mandatory = $true)][string]$ModelId,
        [Parameter(Mandatory = $true)][string]$ModelDir
    )

    $resolvedModelDir = [System.IO.Path]::GetFullPath($ModelDir)
    if (-not (Test-Path -LiteralPath $resolvedModelDir -PathType Container)) {
        throw "Talk sherpa model directory does not exist: $resolvedModelDir"
    }

    $model = Get-TalkSherpaModelSpec -ModelId $ModelId
    $tokens = Find-TalkSherpaModelFile -ModelDir $resolvedModelDir -Kind tokens -Required
    $encoder = Find-TalkSherpaModelFile -ModelDir $resolvedModelDir -Kind encoder -Required
    $decoder = Find-TalkSherpaModelFile -ModelDir $resolvedModelDir -Kind decoder -Required
    $joiner = ''
    if ([string]$model.Family -eq 'transducer') {
        $joiner = Find-TalkSherpaModelFile -ModelDir $resolvedModelDir -Kind joiner -Required
    }

    $snippet = New-TalkSherpaModelConfigSnippet `
        -ModelSpec $model `
        -TokensPath $tokens `
        -EncoderPath $encoder `
        -DecoderPath $decoder `
        -JoinerPath $joiner

    [pscustomobject]@{
        Valid = $true
        ModelId = $model.Id
        ModelName = $model.ModelName
        ModelFamily = $model.Family
        ModelDir = $resolvedModelDir
        TokensPath = $tokens
        EncoderPath = $encoder
        DecoderPath = $decoder
        JoinerPath = $joiner
        Provider = $model.Provider
        NumThreads = $model.NumThreads
        SampleRateHz = $model.SampleRateHz
        DecodingMethod = $model.DecodingMethod
        ConfigSnippet = $snippet
    }
}

function Assert-TalkSherpaPathInsideRoot {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Path
    )

    $resolvedRoot = [System.IO.Path]::GetFullPath($Root).TrimEnd('\', '/')
    $resolvedPath = [System.IO.Path]::GetFullPath($Path)
    if (-not ($resolvedPath.StartsWith($resolvedRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase))) {
        throw "Refusing to operate on path outside Talk sherpa model root: $resolvedPath"
    }
}

function Expand-TalkSherpaModelArchive {
    param(
        [Parameter(Mandatory = $true)][string]$ArchivePath,
        [Parameter(Mandatory = $true)][string]$DestinationRoot,
        [Parameter(Mandatory = $true)][string]$ModelDir,
        [switch]$Force
    )

    $resolvedArchivePath = [System.IO.Path]::GetFullPath($ArchivePath)
    if (-not (Test-Path -LiteralPath $resolvedArchivePath -PathType Leaf)) {
        throw "Talk sherpa archive does not exist: $resolvedArchivePath"
    }

    $tar = Get-Command tar.exe -ErrorAction SilentlyContinue
    if ($null -eq $tar) {
        throw 'tar.exe is required to extract sherpa-onnx .tar.bz2 model archives on Windows'
    }

    $resolvedDestinationRoot = [System.IO.Path]::GetFullPath($DestinationRoot)
    $resolvedModelDir = [System.IO.Path]::GetFullPath($ModelDir)
    Assert-TalkSherpaPathInsideRoot -Root $resolvedDestinationRoot -Path $resolvedModelDir

    if ((Test-Path -LiteralPath $resolvedModelDir) -and -not $Force) {
        throw "Talk sherpa model directory already exists; use -Force to replace it: $resolvedModelDir"
    }

    New-Item -ItemType Directory -Path $resolvedDestinationRoot -Force | Out-Null
    $extractDir = Join-Path $resolvedDestinationRoot ('_extracting-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $extractDir -Force | Out-Null
    try {
        & $tar.Source -xjf $resolvedArchivePath -C $extractDir
        if ($LASTEXITCODE -ne 0) {
            throw "tar.exe failed to extract Talk sherpa archive with exit code $LASTEXITCODE"
        }

        $extractedChildren = @(Get-ChildItem -LiteralPath $extractDir -Directory)
        $sourceDir = if ($extractedChildren.Count -eq 1) {
            $extractedChildren[0].FullName
        } else {
            $extractDir
        }

        if (Test-Path -LiteralPath $resolvedModelDir) {
            Assert-TalkSherpaPathInsideRoot -Root $resolvedDestinationRoot -Path $resolvedModelDir
            Remove-Item -LiteralPath $resolvedModelDir -Recurse -Force
        }
        Move-Item -LiteralPath $sourceDir -Destination $resolvedModelDir
    }
    finally {
        if (Test-Path -LiteralPath $extractDir) {
            Remove-Item -LiteralPath $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

function Install-TalkSherpaModel {
    [CmdletBinding()]
    param(
        [string]$ModelId = 'zipformer-zh-en-punct-int8-480ms',
        [string]$DestinationRoot,
        [string]$ArchivePath,
        [switch]$SkipDownload,
        [switch]$Force,
        [switch]$PassThru
    )

    $model = Get-TalkSherpaModelSpec -ModelId $ModelId
    $resolvedDestinationRoot = if ([string]::IsNullOrWhiteSpace($DestinationRoot)) {
        [System.IO.Path]::GetFullPath((Resolve-TalkSherpaDefaultModelRoot))
    } else {
        [System.IO.Path]::GetFullPath($DestinationRoot)
    }
    $modelDir = Join-Path $resolvedDestinationRoot $model.Id

    if ($Force -or -not (Test-Path -LiteralPath $modelDir -PathType Container)) {
        if ($SkipDownload -and [string]::IsNullOrWhiteSpace($ArchivePath)) {
            throw "Talk sherpa model is not installed and -SkipDownload was set: $modelDir"
        }

        New-Item -ItemType Directory -Path $resolvedDestinationRoot -Force | Out-Null
        $resolvedArchivePath = if (-not [string]::IsNullOrWhiteSpace($ArchivePath)) {
            [System.IO.Path]::GetFullPath($ArchivePath)
        } else {
            $downloadDir = Join-Path $resolvedDestinationRoot '_downloads'
            New-Item -ItemType Directory -Path $downloadDir -Force | Out-Null
            Join-Path $downloadDir $model.ArchiveName
        }

        if (-not (Test-Path -LiteralPath $resolvedArchivePath -PathType Leaf)) {
            if ($SkipDownload) {
                throw "Talk sherpa archive is missing and -SkipDownload was set: $resolvedArchivePath"
            }
            Invoke-WebRequest -Uri $model.ArchiveUrl -OutFile $resolvedArchivePath
        }

        Expand-TalkSherpaModelArchive `
            -ArchivePath $resolvedArchivePath `
            -DestinationRoot $resolvedDestinationRoot `
            -ModelDir $modelDir `
            -Force:$Force
    }

    $validation = Test-TalkSherpaModelInstall -ModelId $model.Id -ModelDir $modelDir
    $snippetPath = Join-Path $validation.ModelDir 'talk-local-daemon.toml.snippet'
    Set-Content -LiteralPath $snippetPath -Value ($validation.ConfigSnippet + [Environment]::NewLine) -Encoding UTF8
    $validation | Add-Member -NotePropertyName ConfigSnippetPath -NotePropertyValue $snippetPath -Force

    if ($PassThru) {
        return $validation
    }

    $validation
}

if ($MyInvocation.InvocationName -ne '.') {
    Install-TalkSherpaModel `
        -ModelId $ModelId `
        -DestinationRoot $DestinationRoot `
        -ArchivePath $ArchivePath `
        -SkipDownload:$SkipDownload `
        -Force:$Force `
        -PassThru:$PassThru
}
