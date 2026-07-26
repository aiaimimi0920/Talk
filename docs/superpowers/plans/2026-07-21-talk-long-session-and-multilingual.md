# Talk Long Session And Multilingual Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make long Talk sessions responsive, make HUD transcript scrolling stable, reduce and measure live correction latency, and enable Chinese-English-Japanese mixed transcription.

**Architecture:** Preserve the existing Win32 desktop, Tokio runtime, embedded sherpa worker, and first-run model bootstrap. Add pure policy helpers in `talk-desktop`, bounded live-pump behavior in `talk-runtime`, a configurable live clipboard settle delay in `talk-insert`, bounded correction concurrency in the desktop worker, and a verified multilingual model catalog entry.

**Tech Stack:** Rust, Tokio, windows-sys, sherpa-onnx, PowerShell/Pester, Cargo workspace tests.

---

### Task 1: Stable wheel scrolling and auto-follow

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add failing contract tests for positive/negative wheel deltas, top/bottom clamping, preserving a user-selected offset, and restoring auto-follow only at the bottom.
- [ ] Run the focused desktop contract tests and verify they fail because the wheel/auto-follow policy helpers do not exist.
- [ ] Add pure scroll policy helpers and wire `WM_MOUSEWHEEL`, scrollbar dragging, transcript refresh, HUD reset, and geometry refresh to persistent auto-follow state.
- [ ] Run the focused tests and verify they pass.

### Task 2: Bounded live ASR pumping and history

**Files:**
- Modify: `crates/talk-runtime/src/lib.rs`

- [ ] Add failing runtime tests proving a live pump budget processes one PCM chunk per call and retained ASR history cannot grow past its configured maximum.
- [ ] Run the focused runtime tests and verify the unbounded implementation fails them.
- [ ] Add a maximum-chunk parameter to the internal PCM drain path, use a one-chunk budget for live UI pumping, leave stop-time drain unlimited, and retain only the latest useful ASR events.
- [ ] Run all `talk-runtime` tests and verify they pass.

### Task 3: Non-blocking long-text HUD refresh

**Files:**
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add a failing policy test proving waveform-only refresh does not request transcript layout work.
- [ ] Run the focused test and verify it fails.
- [ ] Track whether text or geometry changed and skip full transcript layout on waveform-only timer ticks; invalidate only the waveform rectangle in that case.
- [ ] Run the focused desktop tests and verify they pass.

### Task 4: Short live clipboard settle delay

**Files:**
- Modify: `crates/talk-insert/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Test: `crates/talk-insert/tests/insert_contract.rs`

- [ ] Add a failing insert contract test for constructing a clipboard inserter with an explicit settle delay while retaining the default constructor.
- [ ] Run the focused insert tests and verify the explicit-delay API is missing.
- [ ] Store the settle delay in `ClipboardPasteInserter`, keep 500 ms for the default constructor, and use a short live-only delay from the desktop streaming insertion path.
- [ ] Run all `talk-insert` tests and the focused desktop tests.

### Task 5: Bounded concurrent correction with timing evidence

**Files:**
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] Add failing policy tests for queue capacity, default concurrency of three, timing outcome formatting, and bounded stop-drain behavior.
- [ ] Run the focused tests and verify they fail.
- [ ] Replace the unbounded correction channel with a bounded channel and `try_send`, launch correction tasks behind a three-permit semaphore, record queue/provider/apply/total timings without transcript or credentials, and complete rejected jobs cleanly.
- [ ] Add a fixed stop-drain timeout; on timeout cancel remaining live feedback and continue to the existing final aggregate path.
- [ ] Run focused desktop and runtime tests.

### Task 6: Multilingual default model

**Files:**
- Modify: `crates/talk-desktop/src/model_bootstrap.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Test: `crates/talk-desktop/tests/model_bootstrap_contract.rs`
- Test: `crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `README.md`

- [ ] Add failing catalog and daemon-launch tests for the multilingual model ID, SHA-256, and exact encoder/decoder/joiner filenames.
- [ ] Run the focused tests and verify they fail against the old Chinese-English model.
- [ ] Update the product bootstrap catalog and managed-daemon discovery while retaining legacy model discovery as a fallback.
- [ ] Run the model and desktop contract tests.

### Task 7: Integration verification and r7 release

**Files:**
- Modify only if tests require it: `scripts/Publish-TalkRelease.ps1`
- Test: `scripts/tests/Publish-TalkRelease.Tests.ps1`
- Create: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260721-r7\Talk.exe`
- Create: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260721-r7\talk.toml`

- [ ] Run `cargo fmt --all -- --check` and `git diff --check`.
- [ ] Run `cargo check --workspace --all-targets` and `cargo test --workspace`.
- [ ] Run `Invoke-Pester -Path scripts/tests/Publish-TalkRelease.Tests.ps1 -PassThru`.
- [ ] Build and publish r7 without stopping r5, then run smoke checks on a random unused loopback port.
- [ ] Verify the release directory contains exactly `Talk.exe` and `talk.toml`, and verify `max_recording_seconds = 0` remains in the product configuration.

