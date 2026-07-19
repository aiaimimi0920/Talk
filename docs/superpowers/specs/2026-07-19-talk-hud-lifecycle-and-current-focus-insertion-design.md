# Talk HUD Lifecycle and Current-Focus Insertion Design

## Goal

Make the packaged Talk desktop flow match the existing product contract:

1. local streaming ASR text appears in yellow while recognition is still provisional;
2. provider-corrected text appears in white;
3. only the white corrected text is eligible for automatic insertion;
4. automatic insertion targets the editable control that has focus when correction finishes;
5. the recording HUD no longer makes its text visibly flicker while the waveform updates.

All implementation and tests remain inside the independent `Talk` repository. Release output remains under `Neuro/release/Talk` and keeps the single-executable plus configuration-file product shape.

## Current Failure

The lifecycle model already maps pre-recognized text to `[245, 190, 72]` and corrected text to `[245, 247, 250]`, but the real listening HUD ignores that model and paints partial text with the generic muted terminal color.

The recording timer runs every 48 ms. On every tick it updates the waveform, repositions and resizes the HUD with `SetWindowPos`, reapplies the window region, and invalidates the entire client area. Even when geometry and recognized text are unchanged, the text and background are redrawn with direct GDI painting. This produces visible text flicker.

The final output planner currently accepts clipboard insertion only when the current focus target is the same control captured when recording started. A different foreground window or control therefore produces `ShowCopyPopupOnly`, which the runtime converts to `DryRunOnly`. Real session diagnostics confirm this is why otherwise successful recognition and correction sessions do not insert.

## Chosen Behavior

### Text lifecycle

- `AudioWave`: waveform only, no text, not insertable.
- `PreRecognized`: local streaming ASR text, yellow, not insertable.
- `Corrected`: provider-processed final text, white, insertable according to the active voice mode.

Talk must not insert local provisional text. The existing lifecycle gate remains the authority: only `DesktopTextLifecycleState::Corrected` may produce `UseConfiguredOutput`.

### Focus target

At the final insertion boundary, Talk captures the current foreground focus again. For clipboard-paste output:

- if the current target is demonstrably editable, Talk inserts into that current target even when it differs from the recording-start target;
- if the current target is missing, explicitly non-editable, or ambiguous, Talk does not paste and shows the copy popup;
- command mode remains GUI-only;
- other mode-specific output policy remains unchanged.

The recording-start target remains in diagnostics so target movement is still observable. It is no longer an insertion veto for the final corrected result.

### Corrected HUD

When the runtime reaches the final insertion hook, it publishes the corrected output to the UI thread before returning `UseConfiguredOutput`. The HUD paints that output in white. A successful worker completion shows the same corrected result briefly after insertion. Failed sessions discard the corrected-success presentation and continue to show the failure HUD.

### Flicker prevention

The HUD stores its last applied geometry. A refresh computes a geometry update plan:

- identical geometry: do not call `SetWindowPos` and do not rebuild the window region;
- position-only change: reposition without rebuilding the region;
- size or corner-radius change: resize and rebuild the region once.

During recording, when only waveform values changed, Talk invalidates only the waveform rectangle. It invalidates the whole HUD only when partial text or geometry changed. Invalidations do not request background erasure because the HUD paint path covers its own background and already suppresses `WM_ERASEBKGND`.

## Components

### `crates/talk-desktop/src/lib.rs`

Add pure, testable models for HUD geometry update decisions and corrected-result HUD construction. Change the final clipboard output planner to select the current editable target while retaining the non-editable fallback.

### `crates/talk-desktop/src/main.rs`

Cache applied HUD geometry, narrow waveform-only invalidation, paint provisional text yellow, render corrected detail text white, and bridge corrected runtime output to the UI thread. Keep all Win32 focus capture and diagnostic persistence in the existing desktop boundary.

### `crates/talk-desktop/tests/desktop_contract.rs`

Cover:

- unchanged HUD geometry produces no reposition or region update;
- size/radius changes request the correct updates;
- listening partial text is the yellow, non-insertable lifecycle;
- corrected HUD text is white and insertable;
- a current editable target in a different window is selected for final insertion;
- a current non-editable target still falls back to the copy popup.

## Error Handling

No editable current target is not treated as a runtime failure. Talk completes recognition and correction, persists diagnostics, and presents the corrected result in the copy popup. Provider, ASR, clipboard, or insertion errors retain their existing failed-session behavior.

## Verification

Automated verification:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Release and manual verification:

1. publish a new single-executable release under `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk`;
2. run `Talk.exe` from a writable working directory;
3. focus an editable input, trigger recording, and speak;
4. confirm partial text is yellow and stable while the waveform moves;
5. move focus to another editable input before correction completes;
6. confirm the corrected text appears white and is pasted into the currently focused input;
7. repeat with focus on a non-editable surface and confirm Talk shows the copy popup instead of pasting.
