# Talk Code Review And Optimization Plan

Date: 2026-08-15

## Scope

This review covers the current Talk worktree and preserves the standalone Talk
repository boundary. It targets correctness, bounded resource use, startup and
streaming overhead, release safety, and maintainability. It does not alter the
two-file product contract (`Talk.exe` plus `talk.toml`) or claim recognition
quality improvements without a valid real-microphone corpus.

## Baseline

- Repository: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk`
- Branch: `talk-accuracy-20260726`
- Reviewed HEAD: `80d5d092c41116921136b8520ed4c81f740cf7e5`
- The worktree already contained extensive uncommitted work before this review;
  those changes must be preserved.
- Initial `cargo fmt --all -- --check` and
  `cargo check --workspace --all-targets` passed.

## Review Decisions

| Priority | Finding | Decision |
| --- | --- | --- |
| P0 | An arbitrary release `VersionId` could escape `ReleaseRoot` before a recursive delete. | Validate a single safe path component and independently enforce child-path containment. |
| P1 | Concurrent model bootstrap operations shared one partial archive and could replace or remove a valid model directory. | Use unique staging paths, preserve a valid installation, converge concurrent activations, and clean partial files on every exit path. |
| P1 | Adjacent repeated partials or revised finals for one streaming ASR segment occupied duplicate retained history entries. | Coalesce same-kind adjacent updates in O(1) while preserving the public partial-to-final event boundary. |
| P1 | The local capability server had no bound on accepted connections or blocking HTTP readers. | Acquire a bounded connection permit before `accept` and hold it for the complete request. |
| P2 | Rust 1.95 Clippy identified avoidable conversions, manual range/divisibility checks, redundant borrows, and verbose option handling. | Apply semantics-preserving fixes and run Clippy with only the existing high-arity runtime API lint exempted. |

## Findings Not Changed In This Pass

- Distinct final ASR segments are retained because rebuilding the complete
  transcript requires their text. This is necessary transcript state, not
  duplicate event history; silently evicting it would truncate long dictation.
- The Windows device-selection `expect` calls follow a helper that returns
  `Some` only for a name selected from the same in-memory device list. No
  device-enumeration race exists at that boundary, so replacing them would not
  fix a demonstrated failure.
- `audio.max_recording_seconds = 0` is the packaged unlimited-recording contract,
  not an invalid configuration.
- Unauthenticated health/capability reads remain loopback-only discovery
  endpoints; invocation continues to require the manifest bearer token.
- The tag workflow publishes the two-file ProductProfile. An engineering-bundle
  `.internal` ZIP concern does not apply to that product workflow.

## Execution Plan

- [x] Add release identifier validation, containment checks, and Pester coverage.
- [x] Make model installation idempotent and concurrency-safe, with catalog path
  validation and deterministic regression tests.
- [x] Coalesce common streaming ASR revision sequences and add a long revision
  flood regression test.
- [x] Bound capability-server connection tasks.
- [x] Resolve actionable Rust 1.95 Clippy findings without changing behavior.
- [x] Run focused tests, full format/check/test gates, and release-script tests.
- [x] Freeze the reviewed source snapshot before publication.

Release publication, independent payload verification, and packaged smoke are
recorded in the external `_ci` evidence and final report. The source plan is not
edited after publication so the captured source snapshot remains exact.

## Acceptance Gates

1. `cargo fmt --manifest-path Cargo.toml --all -- --check`
2. `cargo check --workspace --all-targets`
3. `cargo test --workspace`
4. `cargo clippy --workspace --all-targets -- -D warnings -A clippy::too_many_arguments`
5. `scripts/tests/Publish-TalkRelease.Tests.ps1`
6. Fresh `Publish-TalkRelease.ps1 -ProductProfile -EmitEvidence`
7. Independent `Test-TalkProductRelease.ps1`
8. Exact payload is only `Talk.exe` and `talk.toml`, with no directories
9. Packaged desktop smoke succeeds and no `Talk` process remains

The high-arity runtime entrypoints remain explicit technical debt. Refactoring
them into request/context objects is a separate API migration, not a safe
mechanical change for this optimization pass.
