# Talk Managed Local ASR Daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let `talk-desktop.exe` automatically use the packaged local ASR daemon when `speculative.local_asr = "streaming_service"` is enabled.

**Architecture:** Keep the engine-neutral loopback WebSocket protocol. Add a small desktop-side launch planner in `talk-desktop` for release-sibling `.internal/talk-local-asr-sherpa.exe`, then wire the Windows shell to start and keep that daemon alive before opening a live streaming ASR session; if the daemon is absent, preserve the existing external-service behavior.

**Tech Stack:** Rust desktop crate, Windows `std::process::Command`, loopback TCP readiness probe, existing Talk config and release layout.

---

## File structure

- Modify `Talk/crates/talk-desktop/src/lib.rs`: add pure helpers for packaged daemon path resolution and endpoint-to-bind launch planning.
- Modify `Talk/crates/talk-desktop/tests/desktop_contract.rs`: add tests for release-sibling daemon discovery, missing-daemon fallback, and loopback bind extraction.
- Modify `Talk/crates/talk-desktop/src/main.rs`: add a managed daemon process slot to `SharedState`, ensure the packaged daemon is started before `LocalStreamingAsrLiveSession::start`, and terminate it on exit.
- Validate with targeted desktop tests plus full Talk verification.

## Task 1: Add pure launch planning helpers

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [x] **Step 1: Write failing tests**

Add tests that create a fake release directory:

```rust
#[test]
fn packaged_local_asr_daemon_launch_plan_finds_release_internal_daemon() {
    let temp_dir = unique_temp_dir("talk-desktop-local-asr-release");
    let release_dir = temp_dir.join("release");
    let internal_dir = release_dir.join(".internal");
    fs::create_dir_all(&internal_dir).expect("create internal dir");
    let daemon_path = internal_dir.join("talk-local-asr-sherpa.exe");
    fs::write(&daemon_path, b"fake exe").expect("write daemon marker");
    let executable_path = release_dir.join("talk-desktop.exe");

    let plan = desktop_packaged_local_asr_daemon_launch_plan(
        &executable_path,
        "ws://127.0.0.1:53171/asr",
    )
    .expect("valid launch plan")
    .expect("packaged daemon should be found");

    assert_eq!(plan.executable_path, daemon_path);
    assert_eq!(plan.bind, "127.0.0.1:53171");
    assert_eq!(plan.args, vec!["--bind", "127.0.0.1:53171"]);
}
```

Also add:

```rust
#[test]
fn packaged_local_asr_daemon_launch_plan_returns_none_when_daemon_is_missing() { ... }

#[test]
fn packaged_local_asr_daemon_launch_plan_normalizes_localhost_to_ipv4_loopback() { ... }

#[test]
fn packaged_local_asr_daemon_launch_plan_uses_bracketed_ipv6_loopback_bind() { ... }
```

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop --test desktop_contract packaged_local_asr_daemon -- --nocapture
```

Expected before implementation: unresolved import/function failures.

- [x] **Step 2: Implement launch planning helpers**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopLocalAsrDaemonLaunchPlan {
    pub executable_path: PathBuf,
    pub bind: String,
    pub args: Vec<String>,
}
```

Add functions:

```rust
pub fn desktop_packaged_local_asr_daemon_path(desktop_executable_path: &Path) -> PathBuf;
pub fn desktop_local_asr_daemon_bind_from_endpoint(endpoint: &str) -> Result<Option<String>, String>;
pub fn desktop_packaged_local_asr_daemon_launch_plan(
    desktop_executable_path: &Path,
    endpoint: &str,
) -> Result<Option<DesktopLocalAsrDaemonLaunchPlan>, String>;
```

Behavior:
- path is `<desktop exe dir>\.internal\talk-local-asr-sherpa.exe`;
- if the file is missing, return `Ok(None)`;
- only auto-manage plain `ws://` loopback endpoints with an explicit port;
- normalize `localhost` to `127.0.0.1`;
- preserve bracketed IPv6 bind format, e.g. `[::1]:53171`;
- launch args are exactly `["--bind", bind]`.

- [x] **Step 3: Verify helper tests pass**

Run the same targeted test command. Expected: all new helper tests pass.

## Task 2: Wire managed daemon lifecycle into the Windows shell

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [x] **Step 1: Add managed process state**

Add a `ManagedLocalAsrDaemon` struct:

```rust
struct ManagedLocalAsrDaemon {
    endpoint: String,
    child: std::process::Child,
}
```

Add `local_asr_daemon: Option<ManagedLocalAsrDaemon>` to `SharedState`.

- [x] **Step 2: Start daemon before streaming session connect**

Before `LocalStreamingAsrLiveSession::start`, call a new helper:

```rust
ensure_packaged_local_asr_daemon(&mut shared, &config)?;
```

It should:
- no-op when config is not `streaming_service`;
- no-op when no packaged daemon exists, preserving external manually-started service behavior;
- keep an existing live child for the same endpoint;
- clear exited children;
- spawn the packaged daemon with `--bind <host:port>`;
- briefly probe TCP readiness before returning.

- [x] **Step 3: Stop managed daemon on exit and replacement**

When the app exits, kill/wait the child. If a child exits or endpoint changes, kill/wait the stale child before launching a new one.

- [x] **Step 4: Verify desktop crate**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p talk-desktop
cargo check --manifest-path .\Talk\Cargo.toml -p talk-desktop --all-targets
```

Expected: both exit 0.

## Task 3: Full validation and release refresh

**Files:**
- Modify this plan checkbox state after validation.

- [x] **Step 1: Run full Talk verification**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
cargo test --manifest-path .\Talk\Cargo.toml --workspace
cargo check --manifest-path .\Talk\Cargo.toml --workspace --all-targets
git diff --check -- Talk
```

Expected: all exit 0.

- [x] **Step 2: Build a managed-daemon release**

Run:

```powershell
& .\Talk\scripts\Publish-TalkRelease.ps1 -VersionId 'desktop-shell-managed-local-asr-v1' -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' -SkipSmoke -SkipNativePreflight -SkipNativeReadiness
```

Then verify:

```powershell
Test-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-managed-local-asr-v1\talk-desktop.exe'
Test-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-managed-local-asr-v1\.internal\talk-local-asr-sherpa.exe'
& .\Talk\scripts\Test-TalkReleaseManifest.ps1 -ManifestPath 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-managed-local-asr-v1\manifest.json'
```

Expected: both `Test-Path` calls print `True`; manifest validation exits 0.
