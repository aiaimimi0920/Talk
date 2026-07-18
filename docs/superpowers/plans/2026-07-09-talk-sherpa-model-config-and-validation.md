# Talk Sherpa Model Config and Validation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `talk-desktop.exe` start the packaged local ASR daemon in either dry-run mode or real `sherpa-online` mode from Talk config, without hard-coding sherpa model paths in the desktop binary.

**Architecture:** Keep the desktop-to-ASR protocol engine-neutral and loopback WebSocket based. Add a typed optional `[speculative.streaming_service.local_daemon]` config table in `talk-core`, validate the real-mode model arguments early, and have `talk-desktop` translate that table into `talk-local-asr-sherpa.exe` CLI arguments only when it starts the packaged daemon.

**Tech Stack:** Rust 2021, `serde` TOML config, existing Talk desktop launch-plan tests, packaged `talk-local-asr-sherpa.exe`.

---

## File structure

- Modify `Talk/crates/talk-core/src/lib.rs`: add typed local daemon config enums/structs and validation for required sherpa model paths.
- Modify `Talk/crates/talk-core/tests/config_contract.rs`: cover parsing and missing model path validation.
- Modify `Talk/crates/talk-desktop/src/lib.rs`: preserve the old dry-run launch plan and add a config-aware launch-plan function.
- Modify `Talk/crates/talk-desktop/src/main.rs`: pass `service.local_daemon` into packaged daemon startup.
- Modify `Talk/crates/talk-desktop/tests/desktop_contract.rs`: cover CLI argument generation for transducer sherpa-online mode.
- Modify `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`: document the new desktop config table.
- Modify `Talk/examples/desktop-streaming-service-speculative-config.toml` and release config template comments: show how to opt into real local sherpa models without making missing model files mandatory for default release use.

## Task 1: Core config contract

- [x] **Step 1: Write failing parse test**

Add `parses_speculative_streaming_service_local_daemon_sherpa_config` to `Talk/crates/talk-core/tests/config_contract.rs`.

Expected red result:

```text
no `SpeculativeLocalAsrDaemonMode` in the root
no field `local_daemon` on type `&SpeculativeStreamingServiceConfig`
```

- [x] **Step 2: Write failing validation test**

Add `rejects_speculative_streaming_service_sherpa_daemon_missing_model_paths`.

Expected red result: same missing config types/fields.

- [x] **Step 3: Implement config types and validation**

Add:

```rust
pub enum SpeculativeLocalAsrDaemonMode { DryRun, SherpaOnline }
pub enum SpeculativeSherpaOnlineModelFamily { Transducer, Paraformer }
pub struct SpeculativeLocalAsrDaemonConfig { ... }
```

Validation rules:

- `dry-run` does not require model files.
- `sherpa-online` requires `model`, `tokens`, `encoder`, and `decoder`.
- `sherpa-online + transducer` also requires `joiner`.
- optional paths must not be blank.
- optional strings must not be blank or whitespace-padded.
- `num_threads` and `sample_rate_hz`, when set, must be greater than zero.
- `decoding_method`, when set, must be `greedy_search` or `modified_beam_search`.

- [x] **Step 4: Verify focused core tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-core parses_speculative_streaming_service_local_daemon_sherpa_config -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-core rejects_speculative_streaming_service_sherpa_daemon_missing_model_paths -- --nocapture
```

Expected: both tests pass.

## Task 2: Desktop launch-plan contract

- [x] **Step 1: Write failing desktop test**

Add `packaged_local_asr_daemon_launch_plan_adds_sherpa_model_args_from_config`.

Expected red result:

```text
no `desktop_packaged_local_asr_daemon_launch_plan_with_config` in the root
```

- [x] **Step 2: Implement config-aware launch plan**

Keep the existing `desktop_packaged_local_asr_daemon_launch_plan(path, endpoint)` as a dry-run-compatible wrapper. Add:

```rust
pub fn desktop_packaged_local_asr_daemon_launch_plan_with_config(
    desktop_executable_path: &Path,
    endpoint: &str,
    local_daemon: Option<&SpeculativeLocalAsrDaemonConfig>,
) -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, String>
```

The generated command begins with:

```text
--bind 127.0.0.1:53171
```

When real mode is configured, append:

```text
--mode sherpa-online --model <name> --model-family <transducer|paraformer>
--tokens <path> --encoder <path> --decoder <path> [--joiner <path>]
--provider <provider> --num-threads <n> --sample-rate-hz <hz> --decoding-method <method>
```

- [x] **Step 3: Pass config from desktop startup**

In `ensure_packaged_local_asr_daemon`, pass `service.local_daemon.as_ref()` into the config-aware launch-plan function.

- [x] **Step 4: Verify focused desktop test**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop packaged_local_asr_daemon_launch_plan_adds_sherpa_model_args_from_config -- --nocapture
```

Expected: test passes.

## Task 3: Documentation and example config

- [x] **Step 1: Document `[speculative.streaming_service.local_daemon]`**

Update `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md` with dry-run and sherpa-online config snippets.

- [x] **Step 2: Add commented opt-in examples**

Update the example and release template to show the real-model config block as comments. Do not make the packaged default depend on model files that are not bundled yet.

## Task 4: Full validation

- [x] **Step 1: Format**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
```

- [x] **Step 2: Focused config/desktop tests**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-core speculative_streaming_service -- --nocapture
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop packaged_local_asr_daemon_launch_plan -- --nocapture
```

- [x] **Step 3: Workspace tests and compile check**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
cargo check --manifest-path .\Talk\Cargo.toml --workspace --all-targets
```

- [x] **Step 4: Diff hygiene**

Run:

```powershell
git diff --check -- Talk
```
