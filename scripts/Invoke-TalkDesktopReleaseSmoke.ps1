[CmdletBinding()]
param(
    [string]$BinaryPath,
    [string]$ReleaseDir,
    [string]$SmokeRoot,
    [switch]$ContinueOnFailure,
    [string[]]$Scenario = @(
        'cancel-and-status',
        'hotkey-conflict',
        'broken-config-recovery',
        'native-unavailable-status',
        'openai-compatible-success',
        'openai-compatible-audio-input-insert-success'
    )
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-TalkDesktopScriptRootLayout {
    param([string]$ScriptRoot = $PSScriptRoot)

    $resolvedScriptRoot = [System.IO.Path]::GetFullPath($ScriptRoot)
    $talkBaseRoot = Split-Path -Parent $resolvedScriptRoot
    $neuroRoot = Split-Path -Parent $talkBaseRoot
    $isPackagedReleaseDir = Test-Path -LiteralPath (Join-Path $resolvedScriptRoot 'talk-desktop.exe')
    $releaseRoot = if ($isPackagedReleaseDir) {
        $talkBaseRoot
    } else {
        Join-Path $neuroRoot 'release\Talk'
    }

    [pscustomobject][ordered]@{
        scriptRoot = $resolvedScriptRoot
        talkBaseRoot = $talkBaseRoot
        neuroRoot = $neuroRoot
        releaseRoot = $releaseRoot
        isPackagedReleaseDir = $isPackagedReleaseDir
        defaultReleaseDir = if ($isPackagedReleaseDir) { $resolvedScriptRoot } else { $null }
    }
}

function Get-TalkRepoRoot {
    (Resolve-TalkDesktopScriptRootLayout).talkBaseRoot
}

function Get-NeuroRoot {
    (Resolve-TalkDesktopScriptRootLayout).neuroRoot
}

function Get-TalkReleaseRoot {
    (Resolve-TalkDesktopScriptRootLayout).releaseRoot
}

function Resolve-LatestTalkReleaseDir {
    param([string]$ReleaseRoot)

    $releaseRoot = if ([string]::IsNullOrWhiteSpace($ReleaseRoot)) {
        Get-TalkReleaseRoot
    } else {
        [System.IO.Path]::GetFullPath($ReleaseRoot)
    }
    if (-not (Test-Path -LiteralPath $releaseRoot)) {
        throw "Talk release root does not exist: $releaseRoot"
    }

    $latest = Get-ChildItem -LiteralPath $releaseRoot -Directory |
        Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'talk-desktop.exe') } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -eq $latest) {
        throw "Talk release root does not contain any Talk desktop release directories: $releaseRoot"
    }

    $latest.FullName
}

function Resolve-TalkDesktopBinaryPath {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$ScriptRoot = $PSScriptRoot
    )

    if (-not [string]::IsNullOrWhiteSpace($BinaryPath)) {
        $resolvedBinaryPath = [System.IO.Path]::GetFullPath($BinaryPath)
        if (-not (Test-Path -LiteralPath $resolvedBinaryPath)) {
            throw "Talk desktop binary does not exist: $resolvedBinaryPath"
        }
        return $resolvedBinaryPath
    }

    $scriptLayout = Resolve-TalkDesktopScriptRootLayout -ScriptRoot $ScriptRoot
    $candidateReleaseDir = $ReleaseDir
    if ([string]::IsNullOrWhiteSpace($candidateReleaseDir)) {
        $candidateReleaseDir = if ($scriptLayout.isPackagedReleaseDir) {
            $scriptLayout.defaultReleaseDir
        } else {
            Resolve-LatestTalkReleaseDir -ReleaseRoot $scriptLayout.releaseRoot
        }
    }

    $resolvedReleaseDir = [System.IO.Path]::GetFullPath($candidateReleaseDir)
    if (-not (Test-Path -LiteralPath $resolvedReleaseDir)) {
        throw "Talk release directory does not exist: $resolvedReleaseDir"
    }

    $resolvedBinaryPath = Join-Path $resolvedReleaseDir 'talk-desktop.exe'
    if (-not (Test-Path -LiteralPath $resolvedBinaryPath)) {
        throw "Talk desktop binary does not exist in release directory: $resolvedBinaryPath"
    }

    $resolvedBinaryPath
}

function Escape-TomlPath {
    param([Parameter(Mandatory = $true)][string]$Path)

    $Path.Replace('\', '\\')
}

function New-TalkSmokeConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$Transcript,
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [ValidateSet('silent', 'native_windows')][string]$AudioBackend = 'silent',
        [ValidateSet('dry_run', 'clipboard_paste')][string]$OutputMode = 'dry_run',
        [ValidateSet('fallback', 'native_windows')][string]$ClipboardBackend = 'fallback'
    )

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "$Hotkey"

[audio]
backend = "$AudioBackend"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "mock"
mock_transcript = "$Transcript"

[output]
mode = "$OutputMode"
restore_clipboard = true
clipboard_backend = "$ClipboardBackend"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkSmokeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$Transcript,
        [ValidateSet('silent', 'native_windows')][string]$AudioBackend = 'silent',
        [ValidateSet('dry_run', 'clipboard_paste')][string]$OutputMode = 'dry_run',
        [ValidateSet('fallback', 'native_windows')][string]$ClipboardBackend = 'fallback'
    )

    $scenarioRoot = Split-Path -Parent $ConfigPath
    $audioDir = Join-Path $scenarioRoot 'audio'
    $logsDir = Join-Path $scenarioRoot 'logs'
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    $configText = New-TalkSmokeConfigContent `
        -Hotkey $Hotkey `
        -Transcript $Transcript `
        -AudioDir $audioDir `
        -LogsDir $logsDir `
        -AudioBackend $AudioBackend `
        -OutputMode $OutputMode `
        -ClipboardBackend $ClipboardBackend
    $configText | Set-Content -LiteralPath $ConfigPath -Encoding UTF8
}

function New-TalkHttpProviderSmokeConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$ProviderEndpoint,
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir
    )

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "$Hotkey"

[audio]
backend = "silent"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "http"
endpoint = "$ProviderEndpoint"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkHttpProviderSmokeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$ProviderEndpoint
    )

    $scenarioRoot = Split-Path -Parent $ConfigPath
    $audioDir = Join-Path $scenarioRoot 'audio'
    $logsDir = Join-Path $scenarioRoot 'logs'
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    $configText = New-TalkHttpProviderSmokeConfigContent `
        -Hotkey $Hotkey `
        -ProviderEndpoint $ProviderEndpoint `
        -AudioDir $audioDir `
        -LogsDir $logsDir
    $configText | Set-Content -LiteralPath $ConfigPath -Encoding UTF8
}

function New-TalkOpenAiCompatibleSmokeConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$AudioTranscriptionsEndpoint,
        [Parameter(Mandatory = $true)][string]$ChatCompletionsEndpoint,
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir
    )

    @"
voice_mode = "command"

[trigger]
mode = "toggle"
toggle_shortcut = "$Hotkey"

[audio]
backend = "silent"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "$AudioTranscriptionsEndpoint"
chat_completions_endpoint = "$ChatCompletionsEndpoint"
transcription_model = "gpt-4o-mini-transcribe"
chat_model = "gpt-4o-mini"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkOpenAiCompatibleSmokeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$AudioTranscriptionsEndpoint,
        [Parameter(Mandatory = $true)][string]$ChatCompletionsEndpoint
    )

    $scenarioRoot = Split-Path -Parent $ConfigPath
    $audioDir = Join-Path $scenarioRoot 'audio'
    $logsDir = Join-Path $scenarioRoot 'logs'
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    $configText = New-TalkOpenAiCompatibleSmokeConfigContent `
        -Hotkey $Hotkey `
        -AudioTranscriptionsEndpoint $AudioTranscriptionsEndpoint `
        -ChatCompletionsEndpoint $ChatCompletionsEndpoint `
        -AudioDir $audioDir `
        -LogsDir $logsDir
    $configText | Set-Content -LiteralPath $ConfigPath -Encoding UTF8
}

function New-TalkOpenAiCompatibleChatAudioInputSmokeConfigContent {
    param(
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$ChatCompletionsEndpoint,
        [Parameter(Mandatory = $true)][string]$AudioDir,
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [ValidateSet('transcribe', 'document', 'generate', 'smart', 'dictate', 'polish', 'translate', 'command')][string]$VoiceMode = 'command',
        [ValidateSet('dry_run', 'clipboard_paste')][string]$OutputMode = 'dry_run',
        [ValidateSet('fallback', 'native_windows')][string]$ClipboardBackend = 'fallback'
    )

    @"
voice_mode = "$VoiceMode"

[trigger]
mode = "toggle"
toggle_shortcut = "$Hotkey"

[audio]
backend = "silent"
max_recording_seconds = 5
sample_rate_hz = 16000
channels = 1
temp_dir = "$(Escape-TomlPath $AudioDir)"

[provider]
kind = "openai_compatible"
transcription_transport = "chat_completions_audio_input"
audio_transcriptions_endpoint = "$ChatCompletionsEndpoint"
chat_completions_endpoint = "$ChatCompletionsEndpoint"
transcription_model = "qwen3-asr-flash"
chat_model = "qwen3.7-plus"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "$OutputMode"
restore_clipboard = true
clipboard_backend = "$ClipboardBackend"

[logging]
dir = "$(Escape-TomlPath $LogsDir)"
"@
}

function Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig {
    param(
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [Parameter(Mandatory = $true)][string]$Hotkey,
        [Parameter(Mandatory = $true)][string]$ChatCompletionsEndpoint,
        [ValidateSet('transcribe', 'document', 'generate', 'smart', 'dictate', 'polish', 'translate', 'command')][string]$VoiceMode = 'command',
        [ValidateSet('dry_run', 'clipboard_paste')][string]$OutputMode = 'dry_run',
        [ValidateSet('fallback', 'native_windows')][string]$ClipboardBackend = 'fallback'
    )

    $scenarioRoot = Split-Path -Parent $ConfigPath
    $audioDir = Join-Path $scenarioRoot 'audio'
    $logsDir = Join-Path $scenarioRoot 'logs'
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    $configText = New-TalkOpenAiCompatibleChatAudioInputSmokeConfigContent `
        -Hotkey $Hotkey `
        -ChatCompletionsEndpoint $ChatCompletionsEndpoint `
        -AudioDir $audioDir `
        -LogsDir $logsDir `
        -VoiceMode $VoiceMode `
        -OutputMode $OutputMode `
        -ClipboardBackend $ClipboardBackend
    $configText | Set-Content -LiteralPath $ConfigPath -Encoding UTF8
}

function Write-BrokenTalkSmokeConfig {
    param([Parameter(Mandatory = $true)][string]$ConfigPath)

    $scenarioRoot = Split-Path -Parent $ConfigPath
    New-Item -ItemType Directory -Path $scenarioRoot -Force | Out-Null

    @"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+F22"

[audio]
backend = "silent"
max_recording_seconds =
"@ | Set-Content -LiteralPath $ConfigPath -Encoding UTF8
}

function Write-TalkAudioOverrideFixture {
    param([Parameter(Mandatory = $true)][string]$AudioPath)

    $parent = Split-Path -Parent $AudioPath
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }

    $bytes = [byte[]](
        0x52,0x49,0x46,0x46,0x2C,0x00,0x00,0x00,
        0x57,0x41,0x56,0x45,0x66,0x6D,0x74,0x20,
        0x10,0x00,0x00,0x00,0x01,0x00,0x01,0x00,
        0x80,0x3E,0x00,0x00,0x00,0x7D,0x00,0x00,
        0x02,0x00,0x10,0x00,0x64,0x61,0x74,0x61,
        0x08,0x00,0x00,0x00,0x00,0x00,0x00,0x00,
        0x00,0x00,0x00,0x00
    )
    [System.IO.File]::WriteAllBytes($AudioPath, $bytes)
}

function Write-TalkSmokeProgress {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Message
    )

    $line = '{0} {1}' -f (Get-Date).ToString('o'), $Message
    Add-Content -LiteralPath $Path -Value $line -Encoding UTF8
}

function Write-TalkDesktopSmokeJson {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }

    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText(
        $Path,
        (($Value | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
        $utf8NoBom
    )
}

function Get-TalkDesktopSmokeOptionalPropertyValue {
    param(
        $Object,
        [Parameter(Mandatory = $true)][string]$Name
    )

    if ($null -eq $Object) {
        return $null
    }

    if ($Object -is [System.Collections.IDictionary]) {
        if ($Object.Contains($Name)) {
            return $Object[$Name]
        }
        return $null
    }

    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) {
        return $null
    }
    $property.Value
}

function Get-TalkDesktopForegroundTailFromErrorMessage {
    param([Parameter(Mandatory = $true)][string]$ErrorMessage)

    $match = [regex]::Match($ErrorMessage, 'Foreground tail: \[(?<tail>.*)\]$')
    if (-not $match.Success) {
        return $null
    }

    $tail = [string]$match.Groups['tail'].Value
    if ([string]::IsNullOrWhiteSpace($tail)) {
        return $null
    }

    $tail
}

function Test-TalkDesktopForegroundTrailContainsExternalWindow {
    param(
        [string]$ForegroundTail,
        [Parameter(Mandatory = $true)][string]$TargetWindowTitle
    )

    if ([string]::IsNullOrWhiteSpace($ForegroundTail)) {
        return $false
    }

    $segments = @(
        $ForegroundTail -split '\s+->\s+' |
            ForEach-Object { $_.Trim() } |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    )
    if ($segments.Count -eq 0) {
        return $false
    }

    @($segments | Where-Object { $_ -notlike "*$TargetWindowTitle*" }).Count -gt 0
}

function Get-TalkDesktopInsertFailureClassification {
    param(
        [Parameter(Mandatory = $true)][string]$Scenario,
        [Parameter(Mandatory = $true)][string]$ExpectedOutputText,
        [Parameter(Mandatory = $true)][string]$TargetWindowTitle,
        [Parameter(Mandatory = $true)]$Session,
        [Parameter(Mandatory = $true)][string]$ErrorMessage
    )

    $status = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $Session -Name 'status')
    $outputText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $Session -Name 'output_text')
    $insertOutcome = Get-TalkDesktopSmokeOptionalPropertyValue -Object $Session -Name 'insert_outcome'
    $insertMethod = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $insertOutcome -Name 'method')
    $foregroundTail = Get-TalkDesktopForegroundTailFromErrorMessage -ErrorMessage $ErrorMessage

    if (
        $status -eq 'completed' -and
        $outputText -eq $ExpectedOutputText -and
        $insertMethod -eq 'clipboard_paste' -and
        (Test-TalkDesktopForegroundTrailContainsExternalWindow -ForegroundTail $foregroundTail -TargetWindowTitle $TargetWindowTitle)
    ) {
        return [pscustomobject][ordered]@{
            Scenario = $Scenario
            FailureKind = 'hostile_foreground_environment'
            FailureSummary = 'session completed with clipboard paste, but another foreground window displaced the target before capture'
            ForegroundTail = $foregroundTail
            SessionStatus = $status
            InsertMethod = $insertMethod
        }
    }

    $null
}

function Get-TalkDesktopPrimerFailureClassification {
    param(
        [Parameter(Mandatory = $true)][string]$Scenario,
        [Parameter(Mandatory = $true)][string]$TargetWindowTitle,
        [Parameter(Mandatory = $true)][string]$ErrorMessage
    )

    $foregroundTail = Get-TalkDesktopForegroundTailFromErrorMessage -ErrorMessage $ErrorMessage
    if (Test-TalkDesktopForegroundTrailContainsExternalWindow -ForegroundTail $foregroundTail -TargetWindowTitle $TargetWindowTitle) {
        return [pscustomobject][ordered]@{
            Scenario = $Scenario
            FailureKind = 'hostile_foreground_environment'
            FailureSummary = 'foreground target could not retain focus long enough to accept primer input before hotkey start'
            ForegroundTail = $foregroundTail
        }
    }

    $null
}

function Ensure-TalkDesktopSmokeWin32Type {
    if (([System.Management.Automation.PSTypeName]'TalkDesktopSmokeWin32').Type) {
        return
    }

    Add-Type @"
using System;
using System.Text;
using System.Runtime.InteropServices;
public struct TalkDesktopSmokeRect {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
}
public static class TalkDesktopSmokeWin32 {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr hWnd, EnumWindowsProc lpEnumFunc, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr hWnd, StringBuilder className, int maxCount);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int maxCount);
    [DllImport("user32.dll")] public static extern int GetWindowTextLengthW(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr SetActiveWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageW(IntPtr hWnd, uint Msg, IntPtr wParam, string lParam);
    [DllImport("user32.dll", EntryPoint = "SendMessageW")] public static extern IntPtr SendMessageIntPtrW(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam);
    [DllImport("user32.dll", SetLastError = true)] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out TalkDesktopSmokeRect rect);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [DllImport("user32.dll", SetLastError = true)] public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
}
"@
}

function Get-WindowText {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $length = [TalkDesktopSmokeWin32]::GetWindowTextLengthW($Hwnd)
    $builder = New-Object System.Text.StringBuilder ($length + 64)
    [void][TalkDesktopSmokeWin32]::GetWindowTextW($Hwnd, $builder, $builder.Capacity)
    $builder.ToString()
}

function Get-WindowClass {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $builder = New-Object System.Text.StringBuilder 256
    [void][TalkDesktopSmokeWin32]::GetClassNameW($Hwnd, $builder, $builder.Capacity)
    $builder.ToString()
}

function Get-TalkDesktopForegroundWindowHwnd {
    Ensure-TalkDesktopSmokeWin32Type
    [TalkDesktopSmokeWin32]::GetForegroundWindow()
}

function Get-TalkDesktopForegroundWindowDebugString {
    $foregroundHwnd = Get-TalkDesktopForegroundWindowHwnd
    if ($foregroundHwnd -eq [System.IntPtr]::Zero) {
        return '0x0 [] []'
    }

    $windowText = Get-WindowText -Hwnd $foregroundHwnd
    $windowClass = Get-WindowClass -Hwnd $foregroundHwnd
    ('0x{0:X} [{1}] [{2}]' -f $foregroundHwnd.ToInt64(), $windowText, $windowClass)
}

function Show-TalkDesktopWindowRestored {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $SW_RESTORE = 9
    [void][TalkDesktopSmokeWin32]::ShowWindowAsync($Hwnd, $SW_RESTORE)
}

function Set-TalkDesktopWindowTopmostState {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [switch]$Enable
    )

    $SWP_NOSIZE = 0x0001
    $SWP_NOMOVE = 0x0002
    $SWP_SHOWWINDOW = 0x0040
    $insertAfter = if ($Enable) { [System.IntPtr](-1) } else { [System.IntPtr](-2) }
    [void][TalkDesktopSmokeWin32]::SetWindowPos(
        $Hwnd,
        $insertAfter,
        0,
        0,
        0,
        0,
        ($SWP_NOSIZE -bor $SWP_NOMOVE -bor $SWP_SHOWWINDOW)
    )
}

function Bring-TalkDesktopWindowToTop {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    [void][TalkDesktopSmokeWin32]::BringWindowToTop($Hwnd)
}

function Invoke-TalkDesktopForegroundWindowNative {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [switch]$UseInputUnlock
    )

    $foregroundHwnd = [TalkDesktopSmokeWin32]::GetForegroundWindow()
    $foregroundProcessId = 0
    $foregroundThread = if ($foregroundHwnd -ne [System.IntPtr]::Zero) {
        [TalkDesktopSmokeWin32]::GetWindowThreadProcessId($foregroundHwnd, [ref]$foregroundProcessId)
    } else {
        0
    }
    $targetProcessId = 0
    $targetThread = [TalkDesktopSmokeWin32]::GetWindowThreadProcessId($Hwnd, [ref]$targetProcessId)
    $currentThread = [TalkDesktopSmokeWin32]::GetCurrentThreadId()
    $attachedForeground = $false
    $attachedTarget = $false
    try {
        if ($foregroundThread -ne 0 -and $foregroundThread -ne $currentThread) {
            $attachedForeground = [TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $foregroundThread, $true)
        }
        if (
            $targetThread -ne 0 -and
            $targetThread -ne $currentThread -and
            $targetThread -ne $foregroundThread
        ) {
            $attachedTarget = [TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $targetThread, $true)
        }

        if ($UseInputUnlock) {
            $VK_MENU = 0x12
            $KEYEVENTF_KEYUP = 0x0002
            [TalkDesktopSmokeWin32]::keybd_event([byte]$VK_MENU, 0, 0, [UIntPtr]::Zero)
            [TalkDesktopSmokeWin32]::keybd_event([byte]$VK_MENU, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        }
        [void][TalkDesktopSmokeWin32]::SetForegroundWindow($Hwnd)
    }
    finally {
        if ($attachedTarget) {
            [void][TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $targetThread, $false)
        }
        if ($attachedForeground) {
            [void][TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $foregroundThread, $false)
        }
    }
}

function Wait-TalkDesktopForegroundWindow {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$TargetHwnd,
        [int]$TimeoutMs = 1200,
        [int]$PollIntervalMs = 50
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $foregroundHwnd = Get-TalkDesktopForegroundWindowHwnd
        if ($foregroundHwnd -eq $TargetHwnd) {
            return $true
        }
        Start-Sleep -Milliseconds $PollIntervalMs
    } while ((Get-Date) -lt $deadline)

    $false
}

function Find-WindowByProcessIdAndClass {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 10000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $script:talkDesktopWindowMatch = [System.IntPtr]::Zero
        $callback = [TalkDesktopSmokeWin32+EnumWindowsProc]{
            param([System.IntPtr]$hWnd, [System.IntPtr]$lParam)
            $windowProcessId = 0
            [void][TalkDesktopSmokeWin32]::GetWindowThreadProcessId($hWnd, [ref]$windowProcessId)
            if ($windowProcessId -eq $TargetProcessId -and (Get-WindowClass $hWnd) -eq $ClassName) {
                $script:talkDesktopWindowMatch = $hWnd
                return $false
            }
            return $true
        }

        [void][TalkDesktopSmokeWin32]::EnumWindows($callback, [System.IntPtr]::Zero)
        if ($script:talkDesktopWindowMatch -ne [System.IntPtr]::Zero) {
            return $script:talkDesktopWindowMatch
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    [System.IntPtr]::Zero
}

function Find-VisibleWindowByProcessIdAndClass {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 5000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $script:talkDesktopVisibleWindowMatch = [System.IntPtr]::Zero
        $callback = [TalkDesktopSmokeWin32+EnumWindowsProc]{
            param([System.IntPtr]$hWnd, [System.IntPtr]$lParam)
            $windowProcessId = 0
            [void][TalkDesktopSmokeWin32]::GetWindowThreadProcessId($hWnd, [ref]$windowProcessId)
            if (
                $windowProcessId -eq $TargetProcessId -and
                (Get-WindowClass $hWnd) -eq $ClassName -and
                [TalkDesktopSmokeWin32]::IsWindowVisible($hWnd)
            ) {
                $script:talkDesktopVisibleWindowMatch = $hWnd
                return $false
            }
            return $true
        }

        [void][TalkDesktopSmokeWin32]::EnumWindows($callback, [System.IntPtr]::Zero)
        if ($script:talkDesktopVisibleWindowMatch -ne [System.IntPtr]::Zero) {
            return $script:talkDesktopVisibleWindowMatch
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    [System.IntPtr]::Zero
}

function Wait-TalkDesktopVisibleWindowByProcessIdAndClass {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 5000
    )

    $window = Find-VisibleWindowByProcessIdAndClass `
        -TargetProcessId $TargetProcessId `
        -ClassName $ClassName `
        -TimeoutMs $TimeoutMs
    if ($window -eq [System.IntPtr]::Zero) {
        throw "Timed out waiting for visible window class [$ClassName] in process [$TargetProcessId]"
    }

    $window
}

