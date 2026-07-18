# Talk OpenLess Typeless Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Windows-first Talk desktop shell with tray, global hotkey, and transient HUD while keeping the existing CLI and capability server behavior intact.

**Architecture:** Extract the end-to-end voice session pipeline from `talk-daemon` into a reusable `talk-runtime` crate, then add a new `talk-desktop` crate that hosts a hidden Win32 message loop for tray, hotkey, and HUD behavior. Keep `talk-daemon` as the CLI and loopback capability entry point by delegating pipeline work to the shared runtime crate.

**Tech Stack:** Rust workspace crates, Tokio, existing Talk crates, Win32 APIs via `windows-sys`.

---

### Task 1: Extract shared voice-session runtime

**Files:**
- Create: `Talk/crates/talk-runtime/Cargo.toml`
- Create: `Talk/crates/talk-runtime/src/lib.rs`
- Create: `Talk/crates/talk-runtime/tests/runtime_contract.rs`
- Modify: `Talk/Cargo.toml`
- Modify: `Talk/crates/talk-daemon/Cargo.toml`
- Modify: `Talk/crates/talk-daemon/src/main.rs`

- [ ] **Step 1: Write the failing runtime contract tests**

Add tests in `Talk/crates/talk-runtime/tests/runtime_contract.rs` covering:

```rust
#[tokio::test]
async fn runtime_runs_mock_session_and_reports_phase_sequence() {
    // build TalkConfig with silent audio + mock provider + dry_run output
    // collect emitted phases
    // assert ordered phases:
    // trigger_armed -> recording -> transcribing -> processing -> inserting -> completed
    // assert session log exists and output text matches mock input
}

#[tokio::test]
async fn runtime_persists_failed_session_log_when_provider_fails() {
    // build TalkConfig with invalid HTTP endpoint or disabled native backend path
    // assert runtime returns failure state
    // assert failed session log exists with error text
}
```

Run: `cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test runtime_contract`

Expected: FAIL because `talk-runtime` does not exist yet.

- [ ] **Step 2: Add the new workspace crate skeleton**

Create `Talk/crates/talk-runtime/Cargo.toml` with dependencies on:

```toml
[package]
name = "talk-runtime"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
reqwest.workspace = true
serde.workspace = true
serde_json.workspace = true
talk-audio = { path = "../talk-audio" }
talk-client = { path = "../talk-client" }
talk-core = { path = "../talk-core" }
talk-hotkey = { path = "../talk-hotkey" }
talk-insert = { path = "../talk-insert" }
tokio.workspace = true
uuid.workspace = true
```

Also add `"crates/talk-runtime"` to `Talk/Cargo.toml`.

- [ ] **Step 3: Implement the minimal shared runtime API**

Create `Talk/crates/talk-runtime/src/lib.rs` with:

```rust
pub enum SessionPhase {
    Idle,
    TriggerArmed,
    Recording,
    Transcribing,
    Processing,
    Inserting,
    Completed,
    Failed,
    Cancelled,
}

pub struct VoiceRunReport { /* session, outcome, trigger_events, log_path */ }

pub async fn load_effective_config(path: &Path) -> Result<TalkConfig>;

pub async fn run_voice_session<F>(
    config: &TalkConfig,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(SessionPhase) + Send;
```

Move the existing `load_effective_config`, trigger execution, provider selection,
audio capture, insertion, and session-log persistence logic out of
`talk-daemon/src/main.rs` into this crate with only the minimum API changes
required for sharing.

- [ ] **Step 4: Run the new runtime tests**

Run: `cargo test --manifest-path Talk/Cargo.toml -p talk-runtime --test runtime_contract`

Expected: PASS

- [ ] **Step 5: Refactor `talk-daemon` to use `talk-runtime`**

Update `Talk/crates/talk-daemon/Cargo.toml`:

```toml
talk-runtime = { path = "../talk-runtime" }
```

