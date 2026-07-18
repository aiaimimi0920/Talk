# Talk Sherpa Model Installer and Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Talk's real local streaming ASR path installable and testable from both a source checkout and a packaged desktop release.

**Architecture:** Keep large sherpa-onnx model assets out of the default release package, but ship a first-party installer that downloads or consumes an offline archive, validates the extracted model files, writes a ready-to-copy desktop config snippet, and is included beside `talk-desktop.exe`. The desktop release continues to boot in dry-run local ASR mode until the operator explicitly installs a real model and enables `[speculative.streaming_service.local_daemon]`.

**Tech Stack:** PowerShell 5/Pester release tests, Windows `tar.exe` archive extraction, existing Talk Rust desktop/daemon release publisher, sherpa-onnx streaming model catalog.

---

## File structure

- Create `Talk/scripts/Install-TalkSherpaModel.ps1`: model catalog, archive download/extraction, validation, and config snippet generation.
- Create `Talk/scripts/tests/Install-TalkSherpaModel.Tests.ps1`: unit-level tests for model catalog, validation, Paraformer/Transducer requirements, and `-Force` replacement behavior.
- Modify `Talk/scripts/Publish-TalkRelease.ps1`: copy `Install-TalkSherpaModel.ps1` into every desktop release and record it as a support file.
- Modify `Talk/scripts/tests/Publish-TalkRelease.Tests.ps1`: assert release packages contain the installer and manifest record.
- Create `Talk/docs/LOCAL_SHERPA_MODELS.md`: operator workflow for online and offline model installation.
- Modify `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md`: point real-engine users to the model installer and local model doc.

## Task 1: Build the explicit sherpa model installer

- [x] **Step 1: Add failing installer tests**

Write Pester coverage for:

- recommended model catalog entry `zipformer-zh-en-punct-int8-480ms`;
- extracted Transducer validation requiring `tokens`, `encoder`, `decoder`, and `joiner`;
- missing Transducer `joiner` rejection;
- Paraformer validation without `joiner`;
- `Install-TalkSherpaModel -Force -ArchivePath <archive> -SkipDownload` replacing an existing model directory.

Run:

```powershell
Invoke-Pester -Script .\Talk\scripts\tests\Install-TalkSherpaModel.Tests.ps1
```

Expected RED before implementation: missing script/functions; after adding the `-Force` test, stale installed files are incorrectly retained.

- [x] **Step 2: Implement installer**

Create `Install-TalkSherpaModel.ps1` with:

- `Get-TalkSherpaModelCatalog`;
- `Get-TalkSherpaModelSpec`;
- `Resolve-TalkSherpaDefaultModelRoot`;
- `Find-TalkSherpaModelFile`;
- `Test-TalkSherpaModelInstall`;
- `Expand-TalkSherpaModelArchive`;
- `Install-TalkSherpaModel`.

The installer must:

- default to `zipformer-zh-en-punct-int8-480ms`;
- support online download and offline `-ArchivePath`;
- use `.runtime\models\sherpa-onnx` under the source/release root;
- validate model files before reporting success;
- write `talk-local-daemon.toml.snippet`;
- replace the installed model when `-Force` is passed.

- [x] **Step 3: Verify installer tests**

Run:

```powershell
Invoke-Pester -Script .\Talk\scripts\tests\Install-TalkSherpaModel.Tests.ps1
```

Expected: 5 passed, 0 failed.

## Task 2: Package the installer in desktop releases

- [x] **Step 1: Add failing release test**

Extend `Talk/scripts/tests/Publish-TalkRelease.Tests.ps1` with a test that runs `Publish-TalkRelease -SkipSmoke` into a temp release root and asserts:

- `Install-TalkSherpaModel.ps1` exists in the release root;
- its text contains `function Install-TalkSherpaModel`;
- manifest `supportFiles` contains one `local-asr-model-installer` record.

- [x] **Step 2: Update release publisher**

Modify `Publish-TalkRelease.ps1` to:

- require `scripts\Install-TalkSherpaModel.ps1`;
- copy it to the release root;
- include a manifest support file record:

```powershell
[pscustomobject]@{
    kind = 'local-asr-model-installer'
    path = 'Install-TalkSherpaModel.ps1'
}
```

- [x] **Step 3: Verify release publisher tests**

Run:

```powershell
Invoke-Pester -Script .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected: all publish tests pass and support file count includes the installer.

## Task 3: Document operator workflow

- [x] **Step 1: Add local model installation doc**

Create `Talk/docs/LOCAL_SHERPA_MODELS.md` covering:

- source checkout install command;
- packaged release install command;
- built-in model catalog;
- offline archive installation;
- manual validation helper;
- why model installation is explicit.

- [x] **Step 2: Cross-link from streaming ASR protocol doc**

Update `Talk/docs/LOCAL_STREAMING_ASR_PROTOCOL.md` to point readers to:

- `Talk/scripts/Install-TalkSherpaModel.ps1`;
- `Talk/docs/LOCAL_SHERPA_MODELS.md`;
- remaining real-speech benchmarking as the next hardening step.

## Task 4: Validate and publish a new desktop release

- [x] **Step 1: Format Rust workspace**

Run:

```powershell
cargo fmt --manifest-path .\Talk\Cargo.toml --all
```

- [x] **Step 2: Run PowerShell focused tests**

Run:

```powershell
Invoke-Pester -Script .\Talk\scripts\tests\Install-TalkSherpaModel.Tests.ps1
Invoke-Pester -Script .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

- [x] **Step 3: Run Rust verification**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml --workspace
cargo check --manifest-path .\Talk\Cargo.toml --workspace --all-targets
git diff --check -- Talk
```

- [x] **Step 4: Build release package**

Run:

```powershell
.\Talk\scripts\Publish-TalkRelease.ps1 `
  -VersionId 'desktop-shell-sherpa-installer-v1' `
  -ReleaseRoot 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk' `
  -SkipVerification `
  -SkipSmoke `
  -SkipNativePreflight `
  -SkipNativeReadiness
```

- [x] **Step 5: Validate release artifacts**

Run:

```powershell
.\Talk\scripts\Test-TalkReleaseManifest.ps1 `
  -ManifestPath 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-sherpa-installer-v1\manifest.json'

Test-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-sherpa-installer-v1\talk-desktop.exe'
Test-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-sherpa-installer-v1\Install-TalkSherpaModel.ps1'
Test-Path 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-sherpa-installer-v1\.internal\talk-local-asr-sherpa.exe'
```

Expected: manifest validation exits 0 and all three file checks return `True`.

## Task 5: Next development milestone after this release

- [ ] **Step 1: Install the recommended real model**

Run the installer against `zipformer-zh-en-punct-int8-480ms` from either online download or a pre-downloaded archive.

- [ ] **Step 2: Run a real local ASR daemon probe**

Start `.internal\talk-local-asr-sherpa.exe` in `sherpa-online` mode using the generated snippet's model paths.

- [ ] **Step 3: Benchmark real speech**

Use `Talk/tools/asr-bench` and a short microphone recording set to record latency, RTF, memory, and rough recognition quality for the recommended model.

## Self-review

- Spec coverage: Covers explicit model install, validation, release packaging, documentation, and next real-speech validation milestone.
- Placeholder scan: No TBD/TODO placeholders are present.
- Type consistency: Script names, model IDs, manifest support kinds, and release paths match the current Talk workspace.
