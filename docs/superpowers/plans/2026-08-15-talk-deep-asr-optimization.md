# Talk Deep ASR Optimization Report

Date: 2026-08-15

## Scope and product constraints

This pass optimizes the local streaming recognition path without changing
Talk's local-first architecture or its product payload. The release remains
exactly `Talk.exe` plus `talk.toml`. The reviewed repository snapshot starts at
branch `talk-accuracy-20260726`, HEAD
`80d5d092c41116921136b8520ed4c81f740cf7e5`; the worktree already contained
extensive uncommitted work that must be preserved.

Recognition claims in this report are limited to the six-item captured corpus
at `.runtime/asr-bench/real-mic-corpus-r17-faithful-20260729-r1`. Its manifest
SHA-256 is
`9b09af77ef5b68bd78d5f7d26af650819a432e967aa05462d7f959035701cb0d`.
This is useful regression evidence, but it is not a population-quality study.

## Open-source implementation references

- sherpa-onnx's official streaming server keeps one online stream alive,
  repeatedly accepts and decodes waveform data, resets on endpoints, and feeds
  300 ms of silence before `input_finished`. Talk adopts the tail-padding part
  while retaining its existing stream and endpoint protocol:
  <https://github.com/k2-fsa/sherpa-onnx/blob/master/python-api-examples/streaming_server.py>
- FunASR documents online two-pass chunking and hotword support. Talk retains
  its lower-latency local-first path rather than adding a second recognizer in
  this pass: <https://github.com/modelscope/FunASR/blob/main/runtime/docs/SDK_tutorial_online.md>
- WeNet describes unified streaming/non-streaming recognition and attention
  rescoring. It supports treating chunk size and second-pass work as explicit
  latency/accuracy trade-offs rather than assumed wins:
  <https://arxiv.org/abs/2102.01547>
- faster-whisper's benchmark guidance reinforces that model comparisons must
  hold decoding options and thread settings constant:
  <https://github.com/SYSTRAN/faster-whisper>

## Implemented changes

### Lossless bounded capture handoff

The Windows audio callback now writes to a bounded lock-free `ArrayQueue`
instead of conditionally acquiring the consumer sample-buffer mutex. A collector
thread owns conversion and buffer insertion. Temporary consumer lock contention
therefore no longer silently drops an entire callback. Sustained overload is
bounded to two seconds and is reported as an explicit recording failure instead
of producing silently corrupted audio.

### Stateful streaming conversion

`StreamingCaptureConverter` preserves channel-frame carry, resampler phase, and
filter history across arbitrary CPAL callback boundaries. It performs phase-safe
fixed-channel selection and emits the canonical target format continuously.
Tests compare irregular 44.1 kHz and 48 kHz callback sequences against a
whole-input reference and cover partial frames and anti-phase stereo input.

### Sherpa online finalization and configuration safety

The online recognizer accepts a bounded `online_tail_padding_ms` setting
(default 300 ms, valid range 0 through 2000 ms). Finalization reuses a scratch
buffer to feed silence before `input_finished` and the final decode. A separate
768,000-sample cap prevents malformed sample-rate settings from turning that
scratch buffer into an excessive allocation. The benchmark script exposes and
records the same setting for reproducible A/B
runs. Configuration validation now also rejects BPE modeling units without the
required text vocabulary and distinguishes `bpe.vocab` from a binary
`bpe.model`.

## Controlled benchmark results

All rows use the same corpus, default `zipformer-zh-en-punct-int8-480ms` model,
greedy decoding, two threads, and 80 ms PCM chunks unless stated otherwise.

| Variant | CER | First partial | Final | RTF | Decision |
| --- | ---: | ---: | ---: | ---: | --- |
| Baseline | 0.2908507459 | 482 ms | 1823 ms | 0.221745 | Reference |
| No tail padding | 0.2908507459 | 466 ms | 1816 ms | 0.222606 | Reject |
| 300 ms tail padding | **0.2726689277** | 482 ms | 1884 ms | 0.229565 | Keep |

On this corpus, 300 ms tail padding reduced aggregate CER by approximately
6.25% relative to the 0 ms run. The recovered error was in the long-form item,
where the recognizer retained trailing speech that the 0 ms run cut off. The
measured trade-off was 16 ms more aggregate first-partial latency, 68 ms more
aggregate final latency, and approximately 3.13% higher RTF. Evidence is under
`.runtime/asr-bench/deep-optimization-20260815/post-tail-padding-{0ms,300ms}`.

Two alternatives were measured and rejected:

- A 40 ms chunk run preserved CER but worsened first partial to 851 ms, final to
  3559 ms, and RTF to approximately 0.439. The 80 ms runtime chunk remains.
- The tested Paraformer and multilingual Zipformer candidates produced worse
  CER (approximately 0.459 and 0.488 respectively) than the default model's
  approximately 0.291. The default model remains, with no broader model-quality
  claim beyond this corpus.

## Verification and release gates

- [x] Focused `talk-audio` tests: 68 passed.
- [x] Focused `talk-local-asr-sherpa` tests: 27 passed.
- [x] Benchmark-script Pester tests: 12 passed.
- [x] Fresh 0 ms versus 300 ms controlled A/B.
- [x] Rust format, workspace check, test, and Clippy gates.
- [x] Independent code review of audio concurrency and Sherpa finalization.
- [ ] Fresh product build and exact two-file release verification.

The remaining quality risk is corpus representativeness. A larger, newly
recorded multi-speaker microphone corpus with noise, accents, long pauses, and
mixed-language tails is required before treating the measured CER delta as a
general product-wide improvement.
