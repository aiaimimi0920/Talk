$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopLiveOperatorProbe.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopLiveOperatorProbe helpers' {
    It 'derives the release-local desktop logs directory from an effective config path' {
        $logsDir = Get-TalkDesktopLiveOperatorLogsDir -EffectiveConfigPath 'C:\Release\talk-desktop.runtime-launch.toml'

        $logsDir | Should Be 'C:\Release\.runtime\talk-desktop\logs'
    }

    It 'builds a concise live operator probe summary' {
        $summary = New-TalkDesktopLiveOperatorProbeSummary `
            -SmokeRoot 'C:\Talk\.runtime\live-operator' `
            -LaunchSummary ([pscustomobject]@{
                releaseDir = 'C:\Release'
                binaryPath = 'C:\Release\talk-desktop.exe'
                baseConfigPath = 'C:\Release\talk-desktop.toml'
                effectiveConfigPath = 'C:\Release\talk-desktop.runtime-launch.toml'
                processId = 12345
            }) `
            -Session ([pscustomobject]@{
                status = 'completed'
                transcript = 'What is the capital of France?'
                output_text = 'Paris'
            }) `
            -LogPath 'C:\Release\.runtime\talk-desktop\logs\session.json' `
            -CapturedText 'Paris' `
            -InputDevice 'Virtual Mic' `
            -AudioProbe ([pscustomobject]@{
                requestedInputDevice = 'Virtual Mic'
                selectedInputDevice = 'Virtual Mic'
                peak = 0.25
                rms = 0.125
                silent = $false
            })

        $summary.status | Should Be 'completed'
        $summary.transcript | Should Be 'What is the capital of France?'
        $summary.outputText | Should Be 'Paris'
        $summary.capturedText | Should Be 'Paris'
        $summary.binaryPath | Should Be 'C:\Release\talk-desktop.exe'
        $summary.effectiveConfigPath | Should Be 'C:\Release\talk-desktop.runtime-launch.toml'
        $summary.logPath | Should Be 'C:\Release\.runtime\talk-desktop\logs\session.json'
        $summary.inputDevice | Should Be 'Virtual Mic'
        $summary.audioProbe.selectedInputDevice | Should Be 'Virtual Mic'
        $summary.audioProbe.peak | Should Be 0.25
        $summary.snapshotPath | Should Be 'C:\Talk\.runtime\live-operator\text-target\snapshot.txt'
    }

    It 'treats silent live-operator audio probe summaries as unusable input' {
        $silent = Test-TalkDesktopLiveOperatorAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0
            silent = $true
        })
        $audible = Test-TalkDesktopLiveOperatorAudioProbeHasSignal -ProbeSummary ([pscustomobject]@{
            peak = 0.2
            silent = $false
        })

        $silent | Should Be $false
        $audible | Should Be $true
    }

    It 'waits for the completed session output text to land in the foreground target' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-operator-insert-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot '.runtime\talk-desktop\logs'
            New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris.","error":null}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            $waitedExpectedText = $null

            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            }
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Start-TalkDesktop {
                [pscustomobject]@{
                    releaseDir = 'C:\Release'
                    binaryPath = 'C:\Release\talk-desktop.exe'
                    baseConfigPath = 'C:\Release\talk-desktop.toml'
                    effectiveConfigPath = (Join-Path $tempRoot 'talk-desktop.runtime-launch.toml')
                    processId = 12345
                }
            }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Wait-LatestSessionLog { Get-Item -LiteralPath $logPath }
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
                $script:waitedExpectedText = $ExpectedText
                'Paris.'
            }
            Mock Wait-TalkTextCaptureContains { 'Paris' }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopLiveOperatorProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -TimeoutSeconds 5 `
                -ExpectedText 'Paris'

            $summary.status | Should Be 'completed'
            $summary.capturedText | Should Be 'Paris.'
            $script:waitedExpectedText | Should Be 'Paris.'
            Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly
            Assert-MockCalled Wait-TalkTextCaptureContains -Times 0 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'can skip native audio preflight and wait directly for a manual hotkey-driven session' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-operator-skip-preflight-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logsDir = Join-Path $tempRoot '.runtime\talk-desktop\logs'
            New-Item -ItemType Directory -Path $logsDir -Force | Out-Null
            $logPath = Join-Path $logsDir 'session.json'
            '{"status":"completed","transcript":"测试成功","output_text":"测试成功","error":null}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            }
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Invoke-TalkDesktopLiveOperatorAudioProbe {
                throw 'audio preflight should be skipped'
            }
            Mock Start-TalkDesktop {
                [pscustomobject]@{
                    releaseDir = 'C:\Release'
                    binaryPath = 'C:\Release\talk-desktop.exe'
                    baseConfigPath = 'C:\Release\talk-desktop.toml'
                    effectiveConfigPath = (Join-Path $tempRoot 'talk-desktop.runtime-launch.toml')
                    processId = 12345
                }
            }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Wait-LatestSessionLog { Get-Item -LiteralPath $logPath }
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { '测试成功' }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopLiveOperatorProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -TimeoutSeconds 5 `
                -ExpectedText '测试成功' `
                -InputDevice '麦克风' `
                -SkipAudioProbe

            $summary.status | Should Be 'completed'
            $summary.capturedText | Should Be '测试成功'
            $summary.inputDevice | Should Be '麦克风'
            $summary.audioProbe | Should Be $null
            Assert-MockCalled Invoke-TalkDesktopLiveOperatorAudioProbe -Times 0 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'writes a structured summary when no session log arrives before timeout' {
        $tempRoot = Join-Path $env:TEMP ('talk-live-operator-timeout-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = (Join-Path $tempRoot 'text-target\snapshot.txt')
                    WindowTitle = 'Talk text target'
                }
            }
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Start-TalkDesktop {
                [pscustomobject]@{
                    releaseDir = 'C:\Release'
                    binaryPath = 'C:\Release\talk-desktop.exe'
                    baseConfigPath = 'C:\Release\talk-desktop.toml'
                    effectiveConfigPath = (Join-Path $tempRoot 'talk-desktop.runtime-launch.toml')
                    processId = 12345
                }
            }
            Mock Get-Process { [pscustomobject]@{ Id = 12345 } }
            Mock Find-WindowByProcessIdAndClass { [IntPtr]::new(2) }
            Mock Wait-LatestSessionLog { throw 'session log timeout' }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            {
                Invoke-TalkDesktopLiveOperatorProbe `
                    -BinaryPath 'C:\Release\talk-desktop.exe' `
                    -ReleaseDir 'C:\Release' `
                    -SmokeRoot $tempRoot `
                    -TimeoutSeconds 5
            } | Should Throw 'session log timeout'

            $summaryPath = Join-Path $tempRoot 'live-operator-probe-summary.json'
            Test-Path -LiteralPath $summaryPath | Should Be $true

            $summary = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $summary.status | Should Be 'failed'
            $summary.failureReason | Should Be 'session log timeout'
            $summary.logPath | Should Be ''
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
