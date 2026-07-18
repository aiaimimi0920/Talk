# Talk Single-EXE Product Release Design

## Status

Proposed design approved for implementation by the Talk owner on 2026-07-19.

## Problem

The current Talk release directory is an engineering bundle. It exposes the
desktop executable together with the local ASR daemon, native runtime DLLs,
benchmark tools, PowerShell helpers, probes, and release metadata. That layout
is useful for CI and operator validation, but it is not a user-facing product
package.

The desktop shell currently starts `talk-local-asr-sherpa.exe` as a local
streaming ASR worker. Sherpa-ONNX is linked in shared mode on Windows and needs
four native DLLs. The worker must remain isolated from the desktop process, but
those implementation details must not leak into the user-facing release.

## Goals

- Deliver exactly two user-visible files in the product directory:
  - `Talk.exe`
  - `talk.toml`
- Preserve the local-first streaming ASR path and its worker-process isolation.
- Embed the worker executable and its native DLLs inside `Talk.exe` as a
  verified payload.
- Extract the payload only to a per-user runtime cache under
  `%LOCALAPPDATA%\\Talk\\runtime`.
- Download the selected Zipformer model automatically on first startup when it
  is missing, with a pinned SHA-256 check and atomic installation.
- Keep cloud ASR usable when model download or local worker startup fails.
- Keep benchmark, probe, installer, smoke, and release-validation tools out of
  the user product directory.
- Keep all generated runtime state, downloaded archives, and logs out of the
  Git worktree.

## Non-goals

- This change does not statically link Sherpa-ONNX into the desktop process.
- This change does not embed the 163 MB model archive into the executable.
- This change does not make Paraformer an auto-downloaded first-run model.
- This change does not remove the engineering release/CI artifact path; it
  separates it from the user product path.

## Product Layout

The public product directory is intentionally minimal:

```text
Talk.exe
talk.toml
```

The directory must not contain any other executable, DLL, PowerShell script,
benchmark output, probe, manifest, checksum file, or release summary. CI may
retain a separate validation artifact containing those records.

The release binary is named `Talk.exe` for users. The Cargo target may remain
`talk-desktop.exe` internally; the publish step renames the final product file
without changing runtime path discovery.

The sibling configuration file is renamed to `talk.toml`. The desktop config
resolver continues to support `--config`, but its default lookup prefers the
`talk.toml` file next to the current executable.

## Embedded Runtime Payload

The release publisher builds the following internal payload from the already
validated release artifacts:

```text
talk-local-asr-sherpa.exe
sherpa-onnx-c-api.dll
sherpa-onnx-cxx-api.dll
onnxruntime.dll
onnxruntime_providers_shared.dll
```

The payload is stored as a ZIP archive appended to `Talk.exe`. A fixed trailer
contains:

- magic and format version;
- payload offset and byte length;
- payload SHA-256;
- a manifest of expected relative paths and per-file SHA-256 values.

The publisher writes the trailer only after the normal desktop executable has
been built. The runtime parser reads the trailer from `current_exe()`, verifies
the archive hash and manifest, and rejects malformed or truncated payloads.

Extraction behavior:

1. Resolve `%LOCALAPPDATA%\\Talk\\runtime\\<payload-sha256>`.
2. If the directory already contains a matching verified marker, reuse it.
3. Otherwise extract into a sibling temporary directory.
4. Reject absolute paths, `..` traversal, duplicate paths, and unexpected
   payload members.
5. Verify every extracted file hash.
6. Write a verified marker and atomically rename the temporary directory to the
   content-addressed runtime path.
7. Launch the worker from that cache path with hidden-window flags.

Stale content-addressed runtime directories may be garbage-collected only after
the worker is stopped and never while a current Talk process references them.

## First-Run Model Bootstrap

The first release model is the evidence-selected
`zipformer-zh-en-punct-int8-480ms` model. Its catalog entry is compiled into
the desktop product and includes:

- the existing sherpa-onnx GitHub release URL;
- archive name;
- model ID and family;
- required files;
- the pinned SHA-256
  `fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74`.

The model cache is:

```text
%LOCALAPPDATA%\\Talk\\models\\sherpa-onnx\\zipformer-zh-en-punct-int8-480ms
```

On startup, Talk validates the model marker and required files. If the model
is absent or invalid, Talk performs the following sequence without exposing a
PowerShell installer:

1. Create a `.partial` archive in the per-user download cache.
2. Download over HTTPS using the existing Rust HTTP stack.
3. Verify the complete archive SHA-256 before extraction.
4. Decompress the `tar.bz2` archive in Rust into a temporary model directory.
5. Reject traversal and unexpected extraction paths.
6. Verify `tokens.txt`, encoder, decoder, and joiner files and write a model
   manifest containing the catalog ID and digest.
7. Atomically rename the verified model directory into the model cache.
8. Remove the `.partial` archive only after successful installation.

An interrupted or hash-mismatched download is never treated as installed. A
subsequent startup may retry it. The UI/tray status reports download progress,
verification failure, and retry state without blocking the Windows message
loop.

If download, extraction, or local worker startup fails, Talk records the reason,
keeps the cloud provider route available, and does not crash the desktop shell.
The local ASR route is retried on the next session after the user has network
connectivity or the model cache has been repaired.

Paraformer remains supported by the existing engineering installer and config
paths, but it is not automatically downloaded until a pinned archive digest is
available and its product policy is explicitly approved.

## Configuration

The packaged `talk.toml` keeps the current five-mode and Smart defaults. The
local streaming service remains the preferred speculative route, but model
paths are resolved from the per-user model cache rather than from a release
sibling `.runtime` directory. The config may still explicitly override a model
root for engineering and diagnostic use.

Provider credentials are not embedded in the product release. The config uses
the existing environment-based provider key behavior.

## Release and CI Separation

`Publish-TalkRelease.ps1` gains a product profile as the default behavior:

- build and append the runtime payload;
- write only `Talk.exe` and `talk.toml` to the product directory;
- write validation metadata to a separate CI/evidence directory when requested;
- never copy PowerShell scripts, benchmark executables, probes, model
  installers, or release metadata into the product directory.

The existing engineering bundle behavior may remain available behind an
explicit internal switch, but it must not be the default and must not be used
for the user release path.

## Testing Strategy

### Unit and contract tests

- Payload trailer parsing accepts a valid archive and rejects truncation,
  malformed lengths, bad archive hashes, path traversal, duplicate members,
  and unexpected files.
- Runtime extraction is idempotent and content-addressed.
- Model catalog entries require HTTPS URLs, a 64-character SHA-256, and the
  required model file set.
- Model installation rejects bad hashes, incomplete archives, and traversal.
- Default config resolution prefers sibling `talk.toml`.
- The product publisher emits exactly two files and one visible executable.
- The publisher keeps engineering tools in the separate CI artifact path.

### Verification commands

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo test --workspace
Invoke-Pester .\\scripts\\tests\\Publish-TalkRelease.Tests.ps1
Invoke-Pester .\\scripts\\tests\\Invoke-TalkDesktopReleaseSmoke.Tests.ps1
```

The release acceptance check must assert:

- `Talk.exe` exists;
- `talk.toml` exists;
- no other files exist in the product directory;
- no other `.exe`, `.dll`, or `.ps1` exists in the product directory;
- the embedded payload extracts and verifies on a clean runtime cache;
- a valid cached model starts the local worker;
- an absent model enters download/fallback state without crashing.

## Risks and Mitigations

- **Native payload corruption:** content-addressed extraction plus per-file
  hashes and atomic rename.
- **Model supply-chain drift:** pinned HTTPS URL and SHA-256; no automatic
  download for catalog entries without a digest.
- **First-run latency:** download asynchronously and keep cloud route usable.
- **Disk growth:** content-addressed cache with conservative stale-version
  cleanup and explicit retry-safe partial files.
- **Worker crash:** preserve process isolation and report local ASR failure to
  the desktop status surface.
- **Release regression:** product-layout contract tests run separately from
  engineering-bundle tests.

## Acceptance Criteria

The implementation is complete when a clean Windows directory containing only
`Talk.exe` and `talk.toml` can:

1. start without PowerShell, benchmark, probe, or sibling helper files;
2. extract and verify the embedded ASR worker payload;
3. download and verify the pinned Zipformer archive when the model is absent;
4. start local streaming ASR after a successful first-run install;
5. fall back to the configured cloud route if download or local startup fails;
6. pass all Rust and focused PowerShell contract tests.