function Find-DialogByProcessIdAndTitle {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$Title,
        [int]$TimeoutMs = 5000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $script:talkDesktopDialogMatch = [System.IntPtr]::Zero
        $callback = [TalkDesktopSmokeWin32+EnumWindowsProc]{
            param([System.IntPtr]$hWnd, [System.IntPtr]$lParam)
            $windowProcessId = 0
            [void][TalkDesktopSmokeWin32]::GetWindowThreadProcessId($hWnd, [ref]$windowProcessId)
            if ($windowProcessId -eq $TargetProcessId -and (Get-WindowText $hWnd) -eq $Title) {
                $script:talkDesktopDialogMatch = $hWnd
                return $false
            }
            return $true
        }

        [void][TalkDesktopSmokeWin32]::EnumWindows($callback, [System.IntPtr]::Zero)
        if ($script:talkDesktopDialogMatch -ne [System.IntPtr]::Zero) {
            return $script:talkDesktopDialogMatch
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    [System.IntPtr]::Zero
}

function Get-ChildWindowTexts {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $script:talkDesktopChildTexts = New-Object System.Collections.Generic.List[string]
    $callback = [TalkDesktopSmokeWin32+EnumWindowsProc]{
        param([System.IntPtr]$child, [System.IntPtr]$lParam)
        $text = Get-WindowText $child
        if (-not [string]::IsNullOrWhiteSpace($text)) {
            $script:talkDesktopChildTexts.Add($text) | Out-Null
        }
        return $true
    }

    [void][TalkDesktopSmokeWin32]::EnumChildWindows($Hwnd, $callback, [System.IntPtr]::Zero)
    $script:talkDesktopChildTexts.ToArray()
}

function Get-TalkDesktopDialogText {
    param([Parameter(Mandatory = $true)][System.IntPtr]$DialogHwnd)

    (Get-ChildWindowTexts -Hwnd $DialogHwnd) -join "`n"
}

function Wait-TalkDesktopDialogText {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$DialogHwnd,
        [Parameter(Mandatory = $true)][string]$ExpectedText,
        [int]$TimeoutMs = 5000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    $lastText = ''
    do {
        $lastText = Get-TalkDesktopDialogText -DialogHwnd $DialogHwnd
        if ($lastText -like "*$ExpectedText*") {
            return $lastText
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    throw "Talk dialog text did not contain [$ExpectedText]. Last text: [$lastText]"
}

function Send-TalkDesktopMenuCommand {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][int]$CommandId
    )

    $WM_COMMAND = 0x0111
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_COMMAND,
        [System.IntPtr]$CommandId,
        [System.IntPtr]::Zero
    )
}

function New-TalkDesktopMouseLParam {
    param(
        [Parameter(Mandatory = $true)][int]$X,
        [Parameter(Mandatory = $true)][int]$Y
    )

    (($Y -band 0xFFFF) -shl 16) -bor ($X -band 0xFFFF)
}

function Convert-TalkDesktopOverlayLengthForDpi {
    param(
        [Parameter(Mandatory = $true)][int]$Length,
        [Parameter(Mandatory = $true)][int]$Dpi
    )

    [int][Math]::Round(
        ([double]$Length * ([double]$Dpi / 96.0)),
        0,
        [System.MidpointRounding]::AwayFromZero
    )
}

function Get-TalkDesktopCopyPopupCopyButtonClickPointForDpi {
    param(
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height,
        [Parameter(Mandatory = $true)][int]$Dpi
    )

    $rect = Get-TalkDesktopCopyPopupCopyButtonRectForDpi -Width $Width -Height $Height -Dpi $Dpi

    [pscustomobject][ordered]@{
        X = [int][Math]::Floor(($rect.Left + $rect.Right) / 2)
        Y = [int][Math]::Floor(($rect.Top + $rect.Bottom) / 2)
    }
}

function Get-TalkDesktopCopyPopupLayoutSizeForDpi {
    param(
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height,
        [Parameter(Mandatory = $true)][int]$Dpi
    )

    [pscustomobject][ordered]@{
        Width = Convert-TalkDesktopOverlayLengthForDpi -Length 388 -Dpi $Dpi
        Height = Convert-TalkDesktopOverlayLengthForDpi -Length 156 -Dpi $Dpi
    }
}

function Get-TalkDesktopCopyPopupCopyButtonRectForDpi {
    param(
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height,
        [Parameter(Mandatory = $true)][int]$Dpi
    )

    $layout = Get-TalkDesktopCopyPopupLayoutSizeForDpi -Width $Width -Height $Height -Dpi $Dpi
    $halfWidth = Convert-TalkDesktopOverlayLengthForDpi -Length 44 -Dpi $Dpi
    $top = Convert-TalkDesktopOverlayLengthForDpi -Length 114 -Dpi $Dpi
    $bottom = Convert-TalkDesktopOverlayLengthForDpi -Length 144 -Dpi $Dpi
    $layoutLeft = [int][Math]::Floor($layout.Width / 2) - $halfWidth
    $layoutRight = [int][Math]::Floor($layout.Width / 2) + $halfWidth
    $xScale = if ($Width -gt 0 -and $layout.Width -gt $Width) {
        [double]$layout.Width / [double]$Width
    } else {
        1.0
    }
    $yScale = if ($Height -gt 0 -and $layout.Height -gt $Height) {
        [double]$layout.Height / [double]$Height
    } else {
        1.0
    }
    $left = [int][Math]::Floor([double]$layoutLeft / $xScale)
    $right = [int][Math]::Ceiling([double]$layoutRight / $xScale)
    $top = [int][Math]::Floor([double]$top / $yScale)
    $bottom = [int][Math]::Ceiling([double]$bottom / $yScale)

    [pscustomobject][ordered]@{
        Left = $left
        Top = $top
        Right = $right
        Bottom = $bottom
    }
}

function Get-TalkDesktopCopyPopupClickDiagnosticForDpi {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][int]$Width,
        [Parameter(Mandatory = $true)][int]$Height,
        [Parameter(Mandatory = $true)][int]$Dpi
    )

    $layout = Get-TalkDesktopCopyPopupLayoutSizeForDpi -Width $Width -Height $Height -Dpi $Dpi
    $rect = Get-TalkDesktopCopyPopupCopyButtonRectForDpi -Width $Width -Height $Height -Dpi $Dpi
    $point = Get-TalkDesktopCopyPopupCopyButtonClickPointForDpi -Width $Width -Height $Height -Dpi $Dpi
    $pointInsideRect = $point.X -ge $rect.Left -and $point.X -lt $rect.Right -and $point.Y -ge $rect.Top -and $point.Y -lt $rect.Bottom
    $pointInsideObservedClient = $point.X -ge 0 -and $point.X -lt $Width -and $point.Y -ge 0 -and $point.Y -lt $Height

    [pscustomobject][ordered]@{
        Hwnd = ('0x{0:X}' -f $Hwnd.ToInt64())
        Width = $Width
        Height = $Height
        LayoutWidth = $layout.Width
        LayoutHeight = $layout.Height
        Dpi = $Dpi
        X = $point.X
        Y = $point.Y
        CopyRectLeft = $rect.Left
        CopyRectTop = $rect.Top
        CopyRectRight = $rect.Right
        CopyRectBottom = $rect.Bottom
        PointInsideCopyRect = $pointInsideRect
        PointInsideObservedClient = $pointInsideObservedClient
    }
}

function Get-TalkDesktopWindowDpi {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    Ensure-TalkDesktopSmokeWin32Type
    try {
        $dpi = [TalkDesktopSmokeWin32]::GetDpiForWindow($Hwnd)
        if ($dpi -gt 0) {
            return [int]$dpi
        }
    }
    catch {
        return 96
    }

    96
}

function Get-TalkDesktopWindowClientSize {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    Ensure-TalkDesktopSmokeWin32Type
    $rect = New-Object TalkDesktopSmokeRect
    if (-not [TalkDesktopSmokeWin32]::GetClientRect($Hwnd, [ref]$rect)) {
        throw "Unable to resolve Talk desktop window client size for hwnd 0x$($Hwnd.ToInt64().ToString('X'))"
    }

    [pscustomobject][ordered]@{
        Width = [int]($rect.Right - $rect.Left)
        Height = [int]($rect.Bottom - $rect.Top)
    }
}

function Get-TalkDesktopCopyPopupCopyButtonClickPoint {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $size = Get-TalkDesktopWindowClientSize -Hwnd $Hwnd
    $dpi = Get-TalkDesktopWindowDpi -Hwnd $Hwnd
    Get-TalkDesktopCopyPopupCopyButtonClickPointForDpi -Width $size.Width -Height $size.Height -Dpi $dpi
}

function Send-TalkDesktopWindowLeftClick {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][int]$X,
        [Parameter(Mandatory = $true)][int]$Y
    )

    $WM_MOUSEMOVE = 0x0200
    $WM_LBUTTONDOWN = 0x0201
    $WM_LBUTTONUP = 0x0202
    $MK_LBUTTON = 0x0001
    $lParam = New-TalkDesktopMouseLParam -X $X -Y $Y
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_MOUSEMOVE,
        [System.IntPtr]::Zero,
        [System.IntPtr]$lParam
    )
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_LBUTTONDOWN,
        [System.IntPtr]$MK_LBUTTON,
        [System.IntPtr]$lParam
    )
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_LBUTTONUP,
        [System.IntPtr]::Zero,
        [System.IntPtr]$lParam
    )
}

function Send-TalkDesktopWindowVirtualKeyInput {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][int]$VirtualKey
    )

    Ensure-TalkDesktopSmokeWin32Type
    $WM_KEYDOWN = 0x0100
    $WM_KEYUP = 0x0101
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_KEYDOWN,
        [System.IntPtr]$VirtualKey,
        [System.IntPtr]::Zero
    )
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_KEYUP,
        [System.IntPtr]$VirtualKey,
        [System.IntPtr]::Zero
    )
    Start-Sleep -Milliseconds 150
}

function Send-TalkDesktopHotkeyMessage {
    param([Parameter(Mandatory = $true)][System.IntPtr]$Hwnd)

    $WM_HOTKEY = 0x0312
    $HOTKEY_ID = 1
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $Hwnd,
        $WM_HOTKEY,
        [System.IntPtr]$HOTKEY_ID,
        [System.IntPtr]::Zero
    )
}

function Close-TalkDesktopDialog {
    param([Parameter(Mandatory = $true)][System.IntPtr]$DialogHwnd)

    $WM_CLOSE = 0x0010
    [void][TalkDesktopSmokeWin32]::PostMessageW(
        $DialogHwnd,
        $WM_CLOSE,
        [System.IntPtr]::Zero,
        [System.IntPtr]::Zero
    )
}

function Wait-TalkDesktopDialogClosed {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$Title,
        [int]$TimeoutMs = 5000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $dialog = Find-DialogByProcessIdAndTitle `
            -TargetProcessId $TargetProcessId `
            -Title $Title `
            -TimeoutMs 50
        if ($dialog -eq [System.IntPtr]::Zero) {
            return
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    throw "Talk dialog '$Title' did not close for pid $TargetProcessId"
}

function Wait-TalkDesktopWindowHiddenByProcessIdAndClass {
    param(
        [Parameter(Mandatory = $true)][int]$TargetProcessId,
        [Parameter(Mandatory = $true)][string]$ClassName,
        [int]$TimeoutMs = 5000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        $window = Find-VisibleWindowByProcessIdAndClass `
            -TargetProcessId $TargetProcessId `
            -ClassName $ClassName `
            -TimeoutMs 50
        if ($window -eq [System.IntPtr]::Zero) {
            return
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    throw "Visible window class [$ClassName] did not hide for pid $TargetProcessId"
}

function Get-TalkDesktopClipboardText {
    $clipboard = Get-Clipboard -Raw -ErrorAction Stop
    if ($null -eq $clipboard) {
        return ''
    }
    [string]$clipboard
}

function Set-TalkDesktopClipboardText {
    param(
        [Parameter(Mandatory = $true)][AllowEmptyString()][string]$Value,
        [int]$MaxAttempts = 5,
        [int]$RetryDelayMs = 120
    )

    $attemptCount = [Math]::Max(1, $MaxAttempts)
    for ($attempt = 1; $attempt -le $attemptCount; $attempt++) {
        try {
            Set-Clipboard -Value $Value -ErrorAction Stop
            return
        }
        catch {
            if ($attempt -ge $attemptCount) {
                throw
            }
            Start-Sleep -Milliseconds $RetryDelayMs
        }
    }
}

function Set-TalkDesktopForegroundWindow {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [int]$MaxAttempts = 3,
        [int]$ForegroundTimeoutMs = 1200,
        [int]$RetryDelayMs = 80,
        [switch]$KeepTopmost,
        [switch]$UseInputUnlock
    )

    Ensure-TalkDesktopSmokeWin32Type

    $attemptCount = [Math]::Max(1, $MaxAttempts)
    $foregroundAcquired = $false
    for ($attempt = 1; $attempt -le $attemptCount; $attempt++) {
        try {
            Show-TalkDesktopWindowRestored -Hwnd $Hwnd
            Set-TalkDesktopWindowTopmostState -Hwnd $Hwnd -Enable
            Start-Sleep -Milliseconds 60
            Bring-TalkDesktopWindowToTop -Hwnd $Hwnd
            Invoke-TalkDesktopForegroundWindowNative -Hwnd $Hwnd -UseInputUnlock:$UseInputUnlock
            if (Wait-TalkDesktopForegroundWindow -TargetHwnd $Hwnd -TimeoutMs $ForegroundTimeoutMs) {
                $foregroundAcquired = $true
                break
            }
        }
        finally {
            if (-not $KeepTopmost) {
                Set-TalkDesktopWindowTopmostState -Hwnd $Hwnd -Enable:$false
            }
        }

        if ($attempt -lt $attemptCount) {
            Start-Sleep -Milliseconds $RetryDelayMs
        }
    }

    Start-Sleep -Milliseconds 120
    $foregroundAcquired
}

