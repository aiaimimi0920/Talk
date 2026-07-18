# Talk Independent Repository Design

**Status:** Approved design for implementation

**Goal:** Extract the current `Talk` workspace from the Neuro monorepo into the public GitHub repository `aiaimimi0920/Talk`, mirror the repository-level operating model used by `aiaimimi0920/Hook`, and reconnect the parent Neuro repository through a standard Git submodule without changing Hook.

## Scope

- Operate on `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk` only, plus the parent Neuro index files required to replace the tracked directory with a submodule.
- Preserve the exact current Talk working tree snapshot, including tracked and untracked source, tests, scripts, documentation, examples, and release contracts.
- Do not modify `Neuro\Hook`, its checkout, its remote, its history, or its GitHub settings.
- Keep the recovery copy at `C:\Users\Public\nas_home\AI\GameEditor\_temp\talk` as the pre-migration source of truth.

## Repository Shape

The new repository will be public, use `main` as its default branch, and use the HTTPS remote `https://github.com/aiaimimi0920/Talk.git` without embedding credentials in Git configuration.

The initial repository commit will be a clean snapshot import, matching Hook's existing extraction style. It will not attempt to rewrite or preserve the approximately twenty historical Talk path commits from the Neuro monorepo. The current working tree, including the approved uncommitted five-mode and ASR work, will be included in that snapshot.

Talk will receive repository-local hygiene files:

- `.gitignore` for Rust `target`, `.runtime`, release output, model archives, local credentials, logs, and temporary files.
- `.gitattributes` with LF normalization for source/config/docs and CRLF preservation for PowerShell/Windows command scripts.
- `LICENSE` with the same MIT license family used by Hook, retaining the current Talk workspace license declaration.
- `README.md` updates that describe the standalone repository paths and GitHub Actions entry points without exposing provider credentials.

The parent Neuro repository will stop tracking Talk's individual files and will track one `160000` gitlink plus `.gitmodules`, matching Hook's existing submodule pattern.

## GitHub Actions

The new repository will contain two workflows modeled on Hook but adapted to the Rust workspace:

1. `build-talk.yml`
   - Trigger on pushes to `main` and manual dispatch.
   - Run on `windows-latest`.
   - Check out with `actions/checkout@v5`.
   - Install the pinned Rust toolchain used by the workspace.
   - Run `cargo fmt --check`, `cargo check --workspace --all-targets`, and `cargo test --workspace`.
   - Build `talk-desktop`, `talk-daemon`, `talk-local-asr-sherpa`, and `asr-bench` in release mode.
   - Upload the GUI executable and a credential-free portable verification artifact. CI must not run the live-provider path or package a provider API key.

2. `release-talk-tag.yml`
   - Trigger on tags matching `V*.*.*` and manual dispatch with an explicit tag input.
   - Re-run verification and build the Windows release package under `release/Talk/<tag>`.
   - Run the release manifest/summary validation and package-contract tests.
   - Publish the generated release zip as a GitHub Release asset with `softprops/action-gh-release@v3`.
   - Keep provider credentials external through `TALK_PROVIDER_API_KEY`; no key is written to the repository or release artifact by default.

Workflow permissions will be explicit: `contents: read` for build verification and `contents: write` only for tag release publication.

## Migration Sequence

1. Confirm the backup count, byte total, and key-file hashes.
2. Add repository hygiene/docs/workflows to Talk and validate them locally.
3. Initialize the standalone Talk repository, commit the snapshot, and push `main` to GitHub.
4. Configure public repository metadata and Actions settings to match Hook's public/main model.
5. Replace the parent Neuro `Talk` directory entry with the new Talk submodule at the pushed commit.
6. Verify the standalone checkout, parent submodule status, GitHub workflow files, remote refs, and backup recovery path.

No release package will be generated into the user's `release/Talk` directory unless explicitly needed by the release workflow validation; repository extraction itself only changes the Talk source directory and parent submodule metadata.

## Acceptance Criteria

- `https://github.com/aiaimimi0920/Talk` exists, is public, has `main` as its default branch, and contains the full current Talk snapshot.
- `git clone --recurse-submodules` of Neuro checks out Talk through the submodule and leaves Hook untouched.
- The Talk build workflow passes its formatting, check, test, and release-build gates on Windows.
- A `Vx.x.x` tag can publish a credential-free Windows release asset.
- No Git remote, workflow, config, documentation, or commit contains the supplied GitHub token or a provider API key.
- The original pre-migration snapshot remains restorable from `C:\Users\Public\nas_home\AI\GameEditor\_temp\talk`.

## Known Trade-offs

- Snapshot import sacrifices the old Talk path history in exchange for matching Hook's simpler extraction model and preserving the exact current state.
- GitHub-hosted CI can verify the software contract and build artifacts, but it cannot replace real microphone, foreground-focus, or provider-account smoke on the local Windows host.
- Local Sherpa model files remain operator-installed assets and are not checked into Git.
