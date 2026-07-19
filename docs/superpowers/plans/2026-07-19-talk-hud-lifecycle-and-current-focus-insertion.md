# Talk HUD Lifecycle and Current-Focus Insertion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove recording-text flicker, render local ASR text yellow and corrected text white, and insert only corrected text into the editable control focused when correction finishes.

**Architecture:** Keep lifecycle and geometry decisions as pure models in `talk-desktop/src/lib.rs`, then let the Win32 shell in `talk-desktop/src/main.rs` apply those decisions. The recording timer will cache HUD geometry and invalidate only the waveform when text is unchanged. The final insertion hook will publish corrected text to the UI thread and select the current editable focus target instead of requiring identity with the recording-start target.

**Tech Stack:** Rust workspace, Win32/GDI desktop shell, UI Automation focus capture, Cargo contract tests, PowerShell/Pester release tests.

---

### Task 1: Add HUD geometry update contracts

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write the failing geometry tests**

Add tests that construct `DesktopHudGeometry` values and assert:

```rust
assert_eq!(
    desktop_hud_geometry_update_plan(Some(current), current),
    DesktopHudGeometryUpdatePlan {
        reposition: false,
        reshape: false,
    }
);

assert_eq!(
    desktop_hud_geometry_update_plan(Some(current), resized),
    DesktopHudGeometryUpdatePlan {
        reposition: true,
        reshape: true,
    }
);
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- hud_geometry_update
```

Expected: compilation fails because the geometry types and function do not exist.

- [ ] **Step 3: Implement the pure geometry model**

Add public value types:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopHudGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub corner_radius: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopHudGeometryUpdatePlan {
    pub reposition: bool,
    pub reshape: bool,
}
```

Implement `desktop_hud_geometry_update_plan(current, next)` so first application and size/radius changes reshape, position-only changes reposition, and identical geometry does nothing.

- [ ] **Step 4: Run the focused tests and verify GREEN**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- hud_geometry_update
```

Expected: all matching tests pass.

- [ ] **Step 5: Commit the geometry model**

```powershell
git add crates/talk-desktop/src/lib.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "test: define stable HUD geometry updates"
```

### Task 2: Apply geometry caching and narrow recording invalidation

**Files:**
- Modify: `crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Add geometry state to the overlay**

Import the new geometry model and add:

```rust
hud_geometry: Option<DesktopHudGeometry>,
```

to `OverlayUiState`.

- [ ] **Step 2: Apply geometry only when required**

In `show_hud_model` and `refresh_recording_hud_level`, compute the next geometry, call `desktop_hud_geometry_update_plan`, and execute `SetWindowPos` or `apply_rounded_window_region` only when the plan requests them. Store the applied geometry after the update and clear it when the HUD is hidden.

- [ ] **Step 3: Narrow recording invalidation**

Track whether the partial text changed. If text and geometry are unchanged, invalidate only `desktop_listening_hud_waveform_rect(...)`. Otherwise invalidate the full client area. Pass `0` as the erase flag in both cases.

- [ ] **Step 4: Verify the desktop crate**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- hud_geometry_update
cargo check -p talk-desktop --all-targets
```

Expected: both commands pass without warnings or dead-code errors.

- [ ] **Step 5: Commit the runtime application**

```powershell
git add crates/talk-desktop/src/main.rs
git commit -m "fix: stabilize recording HUD repaint"
```

### Task 3: Connect yellow and white lifecycle colors to the real HUD

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write failing corrected-HUD tests**

Add tests proving:

```rust
let provisional = desktop_text_lifecycle_view_model(
    DesktopTextLifecycleState::PreRecognized,
    "你好",
);
assert_eq!(provisional.text_rgb, Some([245, 190, 72]));
assert!(!provisional.insertable_to_target);

let corrected = desktop_hud_view_model_for_corrected_text("你好！");
assert_eq!(corrected.detail.as_deref(), Some("你好！"));
assert_eq!(desktop_hud_detail_lifecycle(&corrected), Some(DesktopTextLifecycleState::Corrected));
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- corrected_hud
```

Expected: compilation fails because the corrected HUD helpers do not exist.

- [ ] **Step 3: Implement corrected HUD helpers**

Add `desktop_hud_view_model_for_corrected_text` and `desktop_hud_detail_lifecycle`. Listening detail maps to `PreRecognized`; corrected-result HUD detail maps to `Corrected`. Corrected detail uses an expanded HUD metric so wrapped result text has a stable area.

