$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptsRoot = Split-Path $here -Parent
$talkRoot = Split-Path $scriptsRoot -Parent
$summaryScriptPath = Join-Path $scriptsRoot 'Get-TalkReleaseSummary.ps1'
$manifestValidatorScriptPath = Join-Path $scriptsRoot 'Test-TalkReleaseManifest.ps1'
$manifestFixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-manifest.json'
$summaryFixturePath = Join-Path $talkRoot 'contracts\release\examples\talk-release-summary.json'

. $manifestValidatorScriptPath
. $summaryScriptPath

Describe 'Get-TalkReleaseSummary' {
    It 'maps the canonical release manifest fixture into the canonical summary fixture' {
        $manifest = Get-Content -Raw $manifestFixturePath | ConvertFrom-Json
        $expected = Get-Content -Raw $summaryFixturePath | ConvertFrom-Json | ConvertTo-Json -Depth 8 -Compress

        $summary = New-TalkReleaseSummaryObjectFromManifest -Manifest $manifest
        $actual = $summary | ConvertTo-Json -Depth 8 -Compress

        $summary.schemaVersion | Should Be 1
        $summary.manifestSchemaVersion | Should Be 2
        $actual | Should Be $expected
    }

    It 'rejects invalid release manifests before deriving a summary' {
        $manifest = Get-Content -Raw $manifestFixturePath | ConvertFrom-Json
        $manifest.schemaVersion = 1

        {
            New-TalkReleaseSummaryObjectFromManifest -Manifest $manifest
        } | Should Throw
    }
}
