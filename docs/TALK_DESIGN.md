# Talk Design

Talk is the standalone Rust-first voice input app for Neuro. It was formerly
called `HookLess`; that name now refers only to historical documentation,
superseded plans, and migration notes. Current packages, imports, runtime
paths, binaries, and environment variables use the `Talk` / `talk` naming.

Talk stays independent from `Hook/`. Hook may call Talk through a local
capability API or keep a thin compatibility bridge, but Talk owns the product
boundary for voice capture, transcription, insertion, and voice session
evidence.

## Architecture

- Core state and config are pure Rust and have no OS dependencies.
- Audio, hotkey, insertion, and provider clients are split into small crates so
  they can remain testable and be exposed through a local Talk API.
- The daemon is the current development harness, smoke surface, and local
  capability service via `serve`. It can still sit behind a future Talk desktop
  UI.
- Talk now also has a Windows-first desktop shell binary, `talk-desktop`, that
  stays resident in the background, registers the configured global hotkey,
  shows a small HUD for state changes, and drives the same Talk runtime pipeline
  used by the daemon.
- The daemon records local smoke evidence as runtime artifacts, not source: an
  audio artifact under the configured audio temp directory and one session JSON
  under the configured logging directory. The session JSON includes trigger
  mode/events, transcript/output, final status, insertion outcome, and failure
  reason when the audio, provider, processing, or insertion stage fails.
- Trigger execution is selected from configuration. `once` drives either
  toggle or push-to-talk through the hotkey state machine so smoke artifacts
  prove the session was started/stopped through the same tested trigger
  boundary that Talk uses in standalone mode and exposes to peers.
- The desktop shell turns that trigger model into a real Windows interaction
  surface. `toggle` starts recording on the first hotkey press and stops on the
  second; `push_to_talk` starts on press and stops on release or timeout.
  The current packaged desktop baseline uses `toggle + RightAlt` so Talk
  matches the core Typeless start/stop gesture out of the box.
  The desktop shell now also supports a small Typeless-style action set from
  one config: `RightAlt` for the primary dictation route,
  `RightAlt+/` for translate, and `RightAlt+Space` for ask / assistant mode.
  To support that correctly, the desktop shell now uses a side-aware low-level
  keyboard hook for shortcuts that cannot be represented faithfully through
  `RegisterHotKey`, while still keeping the simpler `RegisterHotKey` path for
  legacy side-agnostic chords.
- If the configured hotkey is malformed or cannot be registered because another
  app already owns it, Talk should stay alive in tray-only mode and surface the
  problem through the HUD / tray status instead of terminating. Tray actions can
  still start dictation manually and reload the Talk config after the user fixes
  it.
- If the Talk config source itself is unavailable or invalid, Talk should still
  start the tray shell and report a config-unavailable state. In that mode the
  shell must keep recovery affordances such as opening or reloading the config,
  but it should not start a dictation session until a valid config has been
  loaded.
- During recording, the desktop shell should expose a distinct cancel path in
  addition to stop. `Stop` means continue the pipeline with the captured audio;
  `Cancel` means discard the active recording, persist a cancelled session log,
  and return to idle without transcription or insertion.
- Recovery-oriented tray UI should include a short detail line explaining the
  current hotkey/config problem so users do not need to infer why Talk is not
  ready.
- The tray should also expose a lightweight status surface so users can inspect
  current state, active recovery details, config/log paths, and the last
  session outcome without opening a full settings window.
- Audio execution is selected from configuration. `silent` writes a readable
  PCM WAV for automated smoke without microphone mutation. `native_windows`
  uses the explicit Windows microphone path through CPAL, converts captured
  input into the configured PCM WAV artifact, and must fail loudly rather than
  silently falling back to silent WAV when native capture is unavailable or
  disabled.
- Provider execution is selected from configuration. `mock` uses an in-process
  transcript and no-op processor; `http` posts the audio artifact/context and
  transcript/mode/context to the configured endpoint through the HTTP client
  boundaries. `openai_compatible` now supports both:
  - standard multipart `/v1/audio/transcriptions` transcription; and
  - `chat/completions` audio-input transcription via
    `transcription_transport = "chat_completions_audio_input"`.
  This is the current recommended path family for real Typeless-style hotkey
  conversation. The repo includes both lower-level custom adapter examples
  (`examples/desktop-http-safe-config.toml`,
  `examples/desktop-http-live-config.toml`) and recommended OpenAI-compatible
  examples (`examples/desktop-openai-compatible-safe-config.toml`,
  `examples/desktop-openai-compatible-live-config.toml`) plus Qwen audio-input
  examples (`examples/once-qwen-audio-input-safe-config.toml`,
  `examples/desktop-qwen-audio-input-live-config.toml`) so Talk can be
  exercised as a real hotkey shell instead of only a mock config.