Update `Talk/crates/talk-daemon/src/main.rs` so:

- config loading calls `talk_runtime::load_effective_config`;
- `once` and `/v1/invoke` call `talk_runtime::run_voice_session`;
- CLI output formatting stays local to `talk-daemon`.

- [ ] **Step 6: Verify existing CLI behavior still works**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-daemon --test cli_contract
cargo run --manifest-path Talk/Cargo.toml -p talk-daemon -- check --config Talk/examples/dev-config.toml
cargo run --manifest-path Talk/Cargo.toml -p talk-daemon -- once --config Talk/examples/dev-config.toml --mock-text "plan runtime smoke"
```

Expected:
- CLI contract tests PASS
- `check` prints `config ok`
- `once` prints `once ok`

### Task 2: Add desktop-shell state and Win32-safe boundaries

**Files:**
- Create: `Talk/crates/talk-desktop/Cargo.toml`
- Create: `Talk/crates/talk-desktop/src/main.rs`
- Create: `Talk/crates/talk-desktop/src/app.rs`
- Create: `Talk/crates/talk-desktop/src/hotkey.rs`
- Create: `Talk/crates/talk-desktop/src/hud.rs`
- Create: `Talk/crates/talk-desktop/src/tray.rs`
- Create: `Talk/crates/talk-desktop/src/win32.rs`
- Create: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/Cargo.toml`

- [ ] **Step 1: Write the failing desktop logic tests**

Add tests in `Talk/crates/talk-desktop/tests/desktop_contract.rs` for pure logic:

```rust
#[test]
fn hud_text_maps_runtime_phases_to_short_openless_style_messages() {
    // assert phase -> "Talk: listening", "Talk: transcribing", etc.
}

#[test]
fn tray_menu_state_exposes_start_when_idle_and_stop_when_recording() {
    // assert menu model changes with app state
}

#[test]
fn shell_ignores_duplicate_start_requests_while_busy() {
    // assert second activation during active session does not queue another run
}
```

Run: `cargo test --manifest-path Talk/Cargo.toml -p talk-desktop --test desktop_contract`

Expected: FAIL because `talk-desktop` does not exist yet.

- [ ] **Step 2: Add the desktop crate skeleton**

Create `Talk/crates/talk-desktop/Cargo.toml`:

```toml
[package]
name = "talk-desktop"
version.workspace = true
edition.workspace = true
license.workspace = true

[[bin]]
name = "talk-desktop"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
talk-client = { path = "../talk-client" }
talk-core = { path = "../talk-core" }
talk-hotkey = { path = "../talk-hotkey" }
talk-runtime = { path = "../talk-runtime" }
tokio.workspace = true
uuid.workspace = true

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = [
  "Win32_Foundation",
  "Win32_Graphics_Gdi",
  "Win32_System_LibraryLoader",
  "Win32_System_Threading",
  "Win32_UI_Shell",
  "Win32_UI_WindowsAndMessaging"
] }
```

Add `"crates/talk-desktop"` to `Talk/Cargo.toml`.

- [ ] **Step 3: Implement pure shell-state modules first**

Add:

- `src/app.rs` for app state, tray menu model, phase-to-HUD mapping;
- `src/hotkey.rs` for shortcut parsing / hotkey action mapping;
- `src/hud.rs` for HUD message data model and hide timing;

Keep these modules mostly platform-agnostic so the first tests can pass before
any raw Win32 calls exist.

- [ ] **Step 4: Run desktop logic tests**

Run: `cargo test --manifest-path Talk/Cargo.toml -p talk-desktop --test desktop_contract`

Expected: PASS

