# Talk Single-EXE Product Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Change Talk's user-facing release from an engineering bundle into exactly `Talk.exe` plus `talk.toml`, while preserving local-first ASR through an embedded, verified worker payload and automatic first-run Zipformer installation.

**Architecture:** Keep `talk-local-asr-sherpa` as an isolated worker because Sherpa-ONNX currently uses shared Windows DLLs and static linking previously failed. Append a ZIP payload and fixed trailer to the desktop executable; the desktop process verifies and extracts that payload into a content-addressed `%LOCALAPPDATA%\\Talk\\runtime` directory before launching the hidden worker. Add a focused Rust model-bootstrap module that downloads, hashes, safely extracts, and atomically installs the pinned Zipformer archive under `%LOCALAPPDATA%\\Talk\\models\\sherpa-onnx`.

**Tech Stack:** Rust 2021 workspace, `reqwest` with rustls, `sha2`, `zip`, `tar`, `bzip2`, Tokio, Windows PowerShell release tooling, Pester 3.4 contract tests.

---

## File Map

- Create: `crates/talk-desktop/src/product_payload.rs` - payload trailer parsing, archive validation, content-addressed extraction, and worker path resolution.
- Create: `crates/talk-desktop/src/model_bootstrap.rs` - pinned model catalog, cache validation, HTTPS download, SHA-256 verification, tar.bz2 extraction, and atomic install.
- Modify: `crates/talk-desktop/src/lib.rs` - export product constants, prefer `talk.toml`, and expose payload/model helpers used by the Windows shell and tests.
- Modify: `crates/talk-desktop/src/main.rs` - bootstrap embedded runtime and first-run model asynchronously, show status, and preserve cloud fallback.
- Modify: `crates/talk-desktop/Cargo.toml` - add `reqwest`, `sha2`, `zip`, `tar`, and `bzip2` dependencies through workspace versions.
- Modify: `Cargo.toml` - add shared dependency declarations and the package metadata needed by the new module.
- Modify: `scripts/Publish-TalkRelease.ps1` - add product profile, build/append payload, write only `Talk.exe` and `talk.toml` to the product directory, and keep validation metadata outside that directory.
- Modify: `scripts/Test-TalkReleaseManifest.ps1` and `scripts/Test-TalkReleaseSummary.ps1` - validate the product profile without requiring engineering support files.
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1` - replace bundle-default assertions with product-layout assertions and retain explicit engineering-profile coverage.
- Modify: `crates/talk-desktop/tests/desktop_contract.rs` - add config-name and payload/model integration contracts.
- Create: `crates/talk-desktop/tests/product_payload_contract.rs` - focused payload parser/extractor tests.
- Create: `crates/talk-desktop/tests/model_bootstrap_contract.rs` - catalog, archive hash, safe extraction, and atomic-install tests.
- Modify: `.github/workflows/build-talk.yml` and `.github/workflows/release-talk-tag.yml` - publish the two-file product artifact and keep engineering evidence as a separate artifact.
- Modify: `docs/LOCAL_SHERPA_MODELS.md`, `docs/LOCAL_STREAMING_ASR_PROTOCOL.md`, and the single-EXE spec - document first-run bootstrap and remove user-facing PowerShell installation instructions from the product path.

### Task 1: Establish failing product-layout contracts

**Files:**
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1`
- Modify: `scripts/Test-TalkReleaseManifest.ps1`

- [x] **Step 1: Write the failing product-layout tests**

Add a publish fixture assertion with these exact expectations:

```powershell
$files = @(Get-ChildItem -LiteralPath $result.DestinationDir -Recurse -File)
@($files | ForEach-Object { $_.FullName.Substring($result.DestinationDir.Length).TrimStart('\\') } | Sort-Object) |
    Should Be @('Talk.exe', 'talk.toml')
@($files | Where-Object Extension -in @('.exe', '.dll', '.ps1')).Count | Should Be 1
```

Add a manifest contract that requires `profile = 'product'`, exactly one executable named `Talk.exe`, and no `supportFiles` paths inside the product directory.

- [x] **Step 2: Run only the new tests and verify RED**

Run:

```powershell
Invoke-Pester -Script .\\scripts\\tests\\Publish-TalkRelease.Tests.ps1 -PassThru
```

Expected: the new assertions fail because the current publisher writes `.internal`, PowerShell files, manifests, and the `talk-desktop.exe` name.

- [x] **Step 3: Commit the failing contract**

