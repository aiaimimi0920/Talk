# Talk Smart Routing and Information Preservation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep `RightAlt` genuinely Smart while preventing long-form speech, quoted commands, incomplete live clauses, and catastrophic provider rewrites from losing transcript information.

**Architecture:** Add a pure, bounded Smart route analysis that evaluates the complete stop-time transcript with leading-intent and long-form evidence precedence. Keep every speculative/live correction request on faithful `Transcribe`, resolve Smart once for the complete stop aggregate, and validate faithful provider output before it can replace the aggregate. Make stop aggregation consume the same ordered tracker plus HUD-pending segments and reconcile final ASR revisions by source segment.

**Tech Stack:** Rust workspace (`talk-runtime`, `talk-desktop`, `talk-client`), Tokio async tests, PowerShell product publisher/validator, isolated Windows release smoke.

---

## File Map

- Modify: `crates/talk-runtime/src/lib.rs` — expose route context/diagnostics, route complete transcripts, apply faithful-output fallback at the provider boundary, and persist non-sensitive diagnostics.
- Create: `crates/talk-runtime/src/voice_processing.rs` — Smart route analysis and bounded faithful-output validation; no transcript text is stored in diagnostics.
- Create: `crates/talk-runtime/tests/smart_route_contract.rs` — isolated Smart boundary/quoted-command and route-evidence tests.
- Create: `crates/talk-runtime/tests/preservation_contract.rs` — isolated faithful-output and single-provider-request tests.
- Modify: `crates/talk-runtime/tests/runtime_contract.rs` — retain existing compatibility assertions and add only route-context propagation cases that need its helpers.
- Modify: `crates/talk-desktop/src/lib.rs` — canonical HUD-visible segment view, pending-aware stop aggregate, source-aware non-prefix revision merge, and a pure live-correction mode helper.
- Modify: `crates/talk-desktop/src/main.rs` — pass HUD pending segments into stop assembly, pass committed segment count into Smart route context, force live jobs to `Transcribe`, and pass the already-resolved concrete mode to the final whole-document job.
- Modify: `crates/talk-desktop/tests/desktop_contract.rs` — RED/GREEN tests for live mode, HUD/stop equivalence, pending segments, and non-prefix source revision.
- Modify: `crates/talk-desktop/src/main.rs` test module — prove private live/final job construction uses Transcribe and resolved concrete modes respectively.
- Create during verification: `release/Talk/talk-single-exe-20260722-r9/` — isolated product output only; do not touch or stop the existing r8 instance.

## Task 1: Lock the Smart-routing regression contract (RED)

**Files:**
- Create: `crates/talk-runtime/tests/smart_route_contract.rs`

- [ ] **Step 1: Add focused failing router tests.** Add tests beside `smart_route_infers_concrete_mode_from_transcript` with these exact cases:

```rust
#[test]
fn smart_route_keeps_quoted_or_descriptive_commands_as_transcription() {
    assert_eq!(infer_smart_voice_mode("他说打开记事本，然后继续演示。"), VoiceMode::Transcribe);
    assert_eq!(infer_smart_voice_mode("为什么打不开这个频道？"), VoiceMode::Transcribe);
    assert_eq!(infer_smart_voice_mode("如果你打开应用，就会看到这个对话框。"), VoiceMode::Transcribe);
}

#[test]
fn smart_route_keeps_long_body_as_transcription_when_keyword_is_not_leading_intent() {
    let body = "这是一次较长的会议记录。主持人先介绍项目背景，随后他说打开记事本查看配置，然后继续说明风险、时间表和后续安排。参与者逐一反馈，最后确认下周再复盘。".repeat(3);
    assert!(body.chars().filter(|character| !character.is_whitespace()).count() >= 160);
    assert_eq!(infer_smart_voice_mode(&body), VoiceMode::Transcribe);
}

#[test]
fn smart_route_allows_explicit_leading_meta_instruction_to_transform_long_body() {
    let body = "请总结下面的内容：".to_string()
        + &"这是一次较长的会议记录。主持人说明项目背景、风险、时间表和后续安排。参与者确认下周复盘。".repeat(4);
    assert_eq!(infer_smart_voice_mode(&body), VoiceMode::Document);
}

#[test]
fn smart_route_respects_short_direct_command_and_long_form_boundaries() {
    assert_eq!(infer_smart_voice_mode("打开记事本"), VoiceMode::Command);
    let long_command_like = "打开记事本，然后".to_string()
        + &"我继续描述今天的会议背景、参与者反馈、风险清单、时间表以及下周的复盘安排。".repeat(3);
    assert!(long_command_like.chars().filter(|character| !character.is_whitespace()).count() > 80);
    assert_eq!(infer_smart_voice_mode(&long_command_like), VoiceMode::Transcribe);
}
```

