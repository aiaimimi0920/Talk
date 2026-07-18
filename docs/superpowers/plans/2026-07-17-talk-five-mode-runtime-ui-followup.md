# Talk Five-Mode Runtime/UI Follow-up Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Talk's five-mode/text-lifecycle contracts into runtime behavior and desktop UI so only corrected white text reaches the target editor, while command/generate modes use the correct GUI/result routing.

**Architecture:** Keep the existing Rust crate boundaries. `talk-core` owns durable session data, `talk-runtime` owns transcript processing and insertion, and `talk-desktop` owns Windows focus safety, HUD/copy-popup presentation, and mode selection. The first runtime milestone is deterministic and testable: expose transcript/result fields from the runtime, decide insertion from `DesktopModeOutputPolicy`, prevent command-mode insertion, and represent single/dual mode panes in desktop models before drawing richer native UI.

**Tech Stack:** Rust workspace under `Talk`, `talk-core` session events, `talk-runtime` async voice-session runners, `talk-desktop` Win32 overlay/HUD code, existing Cargo and Pester test suites.

---

### Task 1: Add runtime-visible transcript/result reporting

**Files:**
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/tests/runtime_contract.rs`

- [ ] **Step 1: Write failing tests**

Add tests requiring a completed run report/session to expose both:
- transcript text from `VoiceEvent::TranscriptReady`;
- processed output text from `VoiceEvent::ProcessedTextReady`.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test runtime_contract -- report_exposes_transcript_and_processed_output
```

Expected: FAIL because current runtime callers can read output text but do not have an explicit mode-result envelope for desktop UI.

- [ ] **Step 2: Implement minimal read model**

Add a small runtime read model in `talk-runtime/src/lib.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVoiceTextResult {
    pub transcript: Option<String>,
    pub processed_output: Option<String>,
}

pub fn runtime_voice_text_result(report: &VoiceRunReport) -> RuntimeVoiceTextResult {
    RuntimeVoiceTextResult {
        transcript: report.session.transcript().map(str::to_string),
        processed_output: report.session.output_text().map(str::to_string),
    }
}
```

If `VoiceSession::transcript()` does not exist, add the accessor in `talk-core/src/lib.rs` next to the existing `output_text()` accessor.

- [ ] **Step 3: Verify**

Run the same targeted command. Expected: PASS.

### Task 2: Enforce mode output policy at desktop insertion boundary

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Write failing policy tests**

Add tests for a helper:

```rust
pub fn desktop_runtime_insert_directive_for_mode(
    mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
    output_strategy: DesktopOutputStrategy,
    lifecycle_state: DesktopTextLifecycleState,
) -> DesktopRuntimeInsertPlan
```

Expected behavior:
- `Command + Corrected + HonorConfiguredOutput` returns no target insertion and requires GUI result.
- `Generate + Corrected + HonorConfiguredOutput` allows target insertion.
- `Transcribe/Document + PreRecognized + HonorConfiguredOutput` blocks target insertion.
- `Transcribe/Document + Corrected + ShowCopyPopupOnly` blocks target insertion and requires copy-popup/GUI output.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- runtime_insert_directive
```

Expected: FAIL because helper does not exist.

- [ ] **Step 2: Implement helper**

Add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopRuntimeInsertPlan {
    UseConfiguredOutput,
    DryRunOnly,
}
```

Map command mode and non-corrected lifecycle states to `DryRunOnly`. Map corrected generate/transcribe/document with `HonorConfiguredOutput` to `UseConfiguredOutput`. Map `ShowCopyPopupOnly` to `DryRunOnly`.

- [ ] **Step 3: Wire desktop runtime closure**

In `talk-desktop/src/main.rs`, replace the last branch of `before_insert` with the helper. For final processed output, pass `DesktopTextLifecycleState::Corrected`. Preserve the existing focus/restore/paste-shortcut logic only when the helper returns `UseConfiguredOutput`.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- runtime_insert_directive
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop
```

Expected: PASS.

### Task 3: Stop inserting yellow local ASR committed segments directly

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Write failing tests**

Extend `live_streaming_local_segment_plan` tests to require committed local ASR segments to be `DeferToStop` unless an explicit corrected lifecycle state is provided. The visible yellow pre-recognition text remains HUD-only.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- live_streaming_local_segment
```

