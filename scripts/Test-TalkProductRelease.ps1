param(
    [string]$ProductPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function ConvertTo-TalkPayloadHex {
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)

    ($Bytes | ForEach-Object { $_.ToString('x2') }) -join ''
}

function Get-TalkPayloadJsonPropertyOrdinal {
    param(
        [AllowNull()][object]$Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }
    foreach ($property in @($Object.PSObject.Properties)) {
        if ([string]::Equals($property.Name, $Name, [System.StringComparison]::Ordinal)) {
            return $property
        }
    }
    return $null
}

function Assert-TalkPayloadJsonPropertySet {
    param(
        [AllowNull()][object]$Object,
        [Parameter(Mandatory = $true)][string[]]$Names,
        [Parameter(Mandatory = $true)][string]$Context
    )

    if ($null -eq $Object) {
        throw "$Context must be an object"
    }
    $properties = @($Object.PSObject.Properties)
    if ($properties.Count -ne $Names.Count) {
        throw "$Context must contain exactly: $($Names -join ', ')"
    }
    foreach ($name in $Names) {
        if ($null -eq (Get-TalkPayloadJsonPropertyOrdinal -Object $Object -Name $name)) {
            throw "$Context must contain exactly: $($Names -join ', ')"
        }
    }
}

function Read-TalkEmbeddedRuntimePayload {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][byte[]]$Bytes)

    Add-Type -AssemblyName System.IO.Compression | Out-Null
    $trailerLength = 60
    if ($Bytes.Length -lt $trailerLength) {
        throw 'Talk.exe is too small to contain an embedded runtime payload trailer'
    }

    $trailerStart = $Bytes.Length - $trailerLength
    $magic = [System.Text.Encoding]::ASCII.GetBytes('TLPAY001')
    for ($index = 0; $index -lt $magic.Length; $index++) {
        if ($Bytes[$trailerStart + $index] -ne $magic[$index]) {
            throw 'Talk.exe embedded runtime payload magic is missing'
        }
    }

    [uint32]$version = [System.BitConverter]::ToUInt32($Bytes, $trailerStart + 8)
    if ($version -ne 1) {
        throw "unsupported Talk runtime payload version $version"
    }
    [uint64]$archiveLength = [System.BitConverter]::ToUInt64($Bytes, $trailerStart + 12)
    [uint64]$manifestLength = [System.BitConverter]::ToUInt64($Bytes, $trailerStart + 20)
    if ($archiveLength -gt [uint64][int]::MaxValue -or $manifestLength -gt [uint64][int]::MaxValue) {
        throw 'Talk runtime payload length exceeds the supported in-memory range'
    }
    [uint64]$trailerStart64 = [uint64]$trailerStart
    if ($archiveLength -gt $trailerStart64) {
        throw 'Talk runtime payload lengths exceed executable size'
    }
    [uint64]$remainingBeforeTrailer = $trailerStart64 - $archiveLength
    if ($manifestLength -gt $remainingBeforeTrailer) {
        throw 'Talk runtime payload lengths exceed executable size'
    }

    [int]$archiveStart = [int]($trailerStart64 - $archiveLength - $manifestLength)
    [int]$archiveCount = [int]$archiveLength
    [int]$manifestStart = $archiveStart + $archiveCount
    [int]$manifestCount = [int]$manifestLength
    [byte[]]$archiveBytes = New-Object byte[] $archiveCount
    [byte[]]$manifestBytes = New-Object byte[] $manifestCount
    [Array]::Copy($Bytes, [long]$archiveStart, $archiveBytes, 0, [long]$archiveCount)
    [Array]::Copy($Bytes, [long]$manifestStart, $manifestBytes, 0, [long]$manifestCount)

    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $actualArchiveHash = $sha256.ComputeHash($archiveBytes)
    } finally {
        $sha256.Dispose()
    }
    [byte[]]$expectedArchiveHash = New-Object byte[] 32
    [Array]::Copy($Bytes, $trailerStart + 28, $expectedArchiveHash, 0, 32)
    $actualArchiveHex = ConvertTo-TalkPayloadHex -Bytes $actualArchiveHash
    $expectedArchiveHex = ConvertTo-TalkPayloadHex -Bytes $expectedArchiveHash
    if ($actualArchiveHex -cne $expectedArchiveHex) {
        throw 'Talk runtime payload archive SHA-256 mismatch'
    }

    $strictUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
    try {
        $manifestText = $strictUtf8.GetString($manifestBytes)
    } catch {
        throw "parse Talk runtime payload manifest encoding: $($_.Exception.Message)"
    }
    try {
        $manifest = $manifestText | ConvertFrom-Json
    } catch {
        throw "parse Talk runtime payload manifest: $($_.Exception.Message)"
    }
    Assert-TalkPayloadJsonPropertySet `
        -Object $manifest `
        -Names @('schemaVersion', 'files') `
        -Context 'Talk runtime payload manifest'
    $schemaProperty = Get-TalkPayloadJsonPropertyOrdinal -Object $manifest -Name 'schemaVersion'
    $filesProperty = Get-TalkPayloadJsonPropertyOrdinal -Object $manifest -Name 'files'
    $schemaVersionIsInteger = $schemaProperty.Value -is [int] -or $schemaProperty.Value -is [long]
    if (-not $schemaVersionIsInteger -or [int64]$schemaProperty.Value -ne 1) {
        throw 'unsupported Talk runtime payload manifest version'
    }
    if (-not ($filesProperty.Value -is [System.Array])) {
        throw 'Talk runtime payload manifest files must be an array'
    }

    $expectedNames = @(
        'onnxruntime.dll',
        'onnxruntime_providers_shared.dll',
        'sherpa-onnx-c-api.dll',
        'sherpa-onnx-cxx-api.dll',
        'talk-local-asr-sherpa.exe'
    )
    $expectedNameSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
    foreach ($name in $expectedNames) {
        $expectedNameSet.Add($name) | Out-Null
    }
    $manifestFiles = [System.Collections.Generic.Dictionary[string, string]]::new([System.StringComparer]::Ordinal)
    foreach ($file in $filesProperty.Value) {
        Assert-TalkPayloadJsonPropertySet `
            -Object $file `
            -Names @('path', 'sha256') `
            -Context 'Talk runtime payload manifest member'
        $pathProperty = Get-TalkPayloadJsonPropertyOrdinal -Object $file -Name 'path'
        $hashProperty = Get-TalkPayloadJsonPropertyOrdinal -Object $file -Name 'sha256'
        if (-not ($pathProperty.Value -is [string]) -or -not ($hashProperty.Value -is [string])) {
            throw 'Talk runtime payload manifest member path and sha256 must be strings'
        }
        $path = [string]$pathProperty.Value
        $hash = [string]$hashProperty.Value
        if (-not $expectedNameSet.Contains($path)) {
            throw "unexpected Talk runtime payload member $path"
        }
        if ($manifestFiles.ContainsKey($path)) {
            throw "duplicate Talk runtime payload manifest member $path"
        }
        if ($hash -notmatch '^[0-9A-Fa-f]{64}$') {
            throw "invalid Talk runtime payload SHA-256 $hash"
        }
        $manifestFiles.Add($path, $hash.ToLowerInvariant())
    }
    if ($manifestFiles.Count -ne $expectedNames.Count) {
        throw 'Talk runtime payload manifest member set does not match the expected runtime files'
    }
    foreach ($name in $expectedNames) {
        if (-not $manifestFiles.ContainsKey($name)) {
            throw "missing Talk runtime payload member $name"
        }
    }
    $canonicalManifest = [ordered]@{
        schemaVersion = 1
        files = @($expectedNames | ForEach-Object {
            [ordered]@{
                path = $_
                sha256 = $manifestFiles[$_]
            }
        })
    }
    $canonicalManifestText = $canonicalManifest | ConvertTo-Json -Compress -Depth 5
    if ($manifestText -cne $canonicalManifestText) {
        throw 'Talk runtime payload manifest is not in canonical publisher form'
    }

    $archiveStream = [System.IO.MemoryStream]::new($archiveBytes, $false)
    $archive = $null
    try {
        $archive = [System.IO.Compression.ZipArchive]::new(
            $archiveStream,
            [System.IO.Compression.ZipArchiveMode]::Read,
            $false
        )
        $seenNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::Ordinal)
        foreach ($entry in $archive.Entries) {
            $path = [string]$entry.FullName
            if (-not $expectedNameSet.Contains($path)) {
                throw "unexpected Talk runtime payload ZIP member $path"
            }
            if (-not $seenNames.Add($path)) {
                throw "duplicate Talk runtime payload ZIP member $path"
            }
            $entryStream = $entry.Open()
            $entrySha256 = [System.Security.Cryptography.SHA256]::Create()
            try {
                $actualEntryHash = $entrySha256.ComputeHash($entryStream)
            } finally {
                $entrySha256.Dispose()
                $entryStream.Dispose()
            }
            $actualEntryHex = ConvertTo-TalkPayloadHex -Bytes $actualEntryHash
            if ($actualEntryHex -cne $manifestFiles[$path]) {
                throw "Talk runtime payload member $path SHA-256 mismatch"
            }
        }
        if ($seenNames.Count -ne $manifestFiles.Count) {
            throw 'Talk runtime payload ZIP member set does not match the manifest'
        }
        foreach ($name in $expectedNames) {
            if (-not $seenNames.Contains($name)) {
                throw "missing Talk runtime payload ZIP member $name"
            }
        }
    } finally {
        if ($null -ne $archive) {
            $archive.Dispose()
        }
        $archiveStream.Dispose()
    }

    [pscustomobject]@{
        PayloadVersion = [int]$version
        EmbeddedRuntimeSha256 = $actualArchiveHex
        PayloadFiles = @($expectedNames | Sort-Object)
        ArchiveBytes = [int64]$archiveLength
        ManifestBytes = [int64]$manifestLength
    }
}