- [ ] **Step 2: Add explicit threshold tests using the route-analysis API planned in Task 2.** The tests must cover 79/80/81 non-whitespace characters for short direct commands, 159/160 characters for long-form evidence, and two/three/four sentence boundaries. Keep the input strings deterministic and avoid relying on provider output.

- [ ] **Step 3: Run only the router tests and verify RED.**

Run:

```powershell
cargo test -p talk-runtime --test smart_route_contract -- --nocapture
```

Expected: the existing short command test passes, while the quoted/long-body tests fail because the current whole-transcript keyword scan returns `Command`.

- [ ] **Step 4: Commit only the test file.**

```powershell
git add crates/talk-runtime/tests/smart_route_contract.rs
git commit -m "test: cover smart long-form routing boundaries"
```

## Task 2: Implement bounded Smart route analysis and route context (GREEN)

**Files:**
- Create: `crates/talk-runtime/src/voice_processing.rs`
- Modify: `crates/talk-runtime/src/lib.rs`
- Modify: `crates/talk-runtime/tests/smart_route_contract.rs`

- [ ] **Step 1: Define route constants and diagnostics in `voice_processing.rs`.** Use these initial constants, kept private to the module:

```rust
const LEADING_INTENT_MAX_NON_WHITESPACE: usize = 96;
const SHORT_DIRECT_MAX_NON_WHITESPACE: usize = 80;
const LONG_FORM_MIN_NON_WHITESPACE: usize = 160;
const LONG_FORM_MIN_SENTENCE_BOUNDARIES: usize = 3;
const LONG_FORM_MIN_STREAMING_SEGMENTS: usize = 3;
```

Expose `SmartVoiceRouteAnalysis` with only counts, `long_form_evidence`, `matched_leading_category`, `resolved_mode`, and a stable reason string/enum. It must not contain the transcript or log it.

- [ ] **Step 2: Implement the leading-span parser and precedence.** The parser must inspect only the first nonblank clause, capped at 96 non-whitespace characters. Match established Chinese and English leading meta forms for Document, Translate, and Generate, including polite prefixes (`请`, `帮我`, `please`, `help me`). Match a short direct Command only when the complete normalized input is at most 80 non-whitespace characters and the leading span starts with a supported imperative. Reject question, quote, conditional, and descriptive prefixes before action words. After explicit leading matches, classify long-form evidence when any of the three thresholds is met; otherwise return Transcribe.

- [ ] **Step 3: Add the public compatibility wrapper.** Keep `infer_smart_voice_mode(&str) -> VoiceMode` as a wrapper around `analyze_smart_voice_route(transcript, 0)`. Add a route-context value with `streaming_segment_count: usize`, and make runtime resolution consume it. Existing explicit non-Smart shortcuts must bypass Smart analysis exactly as before.

- [ ] **Step 4: Thread route context through stop-time runtime entry points without breaking existing callers.** Keep current functions as default-context wrappers and add `_with_route_context` variants for `run_voice_session_from_transcript_with_insert_hooks` and `run_voice_session_from_local_transcript_with_insert_hooks`. The desktop stop path will call the variants with the number of ordered nonblank committed/pending segments.

- [ ] **Step 5: Run the router tests and the existing runtime contract.**

Run:

```powershell
cargo test -p talk-runtime --test smart_route_contract -- --nocapture
cargo test -p talk-runtime --test runtime_contract -- --nocapture
```

Expected: all route tests pass and no existing runtime contract regresses.

- [ ] **Step 6: Commit the route implementation.**

```powershell
git add crates/talk-runtime/src/voice_processing.rs crates/talk-runtime/src/lib.rs crates/talk-runtime/tests/smart_route_contract.rs
git commit -m "feat: preserve long-form smart transcription intent"
```

## Task 3: Add faithful-output preservation policy (RED then GREEN)

**Files:**
- Create: `crates/talk-runtime/tests/preservation_contract.rs`
- Modify: `crates/talk-runtime/src/voice_processing.rs`
- Modify: `crates/talk-runtime/src/lib.rs`

- [ ] **Step 1: Add pure validator RED tests before wiring it into provider processing.** Test these exact behaviors:

```rust
#[test]
fn faithful_validation_accepts_distributed_punctuation_edits() {
    let input = "第一段介绍背景第二段说明过程第三段列出风险第四段确认安排".repeat(4);
    let output = "第一段介绍背景，第二段说明过程。第三段列出风险；第四段确认安排！".repeat(4);
    let decision = validate_faithful_output(&input, &output);
    assert!(decision.accepted);
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_rejects_catastrophic_compression() {
    let input = "这是一段完整会议记录，包含背景、过程、例子、风险、结论以及后续安排，不能被一句话替代。".repeat(4);
    let decision = validate_faithful_output(&input, "无法直接处理。");
    assert!(!decision.accepted);
    assert_eq!(
        decision.fallback_reason.map(|reason| reason.as_str()),
        Some("catastrophic_compression")
    );
}

#[test]
fn faithful_validation_rejects_distributed_rewrite_but_accepts_small_corrections() {
    let input = "今天讨论项目背景风险时间表和后续安排".repeat(12);
    let small_correction = input.replacen("风险", "主要风险", 2);
    let unrelated = "完全不同的回答内容与原始会议记录没有对应关系".repeat(12);

    assert!(validate_faithful_output(&input, &small_correction).accepted);
    let rejected = validate_faithful_output(&input, &unrelated);
    assert!(!rejected.accepted);
    assert_eq!(
        rejected.fallback_reason.map(|reason| reason.as_str()),
        Some("excessive_sequence_change")
    );
}
```

- [ ] **Step 2: Run the validator tests and verify RED because the validator is not yet present.**

```powershell
cargo test -p talk-runtime --test preservation_contract -- --nocapture
```

- [ ] **Step 3: Implement normalized bounded validation.** Normalize by removing whitespace and punctuation while retaining Unicode alphanumeric content. For inputs with at least 120 normalized characters, require output retention of at least 60%. Compute normalized edit change with a banded Levenshtein calculation whose allowed distance is `floor(max(input_len, output_len) * 0.35)`, using two bounded rows and early exit when the band cannot pass. Do not reuse the existing prefix/suffix segment patch ratio.

- [ ] **Step 4: Apply the policy exactly once after each faithful provider call.** For concrete `Transcribe` or `Dictate`, return the assembled input when validation rejects the provider output. For `Document`, `Generate`, `Translate`, and `Command`, accept the provider output without this policy. Preserve the existing provider error fallback behavior. The fallback must not issue another provider request.

- [ ] **Step 5: Add an integration test with an HTTP text processor returning a 28-character response for a 5,703-character transcript.** Assert that the session is `Completed`, `output_text == transcript`, and the request detector observes exactly one provider request.

- [ ] **Step 6: Run the focused tests and commit.**

```powershell
cargo test -p talk-runtime --test preservation_contract -- --nocapture
cargo test -p talk-runtime --test runtime_contract -- --nocapture
git add crates/talk-runtime/src/voice_processing.rs crates/talk-runtime/src/lib.rs crates/talk-runtime/tests/preservation_contract.rs
git commit -m "fix: reject catastrophic faithful transcript rewrites"
```

## Task 4: Separate live faithful correction from final Smart routing