function Set-TalkDesktopChildInputFocus {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$TargetHwnd,
        [Parameter(Mandatory = $true)][System.IntPtr]$ChildHwnd
    )

    Ensure-TalkDesktopSmokeWin32Type
    if ($TargetHwnd -eq [System.IntPtr]::Zero -or $ChildHwnd -eq [System.IntPtr]::Zero) {
        return $false
    }

    $processId = 0
    $targetThread = [TalkDesktopSmokeWin32]::GetWindowThreadProcessId($TargetHwnd, [ref]$processId)
    if ($targetThread -eq 0) {
        return $false
    }

    $currentThread = [TalkDesktopSmokeWin32]::GetCurrentThreadId()
    $attached = $false
    try {
        if ($currentThread -ne $targetThread) {
            $attached = [TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $targetThread, $true)
        }

        [void][TalkDesktopSmokeWin32]::SetActiveWindow($TargetHwnd)
        [void][TalkDesktopSmokeWin32]::SetFocus($ChildHwnd)
        Start-Sleep -Milliseconds 60
        $true
    }
    finally {
        if ($attached) {
            [void][TalkDesktopSmokeWin32]::AttachThreadInput($currentThread, $targetThread, $false)
        }
    }
}

function Set-TalkTextCaptureTargetForeground {
    param([Parameter(Mandatory = $true)]$Target)

    $foregroundAcquired = Set-TalkDesktopForegroundWindow -Hwnd $Target.Hwnd
    $childHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $Target -Name 'TextBoxHwnd'
    if ($null -ne $childHwnd -and $childHwnd -ne [System.IntPtr]::Zero) {
        Set-TalkDesktopChildInputFocus -TargetHwnd $Target.Hwnd -ChildHwnd $childHwnd | Out-Null
    }
    Start-Sleep -Milliseconds 60
    $foregroundAcquired
}

function Assert-TalkTextCaptureTargetForeground {
    param(
        [Parameter(Mandatory = $true)]$Target,
        [Parameter(Mandatory = $true)][string]$Name
    )

    $foregroundAcquired = Set-TalkDesktopForegroundWindow `
        -Hwnd $Target.Hwnd `
        -MaxAttempts 8 `
        -ForegroundTimeoutMs 1600 `
        -RetryDelayMs 120 `
        -KeepTopmost `
        -UseInputUnlock
    $childHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $Target -Name 'TextBoxHwnd'
    if ($null -ne $childHwnd -and $childHwnd -ne [System.IntPtr]::Zero) {
        Set-TalkDesktopChildInputFocus -TargetHwnd $Target.Hwnd -ChildHwnd $childHwnd | Out-Null
    }
    Start-Sleep -Milliseconds 80
    $foregroundAcquired = $foregroundAcquired -and (Wait-TalkDesktopForegroundWindow -TargetHwnd $Target.Hwnd -TimeoutMs 1000)
    if (-not $foregroundAcquired) {
        $foregroundSummary = Get-TalkDesktopForegroundWindowDebugString
        throw "Expected Talk $Name text capture target to be foreground before hotkey, foreground [$foregroundSummary]"
    }

    $true
}

function Invoke-TalkDesktopPinnedWindowOperation {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][scriptblock]$ScriptBlock
    )

    Set-TalkDesktopWindowTopmostState -Hwnd $Hwnd -Enable
    try {
        & $ScriptBlock
    }
    finally {
        Set-TalkDesktopWindowTopmostState -Hwnd $Hwnd -Enable:$false
    }
}

function Resolve-TalkVirtualKeyCode {
    param([Parameter(Mandatory = $true)][string]$KeyToken)

    $normalized = $KeyToken.Trim().ToUpperInvariant()
    switch -Regex ($normalized) {
        '^SPACE$' {
            return 0x20
        }
        '^SLASH$|^/$' {
            return 0xBF
        }
        '^RIGHT[ _-]?ALT$|^RALT$' {
            return 0xA5
        }
        '^[A-Z]$' {
            return [int][char]$normalized
        }
        '^[0-9]$' {
            return [int][char]$normalized
        }
        '^F([1-9]|1[0-9]|2[0-4])$' {
            return 0x70 + ([int]$Matches[1] - 1)
        }
        default {
            throw "Unsupported Talk desktop hotkey key token for smoke probe: $KeyToken"
        }
    }
}