### Task 3: Implement the Win32 shell

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/src/app.rs`
- Modify: `Talk/crates/talk-desktop/src/hotkey.rs`
- Modify: `Talk/crates/talk-desktop/src/hud.rs`
- Modify: `Talk/crates/talk-desktop/src/tray.rs`
- Modify: `Talk/crates/talk-desktop/src/win32.rs`

- [ ] **Step 1: Write the first failing startup smoke test**

Add a desktop startup test that only checks non-interactive startup helpers:

```rust
#[test]
fn desktop_startup_rejects_invalid_shortcuts_before_shell_boot() {
    // assert startup config validation catches malformed shortcut values
}
```

Run: `cargo test --manifest-path Talk/Cargo.toml -p talk-desktop`

Expected: one or more FAIL cases tied to missing startup validation or shell wiring.

- [ ] **Step 2: Implement Win32 wrappers behind narrow functions**

In `src/win32.rs`, add wrappers for:

- class registration;
- hidden window creation;
- `RegisterHotKey` / `UnregisterHotKey`;
- `Shell_NotifyIconW`;
- popup menu display;
- HUD popup window show/update/hide;
- message-loop dispatch.

Keep unsafe code isolated in this file so `app.rs` remains testable.

- [ ] **Step 3: Implement shell startup and session worker**

In `src/main.rs`, build:

```rust
fn main() -> Result<()> {
    // load effective config
    // create tokio runtime
    // create desktop app state
    // create hidden window + tray
    // register hotkey
    // run message loop
}
```

Use a background worker to call `talk_runtime::run_voice_session` and send
phase updates back to the UI thread.

- [ ] **Step 4: Implement tray actions and HUD updates**

Wire:

- `Start dictation`
- `Stop recording`
- `Open Talk logs folder`
- `Exit Talk`

Map runtime phases to HUD strings and show them with a short auto-hide delay for
terminal states.

- [ ] **Step 5: Run desktop tests and compile checks**

Run:

```powershell
cargo test --manifest-path Talk/Cargo.toml -p talk-desktop
cargo check --manifest-path Talk/Cargo.toml --workspace --all-targets
```

Expected: PASS

### Task 4: End-to-end verification and release output

**Files:**
- Modify: `Talk/README.md`
- Modify: `Talk/docs/TALK_DESIGN.md`
- Create or update runtime/release notes only under `Talk/` as needed
- Write binaries/artifacts under: `release/Talk/<version-id>/`

- [ ] **Step 1: Update docs for the new desktop entry point**

Document:

- `talk-desktop.exe` purpose;
- tray + hotkey + HUD behavior;
- Windows-first limitation;
- manual smoke commands;
- relationship to `talk-daemon`.

- [ ] **Step 2: Run workspace verification**

Run:

```powershell
cargo fmt --manifest-path Talk/Cargo.toml --all -- --check
cargo test --manifest-path Talk/Cargo.toml --workspace
cargo check --manifest-path Talk/Cargo.toml --workspace --all-targets
```

Expected: all PASS

- [ ] **Step 3: Build release binaries**

Run:

```powershell
cargo build --manifest-path Talk/Cargo.toml --release -p talk-daemon -p talk-desktop
```

Expected: release binaries built under `Talk/target/release/`.

- [ ] **Step 4: Copy release outputs into the Talk release directory**

Create a new version folder, for example:

```powershell
$versionId = 'desktop-shell-20260704-v1'
$outDir = 'C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro\\release\\Talk\\' + $versionId
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
Copy-Item 'C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro\\Talk\\target\\release\\talk.exe' (Join-Path $outDir 'talk.exe')
Copy-Item 'C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro\\Talk\\target\\release\\talk-desktop.exe' (Join-Path $outDir 'talk-desktop.exe')
```

Also write a short `BUILD_INFO.txt` summarizing:

- commit if available;
- build date;
- included binaries;
- key verification commands.

- [ ] **Step 5: Manual smoke the desktop shell**

Run `talk-desktop.exe`, verify:

- no main window opens;
- tray icon exists;
- hotkey starts a session;
- HUD status changes appear;
- a session log is written.

If the environment is not suitable for interactive verification, explicitly note
that automated build/test verification passed and manual desktop smoke remains
the only outstanding human check.
