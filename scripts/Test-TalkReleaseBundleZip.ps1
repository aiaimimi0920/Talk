[CmdletBinding()]
param(
    [string]$ZipPath,
    [string]$ExpectedVersionId,
    [string]$WorkRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-TalkReleaseBundleZipWorkRoot {
    param(
        [Parameter(Mandatory = $true)][string]$ResolvedZipPath,
        [string]$WorkRoot
    )

    if (-not [string]::IsNullOrWhiteSpace($WorkRoot)) {
        return [System.IO.Path]::GetFullPath($WorkRoot)
    }

    $zipName = [System.IO.Path]::GetFileNameWithoutExtension($ResolvedZipPath)
    Join-Path ([System.IO.Path]::GetTempPath()) ('talk-release-zip-verify-' + $zipName + '-' + [guid]::NewGuid().ToString('N'))
}

function Get-TalkReleaseBundleZipExpandedDir {
    param(
        [Parameter(Mandatory = $true)][string]$WorkRoot,
        [Parameter(Mandatory = $true)][string]$ResolvedZipPath
    )

    $zipName = [System.IO.Path]::GetFileNameWithoutExtension($ResolvedZipPath)
    $direct = Join-Path $WorkRoot $zipName
    if (Test-Path -LiteralPath $direct -PathType Container) {
        return $direct
    }

    $directories = @(Get-ChildItem -LiteralPath $WorkRoot -Directory -ErrorAction SilentlyContinue)
    if ($directories.Count -eq 1) {
        return $directories[0].FullName
    }

    $WorkRoot
}

function Test-TalkReleaseBundleReadmeBacktickDamage {
    param([Parameter(Mandatory = $true)][string]$ReadmeText)

    ($ReadmeText -match "`t") -or ($ReadmeText -match "`a")
}

function Test-TalkReleaseBundleZip {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ZipPath,
        [string]$ExpectedVersionId,
        [string]$WorkRoot
    )

    $resolvedZipPath = [System.IO.Path]::GetFullPath($ZipPath)
    if (-not (Test-Path -LiteralPath $resolvedZipPath -PathType Leaf)) {
        throw "Talk release zip does not exist: $resolvedZipPath"
    }

    $resolvedWorkRoot = Resolve-TalkReleaseBundleZipWorkRoot -ResolvedZipPath $resolvedZipPath -WorkRoot $WorkRoot
    if (Test-Path -LiteralPath $resolvedWorkRoot) {
        throw "Talk release zip verification work root already exists: $resolvedWorkRoot"
    }
    New-Item -ItemType Directory -Path $resolvedWorkRoot -Force | Out-Null
    Expand-Archive -LiteralPath $resolvedZipPath -DestinationPath $resolvedWorkRoot -Force

    $expandedDir = Get-TalkReleaseBundleZipExpandedDir -WorkRoot $resolvedWorkRoot -ResolvedZipPath $resolvedZipPath
    $requiredFiles = @(
        'README.md',
        'manifest.json',
        'release-summary.json',
        'checksums.sha256',
        'talk-desktop.exe',
        'talk-desktop.toml',
        'Start-TalkDesktop.ps1',
        'Invoke-TalkDesktopLiveHotkeyProbe.ps1',
        'Invoke-TalkDesktopReleaseSmoke.ps1',
        '.internal\talk.exe'
    )
    $missing = New-Object 'System.Collections.Generic.List[string]'
    foreach ($relativePath in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $expandedDir $relativePath) -PathType Leaf)) {
            $missing.Add($relativePath) | Out-Null
        }
    }
    if ($missing.Count -gt 0) {
        throw ('required release files are missing: ' + ($missing.ToArray() -join ', '))
    }

    $manifest = Get-Content -LiteralPath (Join-Path $expandedDir 'manifest.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($manifest.schemaVersion -ne 2) {
        throw "manifest schemaVersion must be 2, got $($manifest.schemaVersion)"
    }
    if ([string]$manifest.app -ne 'Talk') {
        throw "manifest app must be Talk, got $($manifest.app)"
    }
    if (-not [string]::IsNullOrWhiteSpace($ExpectedVersionId) -and [string]$manifest.versionId -ne $ExpectedVersionId) {
        throw "manifest versionId mismatch: expected $ExpectedVersionId got $($manifest.versionId)"
    }
    $readmeManifestRecords = @($manifest.supportFiles | Where-Object { $_.kind -eq 'release-readme' -and $_.path -eq 'README.md' })
    if ($readmeManifestRecords.Count -ne 1) {
        throw 'manifest supportFiles must contain exactly one release-readme entry for README.md'
    }

    $readmeText = Get-Content -LiteralPath (Join-Path $expandedDir 'README.md') -Raw -Encoding UTF8
    $readme = [pscustomobject][ordered]@{
        hasLaunchCommand = ($readmeText -match 'Start-TalkDesktop\.ps1')
        hasHotkeyProbeCommand = ($readmeText -match 'Invoke-TalkDesktopLiveHotkeyProbe\.ps1')
        hasAudioOverridePath = ($readmeText -match 'AudioOverridePath')
        hasMarkdownFences = ($readmeText -match '```powershell')
        hasEscapedBacktickDamage = (Test-TalkReleaseBundleReadmeBacktickDamage -ReadmeText $readmeText)
    }
    if (-not $readme.hasLaunchCommand -or -not $readme.hasHotkeyProbeCommand -or -not $readme.hasAudioOverridePath -or -not $readme.hasMarkdownFences) {
        throw 'README.md is missing required launch, hotkey probe, audio override, or markdown fence content'
    }
    if ($readme.hasEscapedBacktickDamage) {
        throw 'README.md appears to contain PowerShell backtick escape damage'
    }

    $checksumFailures = New-Object 'System.Collections.Generic.List[string]'
    $checked = 0
    Get-Content -LiteralPath (Join-Path $expandedDir 'checksums.sha256') -Encoding UTF8 | ForEach-Object {
        $line = [string]$_
        if ([string]::IsNullOrWhiteSpace($line)) {
            return
        }
        if ($line -notmatch '^(.+?)\s+([0-9a-fA-F]{64})$') {
            $checksumFailures.Add("malformed checksum line: $line") | Out-Null
            return
        }
        $relativePath = $Matches[1].Trim()
        $expectedHash = $Matches[2].ToLowerInvariant()
        $filePath = Join-Path $expandedDir $relativePath
        if (-not (Test-Path -LiteralPath $filePath -PathType Leaf)) {
            $checksumFailures.Add("missing checksum file: $relativePath") | Out-Null
            return
        }
        $actualHash = (Get-FileHash -LiteralPath $filePath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actualHash -ne $expectedHash) {
            $checksumFailures.Add("hash mismatch: $relativePath expected=$expectedHash actual=$actualHash") | Out-Null
            return
        }
        $script:checked++
    }
    if ($checksumFailures.Count -gt 0) {
        throw ('checksum verification failed: ' + ($checksumFailures.ToArray() -join '; '))
    }

    [pscustomobject][ordered]@{
        status = 'passed'
        zipPath = $resolvedZipPath
        expandedDir = $expandedDir
        versionId = [string]$manifest.versionId
        checkedFiles = $checked
        failureCount = 0
        failures = @()
        requiredFilesMissing = $missing.ToArray()
        manifest = [pscustomobject][ordered]@{
            schemaVersion = $manifest.schemaVersion
            app = [string]$manifest.app
            hasReleaseReadme = ($readmeManifestRecords.Count -eq 1)
        }
        readme = $readme
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    Test-TalkReleaseBundleZip `
        -ZipPath $ZipPath `
        -ExpectedVersionId $ExpectedVersionId `
        -WorkRoot $WorkRoot
}