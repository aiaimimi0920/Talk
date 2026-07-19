# Talk Runtime Credential Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make direct `Talk.exe` launches reuse the current user's DashScope credential file while preserving explicit configuration, local fallback, and credential-free release artifacts.

**Architecture:** Add a small credential resolver inside `talk-runtime`. It evaluates explicit config, the configured environment variable, and the legacy per-user JSON file through one shared function; the JSON fallback is restricted to DashScope endpoints. Provider construction and cloud-availability checks call the same resolver, while local transcript behavior remains unchanged when no credential is available.

**Tech Stack:** Rust workspace, `serde_json`, `reqwest::Url`, Tokio runtime tests, PowerShell product publisher.

---

### Task 1: Add failing credential resolver contracts

**Files:**
- Create: `crates/talk-runtime/src/credentials.rs`
- Modify: `crates/talk-runtime/src/lib.rs`

- [x] **Step 1: Add isolated resolver tests**

Declare `mod credentials;` in `lib.rs`. Create `credentials.rs` with a `#[cfg(test)]` module that imports the not-yet-implemented `resolve_provider_credential_with`, `ProviderCredential`, and `ProviderCredentialSource`. The test helper must write JSON to a unique `std::env::temp_dir().join("talk-runtime-credentials").join(Uuid::new_v4().to_string())` path and construct a valid OpenAI-compatible `TalkConfig` through `TalkConfig::from_toml_str`.

The first RED tests use this API:

```rust
fn resolve_provider_credential_with<F>(
    config: &TalkConfig,
    env_lookup: F,
    legacy_json_path: Option<&Path>,
) -> ProviderCredential
where
    F: Fn(&str) -> Option<String>;

enum ProviderCredentialSource {
    ExplicitConfig,
    Environment,
    LegacyJson,
    Unavailable,
}
```

Add assertions equivalent to:

```rust
let credential = resolve_provider_credential_with(
    &config,
    |name| (name == "TALK_PROVIDER_API_KEY").then(|| "environment-key".to_string()),
    Some(&legacy_path),
);
assert_eq!(credential.source(), ProviderCredentialSource::ExplicitConfig);
assert_eq!(credential.api_key(), Some("configured-key"));
```

Repeat with `provider.api_key = None` to prove the environment wins, then with an empty environment closure to prove each of `apiKey`, `api_key`, and `key` can supply `LegacyJson`. Add separate assertions that malformed JSON, blank/padded values, and a config whose endpoints use `https://example.invalid` return `Unavailable`. Tests must never format or print the credential object.

- [x] **Step 2: Run the focused tests and verify RED**

Run:

```powershell
cargo test -p talk-runtime credentials::tests -- --nocapture
```

Expected: compilation/test failure because the resolver test API and JSON fallback do not exist yet.

- [x] **Step 3: Commit the failing contract tests**

```powershell
git add crates/talk-runtime/src/credentials.rs crates/talk-runtime/src/lib.rs
git commit -m "test: define runtime credential discovery contract"
```

### Task 2: Implement one shared runtime resolver

**Files:**
- Modify: `crates/talk-runtime/src/credentials.rs`
- Modify: `crates/talk-runtime/src/lib.rs`

- [x] **Step 1: Implement source and resolver types**

Implement the types without deriving `Debug` for `ProviderCredential`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderCredentialSource {
    ExplicitConfig,
    Environment,
    LegacyJson,
    Unavailable,
}

pub(crate) struct ProviderCredential {
    source: ProviderCredentialSource,
    api_key: Option<String>,
}

impl ProviderCredential {
    pub(crate) fn source(&self) -> ProviderCredentialSource {
        self.source
    }

    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub(crate) fn into_api_key(self) -> Option<String> {
        self.api_key
    }
}
```

- [x] **Step 2: Implement precedence and JSON parsing**

Implement `resolve_provider_credential_with` so `valid_key` accepts only nonblank strings whose trimmed value equals the original. Resolve explicit config, then the configured environment lookup, then the legacy file. Parse the legacy file with `serde_json::Value`, look up `apiKey`, `api_key`, and `key` in that order, and return `Unavailable` for every file or parse error.

Gate the legacy fallback with:

```rust
fn is_dashscope_endpoint(value: Option<&str>) -> bool {
    let Some(value) = value else { return false };
    let Ok(url) = reqwest::Url::parse(value) else { return false };
    url.scheme() == "https" && url.host_str() == Some("dashscope.aliyuncs.com")
}
```

Require both `audio_transcriptions_endpoint` and `chat_completions_endpoint` to pass. Production `resolve_provider_credential` obtains the per-user path from `USERPROFILE`, falling back to `HOME`, and appends `.neuro/qwen-platform/qwen-dashscope-openai/api-key/manual-live.json`.

- [x] **Step 3: Connect all provider paths to the resolver**

Import `resolve_provider_credential` into `lib.rs`. Replace `provider_text_processing_credentials_available` with:

```rust
pub fn provider_text_processing_credentials_available(config: &TalkConfig) -> bool {
    match config.provider.kind {
        ProviderKind::Mock | ProviderKind::Http => true,
        ProviderKind::OpenAiCompatible => resolve_provider_credential(config).api_key().is_some(),
    }
}
```

Replace the private environment-only resolver with:

```rust
fn resolve_provider_api_key(config: &TalkConfig) -> Option<String> {
    resolve_provider_credential(config).into_api_key()
}
```

Remove `?` from the two provider constructor calls because resolution no longer fails when a source is missing or malformed.

- [x] **Step 4: Run the focused tests and verify GREEN**

Run:

```powershell
cargo test -p talk-runtime credentials::tests -- --nocapture
```

Expected: all credential resolver tests pass.

- [x] **Step 5: Commit the resolver**

```powershell
git add crates/talk-runtime/src/credentials.rs crates/talk-runtime/src/lib.rs
git commit -m "feat: discover runtime DashScope credentials"
```

### Task 3: Preserve no-credential and release contracts

**Files:**
- Modify: `crates/talk-runtime/tests/runtime_contract.rs`
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1` only if the existing contract lacks the exact product assertion

