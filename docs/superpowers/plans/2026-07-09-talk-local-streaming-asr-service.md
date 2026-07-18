# Talk Local Streaming ASR Service Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Talk from post-stop local ASR adapters toward a real local streaming ASR boundary suitable for sherpa-onnx Zipformer / Paraformer.

**Architecture:** Talk desktop will depend on a generic loopback streaming ASR protocol, not on a concrete engine name. The service receives PCM chunks and emits partial/final text; Talk inserts local final text first and uses the existing provider text processor only for asynchronous cloud correction.

**Tech Stack:** Rust workspace crates under `Talk/crates`, TOML config via `talk-core`, JSON protocol DTOs via `talk-client`, future loopback WebSocket transport, current fallback `external_command` JSONL path.

---

## File structure

- Create `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`: protocol contract for daemon authors and desktop integration.
- Modify `Talk/crates/talk-core/src/lib.rs`: add `[speculative.streaming_service]` config struct, defaults, and validation.
- Modify `Talk/crates/talk-core/tests/config_contract.rs`: prove streaming service config parses and invalid endpoints/timeouts fail.
- Modify `Talk/crates/talk-client/src/streaming_asr.rs`: add engine-neutral client/server protocol message types and JSON parser/serializer helpers.
- Modify `Talk/crates/talk-client/tests/streaming_asr_contract.rs`: prove protocol messages round-trip, audio chunks are base64 encoded, and bad server events fail clearly.
- Add `Talk/examples/desktop-streaming-service-speculative-config.toml`: runnable config shape for the future sherpa adapter.
- Update `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`: point from `external_command` fallback toward `streaming_service` as the target path.

## Task 1: Document the protocol and target architecture

**Files:**
- Create: `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`
- Modify: `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`

- [x] **Step 1: Write the protocol document**

Document localhost WebSocket transport, client messages (`start`, `audio`, `stop`, `cancel`), server messages (`ready`, `partial`, `final`, `error`), and Talk config shape.

- [x] **Step 2: Update existing local-first doc**

Clarify that `external_command` is a fallback and `streaming_service` is the preferred target for real-time partial text.

## Task 2: Add streaming service config contract

**Files:**
- Modify: `Talk/crates/talk-core/tests/config_contract.rs`
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Add: `Talk/examples/desktop-streaming-service-speculative-config.toml`

- [x] **Step 1: Write failing config tests**

Add tests proving this TOML parses:

```toml
[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "provider_text_processor"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
```

Add rejection tests for non-WebSocket endpoint, non-loopback host, zero sample rate, zero channels, and zero timeouts.

- [x] **Step 2: Run test and confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-core --test config_contract streaming_service
```

Expected: compile/test failure because `SpeculativeStreamingServiceConfig` fields do not exist yet.

- [x] **Step 3: Implement minimal config support**

Add `SpeculativeStreamingServiceConfig`, default values, optional nested field on `SpeculativeConfig`, and validation only when `enabled && local_asr == "streaming_service"`.

- [x] **Step 4: Run test and confirm GREEN**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-core --test config_contract streaming_service
```

Expected: all streaming service config tests pass.

## Task 3: Add protocol DTOs and JSON helpers

**Files:**
- Modify: `Talk/crates/talk-client/tests/streaming_asr_contract.rs`
- Modify: `Talk/crates/talk-client/src/streaming_asr.rs`

- [x] **Step 1: Write failing protocol tests**

Tests must cover:

- `start` client message serializes with `sample_rate_hz`, `channels`, and optional `language`.
- `audio` client message encodes raw PCM as base64.
- `partial` and `final` server messages parse into `StreamingAsrEvent`.
- `ready` server message exposes engine/model/sample format.
- `error` server message returns a `TalkError::Provider` when converted to an ASR event.

- [x] **Step 2: Run test and confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-client --test streaming_asr_contract local_streaming
```

Expected: compile failure because the protocol types/functions do not exist.

- [x] **Step 3: Implement minimal protocol support**

Add:

- `LocalStreamingAsrClientMessage`
- `LocalStreamingAsrServerMessage`
- `LocalStreamingAsrReady`
- `LocalStreamingAsrStart`
- `LocalStreamingAsrAudio`
- `serialize_local_streaming_asr_client_message`
- `parse_local_streaming_asr_server_message`
- `local_streaming_server_message_to_asr_event`

- [x] **Step 4: Run test and confirm GREEN**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-client --test streaming_asr_contract local_streaming
```

Expected: protocol tests pass.

## Task 4: Verify workspace health

**Files:**
- All modified Talk files.

- [x] **Step 1: Format**

Run:

```powershell
cargo fmt --manifest-path Talk/Cargo.toml --all
```

- [x] **Step 2: Test**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml --workspace
```

- [x] **Step 3: Compile check**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml --workspace --all-targets
```

