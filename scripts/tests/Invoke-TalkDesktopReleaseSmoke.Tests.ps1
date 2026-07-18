$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$scriptPath = Join-Path (Split-Path $here -Parent) 'Invoke-TalkDesktopReleaseSmoke.ps1'

. $scriptPath

Describe 'Invoke-TalkDesktopReleaseSmoke helpers' {
    It 'resolves an explicit talk-desktop binary path' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $binaryPath = Join-Path $tempRoot 'talk-desktop.exe'
            Set-Content -LiteralPath $binaryPath -Value '' -Encoding ASCII

            Resolve-TalkDesktopBinaryPath -BinaryPath $binaryPath | Should Be $binaryPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'derives talk-desktop.exe from a release directory' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $binaryPath = Join-Path $tempRoot 'talk-desktop.exe'
            Set-Content -LiteralPath $binaryPath -Value '' -Encoding ASCII

            Resolve-TalkDesktopBinaryPath -ReleaseDir $tempRoot | Should Be $binaryPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'uses the packaged release directory by default when the script root already contains talk-desktop.exe' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $releaseRoot = Join-Path $tempRoot 'release\Talk'
            $packagedReleaseDir = Join-Path $releaseRoot 'desktop-shell-v85'
            New-Item -ItemType Directory -Path $packagedReleaseDir -Force | Out-Null

            $binaryPath = Join-Path $packagedReleaseDir 'talk-desktop.exe'
            Set-Content -LiteralPath $binaryPath -Value '' -Encoding ASCII

            Resolve-TalkDesktopBinaryPath -ScriptRoot $packagedReleaseDir | Should Be $binaryPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'selects the newest release directory that actually contains talk-desktop.exe' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $unrelatedDir = Join-Path $tempRoot 'blur-brush'
            $olderTalkDir = Join-Path $tempRoot 'desktop-shell-older'
            $newerTalkDir = Join-Path $tempRoot 'desktop-shell-newer'

            New-Item -ItemType Directory -Path $unrelatedDir | Out-Null
            New-Item -ItemType Directory -Path $olderTalkDir | Out-Null
            New-Item -ItemType Directory -Path $newerTalkDir | Out-Null

            Set-Content -LiteralPath (Join-Path $olderTalkDir 'talk-desktop.exe') -Value '' -Encoding ASCII
            Set-Content -LiteralPath (Join-Path $newerTalkDir 'talk-desktop.exe') -Value '' -Encoding ASCII

            (Get-Item -LiteralPath $unrelatedDir).LastWriteTime = [datetime]'2026-07-04T23:59:59'
            (Get-Item -LiteralPath $olderTalkDir).LastWriteTime = [datetime]'2026-07-04T22:00:00'
            (Get-Item -LiteralPath $newerTalkDir).LastWriteTime = [datetime]'2026-07-04T23:00:00'

            Resolve-LatestTalkReleaseDir -ReleaseRoot $tempRoot | Should Be $newerTalkDir
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'builds escaped smoke config content for audio and log paths' {
        $configText = New-TalkSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F24' `
            -Transcript 'desktop smoke' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs'

        $configText | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F24"'
        $configText | Should Match 'mock_transcript = "desktop smoke"'
        $configText | Should BeLike '*temp_dir = "C:\\Talk Smoke\\audio"*'
        $configText | Should BeLike '*dir = "C:\\Talk Smoke\\logs"*'
    }

    It 'builds native unavailable smoke config content with explicit native backends' {
        $configText = New-TalkSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F21' `
            -Transcript 'desktop native status smoke' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs' `
            -AudioBackend 'native_windows' `
            -OutputMode 'clipboard_paste' `
            -ClipboardBackend 'native_windows'

        $configText | Should Match 'backend = "native_windows"'
        $configText | Should Match 'mode = "clipboard_paste"'
        $configText | Should Match 'clipboard_backend = "native_windows"'
    }

    It 'builds http provider smoke config content with an explicit provider endpoint' {
        $configText = New-TalkHttpProviderSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F20' `
            -ProviderEndpoint 'http://127.0.0.1:18080/provider' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs'

        $configText | Should Match 'toggle_shortcut = "Ctrl\+Alt\+F20"'
        $configText | Should Match 'kind = "http"'
        $configText | Should Match 'endpoint = "http://127.0.0.1:18080/provider"'
        $configText | Should Match 'mode = "dry_run"'
    }

    It 'builds openai-compatible smoke config content with explicit audio and chat endpoints' {
        $configText = New-TalkOpenAiCompatibleSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F19' `
            -AudioTranscriptionsEndpoint 'http://127.0.0.1:4200/v1/audio/transcriptions' `
            -ChatCompletionsEndpoint 'http://127.0.0.1:4200/v1/chat/completions' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs'

        $configText | Should Match 'kind = "openai_compatible"'
        $configText | Should Match 'audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"'
        $configText | Should Match 'chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"'
        $configText | Should Match 'transcription_model = "gpt-4o-mini-transcribe"'
        $configText | Should Match 'chat_model = "gpt-4o-mini"'
        $configText | Should Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
        $configText | Should Match 'voice_mode = "command"'
    }

    It 'builds openai-compatible chat-audio-input smoke config content with explicit chat endpoint' {
        $configText = New-TalkOpenAiCompatibleChatAudioInputSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F18' `
            -ChatCompletionsEndpoint 'http://127.0.0.1:4300/v1/chat/completions' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs'

        $configText | Should Match 'kind = "openai_compatible"'
        $configText | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $configText | Should Match 'audio_transcriptions_endpoint = "http://127.0.0.1:4300/v1/chat/completions"'
        $configText | Should Match 'chat_completions_endpoint = "http://127.0.0.1:4300/v1/chat/completions"'
        $configText | Should Match 'transcription_model = "qwen3-asr-flash"'
        $configText | Should Match 'chat_model = "qwen3.7-plus"'
        $configText | Should Match 'api_key_env = "TALK_PROVIDER_API_KEY"'
        $configText | Should Match 'voice_mode = "command"'
    }

    It 'builds openai-compatible chat-audio-input insert smoke config content with native clipboard output' {
        $configText = New-TalkOpenAiCompatibleChatAudioInputSmokeConfigContent `
            -Hotkey 'Ctrl+Alt+F16' `
            -ChatCompletionsEndpoint 'http://127.0.0.1:4301/v1/chat/completions' `
            -AudioDir 'C:\Talk Smoke\audio' `
            -LogsDir 'C:\Talk Smoke\logs' `
            -VoiceMode 'transcribe' `
            -OutputMode 'clipboard_paste' `
            -ClipboardBackend 'native_windows'

        $configText | Should Match 'transcription_transport = "chat_completions_audio_input"'
        $configText | Should Match 'voice_mode = "transcribe"'
        $configText | Should Match 'mode = "clipboard_paste"'
        $configText | Should Match 'clipboard_backend = "native_windows"'
    }

    It 'resolves a desktop insert-target diagnostic sidecar next to a session log' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-diag-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $sessionLogPath = Join-Path $tempRoot 'session.json'
            $diagnosticPath = Join-Path $tempRoot 'session.desktop-insert-target.json'
            Set-Content -LiteralPath $sessionLogPath -Value '{}' -Encoding UTF8
            Set-Content -LiteralPath $diagnosticPath -Value '{}' -Encoding UTF8

            Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $sessionLogPath | Should Be $diagnosticPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'ignores insert-target sidecars when resolving the latest session log' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-logs-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $sessionLogPath = Join-Path $tempRoot 'session.json'
            $diagnosticPath = Join-Path $tempRoot 'session.desktop-insert-target.json'
            Set-Content -LiteralPath $sessionLogPath -Value '{"status":"completed"}' -Encoding UTF8
            Start-Sleep -Milliseconds 20
            Set-Content -LiteralPath $diagnosticPath -Value '{"capturedWindowHandle":"0x303"}' -Encoding UTF8

            (Get-LatestSessionLog -LogsDir $tempRoot).FullName | Should Be $sessionLogPath
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'resolves virtual key codes for hotkey probe chords used by desktop smoke' {
        (Resolve-TalkVirtualKeyCode -KeyToken 'Space') | Should Be 0x20
        (Resolve-TalkVirtualKeyCode -KeyToken 'Slash') | Should Be 0xBF
        (Resolve-TalkVirtualKeyCode -KeyToken 'F16') | Should Be 0x7F
        (Resolve-TalkVirtualKeyCode -KeyToken 'A') | Should Be 0x41
        (Resolve-TalkVirtualKeyCode -KeyToken '9') | Should Be 0x39
        (Resolve-TalkVirtualKeyCode -KeyToken 'RightAlt') | Should Be 0xA5
    }

    It 'packs popup client coordinates into a left-click LPARAM' {
        (New-TalkDesktopMouseLParam -X 10 -Y 20) | Should Be 1310730
        (New-TalkDesktopMouseLParam -X 210 -Y 132) | Should Be 8650962
    }

    It 'sends a complete mouse move, down, and up sequence for popup clicks' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Send-TalkDesktopWindowLeftClick'
        $endMarker = 'function Send-TalkDesktopWindowVirtualKeyInput'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $helperText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $helperText | Should Match '\$WM_MOUSEMOVE = 0x0200'
        $helperText | Should Match '\$WM_LBUTTONDOWN = 0x0201'
        $helperText | Should Match '\$WM_LBUTTONUP = 0x0202'
        $helperText | Should Match '\$MK_LBUTTON = 0x0001'
        $helperText.IndexOf('$WM_MOUSEMOVE = 0x0200') | Should BeLessThan $helperText.IndexOf('$WM_LBUTTONDOWN = 0x0201')
        $helperText.IndexOf('$WM_LBUTTONDOWN = 0x0201') | Should BeLessThan $helperText.IndexOf('$WM_LBUTTONUP = 0x0202')
    }

    It 'calculates DPI-aware copy popup copy button center coordinates' {
        $point96 = Get-TalkDesktopCopyPopupCopyButtonClickPointForDpi -Width 388 -Height 156 -Dpi 96
        $point96.X | Should Be 194
        $point96.Y | Should Be 129

        $point144 = Get-TalkDesktopCopyPopupCopyButtonClickPointForDpi -Width 582 -Height 234 -Dpi 144
        $point144.X | Should Be 291
        $point144.Y | Should Be 193
    }

    It 'builds a DPI-aware copy popup click diagnostic with rect containment' {
        $diagnostic = Get-TalkDesktopCopyPopupClickDiagnosticForDpi `
            -Hwnd ([System.IntPtr]0x1234) `
            -Width 582 `
            -Height 234 `
            -Dpi 144

        $diagnostic.Hwnd | Should Be '0x1234'
        $diagnostic.Width | Should Be 582
        $diagnostic.Height | Should Be 234
        $diagnostic.Dpi | Should Be 144
        $diagnostic.X | Should Be 291
        $diagnostic.Y | Should Be 193
        $diagnostic.CopyRectLeft | Should Be 225
        $diagnostic.CopyRectTop | Should Be 171
        $diagnostic.CopyRectRight | Should Be 357
        $diagnostic.CopyRectBottom | Should Be 216
        $diagnostic.PointInsideCopyRect | Should Be $true
        $diagnostic.PointInsideObservedClient | Should Be $true
    }

    It 'uses observed popup client coordinates when client size is DPI-virtualized' {
        $diagnostic = Get-TalkDesktopCopyPopupClickDiagnosticForDpi `
            -Hwnd ([System.IntPtr]0x1234) `
            -Width 388 `
            -Height 156 `
            -Dpi 144

        $diagnostic.Width | Should Be 388
        $diagnostic.Height | Should Be 156
        $diagnostic.LayoutWidth | Should Be 582
        $diagnostic.LayoutHeight | Should Be 234
        $diagnostic.X | Should Be 194
        $diagnostic.Y | Should Be 129
        $diagnostic.CopyRectLeft | Should Be 150
        $diagnostic.CopyRectTop | Should Be 114
        $diagnostic.CopyRectRight | Should Be 238
        $diagnostic.CopyRectBottom | Should Be 144
        $diagnostic.PointInsideCopyRect | Should Be $true
        $diagnostic.PointInsideObservedClient | Should Be $true
    }

    It 'retries clipboard writes when Set-Clipboard is temporarily busy' {
        $global:TalkClipboardRetryAttempts = 0
        Mock Set-Clipboard {
            $global:TalkClipboardRetryAttempts++
            if ($global:TalkClipboardRetryAttempts -lt 3) {
                throw [System.Runtime.InteropServices.ExternalException]::new('Requested Clipboard operation did not succeed.')
            }
        }

        Set-TalkDesktopClipboardText -Value 'talk clipboard retry'

        $global:TalkClipboardRetryAttempts | Should Be 3
        Assert-MockCalled Set-Clipboard -Times 3 -Exactly
    }

    It 'builds a side-aware global hotkey sequence for RightAlt slash chords' {
        $sequence = Resolve-TalkDesktopGlobalHotkeySequence -Shortcut 'RightAlt+/'
        $sequence.DownKeys | Should Be @(0xA5, 0xBF)
        $sequence.UpKeys | Should Be @(0xBF, 0xA5)
    }

    It 'supports a hotkey operation helper that can hold a shortcut while work runs' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $scriptText | Should Match 'function Invoke-TalkDesktopGlobalHotkeyOperation'
        $scriptText | Should Match 'Invoke-TalkDesktopGlobalHotkeyDown'
        $scriptText | Should Match 'Invoke-TalkDesktopGlobalHotkeyUp'
        $scriptText | Should Match 'try \{[\s\S]*& \$ScriptBlock[\s\S]*\}[\s\S]*finally \{[\s\S]*Invoke-TalkDesktopGlobalHotkeyUp'
    }

    It 'starts a text capture target with child-script variables preserved literally' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        $target = $null
        try {
            Ensure-TalkDesktopSmokeWin32Type

            $target = Start-TalkTextCaptureTarget -ScenarioRoot $tempRoot

            (Test-Path -LiteralPath $target.ReadyPath) | Should Be $true
            $scriptText = Get-Content -LiteralPath $target.ScriptPath -Raw -Encoding UTF8
            $scriptText | Should Match '\$utf8NoBom = New-Object System\.Text\.UTF8Encoding\(\$false\)'
            $scriptText | Should Match '\$form = New-Object System\.Windows\.Forms\.Form'
            $scriptText | Should Match '\$textBox = New-Object System\.Windows\.Forms\.TextBox'
            $scriptText | Should Match '\$handlePath = '
            $scriptText | Should Match '\$textBox\.Handle\.ToInt64\(\)'
            $scriptText | Should Match '\$form\.Add_Activated\(\{'
            $scriptText | Should Match '\$textBox\.Focus\(\) \| Out-Null'
        }
        finally {
            Stop-TalkTextCaptureTarget -Target $target
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'waits until the requested hwnd becomes the foreground window' {
        $targetHwnd = [System.IntPtr]4321
        $otherHwnd = [System.IntPtr]9999

        $script:foregroundProbeCount = 0
        Mock Get-TalkDesktopForegroundWindowHwnd {
            $script:foregroundProbeCount += 1
            if ($script:foregroundProbeCount -ge 3) {
                return $targetHwnd
            }
            return $otherHwnd
        }
        Mock Start-Sleep {}

        $result = Wait-TalkDesktopForegroundWindow `
            -TargetHwnd $targetHwnd `
            -TimeoutMs 500 `
            -PollIntervalMs 10

        $result | Should Be $true
        $script:foregroundProbeCount | Should Be 3
    }

    It 'reapplies textbox child focus after foreground activation when the text target exposes a child handle' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Set-TalkTextCaptureTargetForeground'
        $endMarker = 'function Resolve-TalkVirtualKeyCode'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $functionText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $functionText | Should Match 'Set-TalkDesktopForegroundWindow -Hwnd \$Target\.Hwnd'
        $functionText | Should Match 'Set-TalkDesktopChildInputFocus -TargetHwnd \$Target\.Hwnd -ChildHwnd \$childHwnd'
    }

    It 'uses a topmost focus shim and always clears it after foreground activation' {
        $targetHwnd = [System.IntPtr]2468
        $script:focusCallOrder = New-Object 'System.Collections.Generic.List[string]'

        Mock Ensure-TalkDesktopSmokeWin32Type {
            $script:focusCallOrder.Add('ensure') | Out-Null
        }
        Mock Show-TalkDesktopWindowRestored {
            $script:focusCallOrder.Add('show') | Out-Null
        }
        Mock Set-TalkDesktopWindowTopmostState {
            if ($Enable) {
                $script:focusCallOrder.Add('topmost-on') | Out-Null
            } else {
                $script:focusCallOrder.Add('topmost-off') | Out-Null
            }
        }
        Mock Bring-TalkDesktopWindowToTop {
            $script:focusCallOrder.Add('bring-to-top') | Out-Null
        }
        Mock Invoke-TalkDesktopForegroundWindowNative {
            $script:focusCallOrder.Add('set-foreground') | Out-Null
        }
        Mock Wait-TalkDesktopForegroundWindow {
            $script:focusCallOrder.Add('wait') | Out-Null
            return $true
        }
        Mock Start-Sleep {}

        $result = Set-TalkDesktopForegroundWindow `
            -Hwnd $targetHwnd `
            -MaxAttempts 1 `
            -ForegroundTimeoutMs 250

        $result | Should Be $true
        $script:focusCallOrder | Should Be @(
            'ensure',
            'show',
            'topmost-on',
            'bring-to-top',
            'set-foreground',
            'wait',
            'topmost-off'
        )
    }

    It 'uses thread input attachment when requesting a foreground switch through Win32' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-TalkDesktopForegroundWindowNative'
        $endMarker = 'function Wait-TalkDesktopForegroundWindow'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $functionText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $functionText | Should Match 'GetForegroundWindow'
        $functionText | Should Match 'GetWindowThreadProcessId'
        $functionText | Should Match 'GetCurrentThreadId'
        $functionText | Should Match 'AttachThreadInput'
        $functionText | Should Match 'SetForegroundWindow'
        $functionText | Should Match 'keybd_event'
        $functionText | Should Match '\$VK_MENU = 0x12'
        $functionText | Should Match '\$KEYEVENTF_KEYUP = 0x0002'
        $functionText.IndexOf('$VK_MENU = 0x12') | Should BeLessThan $functionText.IndexOf('SetForegroundWindow')
        $functionText | Should Match 'finally'
        $functionText | Should Match 'AttachThreadInput\([^,]+,[^,]+,\s*\$false\)'
    }

    It 'uses the foreground unlock input pulse only for pre-hotkey foreground assertions' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        $nativeStart = $scriptText.IndexOf('function Invoke-TalkDesktopForegroundWindowNative')
        $nativeEnd = $scriptText.IndexOf('function Wait-TalkDesktopForegroundWindow')
        $nativeText = $scriptText.Substring($nativeStart, $nativeEnd - $nativeStart)
        $nativeText | Should Match '\[switch\]\$UseInputUnlock'
        $nativeText | Should Match 'if \(\$UseInputUnlock\)'
        $nativeText.IndexOf('if ($UseInputUnlock)') | Should BeLessThan $nativeText.IndexOf('SetForegroundWindow')

        $assertStart = $scriptText.IndexOf('function Assert-TalkTextCaptureTargetForeground')
        $assertEnd = $scriptText.IndexOf('function Invoke-TalkDesktopPinnedWindowOperation')
        $assertText = $scriptText.Substring($assertStart, $assertEnd - $assertStart)
        $assertText | Should Match '-UseInputUnlock'

        $refreshStart = $scriptText.IndexOf('function Wait-TalkTextCaptureContainsWithForegroundRefresh')
        $refreshEnd = $scriptText.IndexOf('function Send-TalkTextCaptureTargetText')
        $refreshText = $scriptText.Substring($refreshStart, $refreshEnd - $refreshStart)
        $refreshText | Should Not Match '-UseInputUnlock'
    }

    It 'can preserve a pinned topmost state while acquiring foreground inside a pinned operation' {
        $targetHwnd = [System.IntPtr]8642
        $script:topmostTransitions = New-Object 'System.Collections.Generic.List[string]'

        Mock Ensure-TalkDesktopSmokeWin32Type {}
        Mock Show-TalkDesktopWindowRestored {}
        Mock Set-TalkDesktopWindowTopmostState {
            if ($Enable) {
                $script:topmostTransitions.Add('on') | Out-Null
            } else {
                $script:topmostTransitions.Add('off') | Out-Null
            }
        }
        Mock Bring-TalkDesktopWindowToTop {}
        Mock Invoke-TalkDesktopForegroundWindowNative {}
        Mock Wait-TalkDesktopForegroundWindow { return $true }
        Mock Start-Sleep {}

        $result = Set-TalkDesktopForegroundWindow `
            -Hwnd $targetHwnd `
            -MaxAttempts 1 `
            -ForegroundTimeoutMs 250 `
            -KeepTopmost

        $result | Should Be $true
        $script:topmostTransitions | Should Be @('on')
    }

    It 'clears the topmost focus shim even when the target never becomes foreground' {
        $targetHwnd = [System.IntPtr]1357
        $script:topmostTransitions = New-Object 'System.Collections.Generic.List[string]'

        Mock Ensure-TalkDesktopSmokeWin32Type {}
        Mock Show-TalkDesktopWindowRestored {}
        Mock Set-TalkDesktopWindowTopmostState {
            if ($Enable) {
                $script:topmostTransitions.Add('on') | Out-Null
            } else {
                $script:topmostTransitions.Add('off') | Out-Null
            }
        }
        Mock Bring-TalkDesktopWindowToTop {}
        Mock Invoke-TalkDesktopForegroundWindowNative {}
        Mock Wait-TalkDesktopForegroundWindow { $false }
        Mock Start-Sleep {}

        $result = Set-TalkDesktopForegroundWindow `
            -Hwnd $targetHwnd `
            -MaxAttempts 2 `
            -ForegroundTimeoutMs 100

        $result | Should Be $false
        $script:topmostTransitions | Should Be @('on', 'off', 'on', 'off')
    }

    It 'pins a target window topmost for the duration of a script block and always clears it afterward' {
        $targetHwnd = [System.IntPtr]8642
        $script:pinTransitions = New-Object 'System.Collections.Generic.List[string]'

        Mock Set-TalkDesktopWindowTopmostState {
            if ($Enable) {
                $script:pinTransitions.Add('on') | Out-Null
            } else {
                $script:pinTransitions.Add('off') | Out-Null
            }
        }

        $result = Invoke-TalkDesktopPinnedWindowOperation -Hwnd $targetHwnd -ScriptBlock {
            $script:pinTransitions.Add('body') | Out-Null
            'ok'
        }

        $result | Should Be 'ok'
        $script:pinTransitions | Should Be @('on', 'body', 'off')
    }

    It 'clears a pinned target window topmost state when the wrapped operation throws' {
        $targetHwnd = [System.IntPtr]97542
        $script:pinTransitions = New-Object 'System.Collections.Generic.List[string]'

        Mock Set-TalkDesktopWindowTopmostState {
            if ($Enable) {
                $script:pinTransitions.Add('on') | Out-Null
            } else {
                $script:pinTransitions.Add('off') | Out-Null
            }
        }

        {
            Invoke-TalkDesktopPinnedWindowOperation -Hwnd $targetHwnd -ScriptBlock {
                $script:pinTransitions.Add('body') | Out-Null
                throw 'boom'
            }
        } | Should Throw

        $script:pinTransitions | Should Be @('on', 'body', 'off')
    }

    It 'keeps refreshing the foreground target until captured text appears' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-text-wait-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $snapshotPath = Join-Path $tempRoot 'snapshot.txt'
            Set-Content -LiteralPath $snapshotPath -Value '' -Encoding UTF8

            $script:focusRefreshCount = 0
            Mock Set-TalkDesktopForegroundWindow {
                $script:focusRefreshCount += 1
                if ($script:focusRefreshCount -eq 2) {
                    Set-Content -LiteralPath $snapshotPath -Value 'assistant reply from audio input chat' -Encoding UTF8
                }
            }
            Mock Get-TalkDesktopForegroundWindowDebugString { 'Talk Smoke Text Target' }
            Mock Start-Sleep {}

            $capturedText = Wait-TalkTextCaptureContainsWithForegroundRefresh `
                -Hwnd ([System.IntPtr]1234) `
                -SnapshotPath $snapshotPath `
                -ExpectedText 'assistant reply from audio input chat' `
                -TimeoutMs 500

            $capturedText.Trim() | Should Be 'assistant reply from audio input chat'
            $script:focusRefreshCount | Should BeGreaterThan 1
            Assert-MockCalled Set-TalkDesktopForegroundWindow -Times 2
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Variable -Name focusRefreshCount -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'includes the recent foreground tail when captured text never appears' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-text-timeout-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            $snapshotPath = Join-Path $tempRoot 'snapshot.txt'
            Set-Content -LiteralPath $snapshotPath -Value '' -Encoding UTF8

            $script:foregroundLabelIndex = 0
            Mock Set-TalkDesktopForegroundWindow {}
            Mock Get-TalkDesktopForegroundWindowDebugString {
                $script:foregroundLabelIndex += 1
                switch ($script:foregroundLabelIndex) {
                    1 { 'Talk Smoke Text Target' }
                    2 { 'The Scroll of Taiwu' }
                    default { 'Neuro' }
                }
            }
            Mock Start-Sleep {}

            $errorMessage = ''
            try {
                Wait-TalkTextCaptureContainsWithForegroundRefresh `
                    -Hwnd ([System.IntPtr]4321) `
                    -SnapshotPath $snapshotPath `
                    -ExpectedText 'assistant reply from audio input chat' `
                    -TimeoutMs 200 `
                    -RefreshIntervalMs 10
            } catch {
                $errorMessage = [string]$_.Exception.Message
            }

            $errorMessage | Should Match 'The Scroll of Taiwu -> Neuro'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Variable -Name foregroundLabelIndex -Scope Script -ErrorAction SilentlyContinue
        }
    }

    It 'classifies insert timeout as hostile foreground interference when clipboard paste completed but focus drifted away' {
        $session = [pscustomobject]@{
            status = 'completed'
            output_text = 'assistant reply from audio input chat'
            insert_outcome = [pscustomobject]@{
                method = 'clipboard_paste'
            }
        }

        $failure = Get-TalkDesktopInsertFailureClassification `
            -Scenario 'openai-compatible-audio-input-insert-success' `
            -ExpectedOutputText 'assistant reply from audio input chat' `
            -TargetWindowTitle 'Talk Smoke Text Target' `
            -Session $session `
            -ErrorMessage 'Talk text capture target did not contain [assistant reply from audio input chat]. Last text: []. Foreground tail: [Talk Smoke Text Target -> The Scroll of Taiwu -> Neuro]'

        $failure | Should Not Be $null
        $failure.FailureKind | Should Be 'hostile_foreground_environment'
        $failure.FailureSummary | Should Match 'clipboard paste'
        $failure.ForegroundTail | Should Match 'The Scroll of Taiwu'
    }

    It 'does not throw when insert failure classification inspects a failed session without insert_outcome metadata' {
        $session = [pscustomobject]@{
            status = 'failed'
            output_text = $null
            error = 'provider error: upstream unavailable'
        }

        {
            $failure = Get-TalkDesktopInsertFailureClassification `
                -Scenario 'qwen-global-hotkey-probe' `
                -ExpectedOutputText 'Paris' `
                -TargetWindowTitle 'Talk Smoke Text Target' `
                -Session $session `
                -ErrorMessage 'Expected completed Talk desktop Qwen probe session, got [failed]'

            $failure | Should Be $null
        } | Should Not Throw
    }

    It 'classifies primer timeout as hostile foreground interference when another window steals focus before hotkey start' {
        $failure = Get-TalkDesktopPrimerFailureClassification `
            -Scenario 'openai-compatible-audio-input-insert-success' `
            -TargetWindowTitle 'Talk Smoke Text Target' `
            -ErrorMessage 'Talk text capture target did not contain [talkprimerready]. Last text: []. Foreground tail: [Talk Smoke Text Target -> The Scroll of Taiwu -> Neuro]'

        $failure | Should Not Be $null
        $failure.FailureKind | Should Be 'hostile_foreground_environment'
        $failure.FailureSummary | Should Match 'primer input'
        $failure.ForegroundTail | Should Match 'The Scroll of Taiwu'
    }

    It 'throws smoke classification failures by default but can return them when ContinueOnFailure is requested' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    FailureKind = 'hostile_foreground_environment'
                    FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
                    FailureEvidencePath = (Join-Path $ScenarioRoot 'failure-diagnostic.json')
                }
            }

            {
                Invoke-TalkDesktopReleaseSmoke `
                    -BinaryPath 'C:\fake\talk-desktop.exe' `
                    -SmokeRoot $tempRoot `
                    -Scenario @('openai-compatible-audio-input-insert-success')
            } | Should Throw

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('openai-compatible-audio-input-insert-success') `
                -ContinueOnFailure

            @($results).Count | Should Be 1
            @($results)[0].FailureKind | Should Be 'hostile_foreground_environment'
            @($results)[0].FailureEvidencePath | Should Match 'failure-diagnostic\.json'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
        }
    }

    It 'primes a text capture target through its child textbox handle when one is available' {
        $primerText = 'talkprimerready'
        $snapshotPath = 'C:\Talk\.runtime\text-target\snapshot.txt'
        $childHwnd = [System.IntPtr]4321

        Mock Set-TalkDesktopForegroundWindow {}
        Mock Write-TalkTextCaptureTargetChildText { $true }
        Mock Send-TalkTextCaptureTargetText {}
        Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
            "$primerText"
        }

        $captured = Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd ([System.IntPtr]1234) `
            -SnapshotPath $snapshotPath `
            -PrimerText $primerText `
            -ChildHwnd $childHwnd

        $captured | Should Be $primerText
        Assert-MockCalled Set-TalkDesktopForegroundWindow -Scope It
        Assert-MockCalled Write-TalkTextCaptureTargetChildText -Times 1 -Exactly -ParameterFilter {
            $Text -eq $primerText -and $ChildHwnd -eq $childHwnd
        } -Scope It
        Assert-MockCalled Send-TalkTextCaptureTargetText -Times 0 -Exactly -Scope It
        Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly -Scope It
    }

    It 'primes a text capture target by falling back to keyboard text injection when no child handle is available' {
        $primerText = 'talkprimerready'
        $snapshotPath = 'C:\Talk\.runtime\text-target\snapshot.txt'

        Mock Set-TalkDesktopForegroundWindow {}
        Mock Write-TalkTextCaptureTargetChildText { $true }
        Mock Send-TalkTextCaptureTargetText {}
        Mock Wait-TalkTextCaptureContainsWithForegroundRefresh {
            "$primerText"
        }

        $captured = Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd ([System.IntPtr]1234) `
            -SnapshotPath $snapshotPath `
            -PrimerText $primerText

        $captured | Should Be $primerText
        Assert-MockCalled Set-TalkDesktopForegroundWindow -Scope It
        Assert-MockCalled Write-TalkTextCaptureTargetChildText -Times 0 -Exactly -Scope It
        Assert-MockCalled Send-TalkTextCaptureTargetText -Times 1 -Exactly -ParameterFilter { $Text -eq $primerText } -Scope It
        Assert-MockCalled Wait-TalkTextCaptureContainsWithForegroundRefresh -Times 1 -Exactly -Scope It
    }

    It 'removes primer artifacts from either edge of the observed text when present' {
        (Remove-TalkTextCapturePrimerPrefix -CapturedText '[talk-primer]Paris' -PrimerText '[talk-primer]') | Should Be 'Paris'
        (Remove-TalkTextCapturePrimerPrefix -CapturedText 'Paris[talk-primer]' -PrimerText '[talk-primer]') | Should Be 'Paris'
        (Remove-TalkTextCapturePrimerPrefix -CapturedText '[talk-primer]Paris[talk-primer]' -PrimerText '[talk-primer]') | Should Be 'Paris'
        (Remove-TalkTextCapturePrimerPrefix -CapturedText 'Paris' -PrimerText '[talk-primer]') | Should Be 'Paris'
        (Remove-TalkTextCapturePrimerPrefix -CapturedText '' -PrimerText '[talk-primer]') | Should Be ''
    }

    It 'asserts required status lines for native unavailable desktop state' {
        $dialogText = @'
Current: Talk: native unavailable
Config: C:\Talk\config.toml
Logs: C:\Talk\logs
Hotkey: Ctrl+Alt+F21
Current detail: native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO
Audio backend: native_windows
Audio backend readiness: unavailable
Audio backend detail: native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO
Clipboard backend: native_windows
Clipboard backend readiness: unavailable
Clipboard backend detail: native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD
'@

        {
            Assert-TalkDesktopStatusLines `
                -DialogText $dialogText `
                -ExpectedLines @(
                    'Current: Talk: native unavailable',
                    'Audio backend: native_windows',
                    'Audio backend readiness: unavailable',
                    'Clipboard backend: native_windows',
                    'Clipboard backend readiness: unavailable'
                )
        } | Should Not Throw
    }

    It 'parses structured Talk status dialog fields from dialog text' {
        $dialogText = @'
确定
Current: Talk: native unavailable
Config: C:\Talk\config.toml
Audio backend: native_windows
Audio backend readiness: unavailable
'@

        $fields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText

        $fields['Current'] | Should Be 'Talk: native unavailable'
        $fields['Config'] | Should Be 'C:\Talk\config.toml'
        $fields['Audio backend'] | Should Be 'native_windows'
        $fields['Audio backend readiness'] | Should Be 'unavailable'
    }

    It 'normalizes Talk status dialog fields into stable snapshot keys' {
        $fields = [ordered]@{
            Current = 'Talk: native unavailable'
            'Current detail' = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
            Config = 'C:\Talk\config.toml'
            Logs = 'C:\Talk\logs'
            Hotkey = 'Ctrl+Alt+F21'
            'Hotkey detail' = 'register Talk desktop hotkey: already in use'
            'Last session' = 'cancelled'
            'Last session detail' = 'user cancelled'
            'Audio backend' = 'native_windows'
            'Audio backend readiness' = 'unavailable'
            'Audio backend detail' = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
            'Clipboard backend' = 'native_windows'
            'Clipboard backend readiness' = 'unavailable'
            'Clipboard backend detail' = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
        }

        $snapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $fields

        $snapshot.current | Should Be 'Talk: native unavailable'
        $snapshot.currentDetail | Should Be 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
        $snapshot.configPath | Should Be 'C:\Talk\config.toml'
        $snapshot.logsPath | Should Be 'C:\Talk\logs'
        $snapshot.hotkey | Should Be 'Ctrl+Alt+F21'
        $snapshot.hotkeyDetail | Should Be 'register Talk desktop hotkey: already in use'
        $snapshot.lastSession | Should Be 'cancelled'
        $snapshot.lastSessionDetail | Should Be 'user cancelled'
        $snapshot.audioBackend | Should Be 'native_windows'
        $snapshot.audioBackendReadiness | Should Be 'unavailable'
        $snapshot.audioBackendDetail | Should Be 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
        $snapshot.clipboardBackend | Should Be 'native_windows'
        $snapshot.clipboardBackendReadiness | Should Be 'unavailable'
        $snapshot.clipboardBackendDetail | Should Be 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
    }

    It 'classifies and summarizes native unavailable desktop status fields' {
        $fields = [ordered]@{
            Current = 'Talk: native unavailable'
            'Current detail' = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
            'Audio backend' = 'native_windows'
            'Audio backend readiness' = 'unavailable'
            'Clipboard backend' = 'native_windows'
            'Clipboard backend readiness' = 'unavailable'
        }

        (Get-TalkDesktopStatusKind -Fields $fields) | Should Be 'native_unavailable'
        $summary = Get-TalkDesktopStatusSummary -Fields $fields
        $summary | Should Match 'current=Talk: native unavailable'
        $summary | Should Match 'audio_backend=native_windows'
        $summary | Should Match 'audio_backend_readiness=unavailable'
        $summary | Should Match 'clipboard_backend_readiness=unavailable'
    }

    It 'dispatches native-unavailable-status as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-NativeUnavailableStatusSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'native-unavailable-status'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    DialogText = 'Current: Talk: native unavailable'
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('native-unavailable-status')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'native-unavailable-status'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'dispatches http-provider-success as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-HttpProviderSuccessSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'http-provider-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    Status = 'completed'
                    LogPath = (Join-Path $ScenarioRoot 'session.json')
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('http-provider-success')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'http-provider-success'
            @($results)[0].Status | Should Be 'completed'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'dispatches openai-compatible-success as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-OpenAiCompatibleSuccessSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    Status = 'completed'
                    LogPath = (Join-Path $ScenarioRoot 'session.json')
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('openai-compatible-success')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'openai-compatible-success'
            @($results)[0].Status | Should Be 'completed'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'dispatches openai-compatible-audio-input-success as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-OpenAiCompatibleChatAudioInputSuccessSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    Status = 'completed'
                    LogPath = (Join-Path $ScenarioRoot 'session.json')
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('openai-compatible-audio-input-success')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'openai-compatible-audio-input-success'
            @($results)[0].Status | Should Be 'completed'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'dispatches openai-compatible-audio-input-insert-success as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    Status = 'completed'
                    LogPath = (Join-Path $ScenarioRoot 'session.json')
                    CapturedText = 'assistant reply from audio input chat'
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('openai-compatible-audio-input-insert-success')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'openai-compatible-audio-input-insert-success'
            @($results)[0].Status | Should Be 'completed'
            @($results)[0].CapturedText | Should Be 'assistant reply from audio input chat'
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'dispatches openai-compatible-audio-input-focus-switch-copy-popup-success as a supported smoke scenario' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke {
                param([string]$TalkDesktopBinaryPath, [string]$ScenarioRoot)
                [pscustomobject]@{
                    Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ScenarioRoot = $ScenarioRoot
                    Status = 'completed'
                    PopupVisible = $true
                }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot `
                -Scenario @('openai-compatible-audio-input-focus-switch-copy-popup-success')

            @($results).Count | Should Be 1
            @($results)[0].Scenario | Should Be 'openai-compatible-audio-input-focus-switch-copy-popup-success'
            @($results)[0].Status | Should Be 'completed'
            @($results)[0].PopupVisible | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'primes the text target before firing the insert hotkey in the release insert smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText.IndexOf('Invoke-TalkTextCaptureTargetPrimer') | Should BeGreaterThan -1
        $scenarioText.IndexOf('Invoke-TalkTextCaptureTargetPrimer') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord')
    }

    It 'pins the text target topmost while the insert hotkey sequence and capture wait run' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Invoke-TalkDesktopPinnedWindowOperation -Hwnd \$target\.Hwnd -ScriptBlock'
        $scenarioText.IndexOf('Invoke-TalkDesktopPinnedWindowOperation') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord')
        $scenarioText.IndexOf('Invoke-TalkDesktopPinnedWindowOperation') | Should BeLessThan $scenarioText.IndexOf('Wait-TalkTextCaptureContainsWithForegroundRefresh')
    }

    It 'selects the primer text in the child textbox before firing the insert hotkey so paste can replace it' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Select-TalkTextCaptureTargetChildText -ChildHwnd \$target\.TextBoxHwnd'
        $scenarioText.IndexOf('Select-TalkTextCaptureTargetChildText') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord')
    }

    It 'runs the release insert smoke in transcribe mode so command mode remains GUI-only' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match "-VoiceMode 'transcribe'"
    }

    It 'pins the release insert smoke origin to the text target handles through environment overrides' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText.IndexOf('Start-TalkTextCaptureTarget -ScenarioRoot') | Should BeLessThan $scenarioText.IndexOf('Start-TalkDesktopSmokeInstance')
        $scenarioText | Should Match 'TALK_DESKTOP_INSERT_TARGET_WINDOW'
        $scenarioText | Should Match 'TALK_DESKTOP_INSERT_TARGET_FOCUS'
        $scenarioText | Should Match 'Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd \$target\.Hwnd'
        $scenarioText | Should Match 'Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd \$target\.TextBoxHwnd'
    }

    It 'strips the text-target primer prefix from captured insert text before returning the smoke result' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText.IndexOf('Remove-TalkTextCapturePrimerPrefix') | Should BeGreaterThan -1
        $scenarioText | Should Match 'CapturedText = .*normalizedCapturedText'
    }

    It 'switches foreground to a second target before waiting for the copy popup in the focus-switch smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Start-TalkTextCaptureTarget -ScenarioRoot \(Join-Path \$ScenarioRoot ''origin-target''\)'
        $scenarioText | Should Match 'Start-TalkTextCaptureTarget -ScenarioRoot \(Join-Path \$ScenarioRoot ''alternate-target''\)'
        $scenarioText | Should Match 'Assert-TalkTextCaptureTargetForeground -Target \$alternateTarget -Name ''alternate'''
        $scenarioText | Should Match 'Wait-TalkDesktopVisibleWindowByProcessIdAndClass'
        $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey') | Should BeGreaterThan -1
        $scenarioText.LastIndexOf('Assert-TalkTextCaptureTargetForeground -Target $alternateTarget -Name ''alternate''') | Should BeGreaterThan $scenarioText.LastIndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey')
        $scenarioText.LastIndexOf('Wait-TalkDesktopVisibleWindowByProcessIdAndClass') | Should BeGreaterThan $scenarioText.LastIndexOf('Assert-TalkTextCaptureTargetForeground -Target $alternateTarget -Name ''alternate''')
    }

    It 'waits for the listening HUD after the first focus-switch hotkey before sending the stop hotkey' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match "ClassName 'TalkDesktopHudWindow'"
        $scenarioText | Should Match "Message 'origin-recording-visible'"
        $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey') | Should BeLessThan $scenarioText.IndexOf("ClassName 'TalkDesktopHudWindow'")
        $scenarioText.IndexOf("ClassName 'TalkDesktopHudWindow'") | Should BeLessThan $scenarioText.LastIndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey')
        $scenarioText.IndexOf('Message ''origin-recording-visible''') | Should BeLessThan $scenarioText.LastIndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey')
    }

    It 'requires the origin text target to be foreground before firing focus-switch hotkeys' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Assert-TalkTextCaptureTargetForeground -Target \$originTarget -Name ''origin'''
        $scenarioText | Should Match 'Assert-TalkTextCaptureTargetForeground -Target \$alternateTarget -Name ''alternate'''
        $scenarioText.IndexOf('Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name ''origin''') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey')
        $scenarioText.LastIndexOf('Assert-TalkTextCaptureTargetForeground -Target $alternateTarget -Name ''alternate''') | Should BeGreaterThan $scenarioText.LastIndexOf('Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey')
    }

    It 'returns hostile foreground failures instead of throwing when focus-switch foreground preconditions fail' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scriptText | Should Match 'function New-TalkDesktopFocusSwitchHostileForegroundFailure'
        $scenarioText | Should Match 'New-TalkDesktopFocusSwitchHostileForegroundFailure'
        $scenarioText | Should Match "FailureKind = 'hostile_foreground_environment'"
    }

    It 'keeps focus-switch provider responses slow enough for alternate focus to settle before insertion policy runs' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8

        ([regex]::Matches($scriptText, '-ResponseDelayMs 1500')).Count | Should BeGreaterThan 1
    }

    It 'runs focus-switch copy-popup smokes in transcribe mode so focus switching, not command GUI-only policy, triggers the popup' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $mouseStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke')
        $coreStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore')
        $keyboardStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke')
        $mouseScenarioText = $scriptText.Substring($mouseStart, $coreStart - $mouseStart)
        $coreScenarioText = $scriptText.Substring($coreStart, $keyboardStart - $coreStart)

        $mouseScenarioText | Should Match "-VoiceMode 'transcribe'"
        $coreScenarioText | Should Match "-VoiceMode 'transcribe'"
    }

    It 'does not use fixed insert-target environment overrides for focus-switch copy-popup smokes' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $mouseStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke')
        $coreStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore')
        $keyboardStart = $scriptText.IndexOf('function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke')
        $mouseScenarioText = $scriptText.Substring($mouseStart, $coreStart - $mouseStart)
        $coreScenarioText = $scriptText.Substring($coreStart, $keyboardStart - $coreStart)

        $mouseScenarioText | Should Not Match 'TALK_DESKTOP_INSERT_TARGET_WINDOW'
        $mouseScenarioText | Should Not Match 'TALK_DESKTOP_INSERT_TARGET_FOCUS'
        $coreScenarioText | Should Not Match 'TALK_DESKTOP_INSERT_TARGET_WINDOW'
        $coreScenarioText | Should Not Match 'TALK_DESKTOP_INSERT_TARGET_FOCUS'
    }

    It 'keeps the popup open after mouse copy and then closes it explicitly in the focus-switch smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke'
        $endMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Get-TalkDesktopCopyPopupCopyButtonClickPoint -Hwnd \$popupHwnd'
        $scenarioText | Should Match 'Send-TalkDesktopWindowLeftClick -Hwnd \$popupHwnd -X \$copyClick\.X -Y \$copyClick\.Y'
        $scenarioText | Should Match 'Wait-TalkDesktopWindowHiddenByProcessIdAndClass'
        $scenarioText | Should Match 'Get-TalkDesktopClipboardText'
        $scenarioText | Should Match 'Set-TalkDesktopClipboardText -Value ''talk-copy-popup-pending'''
        $scenarioText | Should Match 'Send-TalkDesktopWindowVirtualKeyInput -Hwnd \$popupHwnd -VirtualKey 0x1B'
        $scenarioText.IndexOf('Get-TalkDesktopCopyPopupCopyButtonClickPoint -Hwnd $popupHwnd') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopWindowLeftClick -Hwnd $popupHwnd -X $copyClick.X -Y $copyClick.Y')
        $scenarioText.IndexOf('Send-TalkDesktopWindowLeftClick -Hwnd $popupHwnd -X $copyClick.X -Y $copyClick.Y') | Should BeLessThan $scenarioText.IndexOf('Get-TalkDesktopClipboardText')
        $scenarioText.IndexOf('Get-TalkDesktopClipboardText') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B')
        $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B') | Should BeLessThan $scenarioText.IndexOf('Wait-TalkDesktopWindowHiddenByProcessIdAndClass')
    }

    It 'does not move foreground onto the copy popup when it becomes visible in the focus-switch popup core flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore'
        $endMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Get-TalkDesktopForegroundWindowHwnd'
        $scenarioText | Should Match '\$popupHwnd'
        $scenarioText.IndexOf('Write-TalkSmokeProgress -Path $progressPath -Message ''copy-popup-visible''') | Should BeLessThan $scenarioText.IndexOf('Get-TalkDesktopForegroundWindowHwnd')
    }

    It 'keeps the popup open after keyboard Enter copy and then closes it explicitly in the keyboard-enter focus-switch smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Send-TalkDesktopWindowVirtualKeyInput -Hwnd \$popupHwnd -VirtualKey 0x0D'
        $scenarioText | Should Match 'Wait-TalkDesktopVisibleWindowByProcessIdAndClass'
        $scenarioText | Should Match 'Send-TalkDesktopWindowVirtualKeyInput -Hwnd \$popupHwnd -VirtualKey 0x1B'
        $scenarioText | Should Match 'Wait-TalkDesktopWindowHiddenByProcessIdAndClass'
        $scenarioText | Should Match 'Get-TalkDesktopClipboardText'
        $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x0D') | Should BeLessThan $scenarioText.IndexOf('Wait-TalkDesktopWindowHiddenByProcessIdAndClass')
        $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x0D') | Should BeLessThan $scenarioText.IndexOf('Get-TalkDesktopClipboardText')
        $scenarioText.IndexOf('Get-TalkDesktopClipboardText') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B')
        $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B') | Should BeLessThan $scenarioText.IndexOf('Wait-TalkDesktopWindowHiddenByProcessIdAndClass')
    }

    It 'sends Escape to the popup and verifies the clipboard sentinel remains unchanged in the keyboard-escape focus-switch smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEscapeSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText | Should Match 'Set-TalkDesktopClipboardText -Value ''talk-copy-popup-escape-sentinel'''
        $scenarioText | Should Match 'Send-TalkDesktopWindowVirtualKeyInput -Hwnd \$popupHwnd -VirtualKey 0x1B'
        $scenarioText | Should Match 'Wait-TalkDesktopWindowHiddenByProcessIdAndClass'
        $scenarioText | Should Match 'Get-TalkDesktopClipboardText'
        $scenarioText.IndexOf('Set-TalkDesktopClipboardText -Value ''talk-copy-popup-escape-sentinel''') | Should BeLessThan $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B')
        $scenarioText.IndexOf('Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B') | Should BeLessThan $scenarioText.IndexOf('Get-TalkDesktopClipboardText')
    }

    It 'includes openai-compatible-audio-input-insert-success in the default release smoke set' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-CancelAndStatusSmoke { [pscustomobject]@{ Scenario = 'cancel-and-status' } }
            Mock Invoke-HotkeyConflictSmoke { [pscustomobject]@{ Scenario = 'hotkey-conflict' } }
            Mock Invoke-BrokenConfigRecoverySmoke { [pscustomobject]@{ Scenario = 'broken-config-recovery' } }
            Mock Invoke-NativeUnavailableStatusSmoke { [pscustomobject]@{ Scenario = 'native-unavailable-status' } }
            Mock Invoke-OpenAiCompatibleSuccessSmoke { [pscustomobject]@{ Scenario = 'openai-compatible-success' } }
            Mock Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-insert-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEscapeSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success' }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot

            ((@($results).Scenario) -contains 'openai-compatible-audio-input-insert-success') | Should Be $true
            ((@($results).Scenario) -contains 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success') | Should Be $true
            ((@($results).Scenario) -contains 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success') | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'includes openai-compatible-audio-input-focus-switch-copy-popup-success in the default release smoke set' {
        $tempRoot = Join-Path $env:TEMP ('talk-desktop-smoke-test-' + [guid]::NewGuid().ToString())
        New-Item -ItemType Directory -Path $tempRoot | Out-Null
        try {
            Mock Ensure-TalkDesktopSmokeWin32Type {}
            Mock Resolve-TalkDesktopBinaryPath { 'C:\fake\talk-desktop.exe' }
            Mock Invoke-CancelAndStatusSmoke { [pscustomobject]@{ Scenario = 'cancel-and-status' } }
            Mock Invoke-HotkeyConflictSmoke { [pscustomobject]@{ Scenario = 'hotkey-conflict' } }
            Mock Invoke-BrokenConfigRecoverySmoke { [pscustomobject]@{ Scenario = 'broken-config-recovery' } }
            Mock Invoke-NativeUnavailableStatusSmoke { [pscustomobject]@{ Scenario = 'native-unavailable-status' } }
            Mock Invoke-OpenAiCompatibleSuccessSmoke { [pscustomobject]@{ Scenario = 'openai-compatible-success' } }
            Mock Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-insert-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success' }
            }
            Mock Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEscapeSmoke {
                [pscustomobject]@{ Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success' }
            }

            $results = Invoke-TalkDesktopReleaseSmoke `
                -BinaryPath 'C:\fake\talk-desktop.exe' `
                -SmokeRoot $tempRoot

            ((@($results).Scenario) -contains 'openai-compatible-audio-input-focus-switch-copy-popup-success') | Should Be $true
            ((@($results).Scenario) -contains 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success') | Should Be $true
            ((@($results).Scenario) -contains 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success') | Should Be $true
        }
        finally {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
    }

    It 'waits for captured insert text before reading the insert-session log in the insert smoke flow' {
        $scriptText = Get-Content -LiteralPath $scriptPath -Raw -Encoding UTF8
        $startMarker = 'function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke'
        $endMarker = 'function Invoke-HotkeyConflictSmoke'
        $startIndex = $scriptText.IndexOf($startMarker)
        $endIndex = $scriptText.IndexOf($endMarker)
        $scenarioText = $scriptText.Substring($startIndex, $endIndex - $startIndex)

        $scenarioText.IndexOf('Wait-TalkTextCaptureContainsWithForegroundRefresh') | Should BeGreaterThan -1
        $scenarioText.IndexOf('Wait-LatestSessionLog') | Should BeGreaterThan -1
        $scenarioText.IndexOf('Wait-TalkTextCaptureContainsWithForegroundRefresh') | Should BeLessThan $scenarioText.IndexOf('Wait-LatestSessionLog')
    }
}
