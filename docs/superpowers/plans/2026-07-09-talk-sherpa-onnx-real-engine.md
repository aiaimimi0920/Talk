# Talk Sherpa ONNX Real Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `talk-local-asr-sherpa.exe` from a dry-run protocol daemon into a real sherpa-onnx streaming Zipformer / Paraformer adapter while preserving the existing desktop WebSocket protocol.

**Architecture:** Keep Talk desktop engine-neutral: desktop still starts/connects to the packaged daemon and sends 16 kHz mono PCM over loopback WebSocket. The daemon gains explicit runtime modes: `dry-run` for package/protocol smoke tests and `sherpa-online` for real sherpa-onnx online recognition. Real mode validates model files at startup, loads one hot recognizer, creates a stream per dictation session, emits changed partial hypotheses while audio arrives, and emits a final result after `stop`.

**Tech Stack:** Rust 2021, `tokio`, `tokio-tungstenite`, `clap`, `base64`, official `sherpa-onnx` Rust crate, existing Talk local streaming ASR protocol.

---

## File structure

- Modify `Talk/tools/talk-local-asr-sherpa/Cargo.toml`: add `base64` and `sherpa-onnx` dependencies for PCM decoding and real streaming inference.
- Modify `Talk/tools/talk-local-asr-sherpa/src/main.rs`: add mode/model CLI parsing, startup validation, dry-run/real engine abstraction, PCM decoding, sherpa recognizer setup, and session-level partial/final emission.
- Modify `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`: document the new real-engine daemon flags and keep the dry-run example.
- Modify `Talk/scripts/Publish-TalkRelease.ps1`: copy sherpa/onnxruntime runtime DLLs beside `.internal/talk-local-asr-sherpa.exe` because the Windows build uses sherpa-onnx shared mode on this host.
- Validate with focused daemon tests first, then daemon compile checks, then broader Talk verification if the native sherpa dependency links on the current host.

## Task 1: Add daemon model configuration validation

**Files:**
- Modify: `Talk/tools/talk-local-asr-sherpa/src/main.rs`

- [x] **Step 1: Write failing config tests**

Add tests for:

```rust
#[test]
fn dry_run_mode_does_not_require_model_files() { ... }

#[test]
fn sherpa_transducer_mode_requires_existing_encoder_decoder_joiner_and_tokens() { ... }

#[test]
fn sherpa_paraformer_mode_requires_existing_encoder_decoder_and_tokens_without_joiner() { ... }

#[test]
fn sherpa_mode_rejects_zero_threads_and_blank_provider() { ... }
```

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa sherpa_ -- --nocapture
```

Expected before implementation: compile failure for missing config types/functions.

- [x] **Step 2: Implement minimal config types**

Add:

```rust
#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
enum DaemonMode { DryRun, SherpaOnline }

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
enum SherpaOnlineModelFamily { Transducer, Paraformer }
```

Then add a validated `DaemonConfig::from_cli(cli)` that:

- keeps `dry-run` as the default mode so the current packaged desktop can still start without large model files;
- requires `--tokens`, `--encoder`, `--decoder`, and `--joiner` for `--mode sherpa-online --model-family transducer`;
- requires `--tokens`, `--encoder`, and `--decoder` for `--mode sherpa-online --model-family paraformer`;
- rejects missing/non-file model paths;
- rejects `--num-threads 0`;
- rejects blank/trimmed `--provider`, `--model`, and `--decoding-method`.

- [x] **Step 3: Verify config tests pass**

Run the same focused daemon test command. Expected: new config tests pass.

## Task 2: Add real sherpa online recognition adapter

**Files:**
- Modify: `Talk/tools/talk-local-asr-sherpa/Cargo.toml`
- Modify: `Talk/tools/talk-local-asr-sherpa/src/main.rs`

- [x] **Step 1: Add official sherpa dependency**

Use the official crate:

```toml
sherpa-onnx = { version = "1.13.4", default-features = false, features = ["shared"] }
base64.workspace = true
```

Static linking was tried first but failed on this Windows host with unresolved MSVC STL vectorized symbols from the upstream `win-x64-static-MT-Release` prebuilt archive. Use shared mode and package the copied DLLs beside the daemon.

- [x] **Step 2: Implement engine abstraction**

Add a small internal abstraction:

```rust
trait LocalStreamingAsrEngine {
    fn ready_engine(&self) -> &str;
    fn ready_model(&self) -> &str;
    fn start_session(&self, sample_rate_hz: u32, channels: u16) -> Result<Box<dyn LocalStreamingAsrSession + Send>>;
}

trait LocalStreamingAsrSession {
    fn accept_pcm_i16_le(&mut self, pcm: &[u8]) -> Result<Option<String>>;
    fn finish(&mut self) -> Result<String>;
}
```

Implement:

- `DryRunEngine`: emits the configured partial once and configured final on `stop`;
- `SherpaOnlineEngine`: owns one `sherpa_onnx::OnlineRecognizer`;
- `SherpaOnlineSession`: owns one `OnlineStream`, converts i16 little-endian PCM to f32 samples in `[-1.0, 1.0]`, decodes while ready, emits a partial only when nonblank text changes, and flushes final text on `finish()`.

- [x] **Step 3: Keep protocol output stable**

Modify `handle_connection` so:

- `ready` still contains `engine`, `model`, `sample_rate_hz`, and `channels`;
- `partial` still contains `type`, `session_id`, `segment_id`, and `text`;
- `final` still contains `type`, `session_id`, `segment_id`, and `text`;
- existing metadata fields may remain for diagnostics but desktop must not depend on them.

- [x] **Step 4: Verify dry-run protocol regression**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa dry_run_daemon_emits_partial_after_first_audio_chunk -- --nocapture
```

Expected: pass; dry-run behavior remains compatible.

## Task 3: Document real-engine invocation

**Files:**
- Modify: `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`

- [x] **Step 1: Add examples**

Document:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p talk-local-asr-sherpa -- `
  --bind 127.0.0.1:53171 `
  --mode sherpa-online `
  --model-family transducer `
  --model zipformer-bilingual-zh-en `
  --tokens C:\models\zipformer\tokens.txt `
  --encoder C:\models\zipformer\encoder-epoch-99-avg-1.int8.onnx `
  --decoder C:\models\zipformer\decoder-epoch-99-avg-1.onnx `
  --joiner C:\models\zipformer\joiner-epoch-99-avg-1.int8.onnx `
  --provider cpu `
  --num-threads 2
```

Also document Paraformer:

```powershell
--mode sherpa-online --model-family paraformer --tokens ... --encoder ... --decoder ...
```

- [x] **Step 2: Explain current packaging boundary**

State that release packaging currently ships the daemon binary, not large model assets; real mode requires the model files to be supplied explicitly or by a later model-packaging task.

## Task 4: Validate the slice

**Files:**
- Update this plan checkboxes after validation.

- [x] **Step 1: Format**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
```

- [x] **Step 2: Focused daemon tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa -- --nocapture
```

- [x] **Step 3: Compile daemon all targets**

Run:

```powershell
cargo check --manifest-path .\Talk\Cargo.toml -p talk-local-asr-sherpa --all-targets
```

- [x] **Step 4: Scope check**

Run:

```powershell
git diff --check -- Talk/tools/talk-local-asr-sherpa Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md Talk/docs/superpowers/plans/2026-07-09-talk-sherpa-onnx-real-engine.md
```

Expected: no whitespace errors in the touched slice.
