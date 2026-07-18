$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopQwenGlobalHotkeySoak helpers' {
    It 'builds a concise soak summary from mixed run records' {
        $summary = New-TalkDesktopQwenGlobalHotkeySoakSummary `
            -SmokeRoot 'C:\Talk\.runtime\qwen-soak' `
            -RunRecords @(
                [pscustomobject]@{
                    iteration = 1
                    status = 'completed'
                    durationMs = 10123
                }
                [pscustomobject]@{
                    iteration = 2
                    status = 'failed'
                    durationMs = 8456
                }
                [pscustomobject]@{
                    iteration = 3
                    status = 'completed'
                    durationMs = 9921
                }
            )

        $summary.totalRuns | Should Be 3
        $summary.successfulRuns | Should Be 2
        $summary.failedRuns | Should Be 1
        $summary.successRate | Should Be 66.67
        $summary.averageDurationMs | Should Be 9500
        $summary.smokeRoot | Should Be 'C:\Talk\.runtime\qwen-soak'
    }

    It 'counts insert-target diagnostics and captured focus handles across soak runs' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-soak-focus-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $diagCapturedPath = Join-Path $tempRoot 'captured.desktop-insert-target.json'
            $diagMissingPath = Join-Path $tempRoot 'missing.desktop-insert-target.json'
            [System.IO.File]::WriteAllText(
                $diagCapturedPath,
                (([pscustomobject]@{
                    capturedWindowHandle = '0x303'
                    capturedFocusHandle = '0x404'
                    capturedPrimaryFocusHandle = '0x404'
                    capturedFallbackFocusHandle = '0x404'
                    capturedFocusSource = 'gui_thread_info'
                    outputStrategy = 'honor_configured_output'
                    restoreAttempted = $true
                } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                (New-Object System.Text.UTF8Encoding($false))
            )
            [System.IO.File]::WriteAllText(
                $diagMissingPath,
                (([pscustomobject]@{
                    capturedWindowHandle = '0x303'
                    capturedFocusHandle = $null
                    capturedPrimaryFocusHandle = '0x303'
                    capturedFallbackFocusHandle = '0x303'
                    capturedFocusSource = $null
                    outputStrategy = 'show_copy_popup_only'
                    restoreAttempted = $true
                } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                (New-Object System.Text.UTF8Encoding($false))
            )

            $summary = New-TalkDesktopQwenGlobalHotkeySoakSummary `
                -SmokeRoot 'C:\Talk\.runtime\qwen-soak' `
                -WarmupRuns 1 `
                -RunRecords @(
                    [pscustomobject]@{
                        iteration = 1
                        status = 'completed'
                        durationMs = 10123
                        insertTargetDiagnosticPath = $diagCapturedPath
                    }
                    [pscustomobject]@{
                        iteration = 2
                        status = 'completed'
                        durationMs = 8456
                        insertTargetDiagnosticPath = $diagMissingPath
                    }
                    [pscustomobject]@{
                        iteration = 3
                        status = 'failed'
                        durationMs = 9921
                        insertTargetDiagnosticPath = ''
                    }
                )

            $summary.insertTargetDiagnosticRuns | Should Be 2
            $summary.insertTargetFocusCapturedRuns | Should Be 1
            $summary.insertTargetFocusMissingRuns | Should Be 1
            $summary.insertTargetFocusCapturedRate | Should Be 50
            $summary.insertTargetFocusSourceGuiThreadInfoRuns | Should Be 1
            $summary.insertTargetFocusSourceAttachedGetFocusRuns | Should Be 0
            $summary.insertTargetFocusSourceUnknownRuns | Should Be 1
            $summary.insertTargetHonorConfiguredOutputRuns | Should Be 1
            $summary.insertTargetShowCopyPopupOnlyRuns | Should Be 1
            $summary.insertTargetTopLevelOnlyRuns | Should Be 1
            $summary.measuredInsertTargetDiagnosticRuns | Should Be 1
            $summary.measuredInsertTargetFocusCapturedRuns | Should Be 0
            $summary.measuredInsertTargetFocusMissingRuns | Should Be 1
            $summary.measuredInsertTargetFocusCapturedRate | Should Be 0
            $summary.measuredInsertTargetFocusSourceGuiThreadInfoRuns | Should Be 0
            $summary.measuredInsertTargetFocusSourceAttachedGetFocusRuns | Should Be 0
            $summary.measuredInsertTargetFocusSourceUnknownRuns | Should Be 1
            $summary.measuredInsertTargetHonorConfiguredOutputRuns | Should Be 0
            $summary.measuredInsertTargetShowCopyPopupOnlyRuns | Should Be 1
            $summary.measuredInsertTargetTopLevelOnlyRuns | Should Be 1
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'distinguishes top-level-only runs that still succeeded from ones that did not' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-soak-top-level-only-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $diagTopLevelOnlyPath = Join-Path $tempRoot 'top-level-only.desktop-insert-target.json'
            [System.IO.File]::WriteAllText(
                $diagTopLevelOnlyPath,
                (([pscustomobject]@{
                    capturedWindowHandle = '0x303'
                    capturedFocusHandle = $null
                    capturedPrimaryFocusHandle = '0x303'
                    capturedFallbackFocusHandle = '0x303'
                    capturedFocusSource = $null
                    restoreAttempted = $true
                } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                (New-Object System.Text.UTF8Encoding($false))
            )

            $summary = New-TalkDesktopQwenGlobalHotkeySoakSummary `
                -SmokeRoot 'C:\Talk\.runtime\qwen-soak' `
                -RunRecords @(
                    [pscustomobject]@{
                        iteration = 1
                        status = 'completed'
                        durationMs = 10123
                        capturedTextMatchesOutput = $true
                        insertTargetDiagnosticPath = $diagTopLevelOnlyPath
                    }
                    [pscustomobject]@{
                        iteration = 2
                        status = 'completed'
                        durationMs = 8456
                        capturedTextMatchesOutput = $false
                        insertTargetDiagnosticPath = $diagTopLevelOnlyPath
                    }
                )

            $summary.insertTargetFocusMissingSuccessfulRuns | Should Be 1
            $summary.insertTargetFocusMissingNonSuccessfulRuns | Should Be 1
            $summary.insertTargetTopLevelOnlySuccessfulRuns | Should Be 1
            $summary.insertTargetTopLevelOnlyNonSuccessfulRuns | Should Be 1
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'reports separate measured success metrics after configured warmup runs are excluded' {
        $summary = New-TalkDesktopQwenGlobalHotkeySoakSummary `
            -SmokeRoot 'C:\Talk\.runtime\qwen-soak' `
            -WarmupRuns 1 `
            -RunRecords @(
                [pscustomobject]@{ iteration = 1; status = 'failed'; durationMs = 12000 }
                [pscustomobject]@{ iteration = 2; status = 'completed'; durationMs = 9000 }
                [pscustomobject]@{ iteration = 3; status = 'completed'; durationMs = 8000 }
            )

        $summary.totalRuns | Should Be 3
        $summary.failedRuns | Should Be 1
        $summary.warmupRuns | Should Be 1
        $summary.measuredRuns | Should Be 2
        $summary.measuredSuccessfulRuns | Should Be 2
        $summary.measuredFailedRuns | Should Be 0
        $summary.measuredSuccessRate | Should Be 100
        $summary.measuredAverageDurationMs | Should Be 8500
    }

    It 'treats only completed probe runs as successful soak iterations' {
        (Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord ([pscustomobject]@{ status = 'completed' })) | Should Be $true
        (Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord ([pscustomobject]@{ status = 'failed' })) | Should Be $false
        (Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord ([pscustomobject]@{ status = 'cancelled' })) | Should Be $false
        (Test-TalkDesktopQwenGlobalHotkeySoakRunSucceeded -RunRecord ([pscustomobject]@{ status = 'completed'; capturedTextMatchesOutput = $false })) | Should Be $false
    }

    It 'runs the underlying Qwen global hotkey probe repeatedly and writes a soak summary file' {
        $tempRoot = Join-Path $env:TEMP ('talk-qwen-global-hotkey-soak-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $script:probeCalls = 0
            Mock Invoke-TalkDesktopQwenGlobalHotkeyProbe {
                $script:probeCalls++
                switch ($script:probeCalls) {
                    1 {
                        $diagPath = Join-Path $tempRoot 'run-01\logs\session.desktop-insert-target.json'
                        New-Item -ItemType Directory -Path (Split-Path -Parent $diagPath) -Force | Out-Null
                        [System.IO.File]::WriteAllText(
                            $diagPath,
                            (([pscustomobject]@{
                                capturedWindowHandle = '0x303'
                                capturedFocusHandle = '0x404'
                                capturedPrimaryFocusHandle = '0x404'
                                capturedFallbackFocusHandle = '0x404'
                                capturedFocusSource = 'gui_thread_info'
                                outputStrategy = 'honor_configured_output'
                                restoreAttempted = $true
                            } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                            (New-Object System.Text.UTF8Encoding($false))
                        )
                        [pscustomobject]@{
                            status = 'completed'
                            transcript = 'What is the capital of France?'
                            outputText = 'Paris.'
                            capturedText = 'Paris.'
                            smokeRoot = 'C:\Talk\.runtime\qwen-soak\run-01'
                            summaryPath = 'C:\Talk\.runtime\qwen-soak\run-01\qwen-global-hotkey-probe-summary.json'
                            insertTargetDiagnosticPath = $diagPath
                        }
                    }
                    2 {
                        throw 'simulated provider timeout'
                    }
                    default {
                        $diagPath = Join-Path $tempRoot 'run-03\logs\session.desktop-insert-target.json'
                        New-Item -ItemType Directory -Path (Split-Path -Parent $diagPath) -Force | Out-Null
                        [System.IO.File]::WriteAllText(
                            $diagPath,
                            (([pscustomobject]@{
                                capturedWindowHandle = '0x303'
                                capturedFocusHandle = $null
                                capturedPrimaryFocusHandle = '0x303'
                                capturedFallbackFocusHandle = '0x303'
                                capturedFocusSource = $null
                                outputStrategy = 'show_copy_popup_only'
                                restoreAttempted = $true
                            } | ConvertTo-Json -Depth 4) + [Environment]::NewLine),
                            (New-Object System.Text.UTF8Encoding($false))
                        )
                        [pscustomobject]@{
                            status = 'completed'
                            transcript = 'What is the capital of France?'
                            outputText = 'Paris.'
                            capturedText = 'Paris.'
                            smokeRoot = 'C:\Talk\.runtime\qwen-soak\run-03'
                            summaryPath = 'C:\Talk\.runtime\qwen-soak\run-03\qwen-global-hotkey-probe-summary.json'
                            insertTargetDiagnosticPath = $diagPath
                        }
                    }
                }
            }

            $summary = Invoke-TalkDesktopQwenGlobalHotkeySoak `
                -ReleaseDir 'C:\Release' `
                -SmokeRoot $tempRoot `
                -Count 3 `
                -WarmupRuns 1 `
                -AllowFailures

            $summary.totalRuns | Should Be 3
            $summary.successfulRuns | Should Be 2
            $summary.failedRuns | Should Be 1
            $summary.warmupRuns | Should Be 1
            $summary.measuredRuns | Should Be 2
            $summary.measuredSuccessfulRuns | Should Be 1
            $summary.measuredFailedRuns | Should Be 1
            $summary.measuredSuccessRate | Should Be 50
            $summary.insertTargetDiagnosticRuns | Should Be 2
            $summary.insertTargetFocusCapturedRuns | Should Be 1
            $summary.insertTargetFocusMissingRuns | Should Be 1
            $summary.insertTargetFocusSourceGuiThreadInfoRuns | Should Be 1
            $summary.insertTargetFocusSourceAttachedGetFocusRuns | Should Be 0
            $summary.insertTargetFocusSourceUnknownRuns | Should Be 1
            $summary.insertTargetHonorConfiguredOutputRuns | Should Be 1
            $summary.insertTargetShowCopyPopupOnlyRuns | Should Be 1
            $summary.insertTargetTopLevelOnlyRuns | Should Be 1
            @($summary.runs).Count | Should Be 3
            $summary.runs[1].status | Should Be 'failed'
            $summary.runs[1].error | Should Match 'simulated provider timeout'
            $summary.runs[0].insertTargetDiagnosticPath | Should Not BeNullOrEmpty

            $summaryPath = Join-Path $tempRoot 'qwen-global-hotkey-soak-summary.json'
            Test-Path -LiteralPath $summaryPath | Should Be $true

            $saved = Get-Content -LiteralPath $summaryPath -Raw | ConvertFrom-Json
            $saved.failedRuns | Should Be 1
            $saved.insertTargetFocusCapturedRuns | Should Be 1
            $saved.insertTargetShowCopyPopupOnlyRuns | Should Be 1
            $saved.insertTargetTopLevelOnlyRuns | Should Be 1

            Assert-MockCalled Invoke-TalkDesktopQwenGlobalHotkeyProbe -Times 3 -Exactly
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'forwards explicit entrypoint arguments without letting dependency script variables overwrite them' {
        Mock Invoke-TalkDesktopQwenGlobalHotkeySoak {
            [pscustomobject]@{
                releaseDir = $ReleaseDir
                smokeRoot = $SmokeRoot
                count = $Count
                allowFailures = [bool]$AllowFailures
            }
        }

        $result = Invoke-TalkDesktopQwenGlobalHotkeySoakEntryPoint `
            -BinaryPath 'C:\Release\talk-desktop.exe' `
            -ReleaseDir 'C:\Release' `
            -SmokeRoot 'C:\Talk\.runtime\qwen-soak-entrypoint' `
            -ApiKey 'talk-test-key' `
            -ApiKeyJsonPath 'C:\Keys\manual-live.json' `
            -AudioOverridePath 'C:\Audio\probe.wav' `
            -Count 5 `
            -AllowFailures

        $result.releaseDir | Should Be 'C:\Release'
        $result.smokeRoot | Should Be 'C:\Talk\.runtime\qwen-soak-entrypoint'
        $result.count | Should Be 5
        $result.allowFailures | Should Be $true
        Assert-MockCalled Invoke-TalkDesktopQwenGlobalHotkeySoak -Times 1 -Exactly
    }
}
