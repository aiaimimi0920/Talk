# Talk Sentence-Level Correction Patches Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add runtime support for stable local ASR sentence chunks that can be sent as text-only correction requests before/without waiting for a full cloud audio transcription.

**Architecture:** `talk-runtime` owns chunk readiness and correction request emission because it already sees streaming ASR events and segmenter decisions. `talk-desktop` keeps the existing Typeless/OpenLess interaction, final insert, safe patch, and editable popup fallback; this task adds a runtime contract that tells desktop/cloud correction code which local segment is stable enough to correct and what bounded prior context may be sent.

**Tech Stack:** Rust `Talk` workspace, `talk-runtime` speculative state machine, existing `SegmenterConfig`, existing `talk-client` streaming ASR events, existing desktop safe patch/fallback helpers.

---

## File structure

- Modify: `Talk/crates/talk-runtime/src/speculative.rs`
  - Add `SpeculativeCorrectionRequest`.
  - Add a `CorrectionRequested` runtime event.
  - Add `accept_asr_event_with_segmentation` for multi-event sentence-level handling.
  - Track committed segment order and correction-request de-duplication.
- Modify: `Talk/crates/talk-runtime/src/segmenter.rs`
  - Add bounded correction context character count to `SegmenterConfig`.
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
  - Re-export the correction request type.
- Modify: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`
  - Add TDD coverage for correction-ready segments, duplicate correction suppression, duplicate local commit suppression, and bounded context.
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
  - Add a pure `desktop_speculative_correction_job_model` helper that maps runtime `CorrectionRequested` events to either patchable insert anchors or popup-only correction jobs.
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
  - Add TDD coverage for patchable correction jobs, popup-only correction jobs, and disabled/non-correction cases.
- Modify: `Talk/tools/talk-local-asr-sherpa/src/main.rs`
  - Keep partial/final `segment_id` stable within the same local ASR utterance so desktop anchors can match later correction results.
- Modify: `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`
  - Document the local ASR -> stable segment -> text correction -> safe patch/fallback flow.
- Modify: `Talk/docs/superpowers/plans/2026-07-09-talk-local-first-asr-roadmap.md`
  - Mark Task 5 runtime-level correction request work complete after verification.

## Task 1: Runtime correction readiness events

- [x] **Step 1: Write failing tests**

Add tests that call `SpeculativeRuntimeState::accept_asr_event_with_segmentation` and expect:

```rust
SpeculativeRuntimeEvent::CorrectionRequested {
    segment_id,
    local_text,
    context_before,
}
```

The tests must prove:

1. A stable final local sentence emits `LocalSegmentCommitted` and `CorrectionRequested`.
2. A stable partial sentence with punctuation plus enough trailing silence emits a correction request.
3. The same segment never emits duplicate correction requests.
4. Prior committed text is included only as bounded context.

- [x] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test speculative_runtime_contract speculative_runtime_requests_text_correction_when_segment_is_stable -- --nocapture
```

Expected before implementation: compile failure or missing method/variant failure.

- [x] **Step 3: Implement minimal runtime support**

Implement:

```rust
pub struct SpeculativeCorrectionRequest {
    pub segment_id: String,
    pub local_text: String,
    pub context_before: String,
}

pub fn accept_asr_event_with_segmentation(
    &mut self,
    event: StreamingAsrEvent,
    trailing_silence_ms: u64,
    config: &SegmenterConfig,
) -> Result<Vec<SpeculativeRuntimeEvent>, TalkError>
```

Do not change the existing single-event `accept_asr_event` contract.

- [x] **Step 4: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test speculative_runtime_contract -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test segmenter_contract -- --nocapture
```

Expected after implementation: all runtime contract tests pass.

## Task 2: Desktop correction job model and daemon segment identity

- [x] **Step 1: Write failing desktop job model tests**

Add `desktop_contract` tests proving:

1. A runtime `CorrectionRequested` event plus an insert target becomes a patchable `SpeculativeInsertAnchor`.
2. A runtime `CorrectionRequested` event without an insert target becomes a popup-only correction job.
3. Disabled cloud correction or non-correction events do not create jobs.

- [x] **Step 2: Verify desktop RED**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract desktop_correction_job_model_maps_ready_segment_to_patchable_insert_anchor -- --nocapture
```

Expected before implementation: missing `desktop_speculative_correction_job_model` and model types.

- [x] **Step 3: Implement desktop job model**

Implement:

```rust
pub enum DesktopSpeculativeCorrectionOutputTarget {
    PatchInsertedText(SpeculativeInsertAnchor),
    CopyPopupOnly,
}

pub struct DesktopSpeculativeCorrectionJobModel {
    pub segment_id: String,
    pub local_text: String,
    pub context_before: String,
    pub output_target: DesktopSpeculativeCorrectionOutputTarget,
}
```

The helper must not call Win32 APIs. It only prepares a deterministic model for later UI-loop dispatch.

- [x] **Step 4: Verify desktop GREEN**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- --nocapture
```

Expected after implementation: all desktop contract tests pass.

- [x] **Step 5: Write failing daemon stable segment id test**

Extend the dry-run daemon websocket test so the first partial and final message share the same `segment_id`.

- [x] **Step 6: Implement stable daemon segment ids**

Use `dry-run-segment-1` for dry-run partial/final and `sherpa-segment-1` for sherpa-online partial/final within a single utterance.

- [x] **Step 7: Verify daemon GREEN**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa dry_run_daemon_emits_partial_after_first_audio_chunk -- --nocapture
```

Expected after implementation: focused daemon test passes.

## Task 3: Documentation and roadmap state

- [x] **Step 1: Update docs**

Explain that Talk sends local ASR text plus bounded prior text context to the text processor; raw audio is not sent in this correction stage.

- [x] **Step 2: Update roadmap**

Mark Task 5 complete for runtime-level detection/request support and desktop
active-recording dispatch. Desktop now inserts stable live chunks only when the
original editable target is still active, sends bounded text correction context,
tail-guards auto patches, suppresses duplicate stop-time full insertion, and
routes unsafe tails/corrections to the editable popup path.

- [x] **Step 3: Verify broader workspace**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
cargo check --manifest-path .\Talk\Cargo.toml -p talk-runtime --all-targets
cargo check --manifest-path .\Talk\Cargo.toml -p talk-desktop --all-targets
cargo check --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa --all-targets
git diff --check -- Talk
```

Expected: formatting/checks pass; `git diff --check` has no whitespace errors.

Actual verification in this batch:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test speculative_runtime_contract -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-runtime --test segmenter_contract -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract -- --nocapture
cargo check --manifest-path .\Talk\Cargo.toml -p talk-runtime --all-targets
cargo check --manifest-path .\Talk\Cargo.toml -p talk-desktop --all-targets
cargo check --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa --all-targets
git diff --check -- Talk
cargo test --manifest-path .\Talk\Cargo.toml --workspace
Invoke-Pester -Script .\Talk\scripts\tests\Install-TalkSherpaModel.Tests.ps1
Invoke-Pester -Script .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Result: all commands exited 0. `git diff --check` reported only Git
line-ending normalization warnings and no whitespace errors.

## Self-review

- Scope now includes runtime correction request readiness, desktop pure job
  modeling, stable local ASR segment identity, active-recording live chunk
  insertion, text-only correction dispatch for live-inserted chunks, tail-guarded
  patching, duplicate stop-time insertion prevention, and docs.
- No raw audio is introduced into the correction stage.
- Existing desktop focus-safety and popup fallback contracts stay unchanged.
