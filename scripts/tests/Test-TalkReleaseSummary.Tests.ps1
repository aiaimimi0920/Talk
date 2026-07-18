$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptsRoot = Split-Path $here -Parent
$talkRoot = Split-Path $scriptsRoot -Parent
$summaryScriptPath = Join-Path $scriptsRoot 'Get-TalkReleaseSummary.ps1'
$summaryValidatorScriptPath = Join-Path $scriptsRoot 'Test-TalkReleaseSummary.ps1'
$summaryFixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-summary.json'

. $summaryScriptPath
. $summaryValidatorScriptPath

Describe 'Test-TalkReleaseSummary' {
    It 'accepts the canonical Talk release summary fixture' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context $summaryFixturePath
        } | Should Not Throw
    }

    It 'rejects an outdated release summary schema version' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json
        $summary.schemaVersion = 0

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'summary-with-old-schema'
        } | Should Throw
    }

    It 'rejects malformed desktop smoke scenario summaries' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json
        $summary.desktopSmoke.scenarios[0].snapshot = 'bad-snapshot'

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'summary-with-bad-snapshot'
        } | Should Throw
    }

    It 'rejects desktop smoke failure metadata when it is not string-like' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json
        $summary.desktopSmoke.scenarios[0] | Add-Member -NotePropertyName failureKind -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'summary-with-bad-failure-kind'
        } | Should Throw
    }

    It 'rejects desktop smoke retry metadata when the types are invalid' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json
        $summary.desktopSmoke.scenarios[0] | Add-Member -NotePropertyName retryCount -NotePropertyValue 'one' -Force
        $summary.desktopSmoke.scenarios[0] | Add-Member -NotePropertyName retryReason -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'summary-with-bad-retry-fields'
        } | Should Throw
    }

    It 'rejects insert-target diagnostic paths when they are not strings' {
        $summary = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json
        $summary.desktopSmoke.scenarios[0] | Add-Member -NotePropertyName insertTargetDiagnosticPath -NotePropertyValue 42 -Force

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'summary-with-bad-insert-target-diagnostic-path'
        } | Should Throw
    }

    It 'accepts the derived summary emitted from the canonical manifest fixture' {
        $manifestFixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-manifest.json'
        $manifest = Get-Content -Raw $manifestFixturePath | ConvertFrom-Json
        $summary = New-TalkReleaseSummaryObjectFromManifest -Manifest $manifest

        {
            Assert-TalkReleaseSummaryObject -Summary $summary -Context 'derived-summary'
        } | Should Not Throw
    }
}
