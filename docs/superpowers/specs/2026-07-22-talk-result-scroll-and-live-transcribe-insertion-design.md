# Talk Result Scroll And Live Transcribe Insertion Design

## Goal

Fix two desktop transcription workflow defects without weakening foreground-target safety:

1. Long text in the post-session result popup must remain readable through a visible vertical scrollbar.
2. Corrected white transcript segments must be inserted into the recording-origin input target for explicit Transcribe sessions and Smart sessions that are safely resolved as Transcribe.

## Confirmed Evidence

The result popup uses native multiline EDIT controls with `ES_AUTOVSCROLL`, but the controls do not have `WS_VSCROLL`. Text can overflow internally, while the user has no visible scrollbar to drag.

The latest r9 insert-target diagnostics show a Smart session resolved to Transcribe but finished with `show_copy_popup_only`. The pre-trigger Chrome target contained only a top-level window handle. The release/current snapshot identified an editable Chrome control with a UI Automation Runtime ID, TextPattern, and ValuePattern. The current resolver always retained the weaker pending snapshot, so later same-control validation could not succeed.

Live correction also lacks a requested-mode gate. A corrected segment can currently attempt target insertion even for Command or Generate, while a Smart segment has no safe way to wait for route confidence and then insert an already-corrected backlog.

## Design

### Native result-popup scrolling

Keep the existing native multiline EDIT controls. Add `WS_VSCROLL` to the shared editor style used by every popup pane. Single-pane transcription results and dual transcript/result layouts each receive their own native vertical scrollbar. Windows remains responsible for wheel, thumb-drag, keyboard paging, and scroll-range behavior; the custom HUD scrollbar implementation is not duplicated.

### Safe weak-origin upgrade

Allow an origin snapshot to be upgraded only when all of the following are true:

- Existing and candidate snapshots refer to the same top-level window.
- The existing snapshot has no focus handle, caret handle, or UI Automation Runtime ID.
- The candidate is explicitly recognized as editable.
- The candidate has higher capture quality than the existing snapshot.

Do not upgrade across windows, from an origin that already has control identity, to a non-editable candidate, or between conflicting focus/runtime identities. Reuse this rule during initial hotkey target resolution, background enrichment, and the stop-path snapshot handoff so a short-session race cannot preserve a weak origin.

### Mode-aware live insertion

Yellow `PreRecognized` text remains preview-only in every mode. Provider correction continues to run in faithful Transcribe processing mode so the preview can turn white without allowing command execution or generative rewriting.

Target insertion policy is separate from correction processing:

- Explicit Transcribe enables corrected-segment insertion immediately.
- Explicit Command, Generate, and Document never enable live corrected-segment insertion.
- Smart starts with insertion disabled.
- Smart locks live insertion to Transcribe only when bounded Smart analysis resolves to Transcribe and has irreversible long-form evidence.
- When Smart locks, corrected segments without insert anchors are inserted in tracker order exactly once, then future corrected segments insert normally.
- If Smart remains ambiguous until stop, the existing complete stop aggregate performs the final route. A final Transcribe result is inserted once; a non-Transcribe result is handled by its existing mode output policy.

Jobs queued before the Smart lock must consult the current live route at apply time, not only a stale boolean captured when queued. Completed pre-lock corrections remain available in the tracker as a backlog. A HUD refresh is still emitted when correction completes, even while target insertion is gated, so corrected text becomes white in the preview.

### Ordering and fallback

The existing foreground apply gate and live correction apply order remain authoritative. Backlog insertion uses tracker order and records an insert anchor only after a successful target insertion. If the target is no longer safe, no text is pasted into another control. The complete transcript remains available through the post-session popup, now with scrolling.

Stop aggregation continues to use insert anchors to avoid reinserting already-written segments. No rollback of arbitrary application text is attempted.

## Test Strategy

- Private desktop binary test for the popup EDIT style containing `ES_MULTILINE`, `ES_AUTOVSCROLL`, and `WS_VSCROLL`.
- Contract tests for upgrading a same-window window-only origin to a richer editable Chrome/UIA target.
- Contract tests retaining rejection for cross-window, conflicting focus/runtime identity, and non-editable candidates.
- Contract tests for explicit Transcribe, explicit non-Transcribe, Smart pending, and Smart locked-to-Transcribe target-apply policies.
- Contract tests for ordered backlog selection that excludes already anchored or uncorrected segments.
- Focused desktop contract and desktop binary tests, followed by the full Cargo workspace and PowerShell contract suites.
- Build an isolated `talk-single-exe-20260722-r10` product release without modifying r8, r9, or the existing local ASR process.

## Non-Goals

- Do not insert yellow pre-recognition text.
- Do not weaken the same-control requirement for normal strong target identities.
- Do not make Command, Generate, or Document perform incremental transcript insertion.
- Do not duplicate the custom listening-HUD scrollbar in the native result popup.
- Do not modify, stop, or replace existing r8/r9 artifacts or the user's running local ASR process.
- Do not commit or push unless explicitly requested.
