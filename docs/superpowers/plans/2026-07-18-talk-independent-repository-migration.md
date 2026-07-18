# Talk Independent Repository Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish the current Talk workspace as the public standalone repository `aiaimimi0920/Talk`, add Hook-style Windows build and tag-release Actions, and reconnect Neuro through a standard Talk submodule without modifying Hook.

**Architecture:** Treat the current Talk working tree as an approved snapshot import. First make Talk self-contained and safe to publish, then initialize and push its independent Git repository, and only after the remote commit exists replace the parent Neuro file tree with a `160000` gitlink. GitHub credentials remain process-local and never enter files, remotes, commits, or workflow configuration.

**Tech Stack:** Git and Git submodules, GitHub REST API, GitHub Actions on `windows-latest`, Rust 1.95.0, Cargo workspace validation, PowerShell/Pester release contracts.

---

### Task 1: Preserve and verify the recovery snapshot

**Files:**
- Recovery copy: `C:\Users\Public\nas_home\AI\GameEditor\_temp\talk`
- Source: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk`

- [x] **Step 1: Copy the complete working tree**

Copy all source, untracked work, build outputs, and runtime evidence with `robocopy /E /COPY:DAT /DCOPY:DAT /XJ`.

- [x] **Step 2: Verify inventory and hashes**

Confirmed on 2026-07-18:

```text
files:       62080 == 62080
directories: 7842 == 7842
bytes:       30338834400 == 30338834400
```

SHA-256 matched for `Cargo.toml`, `Cargo.lock`, `README.md`, core/desktop source, the release publisher, and the active five-mode plan.

### Task 2: Make release tooling work from a standalone repository root

**Files:**
- Modify: `scripts/Publish-TalkRelease.ps1`
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1`

- [x] **Step 1: Add a failing standalone-layout contract**

Add a Pester test that creates a temporary standalone Talk-shaped directory and requires the release command context to resolve:

```text
workingDirectory = <standalone Talk root>
manifestPath = Cargo.toml
repositoryRoot = <standalone Talk root>
```

Keep the existing monorepo contract:

```text
workingDirectory = <Neuro root>
manifestPath = Talk/Cargo.toml
repositoryRoot = <Neuro root>
```

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected before implementation: FAIL because `Get-NeuroRoot`, verification commands, and the release build command assume `Talk/Cargo.toml` under a parent repository.

- [x] **Step 2: Add a typed release command context helper**

Implement a helper that detects the monorepo only when the parent contains both `.git` and `Talk/Cargo.toml`; otherwise it treats the Talk root as the repository root:

```powershell
[pscustomobject]@{
    RepositoryRoot = $repositoryRoot
    WorkingDirectory = $workingDirectory
    ManifestPath = $manifestPath
}
```

Generate Cargo commands from this context instead of hard-coding `Talk/Cargo.toml`.

- [x] **Step 3: Update build and manifest metadata paths**

Use the resolved context for:

- verification command working directories;
- the release `cargo build` command;
- `manifest.json.repoRoot`;
- `BUILD_INFO.txt` command history.

Do not change the existing release artifact layout.