```powershell
git add scripts/tests/Publish-TalkRelease.Tests.ps1 scripts/Test-TalkReleaseManifest.ps1
git commit -m "test: define single-exe Talk product layout"
```

### Task 2: Add payload format tests before implementation

**Files:**
- Create: `crates/talk-desktop/tests/product_payload_contract.rs`
- Modify: `crates/talk-desktop/src/lib.rs`

- [x] **Step 1: Write valid-trailer and failure tests**

The tests must construct a temporary executable-like byte file and assert:

```rust
let payload = build_test_payload(&[("talk-local-asr-sherpa.exe", b"worker")]);
let parsed = parse_embedded_payload(&payload).expect("valid payload");
assert_eq!(parsed.files[0].path, PathBuf::from("talk-local-asr-sherpa.exe"));
```

Add tests that expect errors for truncated trailers, a mismatched archive hash, an absolute member path, `../escape`, duplicate members, and an unexpected member such as `asr-bench.exe`.

- [x] **Step 2: Run the focused Rust test and verify RED**

Run:

```powershell
cargo test -p talk-desktop --test product_payload_contract
```

Expected: compilation fails because the payload parser and test helper do not exist.

- [x] **Step 3: Commit the failing payload tests**

```powershell
git add crates/talk-desktop/tests/product_payload_contract.rs crates/talk-desktop/src/lib.rs
git commit -m "test: define embedded Talk runtime payload contract"
```

### Task 3: Implement payload parsing and verified extraction

**Files:**
- Create: `crates/talk-desktop/src/product_payload.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/Cargo.toml`
- Modify: `Cargo.toml`

- [x] **Step 1: Add the minimal public API**

Implement these signatures:

```rust
pub const TALK_PAYLOAD_MAGIC: &[u8; 8] = b"TLPAY001";

pub struct EmbeddedPayloadFile {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}

pub struct EmbeddedPayload {
    pub archive_sha256: [u8; 32],
    pub files: Vec<EmbeddedPayloadFile>,
    archive: Vec<u8>,
}

pub fn parse_embedded_payload(executable_bytes: &[u8]) -> Result<EmbeddedPayload, String>;
pub fn ensure_embedded_runtime(executable_path: &Path, runtime_root: &Path) -> Result<PathBuf, String>;
```

Use `sha2` for hashes and `zip` for archive reads. The parser must enforce the fixed trailer lengths, the expected five-member allowlist, UTF-8 relative paths, and per-file hashes.

- [x] **Step 2: Run the focused tests and verify GREEN**

Run:

```powershell
cargo test -p talk-desktop --test product_payload_contract
```

Expected: all payload tests pass, including traversal and corruption rejection.

- [x] **Step 3: Add extraction idempotency and atomic-install tests**

Assert that a valid payload extracts once to `<runtime-root>\\<payload-hash>`, returns the same path on the second call, and leaves no temporary directory after a successful or failed extraction.

- [x] **Step 4: Run payload tests again and commit**

```powershell
cargo test -p talk-desktop --test product_payload_contract
git add crates/talk-desktop/src/product_payload.rs crates/talk-desktop/src/lib.rs crates/talk-desktop/Cargo.toml Cargo.toml crates/talk-desktop/tests/product_payload_contract.rs
git commit -m "feat: extract verified embedded Talk runtime payload"
```

### Task 4: Establish failing model-bootstrap contracts

**Files:**
- Create: `crates/talk-desktop/tests/model_bootstrap_contract.rs`
- Modify: `crates/talk-desktop/src/lib.rs`

- [x] **Step 1: Write catalog and cache validation tests**

Tests must assert:

```rust
let spec = default_zipformer_model_spec();
assert_eq!(spec.id, "zipformer-zh-en-punct-int8-480ms");
assert_eq!(spec.sha256, "fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74");
assert!(spec.url.starts_with("https://"));
```

Add tests for a valid marker, missing required file, wrong marker digest, archive hash mismatch, and archive path traversal.

- [x] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test -p talk-desktop --test model_bootstrap_contract
```

Expected: compilation fails because the catalog and bootstrap functions do not exist.

- [x] **Step 3: Commit the failing model tests**

```powershell
git add crates/talk-desktop/tests/model_bootstrap_contract.rs crates/talk-desktop/src/lib.rs
git commit -m "test: define first-run Zipformer bootstrap contract"
```

### Task 5: Implement model catalog, download, verification, and atomic install

**Files:**
- Create: `crates/talk-desktop/src/model_bootstrap.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/Cargo.toml`
- Modify: `Cargo.toml`

- [x] **Step 1: Add the catalog and cache types**

Implement:

```rust
pub struct ModelSpec {
    pub id: String,
    pub url: String,
    pub archive_name: String,
    pub sha256: String,
    pub required_files: Vec<String>,
}