- Insertion is selected from configuration. `dry_run` records a successful
  dry-run insertion. `clipboard_paste` has a testable library strategy that
  captures the current clipboard, writes text, sends a paste shortcut, and can
  restore the previous clipboard through injected backends. The Talk daemon
  defaults `output.clipboard_backend` to `fallback`, which records an explicit
  fallback outcome without mutating the real foreground clipboard. Manual
  Windows smoke can explicitly set `clipboard_backend = "native_windows"` to
  use the Windows clipboard and send Ctrl+V to the foreground window.
- The packaged desktop release now includes a default `talk-desktop.toml`
  beside `talk-desktop.exe`, and the shell resolves that file automatically
  when `--config` is omitted. Repo development runs still fall back to
  `examples/dev-config.toml`, so the desktop shell keeps both release-side
  usability and local developer convenience.
- The packaged desktop release now also includes a lightweight
  `Start-TalkDesktop.ps1` launcher so operator usage does not need to rebuild
  the `--config` path, API key environment, or temporary hotkey override logic
  by hand each time. Repo-side operator probes can build on top of that
  launcher while still reusing the smoke harness for foreground target
  verification.

## Local-first speculative dictation direction

The default dictation path should not wait for cloud inference before the user
sees text. Local streaming ASR owns the latency budget. Cloud correction is
asynchronous, conservative by default, and applied through the same
target-identity safety rules used by desktop insertion.

The current desktop implementation keeps the stable provider-backed batch path
as the default, but it can now run the local-first path when explicitly enabled:
`speculative.enabled = true` plus `speculative.local_asr = "external_command"`
records the same WAV artifact, runs the external local ASR JSONL adapter,
inserts the local final transcript without waiting for provider text
processing, and then optionally starts asynchronous cloud correction through
`speculative.cloud_correction = "provider_text_processor"`. Safe corrections
patch the original target; unsafe or stale corrections become an editable copy
popup.

## MVP behavior

1. `check` validates config.
2. `once` runs an end-to-end voice session. With the default `silent` audio
   backend it avoids microphone mutation; with the default `fallback` clipboard
   backend it also avoids real clipboard mutation. It drives configured hotkey
   start/stop events, captures an audio artifact through `audio.backend`, feeds
   that artifact through the configured provider boundary, inserts according to
   `output.mode` and `output.clipboard_backend`, and persists the completed or
   failed session state as JSON.
3. Provider-backed desktop smoke now proves that the shell can start and stop
   through its hotkey activation path, call both the lower-level custom HTTP
   provider boundary and the recommended OpenAI-compatible provider boundary,
   and persist a completed session log. The OpenAI-compatible path also
   supports `voice_mode = "command"` so Talk can behave more like a
   hotkey-triggered assistant instead of only raw dictation. On July 5, 2026,
   Talk's CLI path also validated a real DashScope live chain using
   `transcription_transport = "chat_completions_audio_input"`,
   `transcription_model = "qwen3-asr-flash"`, and `chat_model = "qwen3.7-plus"`,
   producing transcript `What is the capital of France?` and final output
   `Paris`. On the same day, the desktop shell path also validated the same
   provider pair through a developer-only fixed-audio override, confirming that
   the desktop hotkey shell can drive a real provider-backed session without
   needing a fake local adapter. That desktop path now also has a foreground
   insertion proof: a real Windows target stayed in front while Talk completed
   the session, and the final provider-backed answer landed in that target via
   native clipboard paste. Making that reliable required two desktop-side
   behaviors that match the OpenLess / Typeless interaction model more closely:
   the HUD must show without activation, and clipboard restoration must wait
   briefly so the target app can consume the pasted text before the original
   clipboard is restored. The release-side verification tooling now also has a
   thin global-hotkey probe wrapper so this path can be exercised through a
   real system-level hotkey chord without depending on the heavy full smoke
   object output. A companion Qwen-specific probe now also combines the real
   provider path with the same real hotkey chord and foreground insertion
   target, so Talk has direct evidence for both fake-provider and live-provider
   versions of the desktop interaction loop. Future phases can replace the placeholder local provider endpoints with production
   Talk-compatible services while keeping Hook as a peer/consumer instead of merging Talk into Hook. The
   Talk-owned local capability API already exists through `serve`: loopback-only HTTP, default manifest
   `%APPDATA%\Neuro\capabilities\talk.json`, optional `--manifest-dir`, bearer
   auth, and `GET /v1/health`, `GET /v1/capabilities`, `POST /v1/invoke`.

## Integration policy

- Hook can call Talk for voice capture or insertion assistance.
- Talk can call Hook for current visual context or canvas operations when Hook
  is installed.
- Loom can mediate semantic, multi-step, memory-backed, or approval-sensitive
  voice workflows.
- Direct Hook-to-Talk calls remain valid for simple, low-latency dictation.

## Reference policy

OpenLess, Voxt, open-typeless, and SpeakMore are product references only. Talk
must not copy their code or project structure.
