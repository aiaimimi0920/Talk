$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopQwenGlobalHotkeyProbe helpers' {
    It 'prefers an explicit api key over json file and environment fallback' {
        $env:TALK_PROVIDER_API_KEY = 'env-key'
        try {
            $tempRoot = Join-Path $env:TEMP ('talk-qwen-key-test-' + [guid]::NewGuid().ToString())
            New-Item -ItemType Directory -Path $tempRoot | Out-Null
            $jsonPath = Join-Path $tempRoot 'manual-live.json'
            '{"apiKey":"json-key"}' | Set-Content -LiteralPath $jsonPath -Encoding UTF8

            $resolved = Resolve-TalkDesktopQwenProbeApiKey `
                -ApiKey 'direct-key' `
                -ApiKeyJsonPath $jsonPath

            $resolved | Should Be 'direct-key'
        }
        finally {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reads api key from json file when direct api key is omitted' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-key-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $jsonPath = Join-Path $tempRoot 'manual-live.json'
            '{"apiKey":"json-key"}' | Set-Content -LiteralPath $jsonPath -Encoding UTF8

            $resolved = Resolve-TalkDesktopQwenProbeApiKey -ApiKeyJsonPath $jsonPath

            $resolved | Should Be 'json-key'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'auto-discovers a local qwen dashscope key when direct settings are omitted' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-key-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        $originalUserProfile = $env:USERPROFILE
        $originalHome = $env:HOME
        $originalTalkProviderApiKey = $env:TALK_PROVIDER_API_KEY
        try {
            Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            $env:USERPROFILE = $tempRoot
            $env:HOME = $tempRoot

            $credentialDir = Join-Path $tempRoot '.neuro\qwen-platform\qwen-dashscope-openai\api-key'
            New-Item -ItemType Directory -Path $credentialDir -Force | Out-Null
            '{"apiKey":"auto-json-key"}' | Set-Content -LiteralPath (Join-Path $credentialDir 'manual-live.json') -Encoding UTF8

            $resolved = Resolve-TalkDesktopQwenProbeApiKey

            $resolved | Should Be 'auto-json-key'
        }
        finally {
            if ($null -eq $originalTalkProviderApiKey) {
                Remove-Item Env:TALK_PROVIDER_API_KEY -ErrorAction SilentlyContinue
            } else {
                $env:TALK_PROVIDER_API_KEY = $originalTalkProviderApiKey
            }
            $env:USERPROFILE = $originalUserProfile
            $env:HOME = $originalHome
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'builds a concise qwen global hotkey probe summary with insert-target diagnostic hints' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-summary-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $diagnosticPath = Join-Path $tempRoot 'session.desktop-insert-target.json'
            [System.IO.File]::WriteAllText(
                $diagnosticPath,
                (([pscustomobject]@{
                    outputStrategy = 'show_copy_popup_only'
                    focusLooksEditable = $false
                    focusClassName = 'Chrome_RenderWidgetHostHWND'
                    automationControlType = 'document'
                    automationFrameworkId = 'Chrome'
                } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                (New-Object System.Text.UTF8Encoding($false))
            )

            $summary = New-TalkDesktopQwenGlobalHotkeyProbeSummary `
                -SmokeRoot 'C:\Talk\.runtime\qwen-probe' `
                -Session ([pscustomobject]@{
                    status = 'completed'
                    transcript = 'What is the capital of France?'
                    output_text = 'Paris'
                }) `
                -ConfigPath 'C:\Talk\.runtime\qwen-probe\config.toml' `
                -LogPath 'C:\Talk\.runtime\qwen-probe\logs\session.json' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav' `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -InsertTargetDiagnosticPath $diagnosticPath

            $summary.status | Should Be 'completed'
            $summary.transcript | Should Be 'What is the capital of France?'
            $summary.outputText | Should Be 'Paris'
            $summary.binaryPath | Should Be 'C:\Release\talk-desktop.exe'
            $summary.snapshotPath | Should Be 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
            $summary.audioOverridePath | Should Be 'C:\Talk\.runtime\qwen-probe\probe.wav'
            $summary.insertTargetDiagnosticPath | Should Be $diagnosticPath
            $summary.insertTargetOutputStrategy | Should Be 'show_copy_popup_only'
            $summary.insertTargetFocusLooksEditable | Should Be $false
            $summary.insertTargetFocusClassName | Should Be 'Chrome_RenderWidgetHostHWND'
            $summary.insertTargetAutomationControlType | Should Be 'document'
            $summary.insertTargetAutomationFrameworkId | Should Be 'Chrome'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'keeps the text target foreground refreshed while waiting for inserted text' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Invoke-TalkTextCaptureTargetPrimer { '[talk-primer]' }
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Invoke-TalkDesktopPinnedWindowOperation { & $ScriptBlock }
            Mock Select-TalkTextCaptureTargetChildText {}
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { 'Paris' }
            Mock Wait-TalkTextCaptureContains { 'Paris' }
            Mock Wait-LatestSessionLog {
                Get-Item -LiteralPath $logPath
            }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav'

            $summary.outputText | Should Be 'Paris'
            Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly
            Assert-MockCalled Wait-TalkTextCaptureContains -Times 0 -Exactly
            Assert-MockCalled Invoke-TalkTextCaptureTargetPrimer -Times 0 -Exactly
            Assert-MockCalled Set-TalkTextCaptureTargetForeground -Times 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'starts the foreground-refresh text wait before it waits for the session log' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-order-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            $callOrder = New-Object 'System.Collections.Generic.List[string]'

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Invoke-TalkTextCaptureTargetPrimer { '[talk-primer]' }
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Invoke-TalkDesktopPinnedWindowOperation {
                $callOrder.Add('pin') | Out-Null
                & $ScriptBlock
            }
            Mock Select-TalkTextCaptureTargetChildText {}
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
                $callOrder.Add('capture') | Out-Null
                'Paris'
            }
            Mock Wait-LatestSessionLog {
                $callOrder.Add('log') | Out-Null
                Get-Item -LiteralPath $logPath
            }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav'

            $summary.outputText | Should Be 'Paris'
            $callOrder.Count | Should Be 3
            $callOrder[0] | Should Be 'pin'
            $callOrder[1] | Should Be 'capture'
            $callOrder[2] | Should Be 'log'
            Assert-MockCalled Invoke-TalkTextCaptureTargetPrimer -Times 0 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'strips the text-target primer prefix from captured foreground text' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-primer-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Invoke-TalkTextCaptureTargetPrimer { 'talkprimerready' }
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Invoke-TalkDesktopPinnedWindowOperation { & $ScriptBlock }
            Mock Select-TalkTextCaptureTargetChildText {}
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { 'talkprimerreadyParis' }
            Mock Wait-LatestSessionLog {
                Get-Item -LiteralPath $logPath
            }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav' `
                -TextCapturePrimer 'talkprimerready'

            $summary.capturedText | Should Be 'Paris'
            Assert-MockCalled Invoke-TalkTextCaptureTargetPrimer -ParameterFilter { $ChildHwnd -eq [IntPtr]::new(2) }
            Assert-MockCalled Select-TalkTextCaptureTargetChildText -Times 1 -Exactly -ParameterFilter { $ChildHwnd -eq [IntPtr]::new(2) }
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'waits briefly for the exact completed session output when the initial captured text is only a prefix' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-exact-output-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris."}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Invoke-TalkTextCaptureTargetPrimer { '[talk-primer]' }
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Invoke-TalkDesktopPinnedWindowOperation { & $ScriptBlock }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { 'Paris' }
            Mock Wait-TalkTextCaptureContains { 'Paris.' }
            Mock Wait-LatestSessionLog { Get-Item -LiteralPath $logPath }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav'

            $summary.outputText | Should Be 'Paris.'
            $summary.capturedText | Should Be 'Paris.'
            $summary.capturedTextMatchesOutput | Should Be $true
            Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly -Scope It
            Assert-MockCalled Wait-TalkTextCaptureContains -Times 1 -Exactly -ParameterFilter {
                $ExpectedText -eq 'Paris.'
            } -Scope It
            Assert-MockCalled Invoke-TalkTextCaptureTargetPrimer -Times 0 -Exactly -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'marks the probe summary when captured foreground text does not exactly match the completed session output' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-mismatch-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            $diagPath = Join-Path $tempRoot 'logs\session.desktop-insert-target.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"completed","transcript":"What is the capital of France?","output_text":"Paris"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8
            '{}' | Set-Content -LiteralPath $diagPath -Encoding UTF8

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Invoke-TalkDesktopPinnedWindowOperation { & $ScriptBlock }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh { 'headline Paris' }
            Mock Wait-TalkTextCaptureContains { 'headline Paris' }
            Mock Wait-LatestSessionLog { Get-Item -LiteralPath $logPath }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key' `
                -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav'

            $summary.capturedText | Should Be 'headline Paris'
            $summary.capturedTextMatchesOutput | Should Be $false
            $summary.insertTargetDiagnosticPath | Should Be $diagPath
            Assert-MockCalled Wait-TalkTextCaptureContains -Times 1 -Exactly -ParameterFilter {
                $ExpectedText -eq 'Paris'
            } -Scope It
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'throws a structured failed-session payload when the provider session fails before producing output text' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-provider-failed-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $logPath = Join-Path $tempRoot 'logs\session.json'
            $diagPath = Join-Path $tempRoot 'logs\session.desktop-insert-target.json'
            New-Item -ItemType Directory -Path (Split-Path -Parent $logPath) -Force | Out-Null
            '{"status":"failed","transcript":null,"output_text":null,"error":"provider error: upstream unavailable"}' |
                Set-Content -LiteralPath $logPath -Encoding UTF8
            '{}' | Set-Content -LiteralPath $diagPath -Encoding UTF8

            Mock Resolve-TalkDesktopBinaryPath { 'C:\Release\talk-desktop.exe' }
            Mock Resolve-TalkDesktopQwenProbeApiKey { 'talk-test-key' }
            Mock Resolve-TalkDesktopQwenProbeAudioOverridePath { 'C:\Talk\.runtime\qwen-probe\probe.wav' }
            Mock Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {}
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Start-TalkTextCaptureTarget {
                [pscustomobject]@{
                    Hwnd = [IntPtr]::new(1)
                    SnapshotPath = 'C:\Talk\.runtime\qwen-probe\text-target\snapshot.txt'
                    TextBoxHwnd = [IntPtr]::new(2)
                }
            }
            Mock Set-TalkTextCaptureTargetForeground {}
            Mock Start-TalkDesktopSmokeInstance { [pscustomobject]@{ } }
            Mock Invoke-TalkDesktopPinnedWindowOperation { & $ScriptBlock }
            Mock Send-TalkDesktopGlobalHotkeyChord {}
            Mock Find-LatestSessionLogIfAvailable { Get-Item -LiteralPath $logPath }
            Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
                throw 'Talk text capture target did not contain [Paris]. Last text: []'
            }
            Mock Stop-TalkDesktopSmokeInstance {}
            Mock Stop-TalkTextCaptureTarget {}

            $payload = $null
            try {
                Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                    -BinaryPath 'C:\Release\talk-desktop.exe' `
                    -SmokeRoot $tempRoot `
                    -ApiKey 'talk-test-key' `
                    -AudioOverridePath 'C:\Talk\.runtime\qwen-probe\probe.wav'
            }
            catch {
                $payload = ([string]$_.Exception.Message | ConvertFrom-Json)
            }

            $payload | Should Not Be $null
            $payload.status | Should Be 'failed'
            $payload.failureKind | Should Be 'session_failed'
            $payload.failureSummary | Should Match 'provider error: upstream unavailable'
            $payload.insertTargetDiagnosticPath | Should Be $diagPath
            $payload.summaryPath | Should Match 'qwen-global-hotkey-probe-summary\.json'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'pins the text target topmost and selects primer text before the Qwen hotkey sequence' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'Set-TalkTextCaptureTargetForeground -Target \$target'
        $scriptText | Should Match 'Invoke-TalkDesktopPinnedWindowOperation -Hwnd \$target\.Hwnd -ScriptBlock'
        $scriptText | Should Match 'Select-TalkTextCaptureTargetChildText -ChildHwnd \$target\.TextBoxHwnd'
        $scriptText | Should Match 'Invoke-TalkTextCaptureTargetPrimer[\s\S]*-ChildHwnd \$target\.TextBoxHwnd'
    }

    It 'retries one hostile foreground probe failure and keeps the successful retry summary' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-retry-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $script:probeAttemptCalls = 0
            $script:probeAttemptRoots = New-Object 'System.Collections.Generic.List[string]'
            Mock Invoke-TalkDesktopQwenGlobalHotkeyProbeAttempt {
                $script:probeAttemptCalls += 1
                $script:probeAttemptRoots.Add($ResolvedSmokeRoot) | Out-Null
                if ($script:probeAttemptCalls -eq 1) {
                    throw (@{
                        status = 'completed'
                        outputText = 'Paris.'
                        capturedText = 'talkprimerready'
                        capturedTextMatchesOutput = $false
                        smokeRoot = $ResolvedSmokeRoot
                        summaryPath = (Join-Path $ResolvedSmokeRoot 'qwen-global-hotkey-probe-summary.json')
                        failureKind = 'hostile_foreground_environment'
                        failureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                        failureEvidencePath = (Join-Path $ResolvedSmokeRoot 'failure-diagnostic.json')
                    } | ConvertTo-Json -Compress)
                }

                [pscustomobject]@{
                    status = 'completed'
                    transcript = 'What is the capital of France?'
                    outputText = 'Paris.'
                    capturedText = 'Paris.'
                    capturedTextMatchesOutput = $true
                    smokeRoot = $ResolvedSmokeRoot
                    summaryPath = (Join-Path $ResolvedSmokeRoot 'qwen-global-hotkey-probe-summary.json')
                }
            }

            $summary = Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                -BinaryPath 'C:\Release\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -ApiKey 'talk-test-key'

            $summary.status | Should Be 'completed'
            $summary.retryCount | Should Be 1
            $summary.retryReason | Should Be 'hostile_foreground_environment'
            $script:probeAttemptCalls | Should Be 2
            $script:probeAttemptRoots | Should Be @(
                (Join-Path $tempRoot 'attempt-01'),
                (Join-Path $tempRoot 'attempt-02')
            )
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Variable -Name probeAttemptCalls -Scope Script -ErrorAction SilentlyContinue
            Remove-Variable -Name probeAttemptRoots -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'throws a structured hostile foreground payload when the retry still fails' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-retry-fail-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $script:probeAttemptCalls = 0
            Mock Invoke-TalkDesktopQwenGlobalHotkeyProbeAttempt {
                $script:probeAttemptCalls += 1
                throw (@{
                    status = 'completed'
                    outputText = 'Paris.'
                    capturedText = 'talkprimerready'
                    capturedTextMatchesOutput = $false
                    smokeRoot = $ResolvedSmokeRoot
                    summaryPath = (Join-Path $ResolvedSmokeRoot 'qwen-global-hotkey-probe-summary.json')
                    failureKind = 'hostile_foreground_environment'
                    failureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                    failureEvidencePath = (Join-Path $ResolvedSmokeRoot 'failure-diagnostic.json')
                } | ConvertTo-Json -Compress)
            }

            {
                Invoke-TalkDesktopQwenGlobalHotkeyProbe `
                    -BinaryPath 'C:\Release\talk-desktop.exe' `
                    -SmokeRoot $tempRoot `
                    -ApiKey 'talk-test-key'
            } | Should Throw

            $script:probeAttemptCalls | Should Be 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Variable -Name probeAttemptCalls -Scope Script -ErrorAction SilentlyContinue
        }
    }
}
