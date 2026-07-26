$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Test-TalkReleaseBundleZip.ps1'

. $scriptPath

function New-TestTalkReleaseZipFixture {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [string]$VersionId = 'talk-desktop-bundle-test'
    )

    $bundleDir = Join-Path $Root $VersionId
    $internalDir = Join-Path $bundleDir '.internal'
    New-Item -ItemType Directory -Path $internalDir -Force | Out-Null

    $files = [ordered]@{
        'README.md' = @"
# Talk Desktop Release

Version: $VersionId

- ``talk-desktop.exe``

``````powershell
.\Start-TalkDesktop.ps1 -ReleaseDir . -InputDevice '麦克风'
.\Invoke-TalkDesktopLiveHotkeyProbe.ps1 -ReleaseDir . -AudioOverridePath 'C:\path\to\known-good.wav' -ExpectedText ''
``````

Use ``api_key`` only for packaged credentials. Successful insertion uses ``clipboard_paste``. Verify ``checksums.sha256``.
"@
        'manifest.json' = @"
{
  "schemaVersion": 2,
  "app": "Talk",
  "sourceProject": "Talk",
  "versionId": "$VersionId",
  "supportFiles": [
    { "kind": "release-summary", "path": "release-summary.json" },
    { "kind": "release-readme", "path": "README.md" }
  ],
  "checksums": "checksums.sha256"
}
"@
        'release-summary.json' = @"
{
  "schemaVersion": 1,
  "app": "Talk",
  "versionId": "$VersionId",
  "manifestPath": "manifest.json",
  "checksumPath": "checksums.sha256"
}
"@
        'talk-desktop.exe' = 'desktop exe'
        'talk-desktop.toml' = 'voice_mode = "transcribe"'
        'Start-TalkDesktop.ps1' = 'function Start-TalkDesktop {}'
        'Invoke-TalkDesktopLiveHotkeyProbe.ps1' = 'function Invoke-TalkDesktopLiveHotkeyProbe {}'
        'Invoke-TalkDesktopReleaseSmoke.ps1' = 'function Start-TalkTextCaptureTarget {}'
        '.internal\talk.exe' = 'talk cli'
    }

    foreach ($entry in $files.GetEnumerator()) {
        $path = Join-Path $bundleDir $entry.Key
        New-Item -ItemType Directory -Path (Split-Path -Parent $path) -Force | Out-Null
        Set-Content -LiteralPath $path -Value $entry.Value -Encoding UTF8
    }

    $checksumLines = Get-ChildItem -LiteralPath $bundleDir -File -Recurse |
        Where-Object { $_.Name -ne 'checksums.sha256' } |
        Sort-Object FullName |
        ForEach-Object {
            $relative = $_.FullName.Substring($bundleDir.Length).TrimStart('\')
            $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            "$relative  $hash"
        }
    Set-Content -LiteralPath (Join-Path $bundleDir 'checksums.sha256') -Value ($checksumLines -join [Environment]::NewLine) -Encoding UTF8

    $zipPath = Join-Path $Root ($VersionId + '.zip')
    Compress-Archive -LiteralPath $bundleDir -DestinationPath $zipPath -Force
    [pscustomobject]@{ BundleDir = $bundleDir; ZipPath = $zipPath; VersionId = $VersionId }
}

Describe 'Test-TalkReleaseBundleZip' {
    It 'validates a release zip with README, manifest, checksums, and entrypoint files' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-zip-test-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $fixture = New-TestTalkReleaseZipFixture -Root $tempRoot

            $result = Test-TalkReleaseBundleZip -ZipPath $fixture.ZipPath -ExpectedVersionId $fixture.VersionId

            $result.status | Should Be 'passed'
            $result.versionId | Should Be $fixture.VersionId
            $result.checkedFiles | Should BeGreaterThan 8
            $result.failureCount | Should Be 0
            $result.readme.hasLaunchCommand | Should Be $true
            $result.readme.hasHotkeyProbeCommand | Should Be $true
            $result.readme.hasAudioOverridePath | Should Be $true
            $result.readme.hasMarkdownFences | Should Be $true
            $result.readme.hasEscapedBacktickDamage | Should Be $false
            $result.manifest.hasReleaseReadme | Should Be $true
            $result.requiredFilesMissing.Count | Should Be 0
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a zip whose README has PowerShell backtick escape damage' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-zip-test-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $fixture = New-TestTalkReleaseZipFixture -Root $tempRoot
            $readmePath = Join-Path $fixture.BundleDir 'README.md'
            Set-Content -LiteralPath $readmePath -Value "- `talk-desktop.exe` damaged by PowerShell escape" -Encoding UTF8
            $checksumLines = Get-ChildItem -LiteralPath $fixture.BundleDir -File -Recurse |
                Where-Object { $_.Name -ne 'checksums.sha256' } |
                Sort-Object FullName |
                ForEach-Object {
                    $relative = $_.FullName.Substring($fixture.BundleDir.Length).TrimStart('\')
                    $hash = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
                    "$relative  $hash"
                }
            Set-Content -LiteralPath (Join-Path $fixture.BundleDir 'checksums.sha256') -Value ($checksumLines -join [Environment]::NewLine) -Encoding UTF8
            Compress-Archive -LiteralPath $fixture.BundleDir -DestinationPath $fixture.ZipPath -Force

            { Test-TalkReleaseBundleZip -ZipPath $fixture.ZipPath -ExpectedVersionId $fixture.VersionId } |
                Should Throw 'README.md appears to contain PowerShell backtick escape damage'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'rejects a zip with a checksum mismatch' {
        $tempRoot = Join-Path $env:TEMP ('talk-release-zip-test-' + [guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $tempRoot -Force | Out-Null
        try {
            $fixture = New-TestTalkReleaseZipFixture -Root $tempRoot
            Add-Content -LiteralPath (Join-Path $fixture.BundleDir 'Start-TalkDesktop.ps1') -Value '# changed after checksum'
            Compress-Archive -LiteralPath $fixture.BundleDir -DestinationPath $fixture.ZipPath -Force

            { Test-TalkReleaseBundleZip -ZipPath $fixture.ZipPath -ExpectedVersionId $fixture.VersionId } |
                Should Throw 'checksum verification failed'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}