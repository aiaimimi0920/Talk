# Talk

[![Build Talk](https://github.com/aiaimimi0920/Talk/actions/workflows/build-talk.yml/badge.svg)](https://github.com/aiaimimi0920/Talk/actions/workflows/build-talk.yml)
[![Release Talk Tag](https://github.com/aiaimimi0920/Talk/actions/workflows/release-talk-tag.yml/badge.svg)](https://github.com/aiaimimi0920/Talk/actions/workflows/release-talk-tag.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

Talk is Neuro's standalone voice input and speech interaction app, formerly
developed under the `HookLess` name.

Talk is an independent local program, not a temporary branch that must be merged
into `Hook/`. It can be used by itself for hotkey-driven dictation and text
insertion, and it can optionally expose local capabilities so Hook and Loom can
call it when installed.

> Naming rule: the official product/subproject name is `Talk`, and the
> machine-readable short name is `talk`. The source directory, Cargo packages
> `talk-*`, binary `talk`, runtime directories, and `TALK_*` environment
> variables now use the current Talk naming.

## Repository

This repository is the standalone Talk source checkout. Clone and validate it
from the repository root:

```powershell
git clone https://github.com/aiaimimi0920/Talk.git
cd Talk
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

The Neuro monorepo consumes Talk as a Git submodule. Talk does not require the
parent repository for normal builds, tests, CLI use, desktop launch, or release
packaging.

Provider credentials are never committed. Use `TALK_PROVIDER_API_KEY` or an
explicit local credential path when running live provider probes. Local
Sherpa-ONNX runtime and model data are downloaded into ignored per-user data
directories and are not committed to this repository.

## Windows product release

The user-facing Windows package is intentionally minimal. It contains exactly:

```text
Talk.exe
talk.toml
```

Copy those two files to a writable directory and start `Talk.exe`. On first
startup, Talk automatically downloads and verifies the pinned Zipformer model
under `%LOCALAPPDATA%\Talk\models\sherpa-onnx`. It also automatically extracts
its hidden local-ASR worker and native Sherpa/ONNX DLLs into
`%LOCALAPPDATA%\Talk\runtime\<payload-hash>`. The download is streamed through
a temporary file, checked against the embedded SHA-256 digest, and installed
atomically. A later launch reuses the verified cache without downloading again.

The product does not require a separate worker executable, DLLs, PowerShell
installer, benchmark binary, or probe script next to `Talk.exe`. If the local
runtime or model cannot be prepared, Talk records the reason and uses the
configured cloud transcription route for that session when one is available.

`talk.toml` is the only user-editable file in the package. For the packaged
DashScope configuration, Talk first checks `TALK_PROVIDER_API_KEY` and then
automatically reuses the standard per-user credential file, when present:

```text
%USERPROFILE%\.neuro\qwen-platform\qwen-dashscope-openai\api-key\manual-live.json
```

That file may contain `apiKey`, `api_key`, or `key`. A custom or non-DashScope
OpenAI-compatible endpoint must provide its credential through
`TALK_PROVIDER_API_KEY` or an explicit local configuration value. A missing
credential does not disable local ASR; it only skips cloud text processing.

The Cargo target `talk-desktop.exe`, `.internal` worker files, model installer
scripts, benchmark tools, and smoke probes remain available in the source tree
for engineering and CI workflows. They are not part of the product directory.

## Product boundary

Talk owns:

- global hotkey / toggle / push-to-talk voice control;
- audio capture and audio artifact planning;
- transcription and optional text-processing provider clients;
- dry-run, clipboard fallback, and explicit native insertion strategies;
- voice session state, logs, failure reasons, and local smoke evidence;
- local capability APIs that can be called by Hook, Loom, or other Neuro apps.

Talk does not own:

- Hook's visual canvas, overlay, screenshot, sticker, or foreground capture UI;
- Loom's agent, workflow, memory, sandbox, or orchestration runtime;
- Gateway's provider credential inventory, routing, relay, or management APIs;
- Tea's ticket, approval, event timeline, or run-evidence source of truth;
- Platform's account, quota, entitlement, operator, or public web surfaces.

## MVP flow

The MVP models a Typeless/OpenLess-style flow:

```text
hotkey -> record/stop -> transcribe -> optional process -> insert -> evidence
```

Current implementation status:

- `talk-core`: config model and voice session state machine.
- `talk-client`: mock and HTTP transcription/text processing interfaces.
- `talk-insert`: dry-run insertion, safe clipboard fallback, a testable
  clipboard-paste strategy boundary, and an explicit Windows-native clipboard
  backend.
- `talk-audio`: audio artifact planning, selectable audio backend
  contracts, readable PCM WAV smoke utilities, and a Windows-native microphone
  capture path.
- `talk-hotkey`: testable toggle/push-to-talk event state machine.
- `talk-daemon`: CLI `check`, `once`, and `serve` commands for MVP smoke
  and local app integration. `once` drives the configured trigger mode through
  the hotkey state machine, captures an audio artifact through `audio.backend`,
  selects the configured provider (`mock` or `http`) for transcribe/process,
  inserts the result, and writes a session JSON file under the configured
  logging directory. `serve` exposes Talk's loopback-only local capability API
  for peers such as Hook and Loom. It honors `output.mode`: `dry_run` records a
  dry-run insert, while `clipboard_paste` uses `output.clipboard_backend` to
  choose either the safe diagnostic fallback or the explicit Windows-native
  clipboard paste path.
- `talk-runtime`: shared end-to-end Talk runtime used by both the CLI daemon
  and the desktop shell.
- `talk-desktop`: Windows-first background shell with a tray icon, real global
  hotkey activation, transient HUD feedback, and direct reuse of the Talk
  runtime pipeline. In `toggle` mode the first hotkey press starts recording and
  the second press stops it; in `push_to_talk` mode recording starts on the
  hotkey press and stops when the shortcut is released or the configured
  recording timeout is reached. On Windows it now also captures the originating
  foreground window at trigger start and best-effort restores that target right
  before `clipboard_paste` insertion so the OpenLess / Typeless-style hotkey
  round-trip remains anchored to the app you started from even if tray / shell
  windows briefly steal focus mid-session.

### Local-first speculative dictation

Talk now has a runnable local-first speculative dictation path behind explicit
config gates. Set `speculative.enabled = true` and
`speculative.local_asr = "external_command"` to run a local ASR adapter that
emits JSON lines. Talk inserts the local final transcript first; if
`speculative.cloud_correction = "provider_text_processor"`, the configured text
processor runs afterward and either applies a conservative same-target patch or
falls back to the editable copy popup. See
[`docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`](docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md)
and
[`examples/desktop-external-asr-speculative-config.toml`](examples/desktop-external-asr-speculative-config.toml).

For packaged desktop use, the current Talk default now mirrors the core
Typeless dictation gesture more closely: `trigger.mode = "toggle"` with
`toggle_shortcut = "RightAlt"`, so one `Right Alt` press starts capture and the
next `Right Alt` press stops it.

The desktop shell can now also route multiple Typeless-style chords from the
same session config:

- `RightAlt`: primary voice input / dictation route
- `RightAlt+/`: translate route
- `RightAlt+Space`: ask / assistant route

Under the hood, Talk desktop now uses a side-aware low-level keyboard hook for
shortcuts that `RegisterHotKey` cannot model correctly, such as `RightAlt`
itself or future chords like `RightAlt+/`. Legacy side-agnostic chords such as
`Ctrl+Alt+F24` still stay on the simpler `RegisterHotKey` path.

## Commands

Run these from the Talk repository root:

```powershell
cargo fmt --manifest-path Cargo.toml --all -- --check
cargo test --manifest-path Cargo.toml --workspace
cargo check --manifest-path Cargo.toml --workspace --all-targets
cargo run --manifest-path Cargo.toml -p talk-daemon -- check --config examples/dev-config.toml
cargo run --manifest-path Cargo.toml -p talk-daemon -- readiness --config examples/dev-config.toml --json
cargo run --manifest-path Cargo.toml -p talk-daemon -- probe-audio --config examples/dev-config.toml --seconds 3 --json
cargo run --manifest-path Cargo.toml -p talk-daemon -- once --config examples/dev-config.toml --mock-text "hello neuro"
cargo run --manifest-path Cargo.toml -p talk-daemon -- once --config examples/once-qwen-audio-input-safe-config.toml --audio-file C:\\path\\to\\sample.wav
cargo run --manifest-path Cargo.toml -p talk-daemon -- play-wav --file C:\\path\\to\\sample.wav --output-device "Virtual Speakers"
cargo run --manifest-path Cargo.toml -p talk-daemon -- serve --config examples/dev-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/dev-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-http-safe-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-http-live-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-openai-compatible-safe-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-openai-compatible-live-config.toml
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-qwen-audio-input-live-config.toml
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopReleaseSmoke.Tests.ps1
Invoke-Pester -Path scripts/tests/Publish-TalkRelease.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopGlobalHotkeyProbe.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopQwenGlobalHotkeyProbe.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopQwenGlobalHotkeySoak.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopQwenNativeMicProbe.Tests.ps1
Invoke-Pester -Path scripts/tests/Start-TalkDesktop.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopLiveOperatorProbe.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopLiveHotkeyProbe.Tests.ps1
Invoke-Pester -Path scripts/tests/Invoke-TalkLiveAudioQwenProbe.Tests.ps1
& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1'
& '.\scripts\Invoke-TalkDesktopGlobalHotkeyProbe.ps1'
& '.\scripts\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1'
& '.\scripts\Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1' -Count 3
& '.\scripts\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1' -ApiKeyJsonPath 'C:\path\to\manual-live.json'
& '.\scripts\Start-TalkDesktop.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51'
& '.\scripts\Start-TalkDesktop.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -ListInputDevices
& '.\scripts\Start-TalkDesktop.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -InputDevice 'Virtual Mic' -ProbeAudio -ProbeSeconds 3
& '.\scripts\Start-TalkDesktop.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -InputDevice '麦克风' -ProbeQwenRoundTrip -ProbeSeconds 3
& '.\scripts\Invoke-TalkDesktopLiveOperatorProbe.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -InputDevice '麦克风'
& '.\scripts\Invoke-TalkDesktopLiveHotkeyProbe.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -InputDevice '麦克风' -AudioProbeSeconds 3
& '.\scripts\Invoke-TalkLiveAudioQwenProbe.ps1' -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51' -InputDevice '麦克风'
& '.\scripts\Publish-TalkRelease.ps1' -VersionId 'desktop-shell-20260704-v7'
```

The product release generated by `scripts/Publish-TalkRelease.ps1 -ProductProfile`
is the two-file package described above. The publisher writes optional build and
smoke evidence outside the product directory under
`release/Talk/_ci/<version-id>/`; GitHub Actions uploads that evidence as a
separate artifact. Engineering launchers and probes are intentionally kept in
the repository and are not copied into the user package.

For source-checkout and CI-only desktop diagnostics, the Cargo target
`talk-desktop.exe` still accepts an explicit `--config` path and the existing
PowerShell probe scripts can inject fixed WAV files, enumerate native devices,
or exercise provider routes. Those commands validate the implementation; they
are not required to install or run the standalone product.

The development config writes local smoke artifacts to:

- `.runtime/talk/audio/*.wav`
- `.runtime/talk/logs/*.json`

The session JSON includes the final status, transcript, output text, trigger
mode, trigger events, and insertion outcome. If provider, processing, or
insertion fails after the trigger sequence starts, the daemon still persists a
failed session JSON with the error reason before returning the original error.
These files are runtime evidence only and are ignored by git.

## Desktop shell behavior

`Talk.exe` is the Windows-first OpenLess / Typeless style Talk shell. The source
Cargo target is named `talk-desktop.exe`, but product releases expose it only as
`Talk.exe`. The shell is intentionally small:

- no main window;
- background lifetime through the tray icon;
- one global shortcut from Talk config;
- short-lived HUD states such as `Talk: listening`, `Talk: transcribing`,
  `Talk: inserting`, `Talk: done`, and `Talk: failed`;
- one active voice session at a time.

Tray actions currently include:

- `Start dictation`
- `Stop recording`
- `Cancel recording`
- `Show Talk status`
- `Open Talk logs folder`
- `Open Talk config`
- `Reload Talk config`
- `Exit Talk`

This shell keeps Talk independent from Hook and reuses the same runtime
pipeline, evidence writing, provider selection, and insertion behavior as
`talk-daemon`.

If the configured global hotkey is invalid or already taken by another app,
`Talk.exe` stays alive in tray-only mode instead of exiting. The
tray menu still lets you start dictation manually, open the config file, reload
the config, and recover without restarting the whole app.

If the Talk config file itself is temporarily broken and cannot be parsed,
`Talk.exe` also stays alive in a config-unavailable tray mode. In that
state, manual dictation is disabled until the config is fixed and reloaded, but
the user can still open the config file and recover without restarting Talk.

When Talk is recording, the tray now distinguishes:

- `Stop recording`: finish the capture and continue into transcribe/process/insert
- `Cancel recording`: discard the in-progress recording and persist a cancelled
  session instead of generating transcript/output text

The tray header also shows a short reason line when Talk is unavailable because
of a hotkey or config problem.

`Show Talk status` opens a small Windows status dialog with:

- current shell state and any current recovery detail;
- resolved config path and logs path;
- current hotkey binding state;
- configured audio / clipboard backend plus native readiness detail when
  `native_windows` is selected;
- last completed / failed / cancelled session summary and detail.

## Provider-backed desktop configs

The default `examples/dev-config.toml` is intentionally conservative:
`mock` provider, `silent` audio, and `dry_run` output. That is ideal for smoke
and CI, but it is not the config you actually want for Typeless-style daily
use.

Talk now ships:

- two lower-level custom-adapter examples;
- two generic OpenAI-compatible examples for providers that expose
  `/v1/audio/transcriptions`;
- two Qwen examples for providers that expose audio understanding through
  `chat/completions` with `input_audio`.

### Recommended OpenAI-compatible configs

These are the configs that move Talk closest to real Typeless-style hotkey
conversation without needing a Talk-specific custom provider process.

- `examples/desktop-openai-compatible-safe-config.toml`
  - `provider.kind = "openai_compatible"`
  - `voice_mode = "command"`
  - `audio.backend = "silent"`
  - `output.mode = "dry_run"`
  - use this first to validate a real model path without touching the real
    microphone or foreground clipboard.
- `examples/desktop-openai-compatible-live-config.toml`
  - `provider.kind = "openai_compatible"`
  - `voice_mode = "dictate"`
  - `trigger.mode = "toggle"`
  - `trigger.toggle_shortcut = "RightAlt"`
  - `desktop.shortcuts.translate_shortcut = "RightAlt+/"`
  - `desktop.shortcuts.ask_shortcut = "RightAlt+Space"`
  - `audio.backend = "native_windows"`
  - `output.mode = "clipboard_paste"`
  - `output.clipboard_backend = "native_windows"`
  - use this for real OpenLess / Typeless-style Windows hotkey conversation,
    with `RightAlt` as dictation and the two extra Typeless-style companion
    routes enabled from the same shell instance.

Both examples currently point at a local OpenAI-compatible gateway shape:

```text
http://127.0.0.1:4200/v1/audio/transcriptions
http://127.0.0.1:4200/v1/chat/completions
```

They also read auth from:

```text
TALK_PROVIDER_API_KEY
```

If you point these configs at a local Gateway instance, set the environment
variable before launching Talk, for example:

```powershell
$env:TALK_PROVIDER_API_KEY = 'your-gateway-or-provider-key'
cargo run --manifest-path Cargo.toml -p talk-desktop -- --config examples/desktop-openai-compatible-live-config.toml
```

If Windows has multiple microphone or virtual-input endpoints, you can
optionally pin the live desktop config to a specific one:

```toml
[audio]
backend = "native_windows"
input_device = "Virtual Mic"
```

### Recommended Qwen ASR + Qwen chat configs

These configs are for providers that do not expose `/v1/audio/transcriptions`
but can still accept audio through `chat/completions` input parts.

- `examples/once-qwen-audio-input-safe-config.toml`
  - `provider.kind = "openai_compatible"`
  - `provider.transcription_transport = "chat_completions_audio_input"`
  - `voice_mode = "command"`
  - `output.mode = "dry_run"`
  - use this with `talk once --audio-file <wav>` to validate a real Qwen ASR
    + Qwen chat path without touching the live microphone or foreground
    clipboard.
- `examples/desktop-qwen-audio-input-live-config.toml`
  - `provider.kind = "openai_compatible"`
  - `provider.transcription_transport = "chat_completions_audio_input"`
  - `voice_mode = "dictate"`
  - `trigger.mode = "toggle"`
  - `trigger.toggle_shortcut = "RightAlt"`
  - `desktop.shortcuts.translate_shortcut = "RightAlt+/"`
  - `desktop.shortcuts.ask_shortcut = "RightAlt+Space"`
  - `audio.backend = "native_windows"`
  - `output.mode = "clipboard_paste"`
  - `output.clipboard_backend = "native_windows"`
  - use this for real Windows hotkey conversation when your upstream supports
    audio input over `chat/completions`.

Both Qwen examples point at:

```text
https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions
```

and read auth from:

```text
TALK_PROVIDER_API_KEY
```

These Qwen live configs also accept the same optional
`audio.input_device = "<device name>"` field when you need Talk to use a
specific Windows microphone / virtual input endpoint instead of the current
default device.

Example safe validation flow:

```powershell
Add-Type -AssemblyName System.Speech
$wav = Join-Path $env:TEMP 'talk-qwen-check.wav'
$synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
$synth.SetOutputToWaveFile($wav)
$synth.Speak('what is the capital of France')
$synth.Dispose()

$env:TALK_PROVIDER_API_KEY = 'your-qwen-key'
cargo run --manifest-path Cargo.toml -p talk-daemon -- `
  once `
  --config examples/once-qwen-audio-input-safe-config.toml `
  --audio-file $wav
```

On July 5, 2026, this exact Talk path was validated locally against DashScope
with:

- `transcription_model = "qwen3-asr-flash"`
- `chat_model = "qwen3.7-plus"`

and produced:

- transcript: `What is the capital of France?`
- final output: `Paris`

On July 5, 2026, the Talk desktop shell path was also validated locally with
the same provider pair by launching `talk-desktop.exe`, injecting a fixed WAV
through the desktop-only audio override, triggering the session through the
desktop hotkey message path, and observing:

- transcript: `What is the capital of France?`
- final output: `Paris.`

On July 5, 2026, the same desktop shell path was further validated with
`output.mode = "clipboard_paste"` and
`output.clipboard_backend = "native_windows"` by keeping a foreground text
target active during the session and confirming that the inserted result
actually landed in that target:

- transcript: `What is the capital of France?`
- final output: `Paris`
- foreground inserted text: `Paris`

### Lower-level custom HTTP adapter configs

These remain useful when you still want to host your own Talk-specific HTTP
adapter that reads local audio paths and returns the simplified Talk-local
contract.

- `examples/desktop-http-safe-config.toml`
  - `provider.kind = "http"`
  - `audio.backend = "silent"`
  - `output.mode = "dry_run"`
  - use this first when validating that your provider contract is correct
    without touching the real microphone or foreground clipboard.
- `examples/desktop-http-live-config.toml`
  - `trigger.mode = "toggle"`
  - `trigger.toggle_shortcut = "RightAlt"`
  - `desktop.shortcuts.translate_shortcut = "RightAlt+/"`
  - `desktop.shortcuts.ask_shortcut = "RightAlt+Space"`
  - `provider.kind = "http"`
  - `audio.backend = "native_windows"`
  - `output.mode = "clipboard_paste"`
  - `output.clipboard_backend = "native_windows"`
  - use this for real OpenLess / Typeless-style Windows operation once the
    provider path is already validated.

Both examples use the same placeholder endpoint:

```text
http://127.0.0.1:18080/provider
```

Replace that with your actual Talk-compatible provider endpoint before manual
use.

### OpenAI-compatible provider contract

When `provider.kind = "openai_compatible"`, Talk uses two standard OpenAI-style
surfaces instead of the older Talk-local custom adapter:

- `provider.audio_transcriptions_endpoint`
- `provider.chat_completions_endpoint`
- `provider.transcription_transport`

The default transcription transport is:

- `provider.transcription_transport = "audio_transcriptions"`

For that default transport, Talk sends:

- `POST multipart/form-data`
- form field `model = <provider.transcription_model>`
- file field `file = <captured wav>`
- optional `Authorization: Bearer <key>` when `provider.api_key` or
  `provider.api_key_env` resolves to a value

Expected transcription response shape:

```json
{
  "text": "transcribed text"
}
```

Talk also supports:

- `provider.transcription_transport = "chat_completions_audio_input"`

For that transport, Talk posts audio bytes to
`provider.audio_transcriptions_endpoint` as an OpenAI-compatible
`chat/completions` request whose first user content part is:

```json
{
  "type": "input_audio",
  "input_audio": {
    "data": "data:audio/wav;base64,..."
  }
}
```

Expected response shape for that transport:

```json
{
  "choices": [
    {
      "message": {
        "content": "transcribed text"
      }
    }
  ]
}
```

Processing request:

```json
{
  "model": "gpt-4o-mini",
  "messages": [
    {
      "role": "system",
      "content": "mode-specific Talk prompt"
    },
    {
      "role": "user",
      "content": "Transcript:\n...\n\nFront context JSON:\n..."
    }
  ]
}
```

Expected processing response shape:

```json
{
  "choices": [
    {
      "message": {
        "content": "assistant reply text"
      }
    }
  ]
}
```

For the packaged/live desktop shell, the current Typeless-style baseline is:

- primary `voice_mode = "dictate"` on `RightAlt`
- `desktop.shortcuts.translate_shortcut = "RightAlt+/"`
- `desktop.shortcuts.ask_shortcut = "RightAlt+Space"`

That means the shell can keep the most Typeless-like default writing flow on
plain `RightAlt`, while still exposing a dedicated assistant route through
`RightAlt+Space`.

### Custom HTTP provider contract

When `provider.kind = "http"`, Talk currently posts the following request
shapes to `provider.endpoint`:

Transcribe request:

```json
{
  "audio_path": "C:\\path\\to\\captured.wav",
  "context": {
    "source": "hook-panel",
    "app_name": "Hook",
    "window_title": "Neuro editor",
    "selected_text": "hello source text"
  }
}
```

Process request:

```json
{
  "transcript": "transcribed text",
  "mode": "dictate",
  "context": {
    "source": "hook-panel",
    "app_name": "Hook",
    "window_title": "Neuro editor",
    "selected_text": "hello source text"
  }
}
```

Both endpoints currently expect the same minimal success response shape:

```json
{
  "text": "provider output text"
}
```

The desktop shell and `talk once` both reuse this exact runtime contract.

## Desktop release smoke automation

`scripts/Invoke-TalkDesktopReleaseSmoke.ps1` automates the Windows desktop
shell smoke that was previously manual. By default it:

- finds the newest `release/Talk/*` directory that actually contains
  `talk-desktop.exe`;
- launches the release desktop shell in isolated temporary configs;
- verifies three recovery flows:
  - cancel path writes a `cancelled` session log and updates `Show Talk status`;
  - hotkey-conflict startup survives and reports `Talk: hotkey unavailable`;
  - broken-config startup survives, reload recovers to idle, and a later cancel
    still writes a `cancelled` session log;
  - native-unavailable startup survives, reports `Talk: native unavailable`,
    and `Show Talk status` includes the explicit native audio / clipboard
    backend readiness lines.
  - openai-compatible-success starts/stops the desktop shell through the real
    hotkey activation path, reaches a local fake OpenAI-compatible provider,
    persists a completed session log, and confirms `Show Talk status` reports
    the last completed provider-backed run.
  - openai-compatible-audio-input-insert-success drives the recommended
    `chat_completions_audio_input` transport through the desktop shell, then
    verifies the full foreground insertion chain by confirming the provider
    result actually lands in a live text target window through the native
    Windows clipboard paste path.
  - openai-compatible-audio-input-focus-switch-copy-popup-success starts from
    one editable target, deliberately switches foreground to a different
    editable target during thinking, and then verifies that Talk inserts the
    corrected result into the currently focused target while leaving the
    origin target unchanged. The historical scenario name is retained for
    release-script compatibility.

`http-provider-success` is still available as an explicit manual scenario when
you want to verify the older Talk-local custom adapter path.

`openai-compatible-audio-input-success` is also available as an explicit manual
scenario when you want to verify the newer `chat_completions_audio_input`
transcription transport through the desktop shell.

`openai-compatible-audio-input-insert-success` is available as an explicit
manual scenario when you want to verify the full Typeless-style desktop chain:
desktop hotkey activation, `chat_completions_audio_input` transcription,
provider-backed completion, native Windows clipboard paste, and actual text
landing in a foreground target window.

`openai-compatible-audio-input-focus-switch-copy-popup-success` is available as
an explicit manual scenario when you want to verify current-focus insertion:

- recording starts from origin window **A**
- if thinking finishes while **A** is still foreground, Talk may insert there
- if foreground has switched to another editable window **B**, Talk inserts
  into **B** instead of writing back to **A**
- if the current target is not editable or cannot be proven safe, Talk keeps
  the corrected result in the copy popup and does not paste

The smoke verifies the provider result, the current-target diagnostic strategy,
and the origin/alternate text-target contents.

The desktop copy popup itself now also supports lightweight keyboard handling:

- `Enter`: copy the latest transcript and close the popup
- `Escape`: close the popup without copying

For a concise release-side probe wrapper around that exact path, use:

```powershell
& '.\scripts\Invoke-TalkDesktopGlobalHotkeyProbe.ps1'
```

That helper runs the foreground insert smoke scenario, but returns and writes a
small summary object instead of the full heavy smoke result. It is useful when
you specifically want evidence for the real system-level hotkey chord path
without manually digging through the larger smoke artifacts.

For the same path against the real DashScope / Qwen OpenAI-compatible provider,
use:

```powershell
& '.\scripts\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json'
```

If the current machine already has the standard local DashScope credential file
at
`%USERPROFILE%\.neuro\qwen-platform\qwen-dashscope-openai\api-key\manual-live.json`,
the shorter no-arg packaged form also works:

```powershell
& 'C:\path\to\release\Talk\desktop-shell-20260705-v51\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v51'
```

On July 5, 2026, that exact packaged no-arg v51 path was validated locally and
produced:

- transcript: `What is the capital of France?`
- final output: `Paris.`
- foreground inserted text: `Paris.`

That probe uses the packaged desktop binary, a real provider API key, a fixed
spoken WAV, the real system-level hotkey chord, and a live foreground text
target. It writes a concise summary JSON so you can confirm transcript, final
provider output, and inserted foreground text without parsing the full smoke
object. The explicit `-ApiKeyJsonPath` form remains available when you want to
override the local default credential file.

When you want a quick **release-side stability pass** instead of a single probe,
use the packaged soak wrapper:

```powershell
& 'C:\path\to\release\Talk\desktop-shell-20260705-v52\Invoke-TalkDesktopQwenGlobalHotkeySoak.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v52' `
  -Count 3
```

That helper repeatedly runs the packaged Qwen global hotkey probe, writes one
subdirectory per run, and then emits a concise aggregate summary with:

- `totalRuns`
- `successfulRuns`
- `failedRuns`
- `successRate`
- `averageDurationMs`
- per-run transcript / output / inserted-text evidence

The per-run probe summary and the aggregate soak summary now also surface the
desktop insert-target sidecar hints that matter for Typeless-style routing
debugging:

- `insertTargetOutputStrategy` / `insertTargetShowCopyPopupOnlyRuns`
- `insertTargetFocusLooksEditable`
- `insertTargetFocusClassName`
- `insertTargetAutomationControlType`
- `insertTargetAutomationFrameworkId`

That makes it much easier to tell whether a run inserted directly into the
foreground editor, or deliberately fell back to the copy popup because the
final focused control no longer looked editable at insert time.

On July 5, 2026, the packaged desktop shell also gained an explicit
"origin-foreground restore" step before native clipboard insertion. That change
was validated with two independent packaged v61 soak runs:

- `.runtime/desktop-qwen-global-hotkey-soak-v61/qwen-global-hotkey-soak-summary.json`
- `.runtime/desktop-qwen-global-hotkey-soak-v61-rerun/qwen-global-hotkey-soak-summary.json`

Both runs completed at 4/4 raw success and 3/3 measured success after one
warmup run, which materially improved repeated-release stability versus the
earlier focus-loss failures.

By default the soak wrapper fails the overall command if any iteration fails,
but it still writes the aggregate summary JSON first. Use `-AllowFailures` when
you want to keep the command exit code green while collecting mixed evidence.

When you want to validate the same path with the **real microphone and a human
voice**, use:

```powershell
& '.\scripts\Invoke-TalkDesktopLiveOperatorProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v39' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json' `
  -InputDevice '麦克风'
```

That operator probe launches the desktop shell against a live `native_windows`
microphone path, opens a foreground text target, prints the hotkey instructions
to the console, waits for a completed session log, and then writes a small
summary JSON with transcript, provider output, and inserted foreground text.

When `-InputDevice` is provided, the live-operator probe now runs a short
preflight `talk.exe probe-audio` capture first and asks the operator to speak
during that window. If the selected input device still comes back with
`silent = true` / `peak = 0`, the script fails early with a summary JSON
instead of launching the full provider-backed hotkey flow.

When you want the packaged desktop shell to run the **same live microphone path**
but have Codex / PowerShell automatically send the start and stop hotkeys for
you, use:

```powershell
& '.\scripts\Invoke-TalkDesktopLiveHotkeyProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v44' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json' `
  -InputDevice '麦克风' `
  -AudioProbeSeconds 3 `
  -InitialDelaySeconds 12 `
  -RecordingSeconds 6 `
  -ExpectedText '测试成功'
```

That probe launches the packaged desktop shell with a toggle-mode native-mic
config, opens a foreground text target, waits for the requested preparation
window, automatically fires the configured global hotkey to start recording,
waits for the requested recording window, fires the hotkey again to stop, and
then writes a summary JSON with transcript, provider output, inserted text, and
the captured WAV artifact path.

When you want to validate the full live hotkey chain against arbitrary spoken
content instead of a fixed expected answer, pass a blank expected-text value:

```powershell
& '.\scripts\Invoke-TalkDesktopLiveHotkeyProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v52' `
  -InputDevice '麦克风' `
  -AudioProbeSeconds 4 `
  -InitialDelaySeconds 8 `
  -RecordingSeconds 6 `
  -TimeoutSeconds 45 `
  -ExpectedText ''
```

On July 5, 2026, this exact v52 packaged live-hotkey path was validated locally
with a real human microphone input and automatic start/stop hotkey delivery. In
that run, Talk completed the full OpenLess / Typeless-style desktop chain:

- preflight native audio probe: non-silent signal on `麦克风`
- live transcript: `你好，今天是一个晴朗的春天。`
- provider output: `你好！春天的天气真好。请问有什么我可以帮您的？`
- foreground inserted text:
  `你好！春天的天气真好。请问有什么我可以帮您的？`

The corresponding evidence summary lives at:

- `.runtime/desktop-live-hotkey-human-v52/live-hotkey-probe-summary.json`

Before it launches the full provider-backed hotkey round-trip, the live-hotkey
probe now also runs a short native `talk.exe probe-audio` preflight capture and
asks the operator to speak during that window. If the selected/default input
device still comes back silent, the script fails early with a summary JSON
instead of launching the desktop shell and sending an empty recording into the
provider path.

When you need to validate Talk against a different OpenAI-compatible upstream
instead of the default DashScope/Qwen pair, the same live-hotkey probe now also
accepts optional provider overrides:

- `-ProviderAudioTranscriptionsEndpoint`
- `-ProviderChatCompletionsEndpoint`
- `-ProviderTranscriptionTransport`
- `-ProviderTranscriptionModel`
- `-ProviderChatModel`

This is useful when the current machine already has a local gateway or another
OpenAI-compatible relay available, but does not have a direct DashScope API key
configured for the packaged default config.

When you want to separate **live microphone capture** from the **provider
round-trip** entirely, use:

```powershell
& '.\scripts\Invoke-TalkLiveAudioQwenProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v39' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json' `
  -InputDevice '麦克风'
```

That helper:

1. captures a short live native audio sample with `talk.exe probe-audio`;
2. refuses to continue if the captured signal is still silent;
3. feeds the resulting WAV directly into `talk.exe once --audio-file` against
   the real Qwen audio-input transport;
4. writes a summary JSON with capture peak/RMS plus the final transcript and
   provider output.

This is useful when you need to know whether a failure comes from:

- the native microphone path itself, or
- the downstream Qwen transcription / response path.

This lower-level live-audio probe also now accepts the same optional provider
override knobs:

- `-ProviderAudioTranscriptionsEndpoint`
- `-ProviderChatCompletionsEndpoint`
- `-ProviderTranscriptionTransport`
- `-ProviderTranscriptionModel`
- `-ProviderChatModel`

so you can reuse the exact same live capture artifact against either the
default DashScope/Qwen path or another OpenAI-compatible relay.

For a more automated native-capture-oriented probe against the real Qwen
provider, use:

```powershell
& '.\scripts\Invoke-TalkDesktopQwenNativeMicProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v38' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json' `
  -InputDevice 'Virtual Mic'
```

That helper still uses `audio.backend = "native_windows"` instead of the fixed
desktop audio-file override. It now accepts `-InputDevice` for multi-endpoint
Windows setups, runs a preflight `talk.exe probe-audio` capture before it
launches the provider-backed desktop shell, and always writes a concise summary
JSON so silent or misrouted input devices are easier to diagnose.

Like the live-hotkey and live-audio probes, this native-mic desktop probe now
also accepts optional provider endpoint/model overrides for non-DashScope
OpenAI-compatible upstreams.

When `-InputDevice` matches a virtual route such as `Virtual Mic` and no
explicit `-SpeakerOutputDevice` is provided, the probe now defaults its speaker
playback side to `Virtual Speakers`. This keeps the automated native-capture
probe on the same virtual audio path instead of accidentally replaying the test
WAV through the machine's unrelated default physical speakers.

If that preflight audio probe reports `silent = true` or `peak = 0`, the script
fails before the provider call. This keeps Talk from sending empty native audio
to Qwen and then pasting a misleading hallucinated reply into the foreground
window.

Example commands:

```powershell
Invoke-Pester -Path scripts/tests/Invoke-TalkDesktopReleaseSmoke.Tests.ps1

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1'

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1' `
  -Scenario @('openai-compatible-success')

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1' `
  -Scenario @('openai-compatible-audio-input-success')

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1' `
  -Scenario @('openai-compatible-audio-input-insert-success')

& '.\scripts\Invoke-TalkDesktopGlobalHotkeyProbe.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260705-v32' `
  -SmokeRoot '.runtime\desktop-global-hotkey-probe-v32'

& '.\scripts\Invoke-TalkDesktopQwenGlobalHotkeyProbe.ps1' `
  -BinaryPath 'C:\path\to\release\Talk\desktop-shell-20260705-v33\talk-desktop.exe' `
  -ApiKeyJsonPath 'C:\path\to\manual-live.json' `
  -SmokeRoot '.runtime\desktop-qwen-global-hotkey-probe-v33'

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1' `
  -Scenario @('http-provider-success')

& '.\scripts\Invoke-TalkDesktopReleaseSmoke.ps1' `
  -ReleaseDir 'C:\path\to\release\Talk\desktop-shell-20260704-v5' `
  -SmokeRoot '.runtime\desktop-release-smoke-v5-manual'
```

The script writes smoke artifacts under `.runtime/desktop-release-smoke-*`
unless `-SmokeRoot` is provided explicitly.

For developer-only verification, `talk-desktop.exe` also supports:

```text
TALK_DESKTOP_AUDIO_FILE_OVERRIDE
```

When that environment variable points at an existing WAV file, the desktop
shell skips live microphone capture for that session and feeds the fixed file
into the normal Talk runtime pipeline. This is intended for smoke automation
and real provider probes, not for normal end-user operation.

Release manifests now also preserve structured desktop smoke status evidence
under `desktopSmoke`, including parsed `Show Talk status` fields such as
`Current`, `Hotkey`, `Audio backend`, `Audio backend readiness`, and
`Clipboard backend readiness` when available for a scenario. Each smoke record
keeps the raw UI-aligned `statusFields` map for fidelity and a normalized
`statusSnapshot` object with stable camelCase keys such as `current`,
`configPath`, `hotkey`, `audioBackendReadiness`, and
`clipboardBackendReadiness`. Recovery scenarios also emit
`beforeReloadStatusSnapshot` and `afterReloadStatusSnapshot`.

These smoke tools operate on source/CI engineering bundles. They are not copied
into a product release. The standalone product reads `talk.toml` next to
`Talk.exe` and starts directly without a PowerShell launcher.

## Desktop release packaging automation

`scripts/Publish-TalkRelease.ps1` is the release-side wrapper for Talk. With
`-ProductProfile`, it:

- reruns the Talk workspace verification commands unless `-SkipVerification` is used;
- rebuilds the desktop shell and local Sherpa worker unless `-SkipBuild` is used;
- appends the worker and required Sherpa/ONNX DLLs to the desktop PE as a
  hash-verified payload;
- writes only `Talk.exe` and `talk.toml` into `release/Talk/<version-id>/`;
- optionally writes product evidence under `release/Talk/_ci/<version-id>/`
  when `-EmitEvidence` is used;
- keeps engineering bundle behavior available only when `-ProductProfile` is
  omitted.

Validate the final product boundary with:

```powershell
.\scripts\Test-TalkProductRelease.ps1 `
  -ProductPath C:\path\to\release\Talk\<version-id>
```

The validator rejects extra files or subdirectories and verifies that
`Talk.exe` ends with the embedded `TLPAY001` runtime payload marker.

The remaining manifest, summary, native-preflight, and smoke contracts below
describe the engineering evidence path. They do not add files to the product
directory.

The generated `manifest.json` now uses `schemaVersion: 2` for the Talk-local
desktop release contract. A canonical example fixture for that schema lives at
`contracts/release/examples/talk-release-manifest.json`. The companion
schema file lives at `contracts/release/manifest.schema.json`, and
`scripts/Test-TalkReleaseManifest.ps1` validates either the fixture or a
real packaged `manifest.json`.

For consumers that do not need the full verbose manifest, Talk also writes a
derived `release-summary.json` file and exposes the same projection through
`scripts/Get-TalkReleaseSummary.ps1`. The summary contract itself is
documented by `contracts/release/summary.schema.json`, and
`scripts/Test-TalkReleaseSummary.ps1` validates either the canonical
fixture or a packaged `release-summary.json`.

The native preflight currently verifies two release-binary contracts:

- `audio-native-disabled`: explicit `audio.backend = "native_windows"` plus
  `TALK_DISABLE_NATIVE_AUDIO=1` must fail and write a failed session log instead
  of silently creating a WAV fallback.
- `clipboard-native-disabled`: explicit
  `output.clipboard_backend = "native_windows"` plus
  `TALK_DISABLE_NATIVE_CLIPBOARD=1` must fail and write a failed session log
  instead of silently falling back to clipboard fallback mode.

Before those negative-path checks, the release wrapper now also runs a positive
native readiness probe against the packaged `talk.exe`. That probe uses Talk's
own `readiness` command and records whether the current Windows machine has:

- a visible configured microphone/input device, or default input device when
  no explicit `audio.input_device` is set, with a supported sample format for
  the `native_windows` audio path;
- callable Windows clipboard access for the `native_windows` clipboard path.

The readiness JSON is written under the smoke/runtime evidence root and then
summarized into both `BUILD_INFO.txt` and `manifest.json` as `nativeReadiness`.
If the packaged Talk binary reports either native backend as unavailable, the
release wrapper now fails instead of silently publishing a package with unknown
native viability.

Example commands:

```powershell
Invoke-Pester -Path scripts/tests/Publish-TalkRelease.Tests.ps1

& '.\scripts\Publish-TalkRelease.ps1' `
  -VersionId 'desktop-shell-20260704-v7' `
  -SmokeRoot '.runtime\desktop-release-smoke-v7-scripted'

& '.\scripts\Publish-TalkRelease.ps1' `
  -VersionId 'desktop-shell-fast-repack' `
  -SkipVerification `
  -SkipBuild `
  -SkipSmoke

& '.\scripts\Publish-TalkRelease.ps1' `
  -VersionId 'desktop-shell-fast-repack-no-preflight' `
  -SkipVerification `
  -SkipBuild `
  -SkipSmoke `
  -SkipNativePreflight

& '.\scripts\Publish-TalkRelease.ps1' `
  -VersionId 'desktop-shell-fast-repack-no-native-readiness' `
  -SkipVerification `
  -SkipBuild `
  -SkipSmoke `
  -SkipNativeReadiness
```

## Native readiness probe

`talk.exe readiness` is the Talk-local environment probe for explicit
`native_windows` backends.

```powershell
cargo run --manifest-path Cargo.toml -p talk-daemon -- readiness `
  --config examples/dev-config.toml `
  --json
```

The JSON includes:

- `audio.configuredBackend`
- `audio.nativeWindows.status|reason|requestedDeviceName|deviceName|availableDeviceNames|defaultSampleRateHz|defaultChannels|sampleFormat`
- `clipboard.configuredBackend`
- `clipboard.nativeWindows.status|reason`
- `allReady`

Talk also exposes a small Windows-only playback helper for debugging native
audio routes without launching the desktop shell:

```powershell
cargo run --manifest-path Cargo.toml -p talk-daemon -- `
  play-wav `
  --file C:\\path\\to\\sample.wav `
  --output-device "Virtual Speakers"
```

This is mainly intended for native-mic and virtual-audio diagnostics. It lets
you confirm that Talk itself can target a named Windows output endpoint before
you blame the desktop shell, provider, or foreground insertion path.

This command is side-effect-light on purpose: it does not start a recording or
paste text into the foreground app. Instead, it proves that Talk can see the
configured native input device (or the current default device when
`audio.input_device` is unset) and can open the Windows clipboard API path
before the release wrapper attempts real native usage.

## Local capability server

Talk can run as a local capability server so Hook, Loom, or another Neuro app
can discover it and request a voice session without embedding Talk's full voice
stack.

```powershell
cargo run --manifest-path Cargo.toml -p talk-daemon -- serve --config examples/dev-config.toml
```

Development options:

```powershell
cargo run --manifest-path Cargo.toml -p talk-daemon -- serve `
  --config examples/dev-config.toml `
  --host 127.0.0.1 `
  --port 0 `
  --manifest-dir .runtime/neuro/capabilities
```

`--port 0` asks Windows to allocate a free local port. `--host` must resolve to
a loopback address (`127.x.x.x`, `::1`, or `localhost`); Talk rejects
non-loopback hosts before writing a manifest. This keeps microphone, clipboard,
and text-insertion capabilities off the LAN by default.

When `--manifest-dir` is omitted, Talk writes:

```text
%APPDATA%\Neuro\capabilities\talk.json
```

If `APPDATA` is unavailable, the development fallback is:

```text
.runtime/neuro/capabilities/talk.json
```

The manifest uses `schemaVersion: 1`, `appId: "talk"`, HTTP transport, and a
process-local bearer token. Consumers must read `transport.authToken` from the
manifest and send it as:

```http
Authorization: Bearer <token>
```

The first server surface is:

```http
GET /v1/health
GET /v1/capabilities
POST /v1/invoke
```

`POST /v1/invoke` accepts the Neuro Local Capability envelope:

```json
{
  "requestId": "request-1",
  "caller": "hook",
  "capability": "voice.capture.once",
  "input": {
    "mode": "dictation",
    "context": {
      "source": "hook-panel"
    }
  }
}
```

Supported capability IDs currently advertised by Talk are:

- `voice.capture.once`
- `voice.dictate`

Product release archives expose the user-facing executable as `Talk.exe` next
to `talk.toml` under `release\Talk\<versionId>\`. The lower-level `talk.exe`
CLI remains a source/engineering target and is not shipped beside the product
executable.

## Audio behavior

- `audio.backend = "silent"`: writes a short readable PCM WAV artifact. This is
  the safe default for automated smoke runs and does not touch the microphone.
- `audio.backend = "native_windows"`: explicitly selects the Windows-native
  microphone path. On Windows it captures through CPAL from either the current
  default input device or the optional `audio.input_device` name match,
  converts the captured buffer into the configured PCM WAV artifact, and then
  continues through the selected provider. It is not allowed to silently fall
  back to the silent WAV smoke path; if native capture is unavailable,
  unsupported, disabled, or fails, the daemon persists a failed session JSON.
  If the native input stream returns only silence, Talk now treats that as a
  capture failure instead of sending empty audio into the provider path and
  risking a hallucinated reply.
  Set `TALK_DISABLE_NATIVE_AUDIO=1` to force this native path to fail before
  any native audio side effects. For manual smoke runs,
  `TALK_NATIVE_AUDIO_SECONDS=<n>` can shorten native capture duration; the
  value is still capped by `audio.max_recording_seconds`.

These `TALK_*` variables are the current supported names.

## Output behavior

- `dry_run`: stores the text through the testable dry-run inserter. This is the
  default development mode.
- `clipboard_paste`: the library exposes a testable strategy boundary that
  writes text, sends a paste shortcut, and optionally restores the previous
  clipboard through injected backends.
- `clipboard_backend = "fallback"`: records a diagnostic `clipboard_fallback`
  outcome without mutating the real foreground clipboard. This is the safe
  default for smoke runs.
- `clipboard_backend = "native_windows"`: explicitly enables the Windows
  clipboard backend and sends Ctrl+V to the foreground window. This is intended
  for manual Windows smoke and future Talk desktop integration, not unattended
  CI. Set `TALK_DISABLE_NATIVE_CLIPBOARD=1` to force this native path to
  fail before any native clipboard or keyboard side effects.

`TALK_DISABLE_NATIVE_CLIPBOARD` is the current supported opt-out variable for
native clipboard mutation.

## Optional integration model

Talk should expose a local capability boundary so peers can discover and call it
when installed:

```text
Hook -> Talk: request one voice session and receive text/session evidence.
Talk -> Hook: send recognized text or request current visual context.
Talk -> Loom: upgrade a voice command into an agent/workflow request.
Talk -> Gateway: transcribe/process text through configured providers.
```

Loom should mediate semantic, multi-step, memory-backed, or approval-sensitive
flows. It should not be required for simple Hook-to-Talk dictation or Talk
standalone insertion.

See `../docs/architecture/neuro-local-app-structure.md` for the canonical
cross-project structure.
