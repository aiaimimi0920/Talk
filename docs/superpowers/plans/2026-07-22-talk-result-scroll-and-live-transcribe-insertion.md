# Talk Result Scroll And Live Transcribe Insertion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add readable post-session result scrolling and make corrected white text insert safely and incrementally for Transcribe and confidently routed Smart sessions.

**Architecture:** Retain native Win32 EDIT scrolling, add a narrowly constrained weak-origin upgrade policy, and separate faithful live correction from mode-aware target insertion. Smart sessions keep a correction backlog until long-form evidence locks the live route to Transcribe, then flush that backlog in order without duplicating stop insertion.

**Tech Stack:** Rust, Win32 `windows-sys`, UI Automation target diagnostics, Tokio correction workers, Cargo tests, PowerShell/Pester release validation.

---

### Task 1: Native result-popup vertical scrollbar

**Files:**
- Modify: `crates/talk-desktop/src/main.rs`
- Test: `crates/talk-desktop/src/main.rs`

- [ ] Add a private binary test named `copy_popup_edit_style_exposes_native_vertical_scrolling` that calls `copy_popup_edit_control_style()` and requires `ES_MULTILINE`, `ES_AUTOVSCROLL`, and `WS_VSCROLL` bits.
- [ ] Run `cargo test -p talk-desktop --bin talk-desktop copy_popup_edit_style_exposes_native_vertical_scrolling` and verify RED because the style helper does not exist.
- [ ] Import `WS_VSCROLL`, extract the existing EDIT style into `copy_popup_edit_control_style() -> u32`, add `WS_VSCROLL`, and pass the helper result to `CreateWindowExW`.
- [ ] Re-run the focused private test and verify GREEN.

### Task 2: Upgrade only same-window window-only origin snapshots

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add a contract test proving a window-only pending Chrome snapshot is replaced by a same-window release snapshot with editable UIA identity.
- [ ] Change the existing recording-enrichment regression test so the same window-only origin is upgraded to the richer editable candidate.
- [ ] Add negative tests for a different top-level window, an existing focus identity, an existing different UIA Runtime ID, and a non-editable candidate.
- [ ] Run the focused origin-resolution tests and verify RED against the current unconditional pending preference and identity requirement.
- [ ] Add private helpers that identify control identity and validate the constrained weak-origin upgrade rule.
- [ ] Apply the rule in `resolve_hotkey_origin_insert_target` and `resolve_hotkey_recording_origin_enrichment` while retaining current strong-identity rejection behavior.
- [ ] Make `begin_recording` record the source of the target actually selected instead of assuming any pending snapshot won.
- [ ] Re-resolve the active origin against the stored release-time snapshot before the stop worker clones its target context.
- [ ] Re-run the focused origin-resolution tests and the complete desktop contract suite.

### Task 3: Mode-aware corrected-segment insertion and Smart backlog flush

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`
- Test: `crates/talk-desktop/src/main.rs`

- [ ] Add contract tests for a new target-apply policy: explicit Transcribe is enabled, explicit Command/Generate/Document are disabled, Smart without a route lock is disabled, and Smart locked to Transcribe is enabled.
- [ ] Add contract tests for ordered backlog candidates: corrected unanchored segments are selected, while yellow/unresolved and already anchored segments are excluded.
- [ ] Run the focused policy tests and verify RED because the policies do not exist.
- [ ] Add public pure policy helpers and a backlog candidate model in `talk-desktop/src/lib.rs`.
- [ ] Add `live_smart_routed_mode` to `ActiveRecording`, plus requested mode and current live route information to live dispatch/correction jobs.
- [ ] After each transcript-changing streaming batch, analyze the current corrected-plus-pending aggregate with `SmartRouteEvidence`; lock only a Smart Transcribe result with long-form evidence.
- [ ] Make correction jobs consult the current live Smart route at apply time so jobs queued before the lock can insert after it.
- [ ] When insertion remains gated, record the corrected result and post a streaming HUD refresh so the preview turns white.
- [ ] When Smart first locks to Transcribe, flush corrected unanchored tracker segments in order through the existing safe insertion helper and mirror successful anchors into active recording state.
- [ ] Keep stop-time dispatch target application disabled; let the complete final route and existing stop reconciliation handle short ambiguous Smart sessions exactly once.
- [ ] Run focused mode/backlog tests, desktop private tests, and the full desktop contract suite.

### Task 4: Full verification and isolated r10 product release

**Files:**
- Modify only if required by a failing release contract: `scripts/Publish-TalkRelease.ps1`
- Create: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260722-r10\Talk.exe`
- Create: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260722-r10\talk.toml`

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run `cargo check --workspace --all-targets`.
- [ ] Run `cargo test --workspace --no-fail-fast`.
- [ ] Run the repository-compatible Pester 3.4 suite over `scripts/tests` and require zero failures.
- [ ] Publish `talk-single-exe-20260722-r10` with `-ProductProfile -SkipSmoke` without stopping any existing Talk/ASR process.
- [ ] Validate that the r10 directory contains exactly `Talk.exe` and `talk.toml`, validate the embedded payload, and record executable/runtime SHA-256 values.
- [ ] Run isolated product smoke cases for a long Smart Transcribe session, a short Smart Command session, faithful-output fallback, and insert-target diagnostics on a unique configuration/log root.
- [ ] Verify no r10 test process remains and the pre-existing local ASR process is still running.

No commits or pushes are part of this plan unless the user explicitly requests them.
