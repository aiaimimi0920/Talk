# Talk Audio Front-End Accuracy — Design

Status: Implemented 2026-07-26.

## Problem

Two defects in `crates/talk-audio/src/lib.rs` degraded the audio signal handed to
both recognition routes (local streaming sherpa and the batch cloud transcriber):

1. **Aliased downsampling.** `encode_captured_pcm_bytes` resampled by
   nearest-neighbour frame picking (`source_frame_index_for_target`) with no
   anti-aliasing filter. A typical 48 kHz / 44.1 kHz laptop microphone
   downsampled 3:1 to 16 kHz folds energy above 8 kHz back into the speech band
   as aliasing on *every* capture, corrupting ASR features.
2. **No gain normalization.** `float_sample_to_i16` only clamped, so quiet
   speakers were handed a low-amplitude signal with no level correction.

## Design

### Band-limited resampling (`resample_mono_to_len`)
Resampling now operates on the pre-downmixed mono signal and produces exactly
`resampled_frame_count(..)` samples (unchanged length contract, so downstream
frame-count expectations hold):

- **Downsample:** Hann-weighted moving average whose radius tracks the
  decimation ratio. The window acts as a low-pass whose nulls sit near the
  target Nyquist frequency, attenuating the energy that nearest-neighbour would
  alias. Stateless per output sample, so it is safe for the per-80 ms streaming
  chunk path (`drain_pcm_chunk`) with only a sub-millisecond edge effect — no
  cross-chunk resampler state is threaded through the recording session.
- **Upsample:** linear interpolation (no aliasing on upsample).
- **Same rate (16 kHz):** bit-identical passthrough.

### Bounded normalization (`normalized_capture_gain`)
Applied only in the batch WAV path (`write_captured_wav`), to the source before
encoding (linear gain commutes with resampling):

- Gain `= min(0.9 / peak, 4x)` for `0.05 <= peak < 0.9`; otherwise `1.0`.
- Peaks `< 0.05` are left untouched so genuinely weak captures stay weak and the
  provider weak-signal reject (`prepared_audio_upload_bytes`) still fires.
- Peaks `>= 0.9` are left untouched (never attenuate a hot signal).

Streaming chunks are intentionally **not** normalized: per-chunk gain would pump
between chunks. A smoothed streaming AGC is deferred as a follow-up.

## Tests

- Internal unit tests (`crates/talk-audio/src/lib.rs`): above-Nyquist tone
  attenuated; in-band tone preserved; non-integer 44.1k→16k frame count + finite;
  same-rate bit-identical passthrough; linear upsample; and the four
  `normalized_capture_gain` boundary cases (quiet lift, weak preserve, hot no-op,
  max-gain cap).
- Contract tests (`crates/talk-audio/tests/audio_contract.rs`): batch
  normalization lifts a quiet valid capture toward target; weak capture preserved
  below the reject threshold. The existing
  `write_captured_wav_downmixes_and_resamples_to_requested_pcm_wav` frame-count
  test is unchanged and still passes.

## Verification

`cargo test -p talk-audio` (all green). Accuracy impact is measurable via
`scripts/Invoke-TalkAsrCorpusBenchmark.ps1` / `tools/asr-bench` on 48 kHz and
44.1 kHz corpora (expect CER improvement on non-16 kHz mics and quiet captures,
no regression on clean 16 kHz — passthrough is identical).