- [ ] **Step 4: Paint lifecycle colors**

In `paint_hud_window`:

- paint listening partial text with `rgb(245, 190, 72)`;
- paint corrected detail text with `rgb(245, 247, 250)`;
- add wrapped detail drawing to the non-listening HUD branch;
- keep waveform, title, controls, and thinking UI unchanged.

- [ ] **Step 5: Verify lifecycle tests and desktop compilation**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- text_lifecycle
cargo test -p talk-desktop --test desktop_contract -- corrected_hud
cargo check -p talk-desktop --all-targets
```

Expected: all commands pass.

- [ ] **Step 6: Commit lifecycle rendering**

```powershell
git add crates/talk-desktop/src/lib.rs crates/talk-desktop/src/main.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "fix: render ASR text lifecycle in HUD"
```

### Task 4: Insert corrected output into the current editable focus

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write failing current-focus tests**

Add a test where origin and current targets are editable but have different window and focus handles. Assert that `desktop_output_plan` returns `HonorConfiguredOutput` with the current target. Preserve tests proving a current button, pane, or non-focusable document returns `ShowCopyPopupOnly`.

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- current_editable_focus
```

Expected: the different-window editable test fails with `ShowCopyPopupOnly`.

- [ ] **Step 3: Change the final target selector**

Update the clipboard-paste target helper to return the current target when `desktop_insert_target_looks_editable(Some(current_target))` is true and the target is not explicitly non-editable. Do not fall back to an origin target when the current focus is unavailable or non-editable.

- [ ] **Step 4: Add corrected-output UI handoff**

Add `PendingCorrectedHud { generation, text }` to `SharedState`. In the final `before_insert` hook, store `RuntimeInsertContext::output_text`, synchronously notify the UI window, and render the corrected HUD before returning `UseConfiguredOutput`. On successful worker completion, show the corrected HUD briefly again; on failure, discard it and show the failure HUD.

- [ ] **Step 5: Verify current-focus and mode gates**

Run:

```powershell
cargo test -p talk-desktop --test desktop_contract -- current_editable_focus
cargo test -p talk-desktop --test desktop_contract -- runtime_insert_directive
cargo test -p talk-desktop --test desktop_contract -- desktop_output_plan
```

Expected: current editable focus is selected, non-editable focus falls back, and only corrected mode-eligible results request configured output.

- [ ] **Step 6: Commit current-focus insertion**

```powershell
git add crates/talk-desktop/src/lib.rs crates/talk-desktop/src/main.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "fix: insert corrected text into current focus"
```

### Task 5: Full verification and release

**Files:**
- Verify: all workspace crates
- Verify: `scripts/tests/Publish-TalkRelease.Tests.ps1`
- Generate: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r5`

- [ ] **Step 1: Format and compile**

Run:

```powershell
cargo fmt --all
cargo fmt --all -- --check
cargo check --workspace --all-targets
```

Expected: all commands exit 0.

- [ ] **Step 2: Run the complete automated suite**

Run:

```powershell
cargo test --workspace
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected: all Rust and Pester tests pass.

- [ ] **Step 3: Stop the old packaged process**

Stop only the running `Talk.exe` whose path is under `talk-single-exe-20260719-r4`. Do not stop unrelated executables.

- [ ] **Step 4: Publish the new release**

Run:

```powershell
.\scripts\Publish-TalkRelease.ps1 `
  -VersionId talk-single-exe-20260719-r5 `
  -ReleaseRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

Expected: the release root contains only `Talk.exe` and `talk.toml`.

- [ ] **Step 5: Launch and inspect the packaged application**

Start `Talk.exe` with `%LOCALAPPDATA%\Talk` as the working directory. Confirm the process stays running and no startup failure is written to the latest desktop log.

- [ ] **Step 6: Manual microphone acceptance**

With an editable input focused, record speech and verify yellow provisional text, stable non-flickering rendering, white corrected text, and automatic paste into the current editable focus. Repeat once with a different editable focus selected before correction finishes and once with a non-editable focus to verify popup fallback.

- [ ] **Step 7: Commit release-ready changes**

```powershell
git status --short
git add docs/superpowers/plans/2026-07-19-talk-hud-lifecycle-and-current-focus-insertion.md
git commit -m "docs: complete HUD lifecycle insertion plan"
```
