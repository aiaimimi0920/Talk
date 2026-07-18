$talkRoot = Split-Path (Split-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) -Parent) -Parent
$buildWorkflowPath = Join-Path $talkRoot '.github\workflows\build-talk.yml'
$releaseWorkflowPath = Join-Path $talkRoot '.github\workflows\release-talk-tag.yml'

function Read-TalkWorkflowText {
    param([Parameter(Mandatory = $true)][string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        return ''
    }

    Get-Content -LiteralPath $Path -Raw -Encoding UTF8
}

Describe 'Talk GitHub Actions contracts' {
    It 'ships the main branch Windows build workflow' {
        Test-Path -LiteralPath $buildWorkflowPath | Should Be $true

        $workflow = Read-TalkWorkflowText -Path $buildWorkflowPath
        $workflow | Should Match 'name:\s*Build Talk'
        $workflow | Should Match 'branches:\s*\r?\n\s*- main'
        $workflow | Should Match 'workflow_dispatch:'
        $workflow | Should Match 'permissions:\s*\r?\n\s*contents:\s*read'
        $workflow | Should Match 'runs-on:\s*windows-latest'
        $workflow | Should Match 'actions/checkout@v5'
        $workflow | Should Match 'dtolnay/rust-toolchain@1\.95\.0'
        $workflow | Should Match 'Swatinem/rust-cache@v2'
        $workflow | Should Match 'cargo fmt --all -- --check'
        $workflow | Should Match 'cargo check --workspace --all-targets'
        $workflow | Should Match 'cargo test --workspace'
        $workflow | Should Match 'Publish-TalkRelease\.ps1'
        $workflow | Should Match '-DisablePackagedApiKeyDiscovery'
        $workflow | Should Match '-SkipVerification'
        $workflow | Should Match '-SkipSmoke'
        $workflow | Should Match '-SkipNativePreflight'
        $workflow | Should Match '-SkipNativeReadiness'
        $workflow | Should Match 'api_key_env'
        $workflow | Should Match 'actions/upload-artifact@v6'
    }

    It 'ships the Vx.x.x Windows tag release workflow' {
        Test-Path -LiteralPath $releaseWorkflowPath | Should Be $true

        $workflow = Read-TalkWorkflowText -Path $releaseWorkflowPath
        $workflow | Should Match 'name:\s*Release Talk Tag'
        $workflow | Should Match "'V\*\.\*\.\*'"
        $workflow | Should Match 'workflow_dispatch:'
        $workflow | Should Match 'contents:\s*write'
        $workflow | Should Match 'runs-on:\s*windows-latest'
        $workflow | Should Match 'actions/checkout@v5'
        $workflow | Should Match 'dtolnay/rust-toolchain@1\.95\.0'
        $workflow | Should Match 'Install-Module Pester'
        $workflow | Should Match 'GitHub-Actions\.Tests\.ps1'
        $workflow | Should Match 'Publish-TalkRelease\.ps1'
        $workflow | Should Match '-DisablePackagedApiKeyDiscovery'
        $workflow | Should Match 'Test-TalkReleaseManifest\.ps1'
        $workflow | Should Match 'Test-TalkReleaseSummary\.ps1'
        $workflow | Should Match 'Compress-Archive'
        $workflow | Should Match 'softprops/action-gh-release@v3'
        $workflow | Should Match '\^V\\d\+\\\.\\d\+\\\.\\d\+\$'
        $workflow | Should Match 'api_key_env'
    }

    It 'never stores provider or GitHub credentials in workflow source' {
        $workflowText = (Read-TalkWorkflowText -Path $buildWorkflowPath) + "`n" +
            (Read-TalkWorkflowText -Path $releaseWorkflowPath)

        $workflowText | Should Not Match 'github_pat_'
        $workflowText | Should Not Match 'ghp_[A-Za-z0-9]+'
        $workflowText | Should Not Match 'sk-[A-Za-z0-9_-]{12,}'
        $workflowText | Should Not Match 'api_key\s*='
    }

    It 'keeps release contract fixtures trackable while ignoring only the root release directory' {
        $gitignore = Get-Content -LiteralPath (Join-Path $talkRoot '.gitignore') -Raw -Encoding UTF8
        $gitignore | Should Match '(?m)^/release/$'
        $gitignore | Should Not Match '(?m)^release/$'

        foreach ($relativePath in @(
            'contracts/release/examples/talk-release-manifest.json',
            'contracts/release/examples/talk-release-summary.json',
            'contracts/release/manifest.schema.json',
            'contracts/release/summary.schema.json'
        )) {
            Test-Path -LiteralPath (Join-Path $talkRoot $relativePath) | Should Be $true
        }
    }
}
