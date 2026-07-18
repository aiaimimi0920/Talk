$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptsRoot = Split-Path $here -Parent
$talkRoot = Split-Path $scriptsRoot -Parent
$validatorScriptPath = Join-Path $scriptsRoot 'Test-TalkReleaseManifest.ps1'
$publishScriptPath = Join-Path $scriptsRoot 'Publish-TalkRelease.ps1'
$fixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-manifest.json'

. $validatorScriptPath
. $publishScriptPath

Describe 'Test-TalkReleaseManifest' {
    It 'accepts the canonical Talk release manifest fixture' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context $fixturePath
        } | Should Not Throw
    }

    It 'rejects an outdated Talk release schema version' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json
        $manifest.schemaVersion = 1

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'fixture-with-old-schema'
        } | Should Throw
    }

    It 'rejects desktop smoke snapshots that are not objects' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json
        $manifest.desktopSmoke[0].statusSnapshot = 'bad-snapshot'

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'fixture-with-bad-snapshot'
        } | Should Throw
    }

    It 'rejects desktop smoke failure metadata when it is not string-like' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json
        $manifest.desktopSmoke[0] | Add-Member -NotePropertyName failureKind -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'fixture-with-bad-failure-kind'
        } | Should Throw
    }

    It 'rejects desktop smoke retry metadata when the types are invalid' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json
        $manifest.desktopSmoke[0] | Add-Member -NotePropertyName retryCount -NotePropertyValue 'one' -Force
        $manifest.desktopSmoke[0] | Add-Member -NotePropertyName retryReason -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'fixture-with-bad-retry-fields'
        } | Should Throw
    }

    It 'rejects insert-target diagnostic paths when they are not strings' {
        $manifest = Get-Content -Raw $fixturePath | ConvertFrom-Json
        $manifest.desktopSmoke[0] | Add-Member -NotePropertyName insertTargetDiagnosticPath -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'fixture-with-bad-insert-target-diagnostic-path'
        } | Should Throw
    }

    It 'accepts the current manifest object emitted by Publish-TalkRelease helpers' {
        $manifest = New-TalkReleaseManifestObject `
            -VersionId 'desktop-shell-validator-v2' `
            -BuiltAt '2026-07-05T05:00:00+08:00' `
            -RepoRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro' `
            -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
            -DestinationDir 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-validator-v2' `
            -CommandRecords @() `
            -ExeRecords @(
                [pscustomobject]@{
                    kind = 'exe'
                    name = 'talk-desktop.exe'
                    path = 'talk-desktop.exe'
                    bytes = 2
                    sha256 = 'sha-talk-desktop'
                }
            ) `
            -BuildLogRecords @() `
            -NativePreflightRecords @() `
            -SmokeResults @(
                [pscustomobject]@{
                    Scenario = 'cancel-and-status'
                    BinaryPath = 'C:\Release\talk-desktop.exe'
                    ConfigPath = 'C:\Talk\.runtime\cancel\config.toml'
                    LogPath = 'C:\Talk\.runtime\cancel\session.json'
                    Status = 'cancelled'
                    DialogText = "Current: Talk: idle`nHotkey: Ctrl+Alt+F24"
                    StatusKind = 'idle'
                    StatusSummary = 'current=Talk: idle; hotkey=Ctrl+Alt+F24'
                    StatusFields = [ordered]@{
                        Current = 'Talk: idle'
                        Hotkey = 'Ctrl+Alt+F24'
                    }
                    StatusSnapshot = [ordered]@{
                        current = 'Talk: idle'
                        hotkey = 'Ctrl+Alt+F24'
                    }
                }
            ) `
            -NativeReadinessResult ([pscustomobject]@{
                ConfigPath = 'C:\Talk\.runtime\native-readiness\config.toml'
                EvidencePath = 'C:\Talk\.runtime\native-readiness\readiness.json'
                AudioStatus = 'ready'
                AudioReason = $null
                AudioDeviceName = 'Microphone Array'
                AudioDefaultSampleRateHz = 48000
                AudioDefaultChannels = 2
                AudioSampleFormat = 'f32'
                ClipboardStatus = 'ready'
                ClipboardReason = $null
                OutputText = '{"app":"talk","allReady":true}'
            })

        {
            Assert-TalkReleaseManifestObject -Manifest $manifest -Context 'generated-manifest'
        } | Should Not Throw
    }
}
