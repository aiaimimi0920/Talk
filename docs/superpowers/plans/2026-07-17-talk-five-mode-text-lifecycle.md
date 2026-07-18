# Talk Five-Mode Text Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Talk's five user-facing modes and the text lifecycle/output policy contracts needed for low-latency local ASR plus final correction.

**Architecture:** Extend the existing Talk mode model from legacy dictate/polish/translate/command into transcribe/document/command/generate/smart while preserving backward-compatible aliases. Add desktop-level contracts that describe mode shortcuts, single-vs-dual text panes, yellow pre-recognition vs white corrected states, and mode-specific insertion behavior. Keep the first implementation focused on deterministic policy/model functions and release-visible defaults; runtime execution can then be wired against these contracts without reworking the UI again.

**Tech Stack:** Rust workspace under `Talk`, existing `talk-core` configuration types, `talk-desktop` UI/interaction model tests, PowerShell release publisher.

---

### Task 1: Extend mode vocabulary and shortcut configuration

**Files:**
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Modify: `Talk/crates/talk-core/tests/config_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add failing tests**

Add tests that require:
- `VoiceMode` to parse `transcribe`, `document`, `command`, `generate`, `smart`.
- Legacy aliases `dictate`, `polish`, and `translate` to remain accepted.
- `DesktopShortcutConfig` to expose one optional direct-entry shortcut per mode.
- `desktop_action_bindings` to create direct mode bindings whose `mode_override` matches the selected mode.

- [ ] **Step 2: Run targeted tests and confirm failure**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-core --test config_contract -- voice_mode
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- mode_shortcut
```

Expected: tests fail because the new mode aliases and mode-specific shortcut fields do not exist yet.

- [ ] **Step 3: Implement minimal mode and shortcut support**

Implement:
- `VoiceMode::{Transcribe, Document, Command, Generate, Smart}`.
- Custom serde alias handling for legacy names.
- `DesktopShortcutConfig` fields:
  - `transcribe_shortcut`
  - `document_shortcut`
  - `command_shortcut`
  - `generate_shortcut`
  - `smart_shortcut`
- `desktop_action_bindings` entries for all configured mode shortcuts.
- Duplicate shortcut validation across all shortcut fields.

- [ ] **Step 4: Re-run targeted tests**

Run the same two commands and confirm they pass.

### Task 2: Add text lifecycle and mode output policy contracts

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add failing tests**

Add tests that require:
- Text stage palette:
  - audio wave state has no insertable text.
  - pre-recognized state is yellow and not insertable.
  - corrected state is white and insertable.
- Mode pane layout:
  - transcribe/document use one text pane.
  - command/generate use two text panes.
  - smart inherits from the routed mode.
- Mode output policy:
  - transcribe/document insert corrected segments.
  - generate inserts only the generated result pane.
  - command never inserts to the target input box.

- [ ] **Step 2: Run desktop contract tests and confirm failure**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- text_lifecycle
```

Expected: tests fail because the lifecycle/policy model does not exist yet.

- [ ] **Step 3: Implement minimal policy/model functions**

Add small deterministic structs/enums/functions in `talk-desktop/src/lib.rs`, without changing runtime behavior yet:
- `DesktopTextLifecycleState`
- `DesktopTextLifecycleViewModel`
- `desktop_text_lifecycle_view_model`
- `DesktopModeTextPaneLayout`
- `desktop_mode_text_pane_layout`
- `DesktopModeOutputPolicy`
- `desktop_mode_output_policy`

- [ ] **Step 4: Re-run targeted tests**

Run the same desktop contract command and confirm it passes.

### Task 3: Add whole-document recorrection safety policy

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add failing tests**

Add tests for:
- Auto-apply whole-document correction only when the inserted text is unchanged and the target is still safe.
- Defer to Talk GUI when the user has manually edited the inserted text.

- [ ] **Step 2: Run targeted tests and confirm failure**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- document_recorrection
```

Expected: tests fail because this policy model does not exist yet.

- [ ] **Step 3: Implement policy function**

Add:
- `DesktopDocumentRecorrectionDecision`
- `desktop_document_recorrection_decision`

Use string equality against the originally inserted text as the first safe-contract version. Any mismatch means `ShowInTalkGuiOnly`.

- [ ] **Step 4: Re-run targeted tests**

Run the same targeted command and confirm it passes.

### Task 4: Release-visible defaults and regression checks

**Files:**
- Modify: `Talk/scripts/Publish-TalkRelease.ps1`
- Modify: `Talk/scripts/tests/Publish-TalkRelease.Tests.ps1`

- [ ] **Step 1: Add failing release tests**

Require generated release config to include the five optional mode shortcut keys and default `voice_mode = "smart"`.

- [ ] **Step 2: Run Pester release tests and confirm failure**

Run:

```powershell
Invoke-Pester -Path .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected: tests fail until release config defaults are updated.

- [ ] **Step 3: Update release config generation**

Update the release config template with mode shortcuts that do not conflict with the primary RightAlt toggle.

- [ ] **Step 4: Re-run release tests**

Run the same Pester command and confirm it passes.

### Task 5: Full validation and release build

**Files:**
- Existing Talk workspace files only.

- [ ] **Step 1: Format check**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all -- --check
```

- [ ] **Step 2: Workspace tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
```

- [ ] **Step 3: Publish release**

Run the repository's Talk release publisher to produce a new desktop executable under:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

- [ ] **Step 4: Verify release artifact**

Confirm the new release directory contains `talk-desktop.exe` and does not expose an unsupported root `talk.exe`.
