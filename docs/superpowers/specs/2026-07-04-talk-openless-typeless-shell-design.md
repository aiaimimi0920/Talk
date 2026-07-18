# Talk OpenLess Typeless Shell Design

## Goal

Add a Windows-first desktop interaction shell for Talk so it behaves like an
OpenLess / Typeless style local dictation app instead of only a CLI daemon. The
first version must stay inside the `Talk/` workspace, keep `talk-daemon` as the
existing smoke and local capability surface, and add a background-resident
experience with:

- global hotkey activation;
- push-to-talk or toggle trigger handling;
- tray-based background lifetime;
- short-lived status HUD feedback;
- reuse of the existing Talk transcription / processing / insertion pipeline.

## Product model

The target interaction is:

```text
Talk starts in background
  -> user presses global shortcut
  -> Talk shows immediate HUD state
  -> Talk records / stops according to trigger mode
  -> Talk transcribes and optionally processes text
  -> Talk inserts into the foreground target
  -> Talk shows success / failure HUD
  -> Talk stays resident in tray for the next activation
```

This is deliberately not a chat window, not a large desktop dashboard, and not a
full settings UI. It is the minimum shell that makes Talk feel like a real
standalone local dictation app.

## Constraints

1. Code changes should stay in `Talk/` except for release artifacts written to
   `release/Talk/`.
2. Existing `talk-daemon check`, `once`, and `serve` behavior must continue to
   work.
3. The desktop shell is Windows-first; unsupported desktop-shell behavior on
   other platforms must fail explicitly instead of silently pretending to work.
4. The shell must reuse the existing Talk voice pipeline instead of spawning a
   second independent implementation.
5. The first version should avoid introducing a heavyweight GUI framework when
   Win32 APIs can cover the tray, hotkey, and HUD needs directly.

## Architecture

### 1. New shared runtime crate

Add a new crate `crates/talk-runtime` that extracts reusable runtime logic out
of `talk-daemon`:

- effective config loading, including Loom-managed config fallback behavior;
- trigger execution through `talk-hotkey`;
- audio capture through `talk-audio`;
- provider and text-processing calls through `talk-client`;
- insertion through `talk-insert`;
- session log persistence and failure persistence;
- a status callback / observer hook so desktop UX can react to session phases.

This crate becomes the single owner of the end-to-end voice session pipeline.
`talk-daemon` and the new desktop shell will both call into it.

### 2. Keep `talk-daemon` as CLI + capability server

`talk-daemon` remains responsible for:

- CLI argument parsing;
- `check`, `once`, and `serve` commands;
- HTTP local capability protocol and request validation;
- output formatting for CLI users.

It stops owning the detailed voice-session execution logic directly and instead
delegates to `talk-runtime`.

### 3. New `talk-desktop` crate

Add a new binary crate `crates/talk-desktop` that owns Windows desktop-shell
interaction:

- single-process background runtime;
- hidden message window and Win32 message loop;
- global hotkey registration with `RegisterHotKey`;
- tray icon lifecycle with `Shell_NotifyIconW`;
- tray menu actions;
- transient HUD window for status feedback;
- invocation of `talk-runtime` voice sessions on a worker thread / Tokio runtime.

The desktop shell is not a replacement for `talk-daemon`; it is a second entry
point that wraps the same pipeline in an always-on UX.

## Runtime behavior

### Session lifecycle

The runtime crate exposes a session runner with explicit phase notifications.
The first version will use these runtime phases:

- `idle`
- `trigger_armed`
- `recording`
- `transcribing`
- `processing`
- `inserting`
- `completed`
- `failed`
- `cancelled`

The desktop shell uses these phases to drive the HUD text and tray tooltip.

### Trigger modes

The initial shell respects the existing config trigger modes:

- `toggle`: first hotkey press starts recording, second press stops it;
- `push_to_talk`: hotkey press starts recording, hotkey release stops it.

For the first implementation, the system-level hotkey handler maps raw Win32
messages into the existing `talk-hotkey` state machine instead of embedding
trigger rules inside the shell.

### Single active session rule

Only one voice session may run at a time.

If the user presses the hotkey while a session is already active:

- in `toggle` mode, the message is interpreted as stop/cancel according to the
  state machine;
- otherwise extra invocations are ignored and the HUD should show a brief
  “busy” style message instead of starting concurrent runs.

### Failure model

Failures must still persist a session log when the trigger sequence already
started, matching current `once` behavior. The desktop shell must surface the
failure through HUD text and tray state but must not swallow the underlying log
evidence.

## Desktop shell components

### Hidden message window

The shell creates a hidden Win32 window class that owns:

- hotkey registration;
- tray notification callbacks;
- timer-driven HUD hide events;
- shutdown coordination.

This keeps tray, hotkey, and HUD lifetimes tied to the same message loop
without introducing a heavyweight app framework.

### Tray icon

The tray menu for the first version should stay small:

- `Start dictation` when idle;
- `Stop recording` when actively recording;
- `Open Talk logs folder`;
- `Exit Talk`.

This menu gives minimal operational control without creating a settings surface.

### HUD

The HUD is a small topmost tool window with no taskbar presence. It only shows
short status text, for example:

- `Talk: listening`
- `Talk: transcribing`
- `Talk: polishing`
- `Talk: inserting`
- `Talk: done`
- `Talk: failed`

The first version does not need waveform visualization, microphone meters, or
interactive controls. It only needs readable state feedback that appears fast
and auto-hides.

### Config source

The desktop shell loads the same Talk config model and Loom-managed override
behavior as the CLI. That keeps hotkey, backend, logging, and output behavior
consistent across:

- `talk once`
- `talk serve`
- `talk-desktop`

## File layout

### New crates

- `Talk/crates/talk-runtime/`
- `Talk/crates/talk-desktop/`

### Existing crates to modify

- `Talk/Cargo.toml`
- `Talk/crates/talk-daemon/Cargo.toml`
- `Talk/crates/talk-daemon/src/main.rs`
- `Talk/crates/talk-daemon/src/loom_config.rs` if config-loading ownership moves
- `Talk/crates/talk-hotkey/src/lib.rs` if raw desktop input helpers are needed
- existing tests in `talk-daemon` that currently validate pipeline behavior

### New test surfaces

- `Talk/crates/talk-runtime/tests/runtime_contract.rs`
- `Talk/crates/talk-desktop/tests/desktop_contract.rs`

## Testing strategy

### Runtime crate

Use contract tests to prove:

- session phase callbacks happen in order;
- success paths still write audio + log artifacts;
- failure paths still persist failed session logs;
- Loom-managed config fallback still behaves exactly like the CLI version;
- insertion mode and provider mode selection are preserved.

### Desktop crate

Avoid trying to fully UI-automate the Windows tray / HUD in the first round.
Instead:

- unit-test hotkey parsing and state transitions;
- unit-test shell state reducers / menu state decisions;
- unit-test HUD text mapping from runtime phases;
- integration-test desktop startup helpers that do not require interactive user
  desktop access;
- keep Win32 wrappers narrow so most logic remains testable outside raw APIs.

### Manual smoke

Manual smoke for the first working build should verify:

1. `talk.exe check`, `once`, and `serve` still pass.
2. `talk-desktop.exe` starts without a main window.
3. Tray icon appears.
4. Hotkey begins a session and shows `listening`.
5. Session advances through transcribing / inserting states.
6. Success or failure HUD is shown and auto-hides.
7. Session JSON evidence is written under the configured logging directory.

## Non-goals for this round

- full settings window;
- onboarding flow;
- waveform or volume meter UI;
- real-time transcript preview;
- multi-session queueing;
- direct replacement of Hook voice UX;
- changing capability protocol semantics;
- introducing a heavyweight cross-platform GUI abstraction.

## Acceptance criteria

The first shell implementation is successful when:

1. The Talk workspace contains a dedicated desktop shell binary.
2. The shell stays resident in the background and exposes a tray icon.
3. A global hotkey can start and stop a session without opening a main window.
4. The shell shows short HUD status updates for active and terminal session
   states.
5. The shell reuses the same runtime pipeline as `talk-daemon`.
6. Existing CLI / capability tests continue to pass.
7. The built executables can be copied to a release folder under
   `release/Talk/`.