function Resolve-TalkDesktopGlobalHotkeySequence {
    param([Parameter(Mandatory = $true)][string]$Shortcut)

    $tokens = @($Shortcut -split '\+' | ForEach-Object { $_.Trim() } | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($tokens.Count -eq 0) {
        throw 'Talk desktop hotkey shortcut must not be blank'
    }

    $modifierKeys = New-Object System.Collections.Generic.List[int]
    $triggerKey = $null
    foreach ($token in $tokens) {
        $normalizedToken = $token.ToUpperInvariant()
        switch -Regex ($normalizedToken) {
            '^RIGHT[ _-]?ALT$|^RALT$' {
                if ($tokens.Count -eq 1) {
                    if ($null -ne $triggerKey) {
                        throw "Talk desktop smoke only supports a single non-modifier trigger key, got [$Shortcut]"
                    }
                    $triggerKey = Resolve-TalkVirtualKeyCode -KeyToken $token
                }
                else {
                    $modifierKeys.Add(0xA5) | Out-Null
                }
            }
            '^CTRL$' { $modifierKeys.Add(0x11) | Out-Null }
            '^CONTROL$' { $modifierKeys.Add(0x11) | Out-Null }
            '^ALT$' { $modifierKeys.Add(0x12) | Out-Null }
            '^SHIFT$' { $modifierKeys.Add(0x10) | Out-Null }
            '^WIN$' { $modifierKeys.Add(0x5B) | Out-Null }
            default {
                if ($null -ne $triggerKey) {
                    throw "Talk desktop smoke only supports a single non-modifier trigger key, got [$Shortcut]"
                }
                $triggerKey = Resolve-TalkVirtualKeyCode -KeyToken $token
            }
        }
    }

    if ($null -eq $triggerKey) {
        throw "Talk desktop hotkey shortcut must include a non-modifier trigger key: $Shortcut"
    }

    $downKeys = New-Object System.Collections.Generic.List[UInt16]
    $upKeys = New-Object System.Collections.Generic.List[UInt16]
    foreach ($modifierKey in $modifierKeys) {
        $downKeys.Add([UInt16]$modifierKey) | Out-Null
    }
    $downKeys.Add([UInt16]$triggerKey) | Out-Null
    $upKeys.Add([UInt16]$triggerKey) | Out-Null
    for ($index = $modifierKeys.Count - 1; $index -ge 0; $index--) {
        $upKeys.Add([UInt16]$modifierKeys[$index]) | Out-Null
    }

    [pscustomobject]@{
        DownKeys = $downKeys.ToArray()
        UpKeys = $upKeys.ToArray()
    }
}

function Invoke-TalkDesktopGlobalHotkeyDown {
    param([Parameter(Mandatory = $true)][string]$Shortcut)

    Ensure-TalkDesktopSmokeWin32Type
    $sequence = Resolve-TalkDesktopGlobalHotkeySequence -Shortcut $Shortcut
    foreach ($virtualKey in $sequence.DownKeys) {
        [TalkDesktopSmokeWin32]::keybd_event([byte]$virtualKey, 0, 0, [UIntPtr]::Zero)
    }
}

function Invoke-TalkDesktopGlobalHotkeyUp {
    param([Parameter(Mandatory = $true)][string]$Shortcut)

    Ensure-TalkDesktopSmokeWin32Type
    $sequence = Resolve-TalkDesktopGlobalHotkeySequence -Shortcut $Shortcut
    $KEYEVENTF_KEYUP = 0x0002
    foreach ($virtualKey in $sequence.UpKeys) {
        [TalkDesktopSmokeWin32]::keybd_event([byte]$virtualKey, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
    }
}

function Invoke-TalkDesktopGlobalHotkeyOperation {
    param(
        [Parameter(Mandatory = $true)][string]$Shortcut,
        [Parameter(Mandatory = $true)][scriptblock]$ScriptBlock
    )

    Invoke-TalkDesktopGlobalHotkeyDown -Shortcut $Shortcut
    try {
        & $ScriptBlock
    }
    finally {
        Invoke-TalkDesktopGlobalHotkeyUp -Shortcut $Shortcut
        Start-Sleep -Milliseconds 120
    }
}

function Send-TalkDesktopGlobalHotkeyChord {
    param([Parameter(Mandatory = $true)][string]$Shortcut)

    Invoke-TalkDesktopGlobalHotkeyOperation -Shortcut $Shortcut -ScriptBlock {}
}

function Assert-TextContains {
    param(
        [Parameter(Mandatory = $true)][string]$Haystack,
        [Parameter(Mandatory = $true)][string]$Needle,
        [Parameter(Mandatory = $true)][string]$Context
    )

    if ($Haystack -notlike "*$Needle*") {
        throw "Assertion failed ($Context): missing [$Needle] in [$Haystack]"
    }
}

function Convert-TalkDesktopStatusTextToMap {
    param([Parameter(Mandatory = $true)][string]$DialogText)

    $fields = [ordered]@{}
    foreach ($rawLine in ($DialogText -split "`r?`n")) {
        $line = $rawLine.Trim()
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        $separatorIndex = $line.IndexOf(':')
        if ($separatorIndex -lt 0) {
            continue
        }
        $name = $line.Substring(0, $separatorIndex).Trim()
        $value = $line.Substring($separatorIndex + 1).Trim()
        if ([string]::IsNullOrWhiteSpace($name)) {
            continue
        }
        $fields[$name] = $value
    }
    $fields
}

function Get-TalkDesktopStatusKind {
    param([Parameter(Mandatory = $true)][System.Collections.IDictionary]$Fields)

    $current = [string]$Fields['Current']
    switch ($current) {
        'Talk: idle' { 'idle' }
        'Talk: hotkey unavailable' { 'hotkey_unavailable' }
        'Talk: config unavailable' { 'config_unavailable' }
        'Talk: native unavailable' { 'native_unavailable' }
        'Talk: failed' { 'failed' }
        'Talk: cancelled' { 'cancelled' }
        default {
            if ([string]::IsNullOrWhiteSpace($current)) {
                'unknown'
            } else {
                ($current.ToLowerInvariant() -replace '[^a-z0-9]+', '_').Trim('_')
            }
        }
    }
}

function Get-TalkDesktopStatusFieldMappings {
    @(
        [ordered]@{ Name = 'Current'; SnapshotKey = 'current'; SummaryKey = 'current' }
        [ordered]@{ Name = 'Current detail'; SnapshotKey = 'currentDetail'; SummaryKey = 'current_detail' }
        [ordered]@{ Name = 'Config'; SnapshotKey = 'configPath'; SummaryKey = $null }
        [ordered]@{ Name = 'Logs'; SnapshotKey = 'logsPath'; SummaryKey = $null }
        [ordered]@{ Name = 'Hotkey'; SnapshotKey = 'hotkey'; SummaryKey = 'hotkey' }
        [ordered]@{ Name = 'Hotkey detail'; SnapshotKey = 'hotkeyDetail'; SummaryKey = 'hotkey_detail' }
        [ordered]@{ Name = 'Last session'; SnapshotKey = 'lastSession'; SummaryKey = 'last_session' }
        [ordered]@{ Name = 'Last session detail'; SnapshotKey = 'lastSessionDetail'; SummaryKey = $null }
        [ordered]@{ Name = 'Audio backend'; SnapshotKey = 'audioBackend'; SummaryKey = 'audio_backend' }
        [ordered]@{ Name = 'Audio backend readiness'; SnapshotKey = 'audioBackendReadiness'; SummaryKey = 'audio_backend_readiness' }
        [ordered]@{ Name = 'Audio backend detail'; SnapshotKey = 'audioBackendDetail'; SummaryKey = $null }
        [ordered]@{ Name = 'Clipboard backend'; SnapshotKey = 'clipboardBackend'; SummaryKey = 'clipboard_backend' }
        [ordered]@{ Name = 'Clipboard backend readiness'; SnapshotKey = 'clipboardBackendReadiness'; SummaryKey = 'clipboard_backend_readiness' }
        [ordered]@{ Name = 'Clipboard backend detail'; SnapshotKey = 'clipboardBackendDetail'; SummaryKey = $null }
    )
}

function Convert-TalkDesktopStatusFieldsToSnapshot {
    param([Parameter(Mandatory = $true)][System.Collections.IDictionary]$Fields)

    $snapshot = [ordered]@{}
    foreach ($mapping in (Get-TalkDesktopStatusFieldMappings)) {
        if (-not $Fields.Contains($mapping.Name)) {
            continue
        }
        $value = [string]$Fields[$mapping.Name]
        if ([string]::IsNullOrWhiteSpace($value)) {
            continue
        }
        $snapshot[$mapping.SnapshotKey] = $value
    }

    [pscustomobject]$snapshot
}

function Get-TalkDesktopStatusSummary {
    param([Parameter(Mandatory = $true)][System.Collections.IDictionary]$Fields)

    $summaryParts = New-Object System.Collections.Generic.List[string]
    foreach ($mapping in (Get-TalkDesktopStatusFieldMappings)) {
        if ([string]::IsNullOrWhiteSpace([string]$mapping.SummaryKey)) {
            continue
        }
        if (-not $Fields.Contains($mapping.Name)) {
            continue
        }
        $value = [string]$Fields[$mapping.Name]
        if ([string]::IsNullOrWhiteSpace($value)) {
            continue
        }
        $summaryParts.Add(("{0}={1}" -f $mapping.SummaryKey, $value)) | Out-Null
    }

    $summaryParts -join '; '
}

function Assert-TalkDesktopStatusLines {
    param(
        [Parameter(Mandatory = $true)][string]$DialogText,
        [Parameter(Mandatory = $true)][string[]]$ExpectedLines
    )

    foreach ($expectedLine in $ExpectedLines) {
        Assert-TextContains `
            -Haystack $DialogText `
            -Needle $expectedLine `
            -Context 'desktop-status-lines'
    }
}

function Assert-TalkDesktopStatusFieldValues {
    param(
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$Fields,
        [Parameter(Mandatory = $true)][System.Collections.IDictionary]$ExpectedFields
    )

    foreach ($expectedName in $ExpectedFields.Keys) {
        if (-not $Fields.Contains($expectedName)) {
            throw "Assertion failed (desktop-status-fields): missing field [$expectedName]"
        }
        $actualValue = [string]$Fields[$expectedName]
        $expectedValue = [string]$ExpectedFields[$expectedName]
        if ($actualValue -ne $expectedValue) {
            throw "Assertion failed (desktop-status-fields): field [$expectedName] expected [$expectedValue] got [$actualValue]"
        }
    }
}

function Format-TalkDesktopSmokeWindowHandleForEnv {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd
    )

    if ($Hwnd -eq [System.IntPtr]::Zero) {
        return $null
    }

    '0x{0:X}' -f $Hwnd.ToInt64()
}

function Start-TalkDesktopSmokeInstance {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ConfigPath,
        [hashtable]$EnvironmentOverrides = @{}
    )

    $previousValues = @{}
    foreach ($name in $EnvironmentOverrides.Keys) {
        $previousValues[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
        [Environment]::SetEnvironmentVariable($name, [string]$EnvironmentOverrides[$name], 'Process')
    }

    try {
        $process = Start-Process `
            -FilePath $TalkDesktopBinaryPath `
            -ArgumentList @('--config', $ConfigPath) `
            -PassThru `
            -WindowStyle Hidden
    }
    finally {
        foreach ($name in $EnvironmentOverrides.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previousValues[$name], 'Process')
        }
    }

    $hwnd = Find-WindowByProcessIdAndClass `
        -TargetProcessId $process.Id `
        -ClassName 'TalkDesktopMessageWindow' `
        -TimeoutMs 10000
    if ($hwnd -eq [System.IntPtr]::Zero) {
        try { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue } catch {}
        throw "Failed to find Talk desktop window for pid $($process.Id)"
    }

    [PSCustomObject]@{
        Process = $process
        Hwnd = $hwnd
        ConfigPath = $ConfigPath
    }
}

function Stop-TalkDesktopSmokeInstance {
    param($Instance)

    if ($null -eq $Instance) {
        return
    }

    try {
        if ($Instance.Hwnd -ne [System.IntPtr]::Zero) {
            Send-TalkDesktopMenuCommand -Hwnd $Instance.Hwnd -CommandId 1008
        }
    } catch {}

    try {
        Wait-Process -Id $Instance.Process.Id -Timeout 5 -ErrorAction Stop
    } catch {
        try { Stop-Process -Id $Instance.Process.Id -Force -ErrorAction Stop } catch {}
    }
}

function Get-LatestSessionLog {
    param([Parameter(Mandatory = $true)][string]$LogsDir)

    $log = Get-ChildItem -LiteralPath $LogsDir -Filter '*.json' |
        Where-Object { $_.Name -notlike '*.desktop-insert-target.json' } |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($null -eq $log) {
        throw "No Talk session log was written under $LogsDir"
    }
    $log
}

function Wait-LatestSessionLog {
    param(
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [int]$TimeoutMs = 8000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        if (Test-Path -LiteralPath $LogsDir) {
            $log = Get-ChildItem -LiteralPath $LogsDir -Filter '*.json' -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -notlike '*.desktop-insert-target.json' } |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($null -ne $log) {
                return $log
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "No Talk session log was written under $LogsDir within ${TimeoutMs}ms"
}

function Find-LatestSessionLogIfAvailable {
    param(
        [Parameter(Mandatory = $true)][string]$LogsDir,
        [int]$TimeoutMs = 1500
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    do {
        if (Test-Path -LiteralPath $LogsDir) {
            $log = Get-ChildItem -LiteralPath $LogsDir -Filter '*.json' -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -notlike '*.desktop-insert-target.json' } |
                Sort-Object LastWriteTime -Descending |
                Select-Object -First 1
            if ($null -ne $log) {
                return $log
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    $null
}

function Resolve-TalkDesktopInsertTargetDiagnosticPath {
    param([string]$SessionLogPath)

    if ([string]::IsNullOrWhiteSpace($SessionLogPath)) {
        return $null
    }

    $resolvedSessionLogPath = [System.IO.Path]::GetFullPath($SessionLogPath)
    if (-not (Test-Path -LiteralPath $resolvedSessionLogPath)) {
        return $null
    }

    $directory = Split-Path -Parent $resolvedSessionLogPath
    $stem = [System.IO.Path]::GetFileNameWithoutExtension($resolvedSessionLogPath)
    $candidate = Join-Path $directory ($stem + '.desktop-insert-target.json')
    if (-not (Test-Path -LiteralPath $candidate)) {
        return $null
    }

    $candidate
}

function Start-TalkFakeHttpProvider {
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [string]$TranscribedText = 'transcribed via desktop http',
        [string]$ProcessedText = 'processed via desktop http'
    )

    New-Item -ItemType Directory -Path $ScenarioRoot -Force | Out-Null
    $requestsPath = Join-Path $ScenarioRoot 'provider-requests.json'
    $readyPath = Join-Path $ScenarioRoot 'provider-ready.txt'

    $portProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), 0)
    $portProbe.Start()
    try {
        $port = ([System.Net.IPEndPoint]$portProbe.LocalEndpoint).Port
    }
    finally {
        $portProbe.Stop()
    }

    $job = Start-Job -ArgumentList $port, $requestsPath, $readyPath, $TranscribedText, $ProcessedText -ScriptBlock {
        param(
            [int]$Port,
            [string]$RequestsPath,
            [string]$ReadyPath,
            [string]$TranscribedText,
            [string]$ProcessedText
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = 'Stop'

        function Find-HeaderEnd {
            param([byte[]]$Bytes)

            for ($index = 0; $index -le ($Bytes.Length - 4); $index++) {
                if (
                    $Bytes[$index] -eq 13 -and
                    $Bytes[$index + 1] -eq 10 -and
                    $Bytes[$index + 2] -eq 13 -and
                    $Bytes[$index + 3] -eq 10
                ) {
                    return ($index + 4)
                }
            }
            -1
        }

        function Read-ProviderRequestBody {
            param([Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object System.Collections.Generic.List[byte]
            $temp = New-Object byte[] 1024
            $headerEnd = -1
            do {
                $read = $Stream.Read($temp, 0, $temp.Length)
                if ($read -le 0) {
                    throw 'connection closed before provider headers'
                }
                for ($offset = 0; $offset -lt $read; $offset++) {
                    $buffer.Add($temp[$offset]) | Out-Null
                }
                $headerEnd = Find-HeaderEnd -Bytes ($buffer.ToArray())
            } while ($headerEnd -lt 0)

            $bytes = $buffer.ToArray()
            $headers = [System.Text.Encoding]::UTF8.GetString($bytes, 0, $headerEnd)
            $contentLengthLine = @($headers -split "`r?`n" | Where-Object {
                    $_ -match '^(?i:content-length):'
                })[0]
            if ([string]::IsNullOrWhiteSpace($contentLengthLine)) {
                throw 'provider request missing Content-Length'
            }
            $contentLength = [int](($contentLengthLine -split ':', 2)[1].Trim())
            $bodyBytes = New-Object byte[] $contentLength
            $alreadyBuffered = $bytes.Length - $headerEnd
            if ($alreadyBuffered -gt 0) {
                [System.Array]::Copy($bytes, $headerEnd, $bodyBytes, 0, [Math]::Min($alreadyBuffered, $contentLength))
            }
            $bodyOffset = [Math]::Min($alreadyBuffered, $contentLength)
            while ($bodyOffset -lt $contentLength) {
                $read = $Stream.Read($bodyBytes, $bodyOffset, $contentLength - $bodyOffset)
                if ($read -le 0) {
                    throw 'connection closed before provider body'
                }
                $bodyOffset += $read
            }

            [System.Text.Encoding]::UTF8.GetString($bodyBytes)
        }

        function Write-ProviderJsonResponse {
            param(
                [Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream,
                [Parameter(Mandatory = $true)][string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $responseHead = "HTTP/1.1 200 OK`r`ncontent-type: application/json`r`ncontent-length: $($bodyBytes.Length)`r`nconnection: close`r`n`r`n"
            $headBytes = [System.Text.Encoding]::UTF8.GetBytes($responseHead)
            $Stream.Write($headBytes, 0, $headBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), $Port)
        $listener.Start()
        try {
            'ready' | Set-Content -LiteralPath $ReadyPath -Encoding ASCII
            $requests = New-Object System.Collections.Generic.List[object]
            for ($requestIndex = 0; $requestIndex -lt 2; $requestIndex++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $client.ReceiveTimeout = 10000
                    $stream = $client.GetStream()
                    $rawBody = Read-ProviderRequestBody -Stream $stream
                    $request = $rawBody | ConvertFrom-Json
                    $requests.Add([ordered]@{
                            index = $requestIndex
                            rawBody = $rawBody
                            request = $request
                        }) | Out-Null
                    $text = if ($requestIndex -eq 0) {
                        $TranscribedText
                    } else {
                        $ProcessedText
                    }
                    Write-ProviderJsonResponse `
                        -Stream $stream `
                        -Body (@{ text = $text } | ConvertTo-Json -Compress)
                }
                finally {
                    $client.Dispose()
                }
            }

            $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
            [System.IO.File]::WriteAllText(
                $RequestsPath,
                (($requests.ToArray() | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
                $utf8NoBom
            )
        }
        finally {
            $listener.Stop()
        }
    }

    $deadline = (Get-Date).AddSeconds(5)
    do {
        if (Test-Path -LiteralPath $readyPath) {
            return [PSCustomObject]@{
                Endpoint = "http://127.0.0.1:$port/provider"
                Job = $job
                RequestsPath = $requestsPath
                ReadyPath = $readyPath
            }
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    try {
        Stop-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
    } finally {
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
    }
    throw "Timed out waiting for fake Talk provider readiness file: $readyPath"
}

function Complete-TalkFakeHttpProvider {
    param(
        [Parameter(Mandatory = $true)]$Provider,
        [int]$TimeoutSeconds = 10
    )

    try {
        $null = Wait-Job -Job $Provider.Job -Timeout $TimeoutSeconds
        if ($Provider.Job.State -ne 'Completed') {
            throw "fake Talk provider did not complete within ${TimeoutSeconds}s"
        }
        Receive-Job -Job $Provider.Job -ErrorAction Stop | Out-Null
        if (-not (Test-Path -LiteralPath $Provider.RequestsPath)) {
            throw "fake Talk provider did not write request evidence: $($Provider.RequestsPath)"
        }
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Stop-TalkFakeHttpProvider {
    param($Provider)

    if ($null -eq $Provider) {
        return
    }

    try {
        Stop-Job -Job $Provider.Job -ErrorAction SilentlyContinue | Out-Null
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Start-TalkFakeOpenAiCompatibleProvider {
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [string]$TranscribedText = 'transcribed via openai-compatible',
        [string]$ProcessedText = 'assistant reply from openai-compatible'
    )

    New-Item -ItemType Directory -Path $ScenarioRoot -Force | Out-Null
    $requestsPath = Join-Path $ScenarioRoot 'provider-requests.json'
    $readyPath = Join-Path $ScenarioRoot 'provider-ready.txt'

    $transcriptionProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), 0)
    $transcriptionProbe.Start()
    try {
        $transcriptionPort = ([System.Net.IPEndPoint]$transcriptionProbe.LocalEndpoint).Port
    }
    finally {
        $transcriptionProbe.Stop()
    }

    $chatProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), 0)
    $chatProbe.Start()
    try {
        $chatPort = ([System.Net.IPEndPoint]$chatProbe.LocalEndpoint).Port
    }
    finally {
        $chatProbe.Stop()
    }

    $job = Start-Job -ArgumentList $transcriptionPort, $chatPort, $requestsPath, $readyPath, $TranscribedText, $ProcessedText -ScriptBlock {
        param(
            [int]$TranscriptionPort,
            [int]$ChatPort,
            [string]$RequestsPath,
            [string]$ReadyPath,
            [string]$TranscribedText,
            [string]$ProcessedText
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = 'Stop'

        function Find-HeaderEnd {
            param([byte[]]$Bytes)

            for ($index = 0; $index -le ($Bytes.Length - 4); $index++) {
                if (
                    $Bytes[$index] -eq 13 -and
                    $Bytes[$index + 1] -eq 10 -and
                    $Bytes[$index + 2] -eq 13 -and
                    $Bytes[$index + 3] -eq 10
                ) {
                    return ($index + 4)
                }
            }
            -1
        }

        function Read-CapturedHttpRequest {
            param([Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object System.Collections.Generic.List[byte]
            $temp = New-Object byte[] 1024
            $headerEnd = -1
            do {
                $read = $Stream.Read($temp, 0, $temp.Length)
                if ($read -le 0) {
                    throw 'connection closed before provider headers'
                }
                for ($offset = 0; $offset -lt $read; $offset++) {
                    $buffer.Add($temp[$offset]) | Out-Null
                }
                $headerEnd = Find-HeaderEnd -Bytes ($buffer.ToArray())
            } while ($headerEnd -lt 0)

            $bytes = $buffer.ToArray()
            $headers = [System.Text.Encoding]::UTF8.GetString($bytes, 0, $headerEnd)
            $contentLengthLine = @($headers -split "`r?`n" | Where-Object {
                    $_ -match '^(?i:content-length):'
                })[0]
            if ([string]::IsNullOrWhiteSpace($contentLengthLine)) {
                throw 'provider request missing Content-Length'
            }
            $contentLength = [int](($contentLengthLine -split ':', 2)[1].Trim())
            $bodyBytes = New-Object byte[] $contentLength
            $alreadyBuffered = $bytes.Length - $headerEnd
            if ($alreadyBuffered -gt 0) {
                [System.Array]::Copy($bytes, $headerEnd, $bodyBytes, 0, [Math]::Min($alreadyBuffered, $contentLength))
            }
            $bodyOffset = [Math]::Min($alreadyBuffered, $contentLength)
            while ($bodyOffset -lt $contentLength) {
                $read = $Stream.Read($bodyBytes, $bodyOffset, $contentLength - $bodyOffset)
                if ($read -le 0) {
                    throw 'connection closed before provider body'
                }
                $bodyOffset += $read
            }

            [pscustomobject]@{
                Headers = $headers
                Body = [System.Text.Encoding]::UTF8.GetString($bodyBytes)
                BodyUtf8Lossy = [System.Text.Encoding]::UTF8.GetString($bodyBytes)
            }
        }

        function Write-ProviderJsonResponse {
            param(
                [Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream,
                [Parameter(Mandatory = $true)][string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $responseHead = "HTTP/1.1 200 OK`r`ncontent-type: application/json`r`ncontent-length: $($bodyBytes.Length)`r`nconnection: close`r`n`r`n"
            $headBytes = [System.Text.Encoding]::UTF8.GetBytes($responseHead)
            $Stream.Write($headBytes, 0, $headBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $transcriptionListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), $TranscriptionPort)
        $chatListener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), $ChatPort)
        $transcriptionListener.Start()
        $chatListener.Start()
        try {
            'ready' | Set-Content -LiteralPath $ReadyPath -Encoding ASCII
            $requests = New-Object System.Collections.Generic.List[object]

            $transcriptionClient = $transcriptionListener.AcceptTcpClient()
            try {
                $transcriptionClient.ReceiveTimeout = 10000
                $stream = $transcriptionClient.GetStream()
                $request = Read-CapturedHttpRequest -Stream $stream
                $requests.Add([ordered]@{
                        kind = 'audio_transcriptions'
                        headers = $request.Headers
                        body = $request.BodyUtf8Lossy
                    }) | Out-Null
                Write-ProviderJsonResponse `
                    -Stream $stream `
                    -Body (@{ text = $TranscribedText } | ConvertTo-Json -Compress)
            }
            finally {
                $transcriptionClient.Dispose()
            }

            $chatClient = $chatListener.AcceptTcpClient()
            try {
                $chatClient.ReceiveTimeout = 10000
                $stream = $chatClient.GetStream()
                $request = Read-CapturedHttpRequest -Stream $stream
                $requestJson = $request.Body | ConvertFrom-Json
                $requests.Add([ordered]@{
                        kind = 'chat_completions'
                        headers = $request.Headers
                        body = $request.Body
                        request = $requestJson
                    }) | Out-Null
                Write-ProviderJsonResponse `
                    -Stream $stream `
                    -Body (@{
                            choices = @(
                                @{
                                    message = @{
                                        content = $ProcessedText
                                    }
                                }
                            )
                        } | ConvertTo-Json -Depth 6 -Compress)
            }
            finally {
                $chatClient.Dispose()
            }

            $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
            [System.IO.File]::WriteAllText(
                $RequestsPath,
                (($requests.ToArray() | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
                $utf8NoBom
            )
        }
        finally {
            $chatListener.Stop()
            $transcriptionListener.Stop()
        }
    }

    $deadline = (Get-Date).AddSeconds(5)
    do {
        if (Test-Path -LiteralPath $readyPath) {
            return [PSCustomObject]@{
                AudioTranscriptionsEndpoint = "http://127.0.0.1:$transcriptionPort/v1/audio/transcriptions"
                ChatCompletionsEndpoint = "http://127.0.0.1:$chatPort/v1/chat/completions"
                Job = $job
                RequestsPath = $requestsPath
                ReadyPath = $readyPath
            }
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    try {
        Stop-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
    }
    finally {
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
    }
    throw "Timed out waiting for fake Talk OpenAI-compatible provider readiness file: $readyPath"
}

function Complete-TalkFakeOpenAiCompatibleProvider {
    param(
        [Parameter(Mandatory = $true)]$Provider,
        [int]$TimeoutSeconds = 10
    )

    try {
        $null = Wait-Job -Job $Provider.Job -Timeout $TimeoutSeconds
        if ($Provider.Job.State -ne 'Completed') {
            throw "fake Talk OpenAI-compatible provider did not complete within ${TimeoutSeconds}s"
        }
        Receive-Job -Job $Provider.Job -ErrorAction Stop | Out-Null
        if (-not (Test-Path -LiteralPath $Provider.RequestsPath)) {
            throw "fake Talk OpenAI-compatible provider did not write request evidence: $($Provider.RequestsPath)"
        }
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Stop-TalkFakeOpenAiCompatibleProvider {
    param($Provider)

    if ($null -eq $Provider) {
        return
    }

    try {
        Stop-Job -Job $Provider.Job -ErrorAction SilentlyContinue | Out-Null
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Start-TalkFakeOpenAiCompatibleChatAudioInputProvider {
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [string]$TranscribedText = 'transcribed via audio input chat',
        [string]$ProcessedText = 'assistant reply from audio input chat',
        [int]$ResponseDelayMs = 0
    )

    New-Item -ItemType Directory -Path $ScenarioRoot -Force | Out-Null
    $requestsPath = Join-Path $ScenarioRoot 'provider-requests.json'
    $readyPath = Join-Path $ScenarioRoot 'provider-ready.txt'

    $portProbe = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), 0)
    $portProbe.Start()
    try {
        $port = ([System.Net.IPEndPoint]$portProbe.LocalEndpoint).Port
    }
    finally {
        $portProbe.Stop()
    }

    $job = Start-Job -ArgumentList $port, $requestsPath, $readyPath, $TranscribedText, $ProcessedText, $ResponseDelayMs -ScriptBlock {
        param(
            [int]$Port,
            [string]$RequestsPath,
            [string]$ReadyPath,
            [string]$TranscribedText,
            [string]$ProcessedText,
            [int]$ResponseDelayMs
        )

        Set-StrictMode -Version Latest
        $ErrorActionPreference = 'Stop'

        function Find-HeaderEnd {
            param([byte[]]$Bytes)

            for ($index = 0; $index -le ($Bytes.Length - 4); $index++) {
                if (
                    $Bytes[$index] -eq 13 -and
                    $Bytes[$index + 1] -eq 10 -and
                    $Bytes[$index + 2] -eq 13 -and
                    $Bytes[$index + 3] -eq 10
                ) {
                    return ($index + 4)
                }
            }
            -1
        }

        function Read-CapturedHttpRequest {
            param([Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream)

            $buffer = New-Object System.Collections.Generic.List[byte]
            $temp = New-Object byte[] 1024
            $headerEnd = -1
            do {
                $read = $Stream.Read($temp, 0, $temp.Length)
                if ($read -le 0) {
                    throw 'connection closed before provider headers'
                }
                for ($offset = 0; $offset -lt $read; $offset++) {
                    $buffer.Add($temp[$offset]) | Out-Null
                }
                $headerEnd = Find-HeaderEnd -Bytes ($buffer.ToArray())
            } while ($headerEnd -lt 0)

            $bytes = $buffer.ToArray()
            $headers = [System.Text.Encoding]::UTF8.GetString($bytes, 0, $headerEnd)
            $contentLengthLine = @($headers -split "`r?`n" | Where-Object {
                    $_ -match '^(?i:content-length):'
                })[0]
            if ([string]::IsNullOrWhiteSpace($contentLengthLine)) {
                throw 'provider request missing Content-Length'
            }
            $contentLength = [int](($contentLengthLine -split ':', 2)[1].Trim())
            $bodyBytes = New-Object byte[] $contentLength
            $alreadyBuffered = $bytes.Length - $headerEnd
            if ($alreadyBuffered -gt 0) {
                [System.Array]::Copy($bytes, $headerEnd, $bodyBytes, 0, [Math]::Min($alreadyBuffered, $contentLength))
            }
            $bodyOffset = [Math]::Min($alreadyBuffered, $contentLength)
            while ($bodyOffset -lt $contentLength) {
                $read = $Stream.Read($bodyBytes, $bodyOffset, $contentLength - $bodyOffset)
                if ($read -le 0) {
                    throw 'connection closed before provider body'
                }
                $bodyOffset += $read
            }

            [pscustomobject]@{
                Headers = $headers
                Body = [System.Text.Encoding]::UTF8.GetString($bodyBytes)
            }
        }

        function Write-ProviderJsonResponse {
            param(
                [Parameter(Mandatory = $true)][System.Net.Sockets.NetworkStream]$Stream,
                [Parameter(Mandatory = $true)][string]$Body
            )

            $bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($Body)
            $responseHead = "HTTP/1.1 200 OK`r`ncontent-type: application/json`r`ncontent-length: $($bodyBytes.Length)`r`nconnection: close`r`n`r`n"
            $headBytes = [System.Text.Encoding]::UTF8.GetBytes($responseHead)
            $Stream.Write($headBytes, 0, $headBytes.Length)
            $Stream.Write($bodyBytes, 0, $bodyBytes.Length)
            $Stream.Flush()
        }

        $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Parse('127.0.0.1'), $Port)
        $listener.Start()
        try {
            'ready' | Set-Content -LiteralPath $ReadyPath -Encoding ASCII
            $requests = New-Object System.Collections.Generic.List[object]
            for ($requestIndex = 0; $requestIndex -lt 2; $requestIndex++) {
                $client = $listener.AcceptTcpClient()
                try {
                    $client.ReceiveTimeout = 10000
                    $stream = $client.GetStream()
                    $request = Read-CapturedHttpRequest -Stream $stream
                    $requestJson = $request.Body | ConvertFrom-Json
                    $requests.Add([ordered]@{
                            index = $requestIndex
                            kind = 'chat_completions'
                            headers = $request.Headers
                            body = $request.Body
                            request = $requestJson
                        }) | Out-Null
                    $responseBody = if ($requestIndex -eq 0) {
                        @{
                            choices = @(
                                @{
                                    message = @{
                                        content = $TranscribedText
                                    }
                                }
                            )
                        } | ConvertTo-Json -Depth 6 -Compress
                    } else {
                        @{
                            choices = @(
                                @{
                                    message = @{
                                        content = $ProcessedText
                                    }
                                }
                            )
                        } | ConvertTo-Json -Depth 6 -Compress
                    }
                    if ($ResponseDelayMs -gt 0) {
                        Start-Sleep -Milliseconds $ResponseDelayMs
                    }
                    Write-ProviderJsonResponse -Stream $stream -Body $responseBody
                }
                finally {
                    $client.Dispose()
                }
            }

            $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
            [System.IO.File]::WriteAllText(
                $RequestsPath,
                (($requests.ToArray() | ConvertTo-Json -Depth 8) + [Environment]::NewLine),
                $utf8NoBom
            )
        }
        finally {
            $listener.Stop()
        }
    }

    $deadline = (Get-Date).AddSeconds(5)
    do {
        if (Test-Path -LiteralPath $readyPath) {
            return [PSCustomObject]@{
                ChatCompletionsEndpoint = "http://127.0.0.1:$port/v1/chat/completions"
                Job = $job
                RequestsPath = $requestsPath
                ReadyPath = $readyPath
            }
        }
        Start-Sleep -Milliseconds 50
    } while ((Get-Date) -lt $deadline)

    try {
        Stop-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
    }
    finally {
        Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
    }
    throw "Timed out waiting for fake Talk OpenAI-compatible chat-audio-input provider readiness file: $readyPath"
}

function Complete-TalkFakeOpenAiCompatibleChatAudioInputProvider {
    param(
        [Parameter(Mandatory = $true)]$Provider,
        [int]$TimeoutSeconds = 10
    )

    try {
        $null = Wait-Job -Job $Provider.Job -Timeout $TimeoutSeconds
        if ($Provider.Job.State -ne 'Completed') {
            throw "fake Talk OpenAI-compatible chat-audio-input provider did not complete within ${TimeoutSeconds}s"
        }
        Receive-Job -Job $Provider.Job -ErrorAction Stop | Out-Null
        if (-not (Test-Path -LiteralPath $Provider.RequestsPath)) {
            throw "fake Talk OpenAI-compatible chat-audio-input provider did not write request evidence: $($Provider.RequestsPath)"
        }
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Stop-TalkFakeOpenAiCompatibleChatAudioInputProvider {
    param($Provider)

    if ($null -eq $Provider) {
        return
    }

    try {
        Stop-Job -Job $Provider.Job -ErrorAction SilentlyContinue | Out-Null
    }
    finally {
        Remove-Job -Job $Provider.Job -Force -ErrorAction SilentlyContinue
    }
}

function Start-TalkTextCaptureTarget {
    param([Parameter(Mandatory = $true)][string]$ScenarioRoot)

    $targetRoot = Join-Path $ScenarioRoot 'text-target'
    New-Item -ItemType Directory -Path $targetRoot -Force | Out-Null
    $scriptPath = Join-Path $targetRoot 'Start-TextCaptureTarget.ps1'
    $readyPath = Join-Path $targetRoot 'ready.txt'
    $snapshotPath = Join-Path $targetRoot 'snapshot.txt'
    $handlePath = Join-Path $targetRoot 'textbox-hwnd.txt'
    $windowTitle = 'Talk Smoke Text Target'

    $readyPathLiteral = $readyPath.Replace("'", "''")
    $snapshotPathLiteral = $snapshotPath.Replace("'", "''")
    $handlePathLiteral = $handlePath.Replace("'", "''")
    $windowTitleLiteral = $windowTitle.Replace("'", "''")
    $childScript = @'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$ErrorActionPreference = 'Stop'
$readyPath = '__READY_PATH__'
$snapshotPath = '__SNAPSHOT_PATH__'
$handlePath = '__HANDLE_PATH__'
$windowTitle = '__WINDOW_TITLE__'
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($readyPath)) | Out-Null

$form = New-Object System.Windows.Forms.Form
$form.Text = $windowTitle
$form.Width = 640
$form.Height = 240
$form.StartPosition = 'CenterScreen'

$textBox = New-Object System.Windows.Forms.TextBox
$textBox.Multiline = $true
$textBox.AcceptsReturn = $true
$textBox.AcceptsTab = $true
$textBox.Dock = 'Fill'
$textBox.Font = New-Object System.Drawing.Font('Consolas', 11)
$form.Controls.Add($textBox)

$timer = New-Object System.Windows.Forms.Timer
$timer.Interval = 150
$timer.Add_Tick({
    [System.IO.File]::WriteAllText($snapshotPath, $textBox.Text, $utf8NoBom)
})
$timer.Start()

$form.Add_Shown({
    [System.IO.File]::WriteAllText($readyPath, 'ready', $utf8NoBom)
    [System.IO.File]::WriteAllText($handlePath, $textBox.Handle.ToInt64().ToString(), $utf8NoBom)
    $form.Activate()
    $textBox.Focus() | Out-Null
})
$form.Add_Activated({
    [System.IO.File]::WriteAllText($handlePath, $textBox.Handle.ToInt64().ToString(), $utf8NoBom)
    $textBox.Focus() | Out-Null
})
$form.Add_FormClosed({
    [System.IO.File]::WriteAllText($snapshotPath, $textBox.Text, $utf8NoBom)
})

[System.Windows.Forms.Application]::Run($form)
'@
    $childScript = $childScript.Replace('__READY_PATH__', $readyPathLiteral)
    $childScript = $childScript.Replace('__SNAPSHOT_PATH__', $snapshotPathLiteral)
    $childScript = $childScript.Replace('__HANDLE_PATH__', $handlePathLiteral)
    $childScript = $childScript.Replace('__WINDOW_TITLE__', $windowTitleLiteral)
    $childScript | Set-Content -LiteralPath $scriptPath -Encoding UTF8

    $process = Start-Process `
        -FilePath 'powershell.exe' `
        -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Sta', '-File', $scriptPath) `
        -PassThru

    $deadline = (Get-Date).AddSeconds(10)
    do {
        if ((Test-Path -LiteralPath $readyPath) -and (Test-Path -LiteralPath $handlePath)) {
            $hwnd = Find-DialogByProcessIdAndTitle -TargetProcessId $process.Id -Title $windowTitle -TimeoutMs 250
            if ($hwnd -ne [System.IntPtr]::Zero) {
                $textBoxHandleRaw = Get-Content -LiteralPath $handlePath -Raw -Encoding UTF8
                $textBoxHandleValue = 0
                if (-not [int64]::TryParse($textBoxHandleRaw.Trim(), [ref]$textBoxHandleValue)) {
                    throw "Talk text capture target textbox handle file did not contain a valid integer hwnd: $handlePath"
                }
                return [PSCustomObject]@{
                    Process = $process
                    Hwnd = $hwnd
                    TextBoxHwnd = [System.IntPtr]::new($textBoxHandleValue)
                    WindowTitle = $windowTitle
                    SnapshotPath = $snapshotPath
                    ReadyPath = $readyPath
                    HandlePath = $handlePath
                    ScriptPath = $scriptPath
                }
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    try {
        Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    } catch {}
    throw "Timed out waiting for Talk text capture target readiness: $readyPath"
}

function Stop-TalkTextCaptureTarget {
    param($Target)

    if ($null -eq $Target) {
        return
    }

    try {
        if ($Target.Hwnd -and $Target.Hwnd -ne [System.IntPtr]::Zero) {
            Close-TalkDesktopDialog -DialogHwnd $Target.Hwnd
        }
    } catch {}

    try {
        Wait-Process -Id $Target.Process.Id -Timeout 5 -ErrorAction Stop
    } catch {
        try { Stop-Process -Id $Target.Process.Id -Force -ErrorAction Stop } catch {}
    }
}

function Wait-TalkTextCaptureContains {
    param(
        [Parameter(Mandatory = $true)][string]$SnapshotPath,
        [Parameter(Mandatory = $true)][string]$ExpectedText,
        [int]$TimeoutMs = 8000
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    $lastText = ''
    do {
        if (Test-Path -LiteralPath $SnapshotPath) {
            $lastText = Get-Content -LiteralPath $SnapshotPath -Raw -Encoding UTF8
            if ([string]$lastText -like "*$ExpectedText*") {
                return $lastText
            }
        }
        Start-Sleep -Milliseconds 100
    } while ((Get-Date) -lt $deadline)

    throw "Talk text capture target did not contain [$ExpectedText]. Last text: [$lastText]"
}

function Wait-TalkTextCaptureContainsWithForegroundRefresh {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][string]$SnapshotPath,
        [Parameter(Mandatory = $true)][string]$ExpectedText,
        [int]$TimeoutMs = 8000,
        [int]$RefreshIntervalMs = 100
    )

    $deadline = (Get-Date).AddMilliseconds($TimeoutMs)
    $lastText = ''
    $foregroundTrail = New-Object 'System.Collections.Generic.List[string]'
    $lastForeground = ''
    do {
        Set-TalkDesktopForegroundWindow -Hwnd $Hwnd | Out-Null
        $foregroundSummary = Get-TalkDesktopForegroundWindowDebugString
        if ($foregroundSummary -ne $lastForeground) {
            $lastForeground = $foregroundSummary
            $foregroundTrail.Add($foregroundSummary) | Out-Null
            if ($foregroundTrail.Count -gt 6) {
                $foregroundTrail.RemoveAt(0)
            }
        }
        if (Test-Path -LiteralPath $SnapshotPath) {
            $lastText = Get-Content -LiteralPath $SnapshotPath -Raw -Encoding UTF8
            if ([string]$lastText -like "*$ExpectedText*") {
                return $lastText
            }
        }
        Start-Sleep -Milliseconds $RefreshIntervalMs
    } while ((Get-Date) -lt $deadline)

    $foregroundTailText = if ($foregroundTrail.Count -gt 0) {
        ($foregroundTrail.ToArray() -join ' -> ')
    } else {
        'none'
    }
    throw "Talk text capture target did not contain [$ExpectedText]. Last text: [$lastText]. Foreground tail: [$foregroundTailText]"
}

function Send-TalkTextCaptureTargetText {
    param([Parameter(Mandatory = $true)][string]$Text)

    $KEYEVENTF_KEYUP = 0x0002
    foreach ($char in $Text.ToCharArray()) {
        $virtualKey = Resolve-TalkVirtualKeyCode -KeyToken ([string]$char)
        [TalkDesktopSmokeWin32]::keybd_event([byte]$virtualKey, 0, 0, [UIntPtr]::Zero)
        [TalkDesktopSmokeWin32]::keybd_event([byte]$virtualKey, 0, $KEYEVENTF_KEYUP, [UIntPtr]::Zero)
        Start-Sleep -Milliseconds 40
    }

    Start-Sleep -Milliseconds 120
}

function Write-TalkTextCaptureTargetChildText {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$ChildHwnd,
        [Parameter(Mandatory = $true)][string]$Text
    )

    Ensure-TalkDesktopSmokeWin32Type
    if ($ChildHwnd -eq [System.IntPtr]::Zero) {
        return $false
    }

    $result = [TalkDesktopSmokeWin32]::SendMessageW($ChildHwnd, 0x000C, [System.IntPtr]::Zero, $Text)
    Start-Sleep -Milliseconds 120
    ($result -ne [System.IntPtr]::Zero)
}

function Select-TalkTextCaptureTargetChildText {
    param([Parameter(Mandatory = $true)][System.IntPtr]$ChildHwnd)

    Ensure-TalkDesktopSmokeWin32Type
    if ($ChildHwnd -eq [System.IntPtr]::Zero) {
        return $false
    }

    [void][TalkDesktopSmokeWin32]::SendMessageIntPtrW($ChildHwnd, 0x00B1, [System.IntPtr]::Zero, [System.IntPtr](-1))
    Start-Sleep -Milliseconds 60
    $true
}

function Invoke-TalkTextCaptureTargetPrimer {
    param(
        [Parameter(Mandatory = $true)][System.IntPtr]$Hwnd,
        [Parameter(Mandatory = $true)][string]$SnapshotPath,
        [Parameter(Mandatory = $true)][string]$PrimerText,
        [System.IntPtr]$ChildHwnd = [System.IntPtr]::Zero,
        [int]$TimeoutMs = 5000
    )

    Set-TalkDesktopForegroundWindow -Hwnd $Hwnd | Out-Null
    if ($ChildHwnd -ne [System.IntPtr]::Zero) {
        Set-TalkDesktopChildInputFocus -TargetHwnd $Hwnd -ChildHwnd $ChildHwnd | Out-Null
    }
    $directWriteCompleted = $false
    if ($ChildHwnd -ne [System.IntPtr]::Zero) {
        $directWriteCompleted = Write-TalkTextCaptureTargetChildText -ChildHwnd $ChildHwnd -Text $PrimerText
    }
    if (-not $directWriteCompleted) {
        Send-TalkTextCaptureTargetText -Text $PrimerText
    }
    Wait-TalkTextCaptureContainsWithForegroundRefresh `
        -Hwnd $Hwnd `
        -SnapshotPath $SnapshotPath `
        -ExpectedText $PrimerText `
        -TimeoutMs $TimeoutMs
}

function Remove-TalkTextCapturePrimerPrefix {
    param(
        [string]$CapturedText,
        [string]$PrimerText
    )

    if ([string]::IsNullOrWhiteSpace($CapturedText)) {
        return ''
    }
    if ([string]::IsNullOrWhiteSpace($PrimerText)) {
        return [string]$CapturedText
    }
    $normalizedText = [string]$CapturedText
    if ($normalizedText.StartsWith($PrimerText, [System.StringComparison]::Ordinal)) {
        $normalizedText = $normalizedText.Substring($PrimerText.Length)
    }
    if ($normalizedText.EndsWith($PrimerText, [System.StringComparison]::Ordinal)) {
        $normalizedText = $normalizedText.Substring(0, $normalizedText.Length - $PrimerText.Length)
    }
    $normalizedText
}

function Invoke-CancelAndStatusSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $configPath = Join-Path $ScenarioRoot 'config.toml'
    Write-TalkSmokeConfig -ConfigPath $configPath -Hotkey 'Ctrl+Alt+F24' -Transcript 'desktop cancel smoke'
    $instance = $null
    try {
        $instance = Start-TalkDesktopSmokeInstance -TalkDesktopBinaryPath $TalkDesktopBinaryPath -ConfigPath $configPath
        Start-Sleep -Milliseconds 400
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1001
        Start-Sleep -Milliseconds 500
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1003
        Start-Sleep -Milliseconds 1200

        $log = Get-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'cancelled') {
            throw "Expected cancelled session log, got [$($session.status)]"
        }

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for cancel-and-status scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: idle'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                'Last session' = 'cancelled'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'cancel-and-status'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            LogPath = $log.FullName
            Status = $session.status
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
    }
}

function Invoke-HttpProviderSuccessSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $provider = $null
    $instance = $null
    try {
        $provider = Start-TalkFakeHttpProvider -ScenarioRoot $ScenarioRoot
        $providerEndpoint = $provider.Endpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        Write-TalkHttpProviderSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey 'Ctrl+Alt+F20' `
            -ProviderEndpoint $providerEndpoint

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath
        Start-Sleep -Milliseconds 400

        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd
        Start-Sleep -Milliseconds 250
        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.transcript -ne 'transcribed via desktop http') {
            throw "Expected transcribed desktop HTTP transcript, got [$($session.transcript)]"
        }
        if ($session.output_text -ne 'processed via desktop http') {
            throw "Expected processed desktop HTTP output, got [$($session.output_text)]"
        }

        Complete-TalkFakeHttpProvider -Provider $provider
        $provider = $null
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake desktop HTTP provider to receive 2 requests, got [$($providerRequests.Count)]"
        }
        if (-not [string]$providerRequests[0].rawBody -or $providerRequests[0].request.audio_path -eq $null) {
            throw 'Desktop HTTP provider transcribe request did not include audio_path'
        }
        if ([string]$providerRequests[1].request.transcript -ne 'transcribed via desktop http') {
            throw "Desktop HTTP provider process request transcript mismatch: [$($providerRequests[1].request.transcript)]"
        }
        if ([string]$providerRequests[1].request.mode -ne 'dictate') {
            throw "Desktop HTTP provider process request mode mismatch: [$($providerRequests[1].request.mode)]"
        }

        Start-Sleep -Milliseconds 500
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for http-provider-success scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: idle'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                'Last session' = 'completed'
                'Last session detail' = 'processed via desktop http'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'http-provider-success'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            ProviderEndpoint = $providerEndpoint
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            Status = $session.status
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeHttpProvider -Provider $provider
    }
}

function Invoke-OpenAiCompatibleSuccessSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $provider = $null
    $instance = $null
    try {
        $provider = Start-TalkFakeOpenAiCompatibleProvider -ScenarioRoot $ScenarioRoot
        $audioTranscriptionsEndpoint = $provider.AudioTranscriptionsEndpoint
        $chatCompletionsEndpoint = $provider.ChatCompletionsEndpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        Write-TalkOpenAiCompatibleSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey 'Ctrl+Alt+F19' `
            -AudioTranscriptionsEndpoint $audioTranscriptionsEndpoint `
            -ChatCompletionsEndpoint $chatCompletionsEndpoint

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides @{
                TALK_PROVIDER_API_KEY = 'talk-test-key'
            }
        Start-Sleep -Milliseconds 400

        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd
        Start-Sleep -Milliseconds 250
        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.transcript -ne 'transcribed via openai-compatible') {
            throw "Expected OpenAI-compatible transcript, got [$($session.transcript)]"
        }
        if ($session.output_text -ne 'assistant reply from openai-compatible') {
            throw "Expected OpenAI-compatible output text, got [$($session.output_text)]"
        }

        Complete-TalkFakeOpenAiCompatibleProvider -Provider $provider
        $provider = $null
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake Talk OpenAI-compatible provider to receive 2 requests, got [$($providerRequests.Count)]"
        }

        $audioRequest = @($providerRequests | Where-Object { $_.kind -eq 'audio_transcriptions' })[0]
        $chatRequest = @($providerRequests | Where-Object { $_.kind -eq 'chat_completions' })[0]
        if ($null -eq $audioRequest -or $null -eq $chatRequest) {
            throw 'OpenAI-compatible provider did not capture both audio_transcriptions and chat_completions requests'
        }
        Assert-TextContains -Haystack ([string]$audioRequest.headers) -Needle 'POST /v1/audio/transcriptions HTTP/1.1' -Context 'openai-compatible-success'
        Assert-TextContains -Haystack ([string]$audioRequest.headers) -Needle 'Authorization: Bearer talk-test-key' -Context 'openai-compatible-success'
        Assert-TextContains -Haystack ([string]$audioRequest.body) -Needle 'gpt-4o-mini-transcribe' -Context 'openai-compatible-success'

        $chatRequestJson = $chatRequest.request
        if ([string]$chatRequestJson.model -ne 'gpt-4o-mini') {
            throw "Expected OpenAI-compatible chat model [gpt-4o-mini], got [$($chatRequestJson.model)]"
        }
        $messageRoles = @($chatRequestJson.messages | ForEach-Object { [string]$_.role })
        if ($messageRoles -notcontains 'system' -or $messageRoles -notcontains 'user') {
            throw "OpenAI-compatible chat request did not contain expected system/user roles: [$($messageRoles -join ', ')]"
        }
        $serializedChatRequest = $chatRequest.request | ConvertTo-Json -Depth 8 -Compress
        Assert-TextContains -Haystack $serializedChatRequest -Needle 'transcribed via openai-compatible' -Context 'openai-compatible-success'

        Start-Sleep -Milliseconds 500
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for openai-compatible-success scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: idle'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                Hotkey = 'Ctrl+Alt+F19'
                'Last session' = 'completed'
                'Last session detail' = 'assistant reply from openai-compatible'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'openai-compatible-success'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            AudioTranscriptionsEndpoint = $audioTranscriptionsEndpoint
            ChatCompletionsEndpoint = $chatCompletionsEndpoint
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            Status = $session.status
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeOpenAiCompatibleProvider -Provider $provider
    }
}

function Invoke-OpenAiCompatibleChatAudioInputSuccessSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $provider = $null
    $instance = $null
    try {
        $provider = Start-TalkFakeOpenAiCompatibleChatAudioInputProvider -ScenarioRoot $ScenarioRoot
        $chatCompletionsEndpoint = $provider.ChatCompletionsEndpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        $audioOverridePath = Join-Path $ScenarioRoot 'fixtures\spoken.wav'
        Write-TalkAudioOverrideFixture -AudioPath $audioOverridePath
        Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey 'Ctrl+Alt+F18' `
            -ChatCompletionsEndpoint $chatCompletionsEndpoint

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides @{
                TALK_PROVIDER_API_KEY = 'talk-test-key'
                TALK_DESKTOP_AUDIO_FILE_OVERRIDE = $audioOverridePath
            }
        Start-Sleep -Milliseconds 400

        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd
        Start-Sleep -Milliseconds 250
        Send-TalkDesktopHotkeyMessage -Hwnd $instance.Hwnd

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.transcript -ne 'transcribed via audio input chat') {
            throw "Expected chat-audio-input transcript, got [$($session.transcript)]"
        }
        if ($session.output_text -ne 'assistant reply from audio input chat') {
            throw "Expected chat-audio-input output text, got [$($session.output_text)]"
        }

        Complete-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        $provider = $null
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake Talk OpenAI-compatible chat-audio-input provider to receive 2 requests, got [$($providerRequests.Count)]"
        }

        $transcriptionRequest = $providerRequests[0]
        $chatRequest = $providerRequests[1]
        Assert-TextContains -Haystack ([string]$transcriptionRequest.headers) -Needle 'POST /v1/chat/completions HTTP/1.1' -Context 'openai-compatible-audio-input-success'
        Assert-TextContains -Haystack ([string]$transcriptionRequest.headers) -Needle 'Authorization: Bearer talk-test-key' -Context 'openai-compatible-audio-input-success'
        if ([string]$transcriptionRequest.request.model -ne 'qwen3-asr-flash') {
            throw "Expected chat-audio-input transcription model [qwen3-asr-flash], got [$($transcriptionRequest.request.model)]"
        }
        $transcriptionContent = @($transcriptionRequest.request.messages[0].content)
        if (@($transcriptionContent).Count -lt 1) {
            throw 'Chat-audio-input transcription request did not include content parts'
        }
        if ([string]$transcriptionContent[0].type -ne 'input_audio') {
            throw "Expected first content part type [input_audio], got [$($transcriptionContent[0].type)]"
        }
        Assert-TextContains `
            -Haystack ([string]$transcriptionContent[0].input_audio.data) `
            -Needle 'data:audio/wav;base64,' `
            -Context 'openai-compatible-audio-input-success'

        if ([string]$chatRequest.request.model -ne 'qwen3.7-plus') {
            throw "Expected chat-audio-input chat model [qwen3.7-plus], got [$($chatRequest.request.model)]"
        }
        $serializedChatRequest = $chatRequest.request | ConvertTo-Json -Depth 8 -Compress
        Assert-TextContains -Haystack $serializedChatRequest -Needle 'transcribed via audio input chat' -Context 'openai-compatible-audio-input-success'

        Start-Sleep -Milliseconds 500
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for openai-compatible-audio-input-success scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: idle'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                Hotkey = 'Ctrl+Alt+F18'
                'Last session' = 'completed'
                'Last session detail' = 'assistant reply from audio input chat'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'openai-compatible-audio-input-success'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            ChatCompletionsEndpoint = $chatCompletionsEndpoint
            AudioOverridePath = $audioOverridePath
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            Status = $session.status
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
    }
}

function Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $hotkey = 'Ctrl+Alt+F16'
    $textCapturePrimer = 'talkprimerready'
    $provider = $null
    $instance = $null
    $target = $null
    $progressPath = Join-Path $ScenarioRoot 'progress.log'
    $sessionLogPath = $null
    try {
        $provider = Start-TalkFakeOpenAiCompatibleChatAudioInputProvider -ScenarioRoot $ScenarioRoot
        Write-TalkSmokeProgress -Path $progressPath -Message 'provider-ready'
        $chatCompletionsEndpoint = $provider.ChatCompletionsEndpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        $audioOverridePath = Join-Path $ScenarioRoot 'fixtures\spoken.wav'
        Write-TalkAudioOverrideFixture -AudioPath $audioOverridePath
        Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey $hotkey `
            -ChatCompletionsEndpoint $chatCompletionsEndpoint `
            -VoiceMode 'transcribe' `
            -OutputMode 'clipboard_paste' `
            -ClipboardBackend 'native_windows'
        Write-TalkSmokeProgress -Path $progressPath -Message 'config-written'

        $target = Start-TalkTextCaptureTarget -ScenarioRoot $ScenarioRoot
        Write-TalkSmokeProgress -Path $progressPath -Message 'target-ready'
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null

        $environmentOverrides = @{
            TALK_PROVIDER_API_KEY = 'talk-test-key'
            TALK_DESKTOP_AUDIO_FILE_OVERRIDE = $audioOverridePath
            TALK_DESKTOP_INSERT_TARGET_WINDOW = Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $target.Hwnd
        }
        if ($target.TextBoxHwnd -and $target.TextBoxHwnd -ne [System.IntPtr]::Zero) {
            $environmentOverrides['TALK_DESKTOP_INSERT_TARGET_FOCUS'] =
                Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $target.TextBoxHwnd
        }

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides $environmentOverrides
        Write-TalkSmokeProgress -Path $progressPath -Message 'desktop-started'
        Start-Sleep -Milliseconds 500
        Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
        try {
            Invoke-TalkTextCaptureTargetPrimer `
                -Hwnd $target.Hwnd `
                -SnapshotPath $target.SnapshotPath `
                -PrimerText $textCapturePrimer `
                -ChildHwnd $target.TextBoxHwnd | Out-Null
        } catch {
            $errorMessage = [string]$_.Exception.Message
            $failure = Get-TalkDesktopPrimerFailureClassification `
                -Scenario 'openai-compatible-audio-input-insert-success' `
                -TargetWindowTitle 'Talk Smoke Text Target' `
                -ErrorMessage $errorMessage
            if ($null -ne $failure) {
                $capturedText = if (Test-Path -LiteralPath $target.SnapshotPath) {
                    Get-Content -LiteralPath $target.SnapshotPath -Raw -Encoding UTF8
                } else {
                    $null
                }
                $diagnosticPath = Join-Path $ScenarioRoot 'failure-diagnostic.json'
                $diagnostic = [pscustomobject][ordered]@{
                    scenario = 'openai-compatible-audio-input-insert-success'
                    failureKind = [string]$failure.FailureKind
                    failureSummary = [string]$failure.FailureSummary
                    primerText = $textCapturePrimer
                    capturedText = if ([string]::IsNullOrEmpty([string]$capturedText)) { $null } else { [string]$capturedText }
                    foregroundTail = [string]$failure.ForegroundTail
                    errorMessage = $errorMessage
                    progressPath = $progressPath
                    snapshotPath = $target.SnapshotPath
                    providerRequestsPath = $providerRequestsPath
                    stage = 'primer'
                }
                Write-TalkDesktopSmokeJson -Path $diagnosticPath -Value $diagnostic
                Write-TalkSmokeProgress -Path $progressPath -Message 'classified-hostile-foreground-primer'

                return [pscustomobject][ordered]@{
                    Scenario = 'openai-compatible-audio-input-insert-success'
                    BinaryPath = $TalkDesktopBinaryPath
                    ConfigPath = $configPath
                    ChatCompletionsEndpoint = $chatCompletionsEndpoint
                    AudioOverridePath = $audioOverridePath
                    ProviderRequestsPath = $providerRequestsPath
                    ProgressPath = $progressPath
                    LogPath = $null
                    InsertTargetDiagnosticPath = $null
                    Status = $null
                    CapturedText = if ([string]::IsNullOrEmpty([string]$capturedText)) { $null } else { [string]$capturedText }
                    FailureKind = [string]$failure.FailureKind
                    FailureSummary = [string]$failure.FailureSummary
                    FailureEvidencePath = $diagnosticPath
                }
            }

            throw
        }
        Write-TalkSmokeProgress -Path $progressPath -Message 'target-primed'
        try {
            $capturedText = Invoke-TalkDesktopPinnedWindowOperation -Hwnd $target.Hwnd -ScriptBlock {
                Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
                if ($target.TextBoxHwnd -and $target.TextBoxHwnd -ne [System.IntPtr]::Zero) {
                    Select-TalkTextCaptureTargetChildText -ChildHwnd $target.TextBoxHwnd | Out-Null
                }

                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Start-Sleep -Milliseconds 250
                Set-TalkTextCaptureTargetForeground -Target $target | Out-Null
                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Write-TalkSmokeProgress -Path $progressPath -Message 'hotkey-fired'

                Wait-TalkTextCaptureContainsWithForegroundRefresh `
                    -Hwnd $target.Hwnd `
                    -SnapshotPath $target.SnapshotPath `
                    -ExpectedText 'assistant reply from audio input chat' `
                    -TimeoutMs 12000
            }
        } catch {
            $errorMessage = [string]$_.Exception.Message
            $log = Find-LatestSessionLogIfAvailable -LogsDir (Join-Path $ScenarioRoot 'logs')
            if ($null -ne $log) {
                $sessionLogPath = $log.FullName
                $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
                $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
                $failure = Get-TalkDesktopInsertFailureClassification `
                    -Scenario 'openai-compatible-audio-input-insert-success' `
                    -ExpectedOutputText 'assistant reply from audio input chat' `
                    -TargetWindowTitle 'Talk Smoke Text Target' `
                    -Session $session `
                    -ErrorMessage $errorMessage
                if ($null -ne $failure) {
                    $capturedText = if (Test-Path -LiteralPath $target.SnapshotPath) {
                        Get-Content -LiteralPath $target.SnapshotPath -Raw -Encoding UTF8
                    } else {
                        $null
                    }
                    $diagnosticPath = Join-Path $ScenarioRoot 'failure-diagnostic.json'
                    $diagnostic = [pscustomobject][ordered]@{
                        scenario = 'openai-compatible-audio-input-insert-success'
                        failureKind = [string]$failure.FailureKind
                        failureSummary = [string]$failure.FailureSummary
                        expectedOutputText = 'assistant reply from audio input chat'
                        capturedText = if ([string]::IsNullOrEmpty([string]$capturedText)) { $null } else { [string]$capturedText }
                        foregroundTail = [string]$failure.ForegroundTail
                        errorMessage = $errorMessage
                        progressPath = $progressPath
                        snapshotPath = $target.SnapshotPath
                        providerRequestsPath = $providerRequestsPath
                        sessionLogPath = $log.FullName
                        insertTargetDiagnosticPath = $insertTargetDiagnosticPath
                        session = [pscustomobject][ordered]@{
                            status = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'status')
                            transcript = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'transcript')
                            outputText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'output_text')
                            insertMethod = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object (Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'insert_outcome') -Name 'method')
                        }
                    }
                    Write-TalkDesktopSmokeJson -Path $diagnosticPath -Value $diagnostic
                    Write-TalkSmokeProgress -Path $progressPath -Message 'classified-hostile-foreground'

                    return [pscustomobject][ordered]@{
                        Scenario = 'openai-compatible-audio-input-insert-success'
                        BinaryPath = $TalkDesktopBinaryPath
                        ConfigPath = $configPath
                        ChatCompletionsEndpoint = $chatCompletionsEndpoint
                        AudioOverridePath = $audioOverridePath
                        ProviderRequestsPath = $providerRequestsPath
                        ProgressPath = $progressPath
                        LogPath = $log.FullName
                        InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
                        Status = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $session -Name 'status')
                        CapturedText = if ([string]::IsNullOrEmpty([string]$capturedText)) { $null } else { (Remove-TalkTextCapturePrimerPrefix -CapturedText ([string]$capturedText) -PrimerText $textCapturePrimer) }
                        FailureKind = [string]$failure.FailureKind
                        FailureSummary = [string]$failure.FailureSummary
                        FailureEvidencePath = $diagnosticPath
                    }
                }
            }

            throw
        }
        Write-TalkSmokeProgress -Path $progressPath -Message 'captured-text-ready'
        $normalizedCapturedText = Remove-TalkTextCapturePrimerPrefix `
            -CapturedText ([string]$capturedText) `
            -PrimerText $textCapturePrimer

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $sessionLogPath = $log.FullName
        $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
        Write-TalkSmokeProgress -Path $progressPath -Message 'session-log-written'
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.output_text -ne 'assistant reply from audio input chat') {
            throw "Expected chat-audio-input output text, got [$($session.output_text)]"
        }

        Complete-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        $provider = $null
        Write-TalkSmokeProgress -Path $progressPath -Message 'provider-completed'
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake Talk OpenAI-compatible chat-audio-input provider to receive 2 requests, got [$($providerRequests.Count)]"
        }

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        Write-TalkSmokeProgress -Path $progressPath -Message 'status-command-sent'
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for openai-compatible-audio-input-insert-success scenario'
        }
        Write-TalkSmokeProgress -Path $progressPath -Message 'status-dialog-found'

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: idle'
        Write-TalkSmokeProgress -Path $progressPath -Message 'status-dialog-text-ready'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                Hotkey = $hotkey
                'Last session' = 'completed'
                'Last session detail' = 'assistant reply from audio input chat'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'native_windows'
                'Clipboard backend readiness' = 'ready'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog
        Write-TalkSmokeProgress -Path $progressPath -Message 'returning-result'

        [PSCustomObject]@{
            Scenario = 'openai-compatible-audio-input-insert-success'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            ChatCompletionsEndpoint = $chatCompletionsEndpoint
            AudioOverridePath = $audioOverridePath
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
            Status = $session.status
            CapturedText = $normalizedCapturedText
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Write-TalkSmokeProgress -Path $progressPath -Message 'finally-begin'
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        Stop-TalkTextCaptureTarget -Target $target
        Write-TalkSmokeProgress -Path $progressPath -Message 'finally-end'
    }
}

function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $hotkey = 'Ctrl+Alt+F15'
    $originPrimer = 'talkoriginready'
    $alternatePrimer = 'talkalternateready'
    $provider = $null
    $instance = $null
    $originTarget = $null
    $alternateTarget = $null
    $progressPath = Join-Path $ScenarioRoot 'progress.log'
    try {
        $provider = Start-TalkFakeOpenAiCompatibleChatAudioInputProvider `
            -ScenarioRoot $ScenarioRoot `
            -ResponseDelayMs 1500
        Write-TalkSmokeProgress -Path $progressPath -Message 'provider-ready'
        $chatCompletionsEndpoint = $provider.ChatCompletionsEndpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        $audioOverridePath = Join-Path $ScenarioRoot 'fixtures\spoken.wav'
        Write-TalkAudioOverrideFixture -AudioPath $audioOverridePath
        Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey $hotkey `
            -ChatCompletionsEndpoint $chatCompletionsEndpoint `
            -VoiceMode 'transcribe' `
            -OutputMode 'clipboard_paste' `
            -ClipboardBackend 'native_windows'
        Write-TalkSmokeProgress -Path $progressPath -Message 'config-written'

        $originTarget = Start-TalkTextCaptureTarget -ScenarioRoot (Join-Path $ScenarioRoot 'origin-target')
        $alternateTarget = Start-TalkTextCaptureTarget -ScenarioRoot (Join-Path $ScenarioRoot 'alternate-target')
        Write-TalkSmokeProgress -Path $progressPath -Message 'targets-ready'

        Set-TalkTextCaptureTargetForeground -Target $originTarget | Out-Null
        $environmentOverrides = @{
            TALK_PROVIDER_API_KEY = 'talk-test-key'
            TALK_DESKTOP_AUDIO_FILE_OVERRIDE = $audioOverridePath
        }

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides $environmentOverrides
        Write-TalkSmokeProgress -Path $progressPath -Message 'desktop-started'
        Start-Sleep -Milliseconds 500

        Set-TalkTextCaptureTargetForeground -Target $originTarget | Out-Null
        Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd $originTarget.Hwnd `
            -SnapshotPath $originTarget.SnapshotPath `
            -PrimerText $originPrimer `
            -ChildHwnd $originTarget.TextBoxHwnd | Out-Null
        Set-TalkTextCaptureTargetForeground -Target $alternateTarget | Out-Null
        Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd $alternateTarget.Hwnd `
            -SnapshotPath $alternateTarget.SnapshotPath `
            -PrimerText $alternatePrimer `
            -ChildHwnd $alternateTarget.TextBoxHwnd | Out-Null
        Write-TalkSmokeProgress -Path $progressPath -Message 'targets-primed'

        try {
            Invoke-TalkDesktopPinnedWindowOperation -Hwnd $originTarget.Hwnd -ScriptBlock {
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null
                if ($originTarget.TextBoxHwnd -and $originTarget.TextBoxHwnd -ne [System.IntPtr]::Zero) {
                    Select-TalkTextCaptureTargetChildText -ChildHwnd $originTarget.TextBoxHwnd | Out-Null
                }
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null

                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Wait-TalkDesktopVisibleWindowByProcessIdAndClass `
                    -TargetProcessId $instance.Process.Id `
                    -ClassName 'TalkDesktopHudWindow' `
                    -TimeoutMs 5000 | Out-Null
                Write-TalkSmokeProgress -Path $progressPath -Message 'origin-recording-visible'
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null
                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Write-TalkSmokeProgress -Path $progressPath -Message 'origin-hotkey-fired'
            } | Out-Null
        }
        catch {
            $errorMessage = [string]$_.Exception.Message
            if (Test-TalkDesktopFocusSwitchForegroundPreconditionError -ErrorMessage $errorMessage) {
                return New-TalkDesktopFocusSwitchHostileForegroundFailure `
                    -ScenarioName 'openai-compatible-audio-input-focus-switch-copy-popup-success' `
                    -ScenarioRoot $ScenarioRoot `
                    -Stage 'origin-hotkey' `
                    -ErrorMessage $errorMessage `
                    -ProgressPath $progressPath `
                    -OriginTarget $originTarget `
                    -AlternateTarget $alternateTarget `
                    -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
                    -ConfigPath $configPath `
                    -ChatCompletionsEndpoint $chatCompletionsEndpoint `
                    -AudioOverridePath $audioOverridePath `
                    -ProviderRequestsPath $providerRequestsPath
            }
            throw
        }

        Set-TalkTextCaptureTargetForeground -Target $alternateTarget | Out-Null
        $capturedAlternate = Wait-TalkTextCaptureContainsWithForegroundRefresh `
            -Hwnd $alternateTarget.Hwnd `
            -SnapshotPath $alternateTarget.SnapshotPath `
            -ExpectedText 'assistant reply from audio input chat' `
            -TimeoutMs 12000
        Write-TalkSmokeProgress -Path $progressPath -Message 'current-focus-text-captured'

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.output_text -ne 'assistant reply from audio input chat') {
            throw "Expected chat-audio-input output text, got [$($session.output_text)]"
        }

        if (-not [string]::IsNullOrWhiteSpace($insertTargetDiagnosticPath)) {
            $insertTargetDiagnostic = Get-Content -LiteralPath $insertTargetDiagnosticPath -Raw -Encoding UTF8 | ConvertFrom-Json
            if ([string]$insertTargetDiagnostic.outputStrategy -ne 'honor_configured_output') {
                throw "Expected insert-target diagnostic outputStrategy [honor_configured_output], got [$($insertTargetDiagnostic.outputStrategy)]"
            }
        }

        Complete-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        $provider = $null
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake Talk OpenAI-compatible chat-audio-input provider to receive 2 requests, got [$($providerRequests.Count)]"
        }

        Start-Sleep -Milliseconds 200
        $originCapturedText = if (Test-Path -LiteralPath $originTarget.SnapshotPath) {
            Get-Content -LiteralPath $originTarget.SnapshotPath -Raw -Encoding UTF8
        } else {
            ''
        }
        $alternateCapturedText = [string]$capturedAlternate

        if ([string]$alternateCapturedText -notlike '*assistant reply from audio input chat*') {
            throw "Expected current alternate target to receive corrected output, got [$alternateCapturedText]"
        }
        if ([string]$originCapturedText -like '*assistant reply from audio input chat*') {
            throw "Expected origin target to remain unchanged after focus switched, got [$originCapturedText]"
        }
        if ($originCapturedText.Trim() -ne $originPrimer) {
            throw "Expected origin target to keep primer text [$originPrimer], got [$originCapturedText]"
        }
        if ([string]$alternateCapturedText -notlike "*$alternatePrimer*") {
            throw "Expected alternate target to retain primer text [$alternatePrimer] alongside corrected output, got [$alternateCapturedText]"
        }

        [pscustomobject][ordered]@{
            Scenario = 'openai-compatible-audio-input-focus-switch-copy-popup-success'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            ChatCompletionsEndpoint = $chatCompletionsEndpoint
            AudioOverridePath = $audioOverridePath
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
            Status = $session.status
            PopupVisible = $false
            PopupHwnd = $null
            CopiedText = $null
            OriginCapturedText = [string]$originCapturedText
            AlternateCapturedText = [string]$alternateCapturedText
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        Stop-TalkTextCaptureTarget -Target $alternateTarget
        Stop-TalkTextCaptureTarget -Target $originTarget
    }
}

function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [Parameter(Mandatory = $true)][string]$ScenarioName,
        [Parameter(Mandatory = $true)][scriptblock]$PopupInteractionScript
    )

    $hotkey = 'Ctrl+Alt+F15'
    $originPrimer = 'talkoriginready'
    $alternatePrimer = 'talkalternateready'
    $provider = $null
    $instance = $null
    $originTarget = $null
    $alternateTarget = $null
    $progressPath = Join-Path $ScenarioRoot 'progress.log'
    try {
        $provider = Start-TalkFakeOpenAiCompatibleChatAudioInputProvider `
            -ScenarioRoot $ScenarioRoot `
            -ResponseDelayMs 1500
        Write-TalkSmokeProgress -Path $progressPath -Message 'provider-ready'
        $chatCompletionsEndpoint = $provider.ChatCompletionsEndpoint
        $providerRequestsPath = $provider.RequestsPath
        $configPath = Join-Path $ScenarioRoot 'config.toml'
        $audioOverridePath = Join-Path $ScenarioRoot 'fixtures\spoken.wav'
        Write-TalkAudioOverrideFixture -AudioPath $audioOverridePath
        Write-TalkOpenAiCompatibleChatAudioInputSmokeConfig `
            -ConfigPath $configPath `
            -Hotkey $hotkey `
            -ChatCompletionsEndpoint $chatCompletionsEndpoint `
            -VoiceMode 'transcribe' `
            -OutputMode 'clipboard_paste' `
            -ClipboardBackend 'native_windows'
        Write-TalkSmokeProgress -Path $progressPath -Message 'config-written'

        $originTarget = Start-TalkTextCaptureTarget -ScenarioRoot (Join-Path $ScenarioRoot 'origin-target')
        $alternateTarget = Start-TalkTextCaptureTarget -ScenarioRoot (Join-Path $ScenarioRoot 'alternate-target')
        Write-TalkSmokeProgress -Path $progressPath -Message 'targets-ready'

        Set-TalkTextCaptureTargetForeground -Target $originTarget | Out-Null
        $environmentOverrides = @{
            TALK_PROVIDER_API_KEY = 'talk-test-key'
            TALK_DESKTOP_AUDIO_FILE_OVERRIDE = $audioOverridePath
        }

        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides $environmentOverrides
        Write-TalkSmokeProgress -Path $progressPath -Message 'desktop-started'
        Start-Sleep -Milliseconds 500

        Set-TalkTextCaptureTargetForeground -Target $originTarget | Out-Null
        Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd $originTarget.Hwnd `
            -SnapshotPath $originTarget.SnapshotPath `
            -PrimerText $originPrimer `
            -ChildHwnd $originTarget.TextBoxHwnd | Out-Null
        Set-TalkTextCaptureTargetForeground -Target $alternateTarget | Out-Null
        Invoke-TalkTextCaptureTargetPrimer `
            -Hwnd $alternateTarget.Hwnd `
            -SnapshotPath $alternateTarget.SnapshotPath `
            -PrimerText $alternatePrimer `
            -ChildHwnd $alternateTarget.TextBoxHwnd | Out-Null
        Write-TalkSmokeProgress -Path $progressPath -Message 'targets-primed'

        try {
            Invoke-TalkDesktopPinnedWindowOperation -Hwnd $originTarget.Hwnd -ScriptBlock {
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null
                if ($originTarget.TextBoxHwnd -and $originTarget.TextBoxHwnd -ne [System.IntPtr]::Zero) {
                    Select-TalkTextCaptureTargetChildText -ChildHwnd $originTarget.TextBoxHwnd | Out-Null
                }
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null

                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Wait-TalkDesktopVisibleWindowByProcessIdAndClass `
                    -TargetProcessId $instance.Process.Id `
                    -ClassName 'TalkDesktopHudWindow' `
                    -TimeoutMs 5000 | Out-Null
                Write-TalkSmokeProgress -Path $progressPath -Message 'origin-recording-visible'
                Assert-TalkTextCaptureTargetForeground -Target $originTarget -Name 'origin' | Out-Null
                Send-TalkDesktopGlobalHotkeyChord -Shortcut $hotkey
                Write-TalkSmokeProgress -Path $progressPath -Message 'origin-hotkey-fired'
            } | Out-Null
        }
        catch {
            $errorMessage = [string]$_.Exception.Message
            if (Test-TalkDesktopFocusSwitchForegroundPreconditionError -ErrorMessage $errorMessage) {
                return New-TalkDesktopFocusSwitchHostileForegroundFailure `
                    -ScenarioName $ScenarioName `
                    -ScenarioRoot $ScenarioRoot `
                    -Stage 'origin-hotkey' `
                    -ErrorMessage $errorMessage `
                    -ProgressPath $progressPath `
                    -OriginTarget $originTarget `
                    -AlternateTarget $alternateTarget `
                    -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
                    -ConfigPath $configPath `
                    -ChatCompletionsEndpoint $chatCompletionsEndpoint `
                    -AudioOverridePath $audioOverridePath `
                    -ProviderRequestsPath $providerRequestsPath
            }
            throw
        }

        $popupHwnd = $null
        try {
            $popupHwnd = Invoke-TalkDesktopPinnedWindowOperation -Hwnd $alternateTarget.Hwnd -ScriptBlock {
                Assert-TalkTextCaptureTargetForeground -Target $alternateTarget -Name 'alternate' | Out-Null
                if ($alternateTarget.TextBoxHwnd -and $alternateTarget.TextBoxHwnd -ne [System.IntPtr]::Zero) {
                    Select-TalkTextCaptureTargetChildText -ChildHwnd $alternateTarget.TextBoxHwnd | Out-Null
                }
                Assert-TalkTextCaptureTargetForeground -Target $alternateTarget -Name 'alternate' | Out-Null
                Write-TalkSmokeProgress -Path $progressPath -Message 'alternate-foreground-armed'
                Wait-TalkDesktopVisibleWindowByProcessIdAndClass `
                    -TargetProcessId $instance.Process.Id `
                    -ClassName 'TalkDesktopCopyPopupWindow' `
                    -TimeoutMs 12000
            }
        }
        catch {
            $errorMessage = [string]$_.Exception.Message
            if (Test-TalkDesktopFocusSwitchForegroundPreconditionError -ErrorMessage $errorMessage) {
                return New-TalkDesktopFocusSwitchHostileForegroundFailure `
                    -ScenarioName $ScenarioName `
                    -ScenarioRoot $ScenarioRoot `
                    -Stage 'alternate-foreground' `
                    -ErrorMessage $errorMessage `
                    -ProgressPath $progressPath `
                    -OriginTarget $originTarget `
                    -AlternateTarget $alternateTarget `
                    -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
                    -ConfigPath $configPath `
                    -ChatCompletionsEndpoint $chatCompletionsEndpoint `
                    -AudioOverridePath $audioOverridePath `
                    -ProviderRequestsPath $providerRequestsPath
            }
            $log = Find-LatestSessionLogIfAvailable -LogsDir (Join-Path $ScenarioRoot 'logs')
            $insertTargetDiagnosticPath = if ($null -ne $log) {
                Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
            } else {
                $null
            }
            $originCapturedText = if (Test-Path -LiteralPath $originTarget.SnapshotPath) {
                Get-Content -LiteralPath $originTarget.SnapshotPath -Raw -Encoding UTF8
            } else {
                $null
            }
            $alternateCapturedText = if (Test-Path -LiteralPath $alternateTarget.SnapshotPath) {
                Get-Content -LiteralPath $alternateTarget.SnapshotPath -Raw -Encoding UTF8
            } else {
                $null
            }
            $diagnosticPath = Join-Path $ScenarioRoot 'failure-diagnostic.json'
            Write-TalkDesktopSmokeJson -Path $diagnosticPath -Value ([pscustomobject][ordered]@{
                scenario = $ScenarioName
                failureKind = 'copy_popup_focus_switch_failed'
                failureSummary = 'focus switched away from the origin target, but Talk did not remain in copy-popup mode'
                errorMessage = $errorMessage
                originSnapshotPath = $originTarget.SnapshotPath
                alternateSnapshotPath = $alternateTarget.SnapshotPath
                originCapturedText = if ([string]::IsNullOrEmpty([string]$originCapturedText)) { $null } else { [string]$originCapturedText }
                alternateCapturedText = if ([string]::IsNullOrEmpty([string]$alternateCapturedText)) { $null } else { [string]$alternateCapturedText }
                sessionLogPath = if ($null -ne $log) { $log.FullName } else { $null }
                insertTargetDiagnosticPath = $insertTargetDiagnosticPath
                progressPath = $progressPath
            })

            return [pscustomobject][ordered]@{
                Scenario = $ScenarioName
                BinaryPath = $TalkDesktopBinaryPath
                ConfigPath = $configPath
                ChatCompletionsEndpoint = $chatCompletionsEndpoint
                AudioOverridePath = $audioOverridePath
                ProviderRequestsPath = $providerRequestsPath
                LogPath = if ($null -ne $log) { $log.FullName } else { $null }
                InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
                OriginCapturedText = if ([string]::IsNullOrEmpty([string]$originCapturedText)) { $null } else { [string]$originCapturedText }
                AlternateCapturedText = if ([string]::IsNullOrEmpty([string]$alternateCapturedText)) { $null } else { [string]$alternateCapturedText }
                FailureKind = 'copy_popup_focus_switch_failed'
                FailureSummary = 'focus switched away from the origin target, but Talk did not remain in copy-popup mode'
                FailureEvidencePath = $diagnosticPath
            }
        }
        Write-TalkSmokeProgress -Path $progressPath -Message 'copy-popup-visible'

        $foregroundAfterPopup = Get-TalkDesktopForegroundWindowHwnd
        if ($foregroundAfterPopup -eq $popupHwnd) {
            $foregroundSummary = Get-TalkDesktopForegroundWindowDebugString
            throw "Expected copy popup hwnd [0x$('{0:X}' -f $popupHwnd.ToInt64())] to stay non-activating, but it became the foreground window [$foregroundSummary]"
        }

        $popupInteractionResult = & $PopupInteractionScript `
            -PopupHwnd $popupHwnd `
            -Instance $instance `
            -ProgressPath $progressPath
        if ($null -eq $popupInteractionResult) {
            throw "Popup interaction script for scenario [$ScenarioName] returned no result"
        }

        $log = Wait-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $insertTargetDiagnosticPath = Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'completed') {
            throw "Expected completed session log, got [$($session.status)]"
        }
        if ($session.output_text -ne 'assistant reply from audio input chat') {
            throw "Expected chat-audio-input output text, got [$($session.output_text)]"
        }

        if (-not [string]::IsNullOrWhiteSpace($insertTargetDiagnosticPath)) {
            $insertTargetDiagnostic = Get-Content -LiteralPath $insertTargetDiagnosticPath -Raw -Encoding UTF8 | ConvertFrom-Json
            if ([string]$insertTargetDiagnostic.outputStrategy -ne 'show_copy_popup_only') {
                throw "Expected insert-target diagnostic outputStrategy [show_copy_popup_only], got [$($insertTargetDiagnostic.outputStrategy)]"
            }
        }

        Complete-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        $provider = $null
        $providerRequests = Get-Content -LiteralPath $providerRequestsPath -Raw | ConvertFrom-Json
        if ($providerRequests -isnot [System.Array]) {
            $providerRequests = @($providerRequests)
        }
        if ($providerRequests.Count -ne 2) {
            throw "Expected fake Talk OpenAI-compatible chat-audio-input provider to receive 2 requests, got [$($providerRequests.Count)]"
        }

        Start-Sleep -Milliseconds 200
        $originCapturedText = if (Test-Path -LiteralPath $originTarget.SnapshotPath) {
            Get-Content -LiteralPath $originTarget.SnapshotPath -Raw -Encoding UTF8
        } else {
            ''
        }
        $alternateCapturedText = if (Test-Path -LiteralPath $alternateTarget.SnapshotPath) {
            Get-Content -LiteralPath $alternateTarget.SnapshotPath -Raw -Encoding UTF8
        } else {
            ''
        }

        if ([string]$alternateCapturedText -like '*assistant reply from audio input chat*') {
            throw "Expected alternate target to remain unchanged while copy popup was visible, got [$alternateCapturedText]"
        }
        if ([string]$originCapturedText -like '*assistant reply from audio input chat*') {
            throw "Expected origin target to remain unchanged while copy popup was visible, got [$originCapturedText]"
        }
        if ($originCapturedText.Trim() -ne $originPrimer) {
            throw "Expected origin target to keep primer text [$originPrimer], got [$originCapturedText]"
        }
        if ($alternateCapturedText.Trim() -ne $alternatePrimer) {
            throw "Expected alternate target to keep primer text [$alternatePrimer], got [$alternateCapturedText]"
        }

        [pscustomobject][ordered]@{
            Scenario = $ScenarioName
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            ChatCompletionsEndpoint = $chatCompletionsEndpoint
            AudioOverridePath = $audioOverridePath
            ProviderRequestsPath = $providerRequestsPath
            LogPath = $log.FullName
            InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
            Status = $session.status
            PopupVisible = ($popupHwnd -ne [System.IntPtr]::Zero)
            PopupHwnd = if ($popupHwnd -ne [System.IntPtr]::Zero) { ('0x{0:X}' -f $popupHwnd.ToInt64()) } else { $null }
            ClipboardText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $popupInteractionResult -Name 'ClipboardText')
            CopiedText = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $popupInteractionResult -Name 'CopiedText')
            OriginCapturedText = [string]$originCapturedText
            AlternateCapturedText = [string]$alternateCapturedText
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
        Stop-TalkFakeOpenAiCompatibleChatAudioInputProvider -Provider $provider
        Stop-TalkTextCaptureTarget -Target $alternateTarget
        Stop-TalkTextCaptureTarget -Target $originTarget
    }
}

function Test-TalkDesktopFocusSwitchForegroundPreconditionError {
    param([string]$ErrorMessage)

    if ([string]::IsNullOrWhiteSpace($ErrorMessage)) {
        return $false
    }

    $ErrorMessage -like 'Expected Talk * text capture target to be foreground before hotkey,*'
}

function New-TalkDesktopFocusSwitchHostileForegroundFailure {
    param(
        [Parameter(Mandatory = $true)][string]$ScenarioName,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot,
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$ErrorMessage,
        [Parameter(Mandatory = $true)][string]$ProgressPath,
        $OriginTarget,
        $AlternateTarget,
        [string]$TalkDesktopBinaryPath,
        [string]$ConfigPath,
        [string]$ChatCompletionsEndpoint,
        [string]$AudioOverridePath,
        [string]$ProviderRequestsPath
    )

    $originSnapshotPath = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $OriginTarget -Name 'SnapshotPath')
    $alternateSnapshotPath = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $AlternateTarget -Name 'SnapshotPath')
    $originCapturedText = if (-not [string]::IsNullOrWhiteSpace($originSnapshotPath) -and (Test-Path -LiteralPath $originSnapshotPath)) {
        Get-Content -LiteralPath $originSnapshotPath -Raw -Encoding UTF8
    } else {
        $null
    }
    $alternateCapturedText = if (-not [string]::IsNullOrWhiteSpace($alternateSnapshotPath) -and (Test-Path -LiteralPath $alternateSnapshotPath)) {
        Get-Content -LiteralPath $alternateSnapshotPath -Raw -Encoding UTF8
    } else {
        $null
    }

    $originWindowHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $OriginTarget -Name 'Hwnd'
    $originFocusHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $OriginTarget -Name 'TextBoxHwnd'
    $alternateWindowHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $AlternateTarget -Name 'Hwnd'
    $alternateFocusHwnd = Get-TalkDesktopSmokeOptionalPropertyValue -Object $AlternateTarget -Name 'TextBoxHwnd'
    $log = Find-LatestSessionLogIfAvailable -LogsDir (Join-Path $ScenarioRoot 'logs')
    $insertTargetDiagnosticPath = if ($null -ne $log) {
        Resolve-TalkDesktopInsertTargetDiagnosticPath -SessionLogPath $log.FullName
    } else {
        $null
    }
    $diagnosticPath = Join-Path $ScenarioRoot 'failure-diagnostic.json'
    $failureSummary = 'foreground target could not retain focus long enough to fire the focus-switch hotkey'
    $diagnostic = [pscustomobject][ordered]@{
        scenario = $ScenarioName
        failureKind = 'hostile_foreground_environment'
        failureSummary = $failureSummary
        stage = $Stage
        errorMessage = $ErrorMessage
        foreground = Get-TalkDesktopForegroundWindowDebugString
        originWindowHandle = Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $originWindowHwnd
        originFocusHandle = Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $originFocusHwnd
        alternateWindowHandle = Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $alternateWindowHwnd
        alternateFocusHandle = Format-TalkDesktopSmokeWindowHandleForEnv -Hwnd $alternateFocusHwnd
        originSnapshotPath = if ([string]::IsNullOrWhiteSpace($originSnapshotPath)) { $null } else { $originSnapshotPath }
        alternateSnapshotPath = if ([string]::IsNullOrWhiteSpace($alternateSnapshotPath)) { $null } else { $alternateSnapshotPath }
        originCapturedText = if ([string]::IsNullOrEmpty([string]$originCapturedText)) { $null } else { [string]$originCapturedText }
        alternateCapturedText = if ([string]::IsNullOrEmpty([string]$alternateCapturedText)) { $null } else { [string]$alternateCapturedText }
        sessionLogPath = if ($null -ne $log) { $log.FullName } else { $null }
        insertTargetDiagnosticPath = $insertTargetDiagnosticPath
        providerRequestsPath = $ProviderRequestsPath
        progressPath = $ProgressPath
    }
    Write-TalkDesktopSmokeJson -Path $diagnosticPath -Value $diagnostic
    Write-TalkSmokeProgress -Path $ProgressPath -Message "classified-hostile-foreground-$Stage"

    [pscustomobject][ordered]@{
        Scenario = $ScenarioName
        BinaryPath = $TalkDesktopBinaryPath
        ConfigPath = $ConfigPath
        ChatCompletionsEndpoint = $ChatCompletionsEndpoint
        AudioOverridePath = $AudioOverridePath
        ProviderRequestsPath = $ProviderRequestsPath
        ProgressPath = $ProgressPath
        LogPath = if ($null -ne $log) { $log.FullName } else { $null }
        InsertTargetDiagnosticPath = $insertTargetDiagnosticPath
        OriginCapturedText = if ([string]::IsNullOrEmpty([string]$originCapturedText)) { $null } else { [string]$originCapturedText }
        AlternateCapturedText = if ([string]::IsNullOrEmpty([string]$alternateCapturedText)) { $null } else { [string]$alternateCapturedText }
        FailureKind = 'hostile_foreground_environment'
        FailureSummary = $failureSummary
        FailureEvidencePath = $diagnosticPath
    }
}

function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore `
        -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
        -ScenarioRoot $ScenarioRoot `
        -ScenarioName 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success' `
        -PopupInteractionScript {
            param(
                [Parameter(Mandatory = $true)][System.IntPtr]$popupHwnd,
                [Parameter(Mandatory = $true)]$instance,
                [Parameter(Mandatory = $true)][string]$progressPath
            )

            Set-TalkDesktopClipboardText -Value 'talk-copy-popup-pending'
            Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x0D
            Start-Sleep -Milliseconds 150
            Wait-TalkDesktopVisibleWindowByProcessIdAndClass `
                -TargetProcessId $instance.Process.Id `
                -ClassName 'TalkDesktopCopyPopupWindow' `
                -TimeoutMs 1000 | Out-Null
            $copiedText = Get-TalkDesktopClipboardText
            if ($copiedText.Trim() -ne 'assistant reply from audio input chat') {
                throw "Expected popup copy text [assistant reply from audio input chat], got [$copiedText]"
            }
            Write-TalkSmokeProgress -Path $progressPath -Message 'copy-popup-keyboard-enter-copied'
            Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B
            Wait-TalkDesktopWindowHiddenByProcessIdAndClass `
                -TargetProcessId $instance.Process.Id `
                -ClassName 'TalkDesktopCopyPopupWindow' `
                -TimeoutMs 5000
            Write-TalkSmokeProgress -Path $progressPath -Message 'copy-popup-keyboard-enter-closed'

            [pscustomobject][ordered]@{
                ClipboardText = [string]$copiedText
                CopiedText = [string]$copiedText
            }
        }
}

function Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEscapeSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmokeCore `
        -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
        -ScenarioRoot $ScenarioRoot `
        -ScenarioName 'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success' `
        -PopupInteractionScript {
            param(
                [Parameter(Mandatory = $true)][System.IntPtr]$popupHwnd,
                [Parameter(Mandatory = $true)]$instance,
                [Parameter(Mandatory = $true)][string]$progressPath
            )

            Set-TalkDesktopClipboardText -Value 'talk-copy-popup-escape-sentinel'
            Send-TalkDesktopWindowVirtualKeyInput -Hwnd $popupHwnd -VirtualKey 0x1B
            Wait-TalkDesktopWindowHiddenByProcessIdAndClass `
                -TargetProcessId $instance.Process.Id `
                -ClassName 'TalkDesktopCopyPopupWindow' `
                -TimeoutMs 5000
            Write-TalkSmokeProgress -Path $progressPath -Message 'copy-popup-keyboard-escape-closed'
            Start-Sleep -Milliseconds 150
            $clipboardText = Get-TalkDesktopClipboardText
            if ($clipboardText.Trim() -ne 'talk-copy-popup-escape-sentinel') {
                throw "Expected popup escape to preserve clipboard sentinel [talk-copy-popup-escape-sentinel], got [$clipboardText]"
            }

            [pscustomobject][ordered]@{
                ClipboardText = [string]$clipboardText
            }
        }
}

function Invoke-HotkeyConflictSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $primaryConfigPath = Join-Path $ScenarioRoot 'primary\config.toml'
    $secondaryConfigPath = Join-Path $ScenarioRoot 'secondary\config.toml'
    Write-TalkSmokeConfig -ConfigPath $primaryConfigPath -Hotkey 'Ctrl+Alt+F23' -Transcript 'primary conflict holder'
    Write-TalkSmokeConfig -ConfigPath $secondaryConfigPath -Hotkey 'Ctrl+Alt+F23' -Transcript 'secondary conflict observer'

    $primary = $null
    $secondary = $null
    try {
        $primary = Start-TalkDesktopSmokeInstance -TalkDesktopBinaryPath $TalkDesktopBinaryPath -ConfigPath $primaryConfigPath
        $secondary = Start-TalkDesktopSmokeInstance -TalkDesktopBinaryPath $TalkDesktopBinaryPath -ConfigPath $secondaryConfigPath
        Start-Sleep -Milliseconds 700

        Send-TalkDesktopMenuCommand -Hwnd $secondary.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $secondary.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for hotkey-conflict scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: hotkey unavailable'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: hotkey unavailable'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Assert-TextContains -Haystack ([string]$statusFields['Hotkey detail']) -Needle 'register Talk desktop hotkey' -Context 'hotkey-conflict'
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'hotkey-conflict'
            BinaryPath = $TalkDesktopBinaryPath
            PrimaryConfigPath = $primaryConfigPath
            SecondaryConfigPath = $secondaryConfigPath
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $secondary
        Stop-TalkDesktopSmokeInstance -Instance $primary
    }
}

function Invoke-BrokenConfigRecoverySmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $configPath = Join-Path $ScenarioRoot 'config.toml'
    Write-BrokenTalkSmokeConfig -ConfigPath $configPath

    $instance = $null
    try {
        $instance = Start-TalkDesktopSmokeInstance -TalkDesktopBinaryPath $TalkDesktopBinaryPath -ConfigPath $configPath
        Start-Sleep -Milliseconds 700

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $beforeReloadDialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($beforeReloadDialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for broken-config-recovery pre-reload state'
        }
        $beforeReloadText = Wait-TalkDesktopDialogText -DialogHwnd $beforeReloadDialog -ExpectedText 'Current: Talk: config unavailable'
        $beforeReloadStatusFields = Convert-TalkDesktopStatusTextToMap -DialogText $beforeReloadText
        $beforeReloadStatusKind = Get-TalkDesktopStatusKind -Fields $beforeReloadStatusFields
        $beforeReloadStatusSummary = Get-TalkDesktopStatusSummary -Fields $beforeReloadStatusFields
        $beforeReloadStatusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $beforeReloadStatusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $beforeReloadStatusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: config unavailable'
            })
        Close-TalkDesktopDialog -DialogHwnd $beforeReloadDialog
        Wait-TalkDesktopDialogClosed -TargetProcessId $instance.Process.Id -Title 'Talk status'

        Write-TalkSmokeConfig -ConfigPath $configPath -Hotkey 'Ctrl+Alt+F22' -Transcript 'recovered config smoke'
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1007
        Start-Sleep -Milliseconds 900

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $afterReloadDialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($afterReloadDialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not reopen after config reload'
        }
        $afterReloadText = Wait-TalkDesktopDialogText -DialogHwnd $afterReloadDialog -ExpectedText 'Current: Talk: idle'
        $afterReloadStatusFields = Convert-TalkDesktopStatusTextToMap -DialogText $afterReloadText
        $afterReloadStatusKind = Get-TalkDesktopStatusKind -Fields $afterReloadStatusFields
        $afterReloadStatusSummary = Get-TalkDesktopStatusSummary -Fields $afterReloadStatusFields
        $afterReloadStatusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $afterReloadStatusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $afterReloadStatusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: idle'
                Hotkey = 'Ctrl+Alt+F22'
                'Audio backend' = 'silent'
                'Clipboard backend' = 'dry_run'
            })
        Close-TalkDesktopDialog -DialogHwnd $afterReloadDialog
        Wait-TalkDesktopDialogClosed -TargetProcessId $instance.Process.Id -Title 'Talk status'

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1001
        Start-Sleep -Milliseconds 400
        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1003
        Start-Sleep -Milliseconds 1200

        $log = Get-LatestSessionLog -LogsDir (Join-Path $ScenarioRoot 'logs')
        $session = Get-Content -LiteralPath $log.FullName -Raw | ConvertFrom-Json
        if ($session.status -ne 'cancelled') {
            throw "Expected cancelled session after config reload, got [$($session.status)]"
        }

        [PSCustomObject]@{
            Scenario = 'broken-config-recovery'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            LogPath = $log.FullName
            Status = $session.status
            BeforeReloadDialogText = $beforeReloadText
            AfterReloadDialogText = $afterReloadText
            BeforeReloadStatusKind = $beforeReloadStatusKind
            BeforeReloadStatusSummary = $beforeReloadStatusSummary
            AfterReloadStatusKind = $afterReloadStatusKind
            AfterReloadStatusSummary = $afterReloadStatusSummary
            BeforeReloadStatusFields = $beforeReloadStatusFields
            AfterReloadStatusFields = $afterReloadStatusFields
            BeforeReloadStatusSnapshot = $beforeReloadStatusSnapshot
            AfterReloadStatusSnapshot = $afterReloadStatusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
    }
}

function Invoke-NativeUnavailableStatusSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$TalkDesktopBinaryPath,
        [Parameter(Mandatory = $true)][string]$ScenarioRoot
    )

    $configPath = Join-Path $ScenarioRoot 'config.toml'
    Write-TalkSmokeConfig `
        -ConfigPath $configPath `
        -Hotkey 'Ctrl+Alt+F21' `
        -Transcript 'desktop native status smoke' `
        -AudioBackend 'native_windows' `
        -OutputMode 'clipboard_paste' `
        -ClipboardBackend 'native_windows'

    $instance = $null
    try {
        $instance = Start-TalkDesktopSmokeInstance `
            -TalkDesktopBinaryPath $TalkDesktopBinaryPath `
            -ConfigPath $configPath `
            -EnvironmentOverrides @{
                TALK_DISABLE_NATIVE_AUDIO = '1'
                TALK_DISABLE_NATIVE_CLIPBOARD = '1'
            }
        Start-Sleep -Milliseconds 700

        Send-TalkDesktopMenuCommand -Hwnd $instance.Hwnd -CommandId 1004
        $dialog = Find-DialogByProcessIdAndTitle -TargetProcessId $instance.Process.Id -Title 'Talk status' -TimeoutMs 5000
        if ($dialog -eq [System.IntPtr]::Zero) {
            throw 'Talk status dialog did not open for native-unavailable-status scenario'
        }

        $dialogText = Wait-TalkDesktopDialogText -DialogHwnd $dialog -ExpectedText 'Current: Talk: native unavailable'
        $statusFields = Convert-TalkDesktopStatusTextToMap -DialogText $dialogText
        $statusKind = Get-TalkDesktopStatusKind -Fields $statusFields
        $statusSummary = Get-TalkDesktopStatusSummary -Fields $statusFields
        $statusSnapshot = Convert-TalkDesktopStatusFieldsToSnapshot -Fields $statusFields
        Assert-TalkDesktopStatusFieldValues `
            -Fields $statusFields `
            -ExpectedFields ([ordered]@{
                Current = 'Talk: native unavailable'
                'Current detail' = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                'Audio backend' = 'native_windows'
                'Audio backend readiness' = 'unavailable'
                'Audio backend detail' = 'native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO'
                'Clipboard backend' = 'native_windows'
                'Clipboard backend readiness' = 'unavailable'
                'Clipboard backend detail' = 'native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD'
            })
        Close-TalkDesktopDialog -DialogHwnd $dialog

        [PSCustomObject]@{
            Scenario = 'native-unavailable-status'
            BinaryPath = $TalkDesktopBinaryPath
            ConfigPath = $configPath
            DialogText = $dialogText
            StatusKind = $statusKind
            StatusSummary = $statusSummary
            StatusFields = $statusFields
            StatusSnapshot = $statusSnapshot
        }
    }
    finally {
        Stop-TalkDesktopSmokeInstance -Instance $instance
    }
}

function Get-TalkDesktopSmokeFailureResults {
    param([AllowEmptyCollection()][object[]]$Results)

    @(
        @($Results) | Where-Object {
            -not [string]::IsNullOrWhiteSpace([string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $_ -Name 'FailureKind'))
        }
    )
}

function Assert-TalkDesktopSmokeResultsPassed {
    param(
        [AllowEmptyCollection()][object[]]$Results,
        [string]$Context = 'Talk desktop release smoke'
    )

    $failures = @(Get-TalkDesktopSmokeFailureResults -Results $Results)
    if ($failures.Count -eq 0) {
        return
    }

    $lines = New-Object System.Collections.Generic.List[string]
    foreach ($failure in $failures) {
        $scenarioName = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $failure -Name 'Scenario')
        $failureKind = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $failure -Name 'FailureKind')
        $failureSummary = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $failure -Name 'FailureSummary')
        $failureEvidencePath = [string](Get-TalkDesktopSmokeOptionalPropertyValue -Object $failure -Name 'FailureEvidencePath')

        $line = "$scenarioName [$failureKind]"
        if (-not [string]::IsNullOrWhiteSpace($failureSummary)) {
            $line += ": $failureSummary"
        }
        if (-not [string]::IsNullOrWhiteSpace($failureEvidencePath)) {
            $line += " (evidence: $failureEvidencePath)"
        }
        $lines.Add(" - $line") | Out-Null
    }

    throw ("{0} failed:`n{1}" -f $Context, ($lines -join "`n"))
}

function Invoke-TalkDesktopReleaseSmoke {
    param(
        [string]$BinaryPath,
        [string]$ReleaseDir,
        [string]$SmokeRoot,
        [string[]]$Scenario,
        [switch]$ContinueOnFailure
    )

    Ensure-TalkDesktopSmokeWin32Type

    $talkDesktopBinaryPath = Resolve-TalkDesktopBinaryPath -BinaryPath $BinaryPath -ReleaseDir $ReleaseDir
    $effectiveScenarioList = if ($Scenario -and $Scenario.Count -gt 0) {
        $Scenario
    } else {
        @(
            'cancel-and-status',
            'hotkey-conflict',
            'broken-config-recovery',
            'native-unavailable-status',
            'openai-compatible-success',
            'openai-compatible-audio-input-insert-success',
            'openai-compatible-audio-input-focus-switch-copy-popup-success'
        )
    }

    $effectiveSmokeRoot = if ([string]::IsNullOrWhiteSpace($SmokeRoot)) {
        Join-Path (Join-Path (Get-TalkRepoRoot) '.runtime') ('desktop-release-smoke-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    } else {
        [System.IO.Path]::GetFullPath($SmokeRoot)
    }
    New-Item -ItemType Directory -Path $effectiveSmokeRoot -Force | Out-Null

    $results = New-Object System.Collections.Generic.List[object]
    foreach ($scenarioName in $effectiveScenarioList) {
        $scenarioRoot = Join-Path $effectiveSmokeRoot $scenarioName
        if (Test-Path -LiteralPath $scenarioRoot) {
            Remove-Item -LiteralPath $scenarioRoot -Recurse -Force
        }
        switch ($scenarioName) {
            'cancel-and-status' {
                $results.Add((Invoke-CancelAndStatusSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'hotkey-conflict' {
                $results.Add((Invoke-HotkeyConflictSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'broken-config-recovery' {
                $results.Add((Invoke-BrokenConfigRecoverySmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'native-unavailable-status' {
                $results.Add((Invoke-NativeUnavailableStatusSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'http-provider-success' {
                $results.Add((Invoke-HttpProviderSuccessSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-success' {
                $results.Add((Invoke-OpenAiCompatibleSuccessSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-audio-input-success' {
                $results.Add((Invoke-OpenAiCompatibleChatAudioInputSuccessSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-audio-input-insert-success' {
                $results.Add((Invoke-OpenAiCompatibleChatAudioInputInsertSuccessSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-audio-input-focus-switch-copy-popup-success' {
                $results.Add((Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-enter-copy-success' {
                $results.Add((Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEnterSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            'openai-compatible-audio-input-focus-switch-copy-popup-keyboard-escape-close-success' {
                $results.Add((Invoke-OpenAiCompatibleChatAudioInputFocusSwitchCopyPopupKeyboardEscapeSmoke -TalkDesktopBinaryPath $talkDesktopBinaryPath -ScenarioRoot $scenarioRoot)) | Out-Null
            }
            default {
                throw "Unsupported Talk desktop smoke scenario: $scenarioName"
            }
        }
    }

    $resultItems = $results.ToArray()
    if (-not $ContinueOnFailure) {
        Assert-TalkDesktopSmokeResultsPassed -Results $resultItems
    }

    $resultItems
}

if ($MyInvocation.InvocationName -ne '.') {
    Invoke-TalkDesktopReleaseSmoke `
        -BinaryPath $BinaryPath `
        -ReleaseDir $ReleaseDir `
        -SmokeRoot $SmokeRoot `
        -Scenario $Scenario `
        -ContinueOnFailure:$ContinueOnFailure
}