- [x] **Step 1: Add a no-credential local transcript regression test**

Retain the existing `local_transcript_completes_without_openai_credentials` test. Add one availability assertion using non-DashScope endpoints and no explicit/env key:

```rust
config.provider.audio_transcriptions_endpoint =
    Some("https://example.invalid/v1/audio/transcriptions".to_string());
config.provider.chat_completions_endpoint =
    Some("https://example.invalid/v1/chat/completions".to_string());
assert!(!provider_text_processing_credentials_available(&config));
```

This proves ambient local DashScope state cannot silently make arbitrary providers appear credentialed.

- [x] **Step 2: Run runtime tests and verify GREEN**

Run:

```powershell
cargo test -p talk-runtime --test runtime_contract
```

Expected: all runtime contract tests pass.

- [x] **Step 3: Verify product package shape**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1 -Output Detailed
```

Expected: the product profile still emits exactly `Talk.exe` and `talk.toml`, and generated config contains `api_key_env` but no inline `api_key =`.

- [x] **Step 4: Commit regression coverage**

```powershell
git add crates/talk-runtime/tests/runtime_contract.rs scripts/tests/Publish-TalkRelease.Tests.ps1
git commit -m "test: preserve credential-free local transcript fallback"
```

### Task 4: Build and validate the standalone product

**Files:**
- Modify: `docs/LOCAL_SHERPA_MODELS.md` only if direct-launch credential behavior is undocumented
- Generate: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r4\Talk.exe`
- Generate: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r4\talk.toml`

- [x] **Step 1: Run workspace verification**

Run:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
```

Expected: all commands exit zero.

- [x] **Step 2: Publish the credential-free product**

Run:

```powershell
.\scripts\Publish-TalkRelease.ps1 `
  -VersionId 'talk-single-exe-20260719-r4' `
  -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
  -ProductProfile `
  -DisablePackagedApiKeyDiscovery `
  -SkipSmoke `
  -SkipNativePreflight `
  -SkipNativeReadiness
```

Do not pass or print `-PackagedApiKey` or `-PackagedApiKeyJsonPath`.

- [x] **Step 3: Validate the generated package**

Run:

```powershell
$product = 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-single-exe-20260719-r4'
.\scripts\Test-TalkProductRelease.ps1 -ProductPath $product
$files = Get-ChildItem -LiteralPath $product -File | Sort-Object Name
if (($files.Name -join ',') -ne 'Talk.exe,talk.toml') { throw 'Unexpected product files' }
$config = Get-Content -Raw -LiteralPath (Join-Path $product 'talk.toml')
if ($config -match '(?m)^api_key\s*=') { throw 'Product config contains an inline API key' }
```

- [x] **Step 4: Run direct-launch smoke**

Stop only the running process whose executable path is inside the old Talk `r2` release. Start `r4\\Talk.exe` with no PowerShell key injection. Trigger one real Alt session and inspect only the newest session JSON fields `status`, `transcript`, `output_text`, and `error`. Expected: cloud processing succeeds using the existing per-user JSON credential; if the external provider is unavailable, the session still completes with the local transcript rather than failing for a missing environment variable.

### Task 5: Publish Talk repository changes

**Files:**
- Modify: Talk repository files only

- [x] **Step 1: Review secret safety and repository status**

Run `git diff --check` and `git status --short`. Read the local credential value only into a PowerShell variable, then use `git grep -l -F -- $secret` and report only whether the match count is zero; never print the secret or matching file content. Confirm generated release files are outside the Talk repository.

- [x] **Step 2: Push Talk main**

Push the implementation commits to `origin/main` without changing Hook or pushing the parent Neuro repository.

- [x] **Step 3: Verify GitHub Actions**

## Completion Evidence

- TDD RED: `cargo test -p talk-runtime credentials::tests -- --nocapture` failed because the resolver API was intentionally absent.
- Runtime GREEN: six credential tests and all `talk-runtime` contracts passed.
- Workspace verification: `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, and `cargo test --workspace` exited zero.
- Release contract: Pester reported 48 passed and zero failed; `r4` contains exactly `Talk.exe` and `talk.toml` with no inline API key.
- Live provider proof: with `TALK_PROVIDER_API_KEY` unset, a real recorded WAV completed DashScope ASR and text processing as session `781382b2-4894-48df-ac92-63fd283d2f5f`, status `completed`, a recognized Chinese greeting output, and no error.
- GitHub proof: Build Talk run `29679976207` completed successfully for commit `27a77bc8424fcdb9b4a93f6806f7a3ae7004d67f`.

Confirm the Talk build workflow completes successfully for the pushed commit and record the run URL without exposing credentials.