pub fn default_zipformer_model_spec() -> ModelSpec;
pub fn resolve_talk_data_root() -> Result<PathBuf, String>;
pub fn validate_installed_model(spec: &ModelSpec, model_dir: &Path) -> Result<(), String>;
```

Use `%LOCALAPPDATA%\\Talk` on Windows and return a clear error when the platform has no usable local-data directory.

- [x] **Step 2: Add the synchronous bootstrap worker with injected downloader**

Implement a testable function with an injected byte source:

```rust
pub fn install_model_from_reader<R: Read>(
    spec: &ModelSpec,
    reader: R,
    model_root: &Path,
) -> Result<PathBuf, String>;
```

The production path uses `reqwest` with rustls to stream into `.partial`, hashes the archive while writing, and invokes the same reader-based installer. Use `tar` plus `bzip2` in Rust; do not invoke `tar.exe` or PowerShell from the product executable.

- [x] **Step 3: Run model-bootstrap tests and verify GREEN**

Run:

```powershell
cargo test -p talk-desktop --test model_bootstrap_contract
```

Expected: catalog, hash, traversal, required-file, and atomic-install tests pass.

- [x] **Step 4: Add retry and cleanup tests, then commit**

Assert that a failed hash leaves only a removable `.partial` file, a successful install writes a marker with the catalog digest, and a second install reuses the validated directory.

```powershell
cargo test -p talk-desktop --test model_bootstrap_contract
git add crates/talk-desktop/src/model_bootstrap.rs crates/talk-desktop/src/lib.rs crates/talk-desktop/src/Cargo.toml Cargo.toml crates/talk-desktop/tests/model_bootstrap_contract.rs
git commit -m "feat: bootstrap and verify first-run Zipformer model"
```

### Task 6: Integrate product runtime and model bootstrap into the desktop shell

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-desktop/src/main.rs`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [x] **Step 1: Write failing path-resolution and fallback tests**

Add tests that sibling `talk.toml` wins over the old `talk-desktop.toml`, local ASR resolves models from `%LOCALAPPDATA%\\Talk\\models\\sherpa-onnx`, and a missing local model produces a cloud-fallback status rather than an unrecoverable startup error.

- [x] **Step 2: Implement runtime/model startup integration**

At desktop startup, resolve the product data root, ensure the embedded runtime payload before the first local-ASR session, and start model bootstrap on a worker thread. Store the latest bootstrap status in shared state. `ensure_packaged_local_asr_daemon` must use the extracted worker path rather than a release sibling `.internal` path.

The status messages must include `downloading`, `verifying`, `ready`, `fallback_cloud`, and `error` states. The worker launch remains hidden and uses the existing loopback endpoint validation.

- [x] **Step 3: Run desktop tests and commit**

```powershell
cargo test -p talk-desktop
git add crates/talk-desktop/src/lib.rs crates/talk-desktop/src/main.rs crates/talk-desktop/tests/desktop_contract.rs
git commit -m "feat: use embedded runtime and first-run model bootstrap"
```

### Task 7: Change the publisher to the product profile

