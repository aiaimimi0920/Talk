# Local-First Speculative Dictation

Talk's long-term voice input architecture is local-first. Local ASR is
responsible for immediate text visibility. Cloud correction is asynchronous and
never blocks the first visible text.

## Pipeline

```text
Mic -> local streaming ASR -> partial/local-final text -> segment detector
                                             -> async cloud correction -> safe patch or popup
```

## Sentence-level correction contract

`talk-runtime` now exposes a sentence-level correction request contract for the
local-first path:

1. Streaming ASR `partial` events still update the visible draft while the user
   is speaking.
2. A segment becomes correction-ready when the segmenter sees ASR finality,
   sentence punctuation plus enough trailing silence, or the configured chunk
   length limit.
3. Once ready, runtime emits `LocalSegmentCommitted` followed by
   `CorrectionRequested { segment_id, local_text, context_before }`.
4. `context_before` is bounded by `SegmenterConfig::correction_context_chars`
   so a text processor receives nearby prior text without receiving an
   unbounded transcript history.
5. Runtime de-duplicates correction requests by segment id. A later ASR final
   for a segment already sent to correction may update the local committed text,
   but it does not emit a second local commit or start a second correction
   request for the same segment.

The correction request contains only text. Raw microphone audio stays on the
local ASR path; cloud/local LLM correction receives `local_text` plus bounded
context and returns a corrected candidate.

## Why this architecture exists

Voice input feels slow when the first visible text waits for a cloud round trip.
Talk therefore treats local ASR as the latency-critical path and cloud models as
quality improvers. The user should see local draft text first, then receive
conservative corrections when Talk can prove the original target is still safe.

## Safety rules

1. Cloud correction never blocks local text display.
2. Ordinary dictation applies only conservative cloud edits automatically.
3. Broad rewrites require polish, translate, ask mode, or user confirmation.
4. A correction is auto-applied only when the original window and focused
   control still match the insert anchor.
5. Stale corrections fall back to a copy/suggestion popup.
6. Local ASR providers speak an engine-neutral event contract so the concrete
   local model can change without rewriting the desktop interaction model.

## Current implementation status

The codebase now has a runnable external local-ASR path plus the safety
contracts needed for cloud correction:

- `talk-core` owns speculative segment/correction config and validation.
- `talk-client` parses external streaming ASR JSON lines, runs the configured
  local ASR command, and chooses the final transcript from ASR events.
- `talk-runtime` can run a voice session from a local transcript without calling
  provider transcription or provider text processing on the latency-critical
  path. It also emits correction-ready sentence chunk events from streaming ASR
  events through `SpeculativeRuntimeState::accept_asr_event_with_segmentation`.
- `talk-desktop` pumps live streaming ASR events during the recording HUD timer
  and feeds them through
  `SpeculativeRuntimeState::accept_asr_event_with_segmentation`. Stable local
  chunks are inserted immediately only when the original editable target is
  still the active target. If focus moved or the target cannot be proven safe,
  Talk leaves the user alone and defers to stop-time handling or popup fallback.
- `talk-desktop` maps runtime `CorrectionRequested` events to patchable
  `SpeculativeInsertAnchor` jobs for live-inserted chunks. Text correction jobs
  send the stable local text plus bounded `context_before` through
  `FrontContext.extra["contextBefore"]`; raw audio is not sent to this
  correction stage.
- `talk-desktop` wires `speculative.enabled = true` and
  `speculative.local_asr = "external_command"` into the normal RightAlt
  recording flow. After recording stops, Talk runs the external local ASR
  command, inserts the local final text immediately, and only then optionally
  starts cloud correction in the background.
- If `speculative.cloud_correction = "provider_text_processor"`, the configured
  provider text processor is used as the asynchronous correction stage. A
  correction is auto-applied only when the original window/focus anchor still
  matches and the edit ratio is conservative; otherwise it falls back to the
  editable copy popup.
- Live desktop insertion is conservative by design. Stop-time full-final
  insertion is disabled once any stable chunk has already been inserted live, so
  Talk does not duplicate text on RightAlt stop. If the final ASR event contains
  an uncommitted tail, Talk attempts to insert only that tail when the original
  target is still active; otherwise it surfaces the tail through the editable
  copy popup.
- Live correction patching is tail-guarded. A correction for an older live
  segment is not auto-applied after a newer segment has been inserted, because
  Talk's patch operation selects text near the current cursor. Such stale
  corrections fall back to the editable popup instead of modifying the wrong
  suffix in another application.

The concrete local RNN-T, Zipformer, Paraformer, Conformer, or Whisper-family
model is intentionally not vendored. The preferred target is
`speculative.local_asr = "streaming_service"`, documented in
[`LOCAL_STREAMING_ASR_PROTOCOL.md`](LOCAL_STREAMING_ASR_PROTOCOL.md). That path
keeps a local model hot and emits partial text while the user is still
recording. The packaged local sherpa daemon keeps the same `segment_id` for
partial and final text in the same utterance so desktop anchors can safely match
local insertion to later correction.

The current executable also keeps `speculative.local_asr = "external_command"`
as a compatibility fallback. In that mode, connect any local engine through
`speculative.external_asr_command`; the command must emit UTF-8 JSON lines
shaped like:

```json
{"type":"partial","segment_id":"seg-1","text":"你好"}
{"type":"final","segment_id":"seg-1","text":"你好。"}
```

The desktop runner also sets:

- `TALK_LOCAL_ASR_AUDIO_FILE` to the recorded WAV path;
- `TALK_LOCAL_ASR_OUTPUT=jsonl`.

`{audio_path}` inside `speculative.external_asr_command` is replaced with the
quoted WAV path before execution.

## Example config

See
[`examples/desktop-external-asr-speculative-config.toml`](../examples/desktop-external-asr-speculative-config.toml)
and
[`examples/external-asr-jsonl-smoke.ps1`](../examples/external-asr-jsonl-smoke.ps1).