Expected: all commands exit 0.

## Task 5: Add loopback WebSocket streaming ASR client

**Files:**
- Modify: `Talk/Cargo.toml`
- Modify: `Talk/crates/talk-client/Cargo.toml`
- Modify: `Talk/crates/talk-client/src/streaming_asr.rs`
- Modify: `Talk/crates/talk-client/src/lib.rs`
- Modify: `Talk/crates/talk-client/tests/streaming_asr_contract.rs`

- [x] **Step 1: Write failing client transport test**

Add an async local WebSocket server test that verifies the client sends `start`, `audio`, `stop`, receives `ready`, `partial`, `final`, and returns `StreamingAsrEvent` values.

- [x] **Step 2: Run test and confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-client --test streaming_asr_contract local_streaming_service_client
```

Expected: compile failure because `LocalStreamingAsrServiceClient` and transport dependencies are not wired.

- [x] **Step 3: Implement minimal WebSocket client**

Add `LocalStreamingAsrServiceClient` with `connect`, `start`, `send_audio`, `stop`, `cancel`, `next_server_message`, and `collect_asr_events_until_final`.

- [x] **Step 4: Run test and confirm GREEN**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-client --test streaming_asr_contract local_streaming_service_client
```

Expected: transport test passes.

## Task 6: Add live PCM drain interface for recordings

**Files:**
- Modify: `Talk/crates/talk-audio/src/lib.rs`
- Modify: `Talk/crates/talk-audio/tests/audio_contract.rs`

- [x] **Step 1: Write failing PCM drain test**

Add a silent-backend recording test that creates `RecordingPcmCursor`, drains one raw PCM chunk before `finish`, checks sequence/sample format/bytes, then verifies the second drain returns `None`.

- [x] **Step 2: Run test and confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-audio --test audio_contract recording_session_drains_raw_pcm_chunks
```

Expected: compile failure because `RecordingPcmCursor` and `RecordingSession::drain_pcm_chunk` do not exist.

- [x] **Step 3: Implement minimal PCM drain**

Expose `RecordingPcmCursor`, `RecordingPcmChunk`, and `RecordingSession::drain_pcm_chunk`; for native Windows, encode newly captured aligned samples into target 16-bit PCM; for silent backend, emit deterministic zero PCM.

- [x] **Step 4: Run test and confirm GREEN**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-audio --test audio_contract recording_session_drains_raw_pcm_chunks
```

Expected: PCM drain test passes.

## Task 7: Wire streaming service into runtime

**Files:**
- Modify: `Talk/crates/talk-runtime/Cargo.toml`
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`

- [x] **Step 1: Write failing runtime service test**

Add an async runtime test that starts a loopback WebSocket server, records with the silent backend, calls `run_local_streaming_asr_service_from_recording`, and verifies Talk sends `start` -> `audio` -> `stop` and returns the final ASR event.

- [x] **Step 2: Run test and confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test speculative_runtime_contract streaming_service_runtime
```

Expected: compile failure because the runtime helper and runtime test WebSocket dev dependencies are missing.

- [x] **Step 3: Implement minimal runtime helper**

Add `run_local_streaming_asr_service_from_recording(config, session_id, recording, language)` that reads `[speculative.streaming_service]`, connects to the daemon, sends `start`, drains available PCM chunks, sends `stop`, and collects ASR events until final.

- [x] **Step 4: Run test and confirm GREEN**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test speculative_runtime_contract streaming_service_runtime
```

Expected: runtime service test passes.

## Task 8: Make desktop `streaming_service` usable from the RightAlt flow

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs` or existing desktop contract tests if the insertion/session decision logic is already covered there.

- [x] **Step 1: Write failing desktop-path test or compile contract**

Add the smallest testable seam proving that `speculative.enabled = true` and `local_asr = "streaming_service"` is routed to the local streaming runtime path instead of the old placeholder failure path.

- [x] **Step 2: Run test/check and confirm RED**

Run the targeted desktop test or, if the existing desktop code is only integration-testable, run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p talk-desktop --all-targets
```

Expected: failure before implementation because `streaming_service` is not handled.

- [x] **Step 3: Implement the routing**

When a live `RecordingSession` stops and the config selects `streaming_service`, call the runtime helper with the still-available recording, derive the final transcript from returned events, then pass it through the existing local transcript insertion path. Keep existing focus-preservation and copy-popup behavior unchanged.

- [x] **Step 4: Run targeted verification and confirm GREEN**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p talk-desktop --all-targets
```

Expected: desktop compiles and no old placeholder failure remains for `streaming_service`.

## Task 9: Add ASR benchmark harness skeleton

