# Talk Two-Level Streaming Correction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add ordered per-clause remote correction during recording and one safe whole-text optimization after local corrections finish.

**Architecture:** Extend the runtime segmenter with clause boundaries. Add a recording-scoped FIFO queue and tracker that survives the transition from recording to stopping. Reuse the existing final document patch path, but compare against the actual inserted local clauses.

**Tech Stack:** Rust, Tokio, Win32 clipboard insertion, existing Talk provider/runtime APIs, Cargo tests, PowerShell release tooling.

---

### Task 1: Clause Segmentation

**Files:** `crates/talk-runtime/src/segmenter.rs`, `crates/talk-runtime/tests/segmenter_contract.rs`, `crates/talk-runtime/tests/speculative_runtime_contract.rs`

- [ ] Add failing tests for `你好，` becoming ready, one-character clauses remaining pending, and the approved example producing three ordered correction requests.
- [ ] Run `cargo test -p talk-runtime --test segmenter_contract` and `cargo test -p talk-runtime --test speculative_runtime_contract`; verify RED.
- [ ] Add `min_clause_chars = 2` and clause punctuation readiness for `， , ； ; ： :`; keep existing sentence, pause, and 30-character fallbacks.
- [ ] Rerun both focused test targets; verify GREEN.
- [ ] Commit with `git commit -m "feat: split streaming speech at clause boundaries"`.

### Task 2: Ordered Local Correction Tracker

**Files:** `crates/talk-desktop/src/lib.rs`, `crates/talk-desktop/src/main.rs`, `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add failing pure-contract tests for ordered corrected summaries, corrected predecessor context, inserted-anchor baselines, stale results, and duplicate results.
- [ ] Run `cargo test -p talk-desktop --test desktop_contract live_correction`; verify RED.
- [ ] Add a recording-scoped tracker with ordered segments, pending count, cancelled/stopping flags, and `tokio::sync::Notify`.
- [ ] Register every correction before queue send; decrement pending after every provider result, including errors.
- [ ] Rebuild each local request context from already corrected predecessor segments.
- [ ] Record corrected text and anchors in the tracker, mirror live anchors into `ActiveRecording`, remove completed segments from yellow HUD accumulation, and preserve `Listening` HUD state.
- [ ] Run `cargo test -p talk-desktop --test desktop_contract live_correction` and `cargo check -p talk-desktop --all-targets`.
- [ ] Commit with `git commit -m "feat: track ordered streaming corrections"`.

### Task 3: Drain Before Stop

**Files:** `crates/talk-desktop/src/main.rs`, `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add failing tests for aggregate text and inserted baselines when some clauses have anchors and others do not.
- [ ] Run `cargo test -p talk-desktop --test desktop_contract streaming_stop`; verify RED.
- [ ] Make the active correction sender optional; on stop close it, mark the tracker stopping, await pending count zero, and snapshot corrected clauses and anchors before selecting the stop policy.
- [ ] On cancel mark the tracker cancelled before closing the sender so queued jobs cannot insert or show stale popups.
- [ ] Run the focused stop and local-correction tests.
- [ ] Commit with `git commit -m "fix: drain local corrections before final processing"`.

### Task 4: One Final Aggregate Pass

**Files:** `crates/talk-desktop/src/lib.rs`, `crates/talk-desktop/src/main.rs`, `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add failing tests proving zero inserted clauses use the normal processed transcript path, while inserted clauses use local dry-run completion plus one final correction job.
- [ ] Run `cargo test -p talk-desktop --test desktop_contract final_correction`; verify RED.
- [ ] Build the final aggregate from corrected tracker text plus the uncommitted ASR tail.
- [ ] Suppress the later correction job when the normal transcript path already performed the one global AI call.
- [ ] When local clauses were inserted, set `full_document_inserted_segments` to actual anchor texts, run one final provider request, and use safe replacement or copy-popup fallback.
- [ ] Run `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo test --workspace`, and `git diff --check`.
- [ ] Commit with `git commit -m "feat: add final aggregate correction pass"`.

### Task 5: Release And Acceptance

**Files:** `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r7\Talk.exe`, `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r7\talk.toml`

- [ ] Re-run the full verification commands.
- [ ] Publish with:

```powershell
.\scripts\Publish-TalkRelease.ps1 `
  -VersionId talk-single-exe-20260719-r7 `
  -ReleaseRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk `
  -ProductProfile
```

- [ ] Verify the package contains only `Talk.exe` and `talk.toml`, record SHA-256 hashes, push `main`, and verify GitHub Actions.
- [ ] Stop only an exact-path r6 process, launch r7 with its sibling config, and verify `Talk.exe` plus the embedded ASR worker.