Expected: FAIL if current logic inserts local committed segments.

- [ ] **Step 2: Implement minimal change**

Add lifecycle-aware helper:

```rust
pub fn live_streaming_segment_plan_for_lifecycle(
    output_mode: OutputMode,
    event: &SpeculativeRuntimeEvent,
    origin_target: Option<&DesktopInsertTargetContext>,
    current_target: Option<&DesktopInsertTargetContext>,
    lifecycle_state: DesktopTextLifecycleState,
) -> DesktopLiveStreamingLocalSegmentPlan
```

The existing `live_streaming_local_segment_plan` should call it with `DesktopTextLifecycleState::PreRecognized`, so direct live insertion is disabled by default. Final corrected insertion still happens through the normal runtime final-output path.

- [ ] **Step 3: Verify**

Run the targeted desktop contract test. Expected: PASS.

### Task 4: Add mode-aware GUI result model for single/dual text panes

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Write failing model tests**

Add tests for:
- `Transcribe/Document` result model has one pane with corrected white text.
- `Generate/Command` result model has two panes: transcript pane and result pane.
- Pre-recognition pane uses yellow and is marked not insertable.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- mode_text_result_model
```

Expected: FAIL because mode-aware GUI result model does not exist.

- [ ] **Step 2: Implement desktop model**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeTextPane {
    pub label: String,
    pub text: String,
    pub lifecycle: DesktopTextLifecycleState,
    pub text_rgb: [u8; 3],
    pub insertable_to_target: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopModeTextResultModel {
    pub layout: DesktopModeTextPaneLayout,
    pub panes: Vec<DesktopModeTextPane>,
}
```

Implement `desktop_mode_text_result_model(mode, routed, transcript, result, result_state)`.

- [ ] **Step 3: Wire copy popup interim output**

Until the native popup draws two edit controls, use the model to compose copy-popup text:
- single pane: result text only;
- dual pane: `转录\n<transcript>\n\n结果\n<result>`.

Command mode always uses this GUI path and never target insertion.

- [ ] **Step 4: Verify**

Run desktop contract tests and `cargo test -p talk-desktop`.

### Task 5: Add whole-document recorrection runtime hook foundation

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Write failing tests**

Add a contract test for a session aggregate:
- stable corrected segments concatenate into `originally_inserted_text`;
- unchanged target returns `AutoApplyToTarget`;
- edited target returns `ShowInTalkGuiOnly`.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- document_recorrection_session
```

Expected: FAIL because there is no session aggregate helper.

- [ ] **Step 2: Implement aggregate helper**

Add:

```rust
pub fn desktop_document_recorrection_session_decision(
    inserted_segments: &[String],
    current_target_text: &str,
    target_still_safe: bool,
) -> DesktopDocumentRecorrectionDecision
```

Use `inserted_segments.concat()` as the original inserted text and delegate to `desktop_document_recorrection_decision`.

- [ ] **Step 3: Wire GUI-only fallback**

Store inserted corrected segments in the active recording state. If final aggregate correction cannot be safely auto-applied, place the optimized full text in the existing copy-popup path instead of overwriting the user-edited target.

- [ ] **Step 4: Verify**

Run targeted desktop contract tests and `cargo test -p talk-desktop`.

### Task 6: Final validation and release artifact

**Files:**
- Existing Talk workspace files only.

- [ ] **Step 1: Format check**

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all -- --check
```

- [ ] **Step 2: Workspace tests**

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
```

- [ ] **Step 3: Release publisher tests**

```powershell
Invoke-Pester -Path .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

- [ ] **Step 4: Build release**

Inspect publisher help, then publish to:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

- [ ] **Step 5: Verify executable shape**

Confirm:
- `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-desktop.exe` exists;
- unsupported root `talk.exe` is absent;
- helper `talk.exe` is only under `.internal` if needed by the release packager.
