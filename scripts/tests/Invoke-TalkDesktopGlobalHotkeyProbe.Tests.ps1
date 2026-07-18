$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopGlobalHotkeyProbe.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopGlobalHotkeyProbe helpers' {
    It 'builds a concise global hotkey probe summary from a smoke result' {
        $summary = New-TalkDesktopGlobalHotkeyProbeSummary `
            -SmokeRoot 'C:\Talk\.runtime\global-hotkey-probe' `
            -Result ([pscustomobject]@{
                Scenario = 'openai-compatible-audio-input-insert-success'
                BinaryPath = 'C:\Release\talk-desktop.exe'
                ConfigPath = 'C:\Talk\.runtime\global-hotkey-probe\config.toml'
                Status = 'completed'
                CapturedText = 'assistant reply from audio input chat'
                LogPath = 'C:\Talk\.runtime\global-hotkey-probe\logs\session.json'
                ProviderRequestsPath = 'C:\Talk\.runtime\global-hotkey-probe\provider-requests.json'
            })

        $summary.scenario | Should Be 'openai-compatible-audio-input-insert-success'
        $summary.binaryPath | Should Be 'C:\Release\talk-desktop.exe'
        $summary.configPath | Should Be 'C:\Talk\.runtime\global-hotkey-probe\config.toml'
        $summary.status | Should Be 'completed'
        $summary.capturedText | Should Be 'assistant reply from audio input chat'
        $summary.logPath | Should Be 'C:\Talk\.runtime\global-hotkey-probe\logs\session.json'
        $summary.providerRequestsPath | Should Be 'C:\Talk\.runtime\global-hotkey-probe\provider-requests.json'
        $summary.snapshotPath | Should Be 'C:\Talk\.runtime\global-hotkey-probe\openai-compatible-audio-input-insert-success\text-target\snapshot.txt'
    }

    It 'dispatches the insert smoke scenario and writes a summary file' {
        $tempRoot = Join-Path $env:TEMP ('talk-global-hotkey-probe-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Invoke-TalkDesktopReleaseSmoke {
                param([string]$BinaryPath, [string]$ReleaseDir, [string]$SmokeRoot, [string[]]$Scenario)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = if ($BinaryPath) { $BinaryPath } else { (Join-Path $ReleaseDir 'talk-desktop.exe') }
                    ConfigPath = (Join-Path $SmokeRoot 'openai-compatible-audio-input-insert-success\config.toml')
                    Status = 'completed'
                    CapturedText = 'assistant reply from audio input chat'
                    LogPath = (Join-Path $SmokeRoot 'openai-compatible-audio-input-insert-success\logs\session.json')
                    ProviderRequestsPath = (Join-Path $SmokeRoot 'openai-compatible-audio-input-insert-success\provider-requests.json')
                }
            }

            $summary = Invoke-TalkDesktopGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot

            $summary.scenario | Should Be 'openai-compatible-audio-input-insert-success'
            $summary.status | Should Be 'completed'
            $summary.summaryPath | Should Be (Join-Path $tempRoot 'global-hotkey-probe-summary.json')
            (Test-Path -LiteralPath $summary.summaryPath) | Should Be $true

            $written = Get-Content -LiteralPath $summary.summaryPath -Raw | ConvertFrom-Json
            $written.scenario | Should Be 'openai-compatible-audio-input-insert-success'
            $written.status | Should Be 'completed'
            $written.capturedText | Should Be 'assistant reply from audio input chat'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
