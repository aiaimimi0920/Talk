# Talk Five-Mode Active Improvement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Talk from a Typeless-style single dictation tool toward a five-mode voice input runtime where local ASR gives immediate yellow pre-recognition text, corrected white text is the only target-insertable text, and each user mode has explicit GUI/output behavior.

**Architecture:** Keep all changes inside `Talk`. `talk-core` owns durable configuration and session events, `talk-client` owns ASR/text-processing provider calls, `talk-runtime` owns voice-session sequencing, streaming/local ASR events, and final insertion hooks, and `talk-desktop` owns Windows focus safety, mode selection, HUD/copy-popup presentation, and desktop insertion policy. The immediate release milestone uses deterministic Rust/Pester contract tests before shipping `talk-desktop.exe` under `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk`.

**Tech Stack:** Rust workspace under `Talk`, Win32 desktop shell in `talk-desktop`, async runtime/session orchestration in `talk-runtime`, OpenAI-compatible text processing in `talk-client`, and PowerShell/Pester release smoke tests.

---

## Product Contracts

- Five user-facing modes:
  - `transcribe`: single text pane, corrected text auto-inserts when the original editor focus remains safe.
  - `document`: single text pane, corrected formal/document text auto-inserts under the same focus-safety gate.
  - `command`: dual panes, transcript plus command execution/result text, never auto-inserts into the target editor.
  - `generate`: dual panes, transcript prompt plus generated result, inserts only the corrected/generated result when the target remains safe.
  - `smart`: routing mode. It chooses one of the four concrete modes from transcript intent, then applies that concrete mode's GUI and insertion policy.
- Text lifecycle:
  - audio wave: raw microphone input, shown only as waveform/audio level.
  - pre-recognized: local ASR text, yellow, unstable, never inserted into the target editor.
  - corrected: cloud/model-corrected text, white, eligible for target insertion.
- Whole-document correction:
  - segment-level corrected text can be inserted incrementally.
  - a later whole-document correction may auto-apply only when the target text still matches what Talk inserted.
  - if the user has edited the target, whole-document correction is shown in Talk GUI only.

---

### Task 1: Freeze the current five-mode foundation and release smoke contracts

**Files:**
- Verify: `Talk/crates/talk-core/src/lib.rs`
- Verify: `Talk/crates/talk-desktop/src/lib.rs`
- Verify: `Talk/crates/talk-runtime/src/lib.rs`
- Verify: `Talk/scripts/Invoke-TalkDesktopReleaseSmoke.ps1`
- Verify: `Talk/scripts/Publish-TalkRelease.ps1`

- [x] **Step 1: Verify focused Rust contracts**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-core --test config_contract -- voice_mode
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- mode_text_result_model
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- runtime_insert_directive
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test runtime_contract -- report_exposes_transcript_and_processed_output
```

Expected: all commands exit 0. These tests prove mode parsing, GUI result models, insertion gating, and runtime transcript/result reporting are present.

- [x] **Step 2: Verify release smoke script contracts**

Run:

```powershell
Invoke-Pester -Path .\Talk\scripts\tests\Invoke-TalkDesktopReleaseSmoke.Tests.ps1
Invoke-Pester -Path .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected: all tests pass. These tests prove command mode remains GUI-only, copy-popup smoke uses transcribe mode for focus-switch behavior, and release shape exposes only the GUI desktop executable at the root.

- [x] **Step 3: Publish a release candidate**

Run:

```powershell
.\Talk\scripts\Publish-TalkRelease.ps1 `
  -VersionId talk-five-mode-runtime-20260718-r4 `
  -ReleaseRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

Expected: publisher exits 0 and writes a release directory.

- [x] **Step 4: Verify executable shape**

Run:

```powershell
$release = 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-five-mode-runtime-20260718-r4'
Test-Path "$release\talk-desktop.exe"
Test-Path "$release\talk.exe"
Test-Path "$release\.internal\talk.exe"
Get-ChildItem -LiteralPath $release
```

Expected:
- root `talk-desktop.exe` exists;
- root `talk.exe` does not exist;
- `.internal\talk.exe` exists only as an internal helper when required by packaging.

### Task 2: Add an explicit Smart routing read model

**Files:**
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/tests/runtime_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [x] **Step 1: Write the failing runtime test**

Add a test proving that smart mode with a generation-style transcript exposes `smart_routed_mode = Some(VoiceMode::Generate)` in the runtime read model, while plain dictation routes to `Transcribe`.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test runtime_contract -- smart_route
```