**Files:**
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add RED tests for the pure live mode policy.** The expected contract is that every live/speculative correction uses `VoiceMode::Transcribe`, including a session whose final mode is Smart or explicit Command. The final whole-document job is the only exception and receives its already-resolved concrete mode.

- [ ] **Step 2: Run the desktop contract tests and verify RED.**

```powershell
cargo test -p talk-desktop --test desktop_contract live_correction_mode -- --nocapture
```

- [ ] **Step 3: Implement the helper and wire only live segment job creation to it.** In `speculative_correction_job_for_live_segment`, set the job processing mode to `Some(VoiceMode::Transcribe)` regardless of `mode_override`. Do not change explicit final mode selection or final whole-document behavior in this step.

- [ ] **Step 4: Resolve Smart once at stop time and pass the concrete mode to the final whole-document job.** When the local stop session returns its `VoiceRunReport`, derive `report.smart_routed_mode.unwrap_or(report.requested_mode)` and store that concrete mode in the final correction job. The final job must never call `process_voice_transcript_text` with `Some(VoiceMode::Smart)`.

- [ ] **Step 5: Run targeted desktop/runtime tests and commit.**

```powershell
cargo test -p talk-desktop --test desktop_contract live_correction_mode -- --nocapture
cargo test -p talk-runtime --test runtime_contract -- --nocapture
git add crates/talk-desktop/src/main.rs crates/talk-desktop/src/lib.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "fix: keep live smart previews faithful"
```

## Task 5: Make stop aggregation canonical and revision-safe

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add RED equivalence tests.** Build tracker segments plus pending HUD entries and assert that the canonical stop text contains every ordered nonblank HUD entry, including pending-only entries, and equals the concatenated HUD-visible text before final revision reconciliation.

- [ ] **Step 2: Add a RED non-prefix revision test.** Use at least three source groups, revise the middle group from `今天去北京。` to `今天去了北京。`, and assert the result is `前段。今天去了北京。后段。` rather than the stale aggregate or a duplicated full string.

- [ ] **Step 3: Run the focused desktop tests and verify RED.**

```powershell
cargo test -p talk-desktop --test desktop_contract streaming_stop_aggregate -- --nocapture
cargo test -p talk-desktop --test desktop_contract streaming_hud_transcript_parts -- --nocapture
```

- [ ] **Step 4: Implement a shared effective-segment view.** Reuse the HUD ordering rules: tracker segments remain ordered; a nonblank pending text for a matching ID replaces only an uncorrected local value; unknown pending IDs append in pending order. Use one source-root helper (`seg-1#3` -> `seg-1`) for reconciliation.

- [ ] **Step 5: Implement source-aware final reconciliation.** Extend the existing API with a sibling `desktop_streaming_stop_aggregate_with_pending(...)` so current callers/tests remain easy to migrate. If final text equals the effective source/local text, keep corrected output. If it is a strict prefix, keep the existing aggregate. If it is a prefix extension, append only the suffix to the affected source group. Otherwise replace only the affected source group with the revised final text and preserve all unaffected groups. If no source group matches, append the nonblank final tail once.

- [ ] **Step 6: Pass the moved `hud_streaming_segments` into the sibling API from `main.rs`.** Preserve the existing `final_runtime_segment_id` fallback chain, but normalize the supplied runtime ID inside aggregation (`seg-1#3` -> `seg-1`) before selecting the affected source group. Use the resulting text as the authoritative session transcript and use its ordered nonblank segment count for Smart route context.

- [ ] **Step 7: Run the focused tests and commit.**

```powershell
cargo test -p talk-desktop --test desktop_contract streaming_stop_aggregate -- --nocapture
cargo test -p talk-desktop --test desktop_contract streaming_hud_transcript_parts -- --nocapture
git add crates/talk-desktop/src/lib.rs crates/talk-desktop/src/main.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "fix: preserve HUD content during streaming stop assembly"
```

## Task 6: Add route/preservation observability and regression coverage

