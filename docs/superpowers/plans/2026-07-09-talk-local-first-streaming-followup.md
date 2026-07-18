# Talk Local-First Streaming Follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Talk's local-first streaming ASR path visibly usable during recording instead of only returning text after stop.

**Architecture:** Keep Talk desktop engine-neutral by talking to a loopback streaming ASR service. The daemon emits `partial` events while audio is still being recorded; desktop keeps the Typeless-style listening HUD active, pumps PCM into the service, and renders the latest partial text without changing focus or insertion behavior.

**Tech Stack:** Rust workspace under `Talk`, `tokio-tungstenite` WebSocket daemon/client, `talk-runtime` live session bridge, `talk-desktop` Win32 HUD model/rendering, Cargo tests/checks.

---

## File structure

- Modify `Talk/tools/talk-local-asr-sherpa/src/main.rs`: expose a dry-run partial-text option and emit one protocol-compatible `partial` message after the first valid audio chunk.
- Modify `Talk/crates/talk-desktop/src/lib.rs`: add a listening HUD view-model helper that can carry the latest partial transcript while preserving `DesktopHudVisualState::Listening`.
- Modify `Talk/crates/talk-desktop/tests/desktop_contract.rs`: prove partial transcript text is attached to the listening HUD, trimmed, and does not alter listening metrics or meter bars.
- Modify `Talk/crates/talk-desktop/src/main.rs`: retain and render the latest streamed partial text during the recording timer pump.
- Modify `Talk/docs/superpowers/plans/2026-07-09-talk-local-streaming-asr-service.md`: cross-reference this follow-up plan as the next implementation phase.

## Task 1: Emit dry-run partials from the local ASR daemon

**Files:**
- Modify: `Talk/tools/talk-local-asr-sherpa/src/main.rs`

- [x] **Step 1: Write the failing daemon test**

Add a unit/integration-style Tokio test named `dry_run_daemon_emits_partial_after_first_audio_chunk` that opens the daemon over a loopback WebSocket, sends `start`, sends one `audio` message, expects a `partial` message with `segment_id = "dry-run-partial"` and text `你好`, then sends `stop` and expects the existing final text `你好。`.

- [x] **Step 2: Run the test and verify RED**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa dry_run_daemon_emits_partial_after_first_audio_chunk
```

Expected before implementation: compile failure because `DaemonConfig` has no `dry_run_partial_text` field.

- [x] **Step 3: Implement dry-run partial support**

Add `--dry-run-partial-text <TEXT>` to the CLI, add `dry_run_partial_text: Option<String>` to `DaemonConfig`, validate it with `validate_nonblank` when present, add `dry_run_partial_emitted: bool` to `StreamingSession`, and send this JSON once after the first valid audio chunk:

```json
{
  "type": "partial",
  "session_id": "<active session id>",
  "segment_id": "dry-run-partial",
  "text": "<dry-run partial text>"
}
```

- [x] **Step 4: Run the daemon test and verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa dry_run_daemon_emits_partial_after_first_audio_chunk
```

Expected after implementation: test exits 0 and the daemon still emits the final dry-run transcript on stop.

## Task 2: Keep streamed partial text visible on the listening HUD

**Files:**
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [x] **Step 1: Write the failing desktop HUD model test**

Add a test named `listening_hud_can_show_latest_streaming_partial_without_leaving_listening_state` that calls:

```rust
desktop_hud_view_model_for_listening_waveform_with_partial(
    [0.0, 0.1, 0.85, 0.25, 0.65, 0.15, 0.4, 0.95, 0.05],
    Some("  你好呀  "),
)
```

Assert:

```rust
assert_eq!(model.visual_state, DesktopHudVisualState::Listening);
assert_eq!(model.title, "Listening");
assert_eq!(model.detail.as_deref(), Some("你好呀"));
assert_eq!(model.progress_percent, None);
assert_eq!(
    model.meter.as_ref().expect("listening meter").bar_heights,
    [4, 5, 16, 8, 13, 6, 10, 17, 4]
);
assert_eq!(
    desktop_hud_metrics_for_view_model(&model),
    DesktopHudMetrics {
        width: 188,
        height: 52,
        bottom_margin: 130,
        corner_radius: 0,
    }
);
```

- [x] **Step 2: Run the test and verify RED**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract listening_hud_can_show_latest_streaming_partial_without_leaving_listening_state
```

Expected before implementation: compile failure because `desktop_hud_view_model_for_listening_waveform_with_partial` does not exist.

- [x] **Step 3: Implement the HUD model helper**

Add:

```rust
pub fn desktop_hud_view_model_for_listening_waveform_with_partial(
    waveform_bins: [f32; 9],
    partial_text: Option<&str>,
) -> DesktopHudViewModel
```

The function reuses `desktop_hud_audio_meter_model_for_waveform`, trims `partial_text`, stores nonblank text in `detail`, and keeps visual state, title, and metrics unchanged.

- [x] **Step 4: Wire desktop runtime pump events into the HUD**

In `refresh_recording_hud_level`, inspect `StreamingAsrEvent` values returned by `pump_available_audio`, store the latest non-final partial text in the overlay state, and rebuild the listening HUD with `desktop_hud_view_model_for_listening_waveform_with_partial(next_bins, latest_partial.as_deref())`. Clear the stored partial when a new HUD model is not listening.

- [x] **Step 5: Run the desktop HUD test and compile check**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract listening_hud_can_show_latest_streaming_partial_without_leaving_listening_state
cargo check --manifest-path .\Talk\Cargo.toml -p talk-desktop --all-targets
```

Expected: both commands exit 0.

## Task 3: Validate and publish a new Talk desktop release

**Files:**
- All modified files under `Talk/`.
- Output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk`.

- [x] **Step 1: Format**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
```

Expected: exit 0.

- [x] **Step 2: Run workspace tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
```

Expected: exit 0.

- [x] **Step 3: Run workspace compile check**

Run:

```powershell
cargo check --manifest-path .\Talk\Cargo.toml --workspace --all-targets
```

Expected: exit 0.

- [x] **Step 4: Check whitespace in Talk diff**

Run:

```powershell
git diff --check -- Talk
```

Expected: exit 0.

- [x] **Step 5: Build the GUI release exe**

Run:

```powershell
& .\Talk\scripts\Publish-TalkRelease.ps1 -VersionId 'desktop-shell-local-streaming-asr-v2' -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk'
```

Expected: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-local-streaming-asr-v2\talk-desktop.exe` exists, and the release root exposes the GUI `talk-desktop.exe` rather than a non-GUI primary binary.

## Self-review

- Spec coverage: Task 1 covers daemon partial protocol; Task 2 covers user-visible live partial feedback while preserving Typeless-style listening state; Task 3 covers validation and release output.
- Placeholder scan: no TBD/TODO placeholders are present.
- Type consistency: function names and paths match the current Talk Rust workspace naming conventions.