Expected: FAIL until the runtime read model exposes the routed concrete mode.

- [x] **Step 2: Implement deterministic route inference**

Add a small function in `talk-runtime/src/lib.rs`:

```rust
pub fn infer_smart_voice_mode(transcript: &str) -> VoiceMode
```

Use conservative keyword rules:
- command: launch/open/close/run/delete/copy/move/action verbs such as `打开`, `关闭`, `启动`, `执行`.
- generate: `生成`, `写一篇`, `创作`, `draft`, `write`.
- document: `公文`, `正式`, `润色`, `改写`, `polish`.
- default: `Transcribe`.

Wire `runtime_voice_text_result` to include `smart_routed_mode` when the session mode is smart and the transcript is available.

- [x] **Step 3: Use the route at the desktop insertion boundary**

In `talk-desktop/src/main.rs`, pass the smart routed mode into:
- `desktop_runtime_insert_directive_for_mode`;
- `desktop_mode_text_result_model`.

Expected behavior:
- smart-to-command never inserts;
- smart-to-generate inserts only the generated result;
- smart-to-transcribe/document follows the single-pane corrected-text insertion policy.

- [x] **Step 4: Verify**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test runtime_contract -- smart_route
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- runtime_insert_directive
```

Expected: both commands pass.

### Task 3: Replace interim dual-pane copy text with editable dual-pane popup state

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [x] **Step 1: Write the failing desktop model test**

Add a test for a copy-popup payload that preserves pane identity instead of flattening dual panes into one string.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- copy_popup_panes
```

Expected: FAIL until `DesktopCopyPopupModel` can carry one editable pane for single-pane modes and two panes for command/generate modes.

- [x] **Step 2: Extend the popup model without breaking existing callers**

Add:

```rust
pub struct DesktopCopyPopupPaneModel {
    pub label: String,
    pub text: String,
    pub editable: bool,
    pub copy_default: bool,
}
```

Add `panes: Vec<DesktopCopyPopupPaneModel>` to `DesktopCopyPopupModel`. Preserve `text` as the default copy payload for existing single-pane callers.

- [x] **Step 3: Draw native dual panes**

In `show_copy_popup`, create one multiline edit control per pane. For command mode, copying defaults to the result pane text. For generate mode, copying and target insertion default to the generated result pane text.

- [x] **Step 4: Verify**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- copy_popup_panes
Invoke-Pester -Path .\Talk\scripts\tests\Invoke-TalkDesktopReleaseSmoke.Tests.ps1
```

Expected: desktop contracts and copy-popup smoke tests pass.

### Task 4: Add whole-document correction auto-apply guard in runtime

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [x] **Step 1: Write the failing safety test**

Add a desktop contract test proving that a full-document correction is auto-applied only when the current target text still equals the concatenated inserted corrected segments.

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- document_recorrection_session
```

Expected: FAIL until the runtime caller records inserted stable segments and consults the session-level correction decision before writing back.

- [x] **Step 2: Record inserted corrected segments**

When Talk inserts corrected text into the target editor in transcribe/document mode, append the exact inserted string to the active session's inserted-segment list.

- [x] **Step 3: Gate full-document replacement**

Before applying any full-document optimization, compare the current target text against `inserted_segments.concat()`. If equal and the focus target is still safe, apply the replacement. Otherwise show the optimized text in Talk GUI only.

- [x] **Step 4: Verify**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- document_recorrection_session
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop
```

Expected: both commands pass.

### Task 5: Full release verification

**Files:**
- Existing Talk workspace files only.

- [x] **Step 1: Format check**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all -- --check
```

- [x] **Step 2: Workspace tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
```

- [x] **Step 3: Release smoke and publish**

Run:

```powershell
Invoke-Pester -Path .\Talk\scripts\tests\Invoke-TalkDesktopReleaseSmoke.Tests.ps1
Invoke-Pester -Path .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
.\Talk\scripts\Publish-TalkRelease.ps1 `
  -VersionId talk-five-mode-runtime-20260718-r4 `
  -ReleaseRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

Expected: all checks pass and the release directory contains a GUI desktop build ready for manual testing.