**Files:**
- Modify: `crates/talk-runtime/src/lib.rs`
- Modify: `crates/talk-runtime/src/voice_processing.rs`
- Modify: `crates/talk-runtime/tests/runtime_contract.rs`
- Modify: `crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Extend the runtime report/log with non-sensitive diagnostics.** Persist requested mode, resolved mode, route reason, input normalized character count, sentence boundary count, streaming segment count, output normalized character count, retention ratio, normalized change ratio, and preservation fallback reason. Never serialize transcript content into diagnostics beyond the existing transcript/output fields.

- [ ] **Step 2: Add tests asserting diagnostics are present for Smart long-form fallback and absent of provider secrets/full duplicate transcript fields.** Keep numeric assertions tolerant of floating-point formatting by checking parsed JSON values or stable reason strings.

- [ ] **Step 3: Add a desktop stop-path unit/integration contract proving the complete tracker aggregate is the input to Smart resolution and that `smart_routed_mode` remains available to existing desktop output policy.**

- [ ] **Step 4: Run all Rust tests touched by this change and commit.**

```powershell
cargo test -p talk-runtime --test smart_route_contract -- --nocapture
cargo test -p talk-runtime --test preservation_contract -- --nocapture
cargo test -p talk-runtime --test runtime_contract -- --nocapture
cargo test -p talk-desktop --test desktop_contract -- --nocapture
git add crates/talk-runtime/src/lib.rs crates/talk-runtime/src/voice_processing.rs crates/talk-runtime/tests/runtime_contract.rs crates/talk-desktop/src/main.rs
git commit -m "test: record smart routing and preservation diagnostics"
```

## Task 7: Workspace verification and isolated r9 product smoke

**Files:**
- Create: `release/Talk/talk-single-exe-20260722-r9/` (generated output only)
- Do not modify: existing r8 release or any running Talk/ASR process

- [ ] **Step 1: Re-read the plan and inspect the diff for unrelated files.** Confirm all pre-existing dirty files remain uncommitted or are included only by their prior commits; stage only the files listed in Tasks 1-6.

- [ ] **Step 2: Run the required repository verification.**

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace --no-fail-fast
Invoke-Pester -Path scripts/tests -PassThru
git diff --check
```

Expected: every command exits 0. If a formatter check is the first failure, run the repository formatter on only the changed Rust files, re-run the same check, and inspect the resulting diff.

- [ ] **Step 3: Build an isolated product release.** Use a unique version id and output root, for example:

```powershell
& .\scripts\Publish-TalkRelease.ps1 `
  -ProductProfile `
  -VersionId 'talk-single-exe-20260722-r9' `
  -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
  -SkipSmoke
```

Then validate the generated product:

```powershell
& .\scripts\Test-TalkProductRelease.ps1 `
  -ProductPath 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260722-r9'
```

- [ ] **Step 4: Run isolated product smokes with unique `LOCALAPPDATA`, hotkey, ASR port, provider endpoint, and log/config directories.** Do not reuse or stop r8. Verify these cases from session logs and HUD/output behavior:

1. Long transcript with quoted `打开` remains full Transcribe output.
2. Short `打开记事本` routes Command.
3. Leading `请总结下面的内容：` with a long body routes Document.
4. A provider returning a 28-character faithful response leaves the full aggregate and marks the session completed.

- [ ] **Step 5: Re-run `git status --short --branch`, inspect generated hashes/logs, and run `git diff --check` once more before any completion claim.**

## Self-Review Checklist

- [ ] Every requirement in `docs/superpowers/specs/2026-07-22-talk-smart-routing-preservation-design.md` maps to a task above.
- [ ] No Smart classification occurs on incomplete live clauses.
- [ ] Long-form evidence cannot be overridden by an incidental body keyword unless an explicit leading meta-instruction matched.
- [ ] The 5,703 -> 28 failure is covered by a one-request preservation test.
- [ ] Stop assembly includes all HUD-visible ordered nonblank segments and handles a non-prefix revision without dropping neighboring sources.
- [ ] Explicit mode shortcuts remain unchanged.
- [ ] All final claims are backed by fresh command output from Task 7.