- [x] **Step 4: Verify both layouts**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
cargo fmt --manifest-path .\Cargo.toml --all -- --check
```

Expected: all publisher tests pass and PowerShell edits do not alter Rust formatting.

### Task 3: Add standalone repository hygiene and public documentation

**Files:**
- Create: `.gitignore`
- Create: `.gitattributes`
- Create: `LICENSE`
- Modify: `README.md`

- [x] **Step 1: Add repository-local ignore rules**

Ignore at minimum:

```gitignore
target/
.runtime/
release/
*.log
tmp-*
tmp/
.vscode/
.idea/
.DS_Store
Thumbs.db
*.orig
*.rej
*.swp
*.swo
**/manual-live.json
**/api-key/*.json
*.onnx
*.onnx.data
*.tar.bz2
```

Keep `Cargo.lock`, source, scripts, contracts, examples, benchmark JSON reports, and docs tracked.

- [x] **Step 2: Add line-ending and binary rules**

Use Hook's conventions with Talk-specific additions:

```gitattributes
* text=auto
*.md text eol=lf
*.json text eol=lf
*.toml text eol=lf
*.lock text eol=lf
*.yml text eol=lf
*.yaml text eol=lf
*.rs text eol=lf
*.ps1 text eol=crlf
*.bat text eol=crlf
*.cmd text eol=crlf
*.exe binary
*.dll binary
*.wav binary
*.onnx binary
```

- [x] **Step 3: Add the MIT license**

Use the same 2026 `yamiyu` MIT license identity as Hook, consistent with `workspace.package.license = "MIT"`.

- [x] **Step 4: Normalize public README commands**

Make standalone commands executable from the Talk repository root:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
cargo run -p talk-daemon -- check --config .\examples\dev-config.toml
```

Remove command-only `Talk/` prefixes, replace machine-specific release examples with neutral paths, add repository/Actions badges, and state that provider credentials and local ASR models are never bundled by default.

- [x] **Step 5: Scan public files for machine paths and credentials**

Run fixed-string and token-pattern scans outside ignored build/runtime directories. Expected:

- no GitHub token prefix;
- no non-placeholder provider key;
- no credential JSON;
- no generated release TOML containing `provider.api_key`.

### Task 4: Add Hook-style GitHub Actions adapted for Talk

**Files:**
- Create: `.github/workflows/build-talk.yml`
- Create: `.github/workflows/release-talk-tag.yml`
- Create: `scripts/tests/GitHub-Actions.Tests.ps1`

- [x] **Step 1: Write failing workflow contract tests**

Require both workflow files, pinned action versions, Windows runners, Rust 1.95.0, explicit permissions, Talk verification commands, credential-free publisher invocation, artifact upload, tag validation, and GitHub Release publication.

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\GitHub-Actions.Tests.ps1
```

Expected before workflow creation: FAIL because the files do not exist.

- [x] **Step 2: Create the main build workflow**

`build-talk.yml` will:

```text
push main / workflow_dispatch
windows-latest
checkout@v5
dtolnay/rust-toolchain@1.95.0
Swatinem/rust-cache@v2
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
Publish-TalkRelease.ps1 with SkipVerification, SkipSmoke,
SkipNativePreflight, SkipNativeReadiness and an explicit repo-local ReleaseRoot
upload-artifact@v6
```

The workflow clears `TALK_PROVIDER_API_KEY` before packaging and asserts generated `talk-desktop.toml` contains `api_key_env`, not `api_key`.

- [x] **Step 3: Create the tag release workflow**

`release-talk-tag.yml` will:

- accept pushed or manually supplied tags matching `V\d+\.\d+\.\d+`;
- install/import Pester 5 for script contracts;
- run publisher and Actions contract tests;
- create `talk-windows-x64-<tag>.zip`;
- validate manifest and summary;
- assert no packaged provider key;
- publish with `softprops/action-gh-release@v3` and `contents: write`.

- [x] **Step 4: Verify workflow contracts**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\GitHub-Actions.Tests.ps1
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected: all tests pass.

### Task 5: Run the complete standalone source validation

**Files:**
- Existing Talk files only.

- [x] **Step 1: Rust formatting and compile validation**

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
```

- [x] **Step 2: Workspace tests**

```powershell
cargo test --workspace
```

Expected: current baseline remains at least 436 passing Rust tests with zero failures.

- [x] **Step 3: PowerShell contract validation**

```powershell
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1
Invoke-Pester -Path .\scripts\tests\Invoke-TalkDesktopReleaseSmoke.Tests.ps1
Invoke-Pester -Path .\scripts\tests\GitHub-Actions.Tests.ps1
```

- [x] **Step 4: Dry-run standalone release packaging**

Build a credential-free package under the permitted Talk release root with GUI/native smoke disabled for CI parity, validate its manifest/summary, then confirm the generated config uses `api_key_env` only.

### Task 6: Initialize and commit the standalone Talk repository

**Files:**
- Create inside Talk: `.git/`
- Track: repository source selected by `.gitignore`

- [ ] **Step 1: Initialize `main` and inspect the candidate index**

```powershell
git init -b main
git add --all
git status --short
git diff --cached --stat
```

Confirm `target`, `.runtime`, generated releases, model binaries, and credential files are absent.

- [ ] **Step 2: Scan the exact staged content**

Use `git grep --cached` and blob-size checks. Reject the commit if it contains:

- the supplied GitHub token;
- provider keys;
- credential JSON;
- files larger than GitHub's 100 MiB limit;
- generated EXE/DLL/model archives.

- [ ] **Step 3: Create the snapshot import commit**

```powershell
git commit -m "feat: import Talk standalone voice input app"
```

Set remote without credentials:

```powershell
git remote add origin https://github.com/aiaimimi0920/Talk.git
```

### Task 7: Create and configure the GitHub repository

**Files:**
- GitHub repository: `aiaimimi0920/Talk`

- [ ] **Step 1: Read authenticated Hook settings**

Use the supplied token only in the current PowerShell process to read Hook merge, Actions, and branch-protection settings. Do not print or persist the token.

- [ ] **Step 2: Create the public repository**

Create through `POST /user/repos` with:

```json
{
  "name": "Talk",
  "description": "Talk standalone voice input and speech interaction app extracted from the Neuro monorepo.",
  "private": false,
  "has_issues": true,
  "has_projects": true,
  "has_wiki": false,
  "has_discussions": false,
  "auto_init": false
}
```

- [ ] **Step 3: Mirror Hook repository settings**

Apply Hook's merge settings, Actions policy, workflow permissions, default branch behavior, and any branch protection that exists and is supported by the account plan.

- [ ] **Step 4: Push without credential persistence**

Use a process-local HTTP Authorization header for:

```powershell
git push -u origin main
```

Verify `git remote -v` contains only the clean HTTPS URL afterward.

### Task 8: Convert Neuro/Talk into a standard submodule

**Files:**
- Modify in parent: `.gitmodules`
- Replace parent index entries: `Talk/**` -> `160000 Talk`
- Move Talk Git metadata under: `.git/modules/Talk`

- [ ] **Step 1: Record the pushed Talk commit**

Confirm local `Talk/main`, `origin/main`, and the GitHub API all resolve to the same SHA.

- [ ] **Step 2: Replace parent tracked files with a gitlink**

From the Neuro parent:

```powershell
git rm -r --cached -- Talk
```

Add the Talk entry to `.gitmodules`, add a `160000` index entry at the pushed SHA, configure `submodule.Talk.url` and `submodule.Talk.active`, then run:

```powershell
git submodule absorbgitdirs Talk
```

- [ ] **Step 3: Verify Hook was untouched**

Compare Hook's worktree status, HEAD, remote, `.git` indirection, and parent gitlink to the pre-migration evidence. No Hook value may change.

- [ ] **Step 4: Commit only the parent integration**

Create a local parent commit containing only `.gitmodules` and the Talk directory-to-gitlink conversion:

```powershell
git commit --only .gitmodules Talk -m "chore: extract Talk as standalone submodule"
```

Do not push the parent Neuro branch because it already contains unrelated unpushed commits.

### Task 9: End-to-end verification

**Files:**
- Local Talk repository
- Parent Neuro repository
- GitHub Talk repository
- Backup directory

- [ ] **Step 1: Verify local standalone repository**

```powershell
git -C Talk status --short --branch
git -C Talk remote -v
git -C Talk fsck --full
```

- [ ] **Step 2: Verify submodule shape**

```powershell
git ls-files -s Talk
git submodule status Talk
git config --get-regexp '^submodule\.Talk\.'
```

Expected: mode `160000`, matching SHA, clean Talk worktree, and clean HTTPS URL.

- [ ] **Step 3: Verify GitHub and Actions**

Use the public API to confirm repository visibility, default branch, workflow discovery, and the first workflow run conclusion. If the workflow is still running, poll until it completes.

- [ ] **Step 4: Reconfirm recovery snapshot**

Confirm the backup remains present with the previously verified file count and byte total. Do not delete or modify it.

- [ ] **Step 5: Security closeout**

Confirm no token appears in Git config, staged content, commit history, workflow files, or release config. Advise rotating the user-supplied GitHub token because it was shared in conversation.