function Test-TalkProductRelease {
    [CmdletBinding()]
    param([Parameter(Mandatory = $true)][string]$ProductPath)

    $resolvedProductPath = [System.IO.Path]::GetFullPath($ProductPath)
    if (-not (Test-Path -LiteralPath $resolvedProductPath -PathType Container)) {
        throw "Talk product release directory does not exist: $resolvedProductPath"
    }

    $files = @(Get-ChildItem -LiteralPath $resolvedProductPath -Recurse -File -Force)
    $relativeFiles = @($files | ForEach-Object {
        $_.FullName.Substring($resolvedProductPath.Length).TrimStart('\\')
    } | Sort-Object)
    $expectedFiles = @('Talk.exe', 'talk.toml')
    if (($relativeFiles -join '|') -cne ($expectedFiles -join '|')) {
        throw "Talk product release must contain exactly Talk.exe and talk.toml; found: $($relativeFiles -join ', ')"
    }
    if (@(Get-ChildItem -LiteralPath $resolvedProductPath -Recurse -Directory -Force).Count -ne 0) {
        throw 'Talk product release must not contain subdirectories'
    }

    $executablePath = Join-Path $resolvedProductPath 'Talk.exe'
    [byte[]]$bytes = [System.IO.File]::ReadAllBytes($executablePath)
    $payload = Read-TalkEmbeddedRuntimePayload -Bytes $bytes

    [pscustomobject]@{
        ProductPath = $resolvedProductPath
        Files = $relativeFiles
        ExecutableBytes = $bytes.Length
        PayloadTrailer = 'TLPAY001'
        PayloadVersion = $payload.PayloadVersion
        EmbeddedRuntimeSha256 = $payload.EmbeddedRuntimeSha256
        PayloadFiles = $payload.PayloadFiles
        PayloadArchiveBytes = $payload.ArchiveBytes
        PayloadManifestBytes = $payload.ManifestBytes
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Test-TalkProductRelease -ProductPath $ProductPath
}