**Files:**
- Create: `Talk/tools/asr-bench/Cargo.toml`
- Create: `Talk/tools/asr-bench/src/main.rs`
- Modify: `Talk/Cargo.toml`
- Create or modify: `Talk/docs/ASR_BENCHMARKING.md`

- [x] **Step 1: Write failing CLI contract test or compile target**

Add a small CLI entry that accepts engine name, optional WAV path, and output JSON path. It must emit fields needed for Talk ASR selection: `engine`, `audio_duration_ms`, `cold_start_ms`, `first_partial_ms`, `final_latency_ms`, `rtf`, `peak_rss_mb`, `text`, and `cer`.

- [x] **Step 2: Run check and confirm RED**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p asr-bench --all-targets
```

Expected: package is missing before implementation.

- [x] **Step 3: Implement deterministic placeholder benchmark mode**

Implement the CLI with a `--dry-run-text` mode so the benchmark schema and release plumbing exist before real sherpa/FunASR/SenseVoice engines are embedded.

- [x] **Step 4: Run check and smoke command**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p asr-bench --all-targets
cargo run --manifest-path Talk/Cargo.toml -p asr-bench -- --engine sherpa-onnx-zipformer --dry-run-text "你好" --output-json target/asr-bench-smoke.json
```

Expected: check passes and JSON output contains all schema fields.

## Task 10: Add sherpa service adapter skeleton

**Files:**
- Create: `Talk/tools/talk-local-asr-sherpa/Cargo.toml`
- Create: `Talk/tools/talk-local-asr-sherpa/src/main.rs`
- Modify: `Talk/Cargo.toml`
- Modify: `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`

- [x] **Step 1: Write failing compile target**

Add the tool package name to workspace verification expectations and run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa --all-targets
```

Expected: package is missing before implementation.

- [x] **Step 2: Implement protocol-compatible skeleton**

Create a loopback-only WebSocket daemon skeleton that accepts `start`, `audio`, `stop`, emits `ready`, and in `--dry-run-text` mode emits a deterministic final message. Keep real sherpa-onnx model loading behind later flags so the protocol and desktop integration can be tested without a large model download.

- [x] **Step 3: Run check**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa --all-targets
```

Expected: daemon skeleton compiles.

## Task 11: Final Talk verification

**Files:**
- All modified files under `Talk/`.

- [x] **Step 1: Format**

Run:

```powershell
cargo fmt --manifest-path Talk/Cargo.toml --all
```

- [x] **Step 2: Test**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml --workspace
```

- [x] **Step 3: Compile check**

Run:

```powershell
cargo check --manifest-path Talk/Cargo.toml --workspace --all-targets
```

- [x] **Step 4: Build desktop release exe**

Run the repository's existing Talk release command/script, then copy or verify the GUI desktop exe under:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk
```

Expected: Talk workspace checks exit 0 and a user-testable GUI executable exists in the release directory.

## Task 12: Move `streaming_service` from post-stop bridge toward live PCM streaming

**Files:**
- Modify: `Talk/crates/talk-client/src/streaming_asr.rs`
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [x] **Step 1: Write failing runtime live-session contract**

Add a runtime test proving a local streaming session can start before stop, pump available PCM into the WebSocket service, collect a `partial` event before stop, and return the accumulated partial plus final event history when stopped.

- [x] **Step 2: Confirm RED**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test speculative_runtime_contract live_streaming_service_session_pumps_partial_events_before_stop
```

Expected before implementation: compile failure because `LocalStreamingAsrLiveSession` does not exist, then assertion failure because pumped events are not retained in stop output.

- [x] **Step 3: Implement client/runtime live session**

Add non-fatal idle polling to `LocalStreamingAsrServiceClient`, add `LocalStreamingAsrLiveSession::start`, `pump_available_audio`, `stop`, and `cancel`, and retain all events collected before stop so final insertion can still use the complete local ASR session history.

- [x] **Step 4: Wire desktop recording timer to live PCM pump**

When `streaming_service` is selected, start `LocalStreamingAsrLiveSession` when live recording begins, keep it beside the `RecordingSession`, pump PCM from `TIMER_RECORDING_LEVEL`, and stop/cancel the same session when the user stops or cancels recording. Keep existing focus and copy-popup insertion behavior unchanged.

- [x] **Step 5: Verify targeted runtime and desktop checks**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test speculative_runtime_contract live_streaming_service_session_pumps_partial_events_before_stop
cargo check --manifest-path Talk/Cargo.toml -p talk-desktop --all-targets
```

Expected: both commands exit 0.

## Follow-up phase

The next local-first streaming UX phase is tracked in:

```text
Talk/docs/superpowers/plans/2026-07-09-talk-local-first-streaming-followup.md
```

That plan covers dry-run daemon partial messages, desktop listening-HUD partial text, final workspace validation, and the next `release\Talk` GUI build.