**Files:**
- Modify: `scripts/Publish-TalkRelease.ps1`
- Modify: `scripts/Test-TalkReleaseManifest.ps1`
- Modify: `scripts/Test-TalkReleaseSummary.ps1`
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1`

- [x] **Step 1: Write the failing payload-builder tests**

Add a fixture test that invokes the payload builder with five known files and asserts that parsing the resulting `Talk.exe` trailer returns the same member list and hashes. Add a product publish test that asserts the destination contains exactly `Talk.exe` and `talk.toml`.

- [x] **Step 2: Implement payload append and product output**

Build the four workspace binaries as before, collect only the worker and four native DLLs into a temporary ZIP, append the ZIP and trailer to `talk-desktop.exe`, copy the result as `Talk.exe`, and write the generated default config as `talk.toml`.

Keep `manifest.json`, `release-summary.json`, `BUILD_INFO.txt`, and checksums under a sibling evidence directory such as `<release-root>\\_ci\\<version-id>` when `-EmitEvidence` is supplied. The product directory itself must not contain them.

- [x] **Step 3: Update validators for product manifests**

The product manifest must describe the two-file directory and the embedded payload hash without requiring `supportFiles`. Engineering manifests retain their current schema behind the explicit internal profile.

- [x] **Step 4: Run focused Pester tests and commit**

```powershell
Invoke-Pester -Script .\\scripts\\tests\\Publish-TalkRelease.Tests.ps1 -PassThru
git add scripts/Publish-TalkRelease.ps1 scripts/Test-TalkReleaseManifest.ps1 scripts/Test-TalkReleaseSummary.ps1 scripts/tests/Publish-TalkRelease.Tests.ps1
git commit -m "feat: publish Talk as single-exe product"
```

### Task 8: Update CI workflows and user documentation

**Files:**
- Modify: `.github/workflows/build-talk.yml`
- Modify: `.github/workflows/release-talk-tag.yml`
- Modify: `docs/LOCAL_SHERPA_MODELS.md`
- Modify: `docs/LOCAL_STREAMING_ASR_PROTOCOL.md`

- [x] **Step 1: Add failing workflow/documentation contract assertions**

Assert that workflows upload the product artifact separately from engineering evidence, and that user documentation describes `Talk.exe` first-run bootstrap rather than asking users to run a PowerShell installer.

- [x] **Step 2: Implement workflow and documentation updates**

Keep the existing native cache preparation and full CI verification. Upload the two-file product directory as the user artifact and upload evidence/engineering bundles as separate named artifacts.

- [x] **Step 3: Run workflow and documentation contract tests**

```powershell
Invoke-Pester -Script .\\scripts\\tests\\GitHub-Actions.Tests.ps1 -PassThru
git add .github/workflows/build-talk.yml .github/workflows/release-talk-tag.yml docs/LOCAL_SHERPA_MODELS.md docs/LOCAL_STREAMING_ASR_PROTOCOL.md
git commit -m "docs: describe single-exe first-run Talk distribution"
```

### Task 9: Full verification and clean-directory acceptance

**Files:**
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1` only if an observed regression requires a focused contract correction.

- [ ] **Step 1: Run the complete Rust and focused PowerShell suite**

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
Invoke-Pester -Script .\\scripts\\tests\\Publish-TalkRelease.Tests.ps1 -PassThru
Invoke-Pester -Script .\\scripts\\tests\\Invoke-TalkDesktopReleaseSmoke.Tests.ps1 -PassThru
Invoke-Pester -Script .\\scripts\\tests\\GitHub-Actions.Tests.ps1 -PassThru
```

Expected: every command exits 0 and Pester reports zero failures.

- [ ] **Step 2: Build and inspect the product release**

```powershell
.\\scripts\\Publish-TalkRelease.ps1 `
  -VersionId talk-single-exe-20260719-r1 `
  -ReleaseRoot C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro\\release\\Talk `
  -ProductProfile `
  -EmitEvidence
```

Assert:

```powershell
$product = 'C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro\\release\\Talk\\talk-single-exe-20260719-r1'
@((Get-ChildItem -LiteralPath $product -Recurse -File).FullName.Substring($product.Length).TrimStart('\\') | Sort-Object) |
  Should Be @('Talk.exe', 'talk.toml')
```

- [ ] **Step 3: Test embedded bootstrap on a clean cache**

Use a temporary `LOCALAPPDATA` directory, launch `Talk.exe` with a cloud-safe config, and verify that the runtime directory is created, the payload marker is valid, and the model bootstrap records either `ready` or an explicit `fallback_cloud` reason without leaving a corrupt final model directory.

- [ ] **Step 4: Commit, push Talk, wait for Actions, and update only the parent gitlink**

```powershell
git status --porcelain=v1
git push origin main
$parent='C:\\Users\\Public\\nas_home\\AI\\GameEditor\\Neuro'
git -C $parent add Talk
git -C $parent commit --only Talk -m "chore: update Talk submodule for single-exe product release"
```

The parent repository must not be pushed and no Hook path may appear in the parent commit.

## Plan Self-Review

- Spec coverage: product layout, embedded runtime payload, first-run model download, safe extraction, cloud fallback, CI separation, tests, and acceptance criteria are covered by Tasks 1 through 9.
- Placeholder scan: no unresolved placeholder marker or unspecified implementation step remains.
- Type consistency: payload APIs are defined once in Task 3 and reused by Tasks 7 and 9; `ModelSpec` and bootstrap APIs are defined in Task 5 and reused by Tasks 6 and 9.
- Scope: no static Sherpa relink is included; engineering bundle behavior remains available only behind an explicit internal path.
