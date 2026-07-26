# Talk Revision-Aware Correction Reconciliation — Design

Status: Implemented 2026-07-26 (runtime-level cut; see Limitations).

## Problem

`SpeculativeRuntimeState::runtime_tail_for_asr_source` reconciled a new ASR
hypothesis against the already-committed text with `strip_prefix`. When streaming
ASR **revised** earlier words (a non-monotonic hypothesis, e.g. committed
`我要去` then revised to `我想去北京`), the prefix no longer matched, the method
returned `None`, and the **entire revised hypothesis was silently dropped** until
the stop-time aggregate recovered it.

## Design (runtime-level, no new events)

Replaced the strip-prefix tail with `reconcile_source_for_segmentation`, which
classifies each hypothesis and returns the byte offset to segment from:

- **Unchanged** (`source == committed`, or `committed` starts with a shorter
  stale `source`) → `None`.
- **Append** (`source` extends `committed`) → committed prefix length; identical
  to the previous behaviour.
- **Revise** (divergence after a common prefix) →
  `invalidate_revised_source_segments`: using per-source committed-prefix-length
  boundaries (`cumulative_source_segment_boundaries`), it rolls back every
  sub-segment beyond the longest common prefix (removing them from
  `committed_segment_ids`, `segments`, `correction_requested_segment_ids`,
  resetting counts/boundaries), then returns the retained prefix end so the
  revised tail is re-segmented.

Because rolled-back sub-segments free their ids, the revised tail is **re-committed
under the same reused ids** (`seg-1`, `seg-1#2`, …). The desktop layer then does
the right thing almost entirely through existing consumers:

- HUD (`upsert_hud_streaming_segment`, keyed by id) updates the revised text in
  place.
- Live insertion (keyed by anchor id) skips already-inserted anchors — no
  double-insert into the target.
- The stop-seam (`desktop_streaming_stop_aggregate_with_pending`) still produces
  the authoritative final output.

When a revision segments into **fewer** pieces than before, the trailing
sub-segment ids have no replacement. `accept_asr_event_with_segmentation` emits a
`SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids }` for those true
orphans (ids rolled back and not re-committed/re-drafted this batch). The HUD
consumers call `remove_hud_streaming_segments` to drop them; the insert dispatch
ignores it (the target is reconciled at stop). Kept/re-committed segments retain
their position, so HUD ordering is preserved.

`longest_common_prefix_len` operates on char boundaries (never splits a UTF-8
codepoint).

## Tests (`crates/talk-runtime/tests/speculative_runtime_contract.rs`)

- Revised words re-committed under the same id (not dropped, not concatenated).
- Revision preserves an untouched shared-prefix segment; only the diverged clause
  is re-committed (reused id, correct `context_before`).
- UTF-8 boundary safety (`北京市` → `北海市`, divergence mid multi-byte prefix).
- Stale shorter-prefix hypothesis is ignored (no-op).
- All pre-existing append/de-dup/clause-split contract tests unchanged.

## Limitations (deferred follow-up)

One live-display edge case is intentionally left to the stop-seam (which already
yields correct final output):

1. A segment already **inserted into the target app** is not rewritten mid-stream
   on revision (reconciled at stop). Rewriting typed text would require caret
   tracking / deletion in the target window.

(The earlier HUD-orphan gap — a revision segmenting into fewer pieces leaving a
stale HUD segment — is now closed via `LocalSegmentsInvalidated`.)

## Verification

`cargo test -p talk-runtime` (22 green) and `cargo test -p talk-desktop --test
desktop_contract` (224 green, consumers unchanged). Accuracy impact (fewer dropped
revisions) is measurable via the CER harness on self-revising utterances.
