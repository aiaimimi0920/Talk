# Streaming tail-context check (2026-10-01)

## Outcome

The existing Sherpa session can return a final transcript before the model has
processed the end of the recording. This is a pipeline correctness fix, not a
model replacement. No cloud API was used.

On a small paired corpus of **10 original public utterances** (one Mandarin,
nine English; 2.2–29.4 seconds), all ten had lower normalized error after flushing
the tail. Across the 944 reference characters, errors changed from 63 to 14
(6.67% to 1.48% CER). English word errors changed from 22/213 to 10/213
(10.33% to 4.69% WER). These numbers describe this regression corpus only, not
expected product accuracy. It includes just two English speakers and one
Mandarin example. Punctuation/casing quality is not evaluated by these scores.

Examples:

- Mandarin reference: `广州市房地产中介协会分析`
  - Before: `广州市房地产中介协会`
  - After: `广州市房地产中介协会分析`
- English reference ends in `the dreams built around it`
  - Before ends in `the dreams`
  - After includes the complete ending

A derived Mandarin tail-stress sample also improves, but is excluded from the
original-corpus totals. 50 ms silence, 5 s silence, and deterministic white-noise
controls produce no transcript in either version. Their existing empty-result
connection-close behavior is unchanged.

**No overall recognition-speed improvement is established.** The extra acoustic
context requires extra inference. Mean per-sample median bulk final latency rose
from 1521 to 1605 ms (+84 ms), and first-partial measurements were mixed
(268 to 293 ms on this sequential two-run comparison). The separate real-time
probe observed approximately 104–140 ms to finish the corrected transcript
after Stop in the tested candidate. It feeds 48 ms chunks, matching the desktop
pump interval, and is distinct from the bulk benchmark. The baseline frequently
finishes almost immediately because it omits the final words. Shared cloud CPU,
run order and the small repeat count prevent reliable speed claims.

The worker now skips native result allocation/JSON decoding when no model
inference step ran. The local client and server also disable TCP batching for
small PCM/control/partial frames. These reduce avoidable work/waiting; they are
not evidence of an end-to-end speedup in this corpus.

## Why tail context is needed

Pinned encoder metadata: `decode_chunk_len=48`, `T=61` feature frames. Sherpa's
[IsReady implementation](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/online-recognizer-transducer-impl.h)
requires a full input window. Calling `input_finished()` alone does not supply
that acoustic right context. The model's own
[export test](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/scripts/zipformer-transducer/x-asr/test_onnx_streaming.py)
pads 100 feature frames (one second). Generic Sherpa examples use shorter tails;
300 ms still lost the English final word, and 660 ms still lost characters in the
Mandarin boundary case here.

The worker defaults to 1000 ms of **computed zero samples**, not a sleep or a wait
for the microphone. `--tail-padding-ms 0..2000` provides a bounded engineering
override for model-specific evaluation, including `0` for the old finalization
behavior. No decoder/model/provider or desktop polling interval was changed.
The microphone resampling phase issue is outside this patch. Model-backed
coverage is limited to the pinned Zipformer. Paraformer and other optional
models were not measured; their tail-context tradeoffs remain unverified.

## Provenance

- Baseline: Talk `a0c49c7046ff2751a3bca6e8d470f1a8e20823c5`
- Tested candidate code: Talk `e9b088902285787729ea9d12d34e9255dedebaf8`
- Model: the existing pinned `zipformer-zh-en-punct-int8-480ms`
- [Official model archive](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2)
- Archive SHA-256: `fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74`
- [Upstream model license: Apache-2.0](https://huggingface.co/GilgameshWind/X-ASR-zh-en)
- First two WAVs and matching references:
  [WeNet test corpus at pinned commit](https://github.com/wenet-e2e/wenet/tree/d17059667d6afe0680d19b3a4948ab825ef25105/test/resources)
  and [reference text](https://github.com/wenet-e2e/wenet/blob/d17059667d6afe0680d19b3a4948ab825ef25105/test/resources/dataset/text)
- Mandarin: [AISHELL-1, Apache-2.0](https://www.openslr.org/33/)
- English: [LibriSpeech, CC BY 4.0](https://openslr.org/12/), by Vassil Panayotov,
  Daniel Povey and collaborators, derived from LibriVox recordings
- Additional eight English WAVs/references: rows 0, 1, 2, 3, 4, 5, 12 and 14 of
  [the pinned Hugging Face LibriSpeech subset](https://huggingface.co/datasets/hf-internal-testing/librispeech_asr_dummy/tree/5be91486e11a2d616f4ec5db8d3fd248585ac07a)
  selected before running the candidate; parquet SHA-256
  `4e69a06fa5edc90921e5e7e39a7084881f8b3ed9c805c574f4f39c6fde27c603`

No user audio, microphone capture, private recordings or new paid endpoint was
used. Audio/model binaries are not committed. Per-WAV hashes, transcripts,
per-run timings, controls and normalized error counts are in
[the machine-readable report](streaming-tail-context-20261001.json).

## Method and reproduction

Linux cloud CPU; Sherpa 1.13.4 native prebuilt runtime, CPU provider, two inference
threads, `greedy_search`. Both builds use the same model, PCM bytes (16 kHz mono
S16LE), Rust debug profile, process layout and existing `asr-bench`. One worker
loads the model and serves each variant's corpus twice. Baseline precedes
candidate; the two batches were not randomized. Model initialization time is not
included: `cold_start_ms` in these existing reports is connection/Ready time.

Run the worker with the pinned model's `tokens.txt`, `encoder.int8.onnx`,
`decoder.onnx` and `joiner.int8.onnx`. Use `--mode sherpa-online --num-threads 2
--tail-padding-ms 1000` for the candidate. Build the baseline commit separately
for an exact before/after comparison; setting padding to zero alone does not
revert transport/result-polling changes.

For each report sample, run `asr-bench --streaming-endpoint
ws://127.0.0.1:53171/asr --audio-wav <file> --reference-text <reference>
--sample-id <id> --chunk-ms 80 --partial-idle-timeout-ms 10 --output-json <report>`.
The bulk sender intentionally retains the original benchmark's 10 ms idle poll
per chunk; these timings are not live microphone latency.

Normalization for this report lowercases Unicode, removes punctuation and
collapses whitespace. CER additionally removes spaces; English WER uses
whitespace-delimited words. Existing raw, case-sensitive `cer` fields are kept in
the JSON and must not be mistaken for the normalized scores. No Chinese WER is
reported because whitespace tokenization is unsuitable here.

The derived Mandarin tail sample removes samples after the last absolute PCM
value >=328 plus 50 ms, leaving 3714.375 ms. It is a boundary stress test, not an
additional independent utterance. The noise control uses NumPy RNG seed 42,
normal distribution mean 0/stddev 2000, cast to S16, for two seconds.

To run the real-model regression test, set `TALK_ASR_TEST_MODEL_DIR` to the
extracted pinned model and `TALK_ASR_TEST_CORPUS_DIR` to the directory containing
`aishell-BAC009S0724W0121.wav` and `librispeech-1995-1837-0001.wav`, then run:

```sh
cargo test -p talk-local-asr-sherpa \
  real_model_preserves_final_words_and_rejects_silence -- --ignored
```

Ordinary CI does not download model/audio binaries. The ignored test must be
run explicitly; passing dry-run mocks does not establish speech accuracy.

### Reproduce the separate real-time probe

`scripts/benchmark_streaming_latency.py` preserves the measured pacing/receive
method without machine-specific paths. It requires Python and the optional
`websockets` package. The corpus manifest is a JSON array of objects with `id`
and `file` (a WAV path relative to that manifest). Use the three sample IDs from
`realtime_probe` in the JSON report, including the documented derived Mandarin
boundary case. All input hashes and references are recorded there.

```sh
python scripts/benchmark_streaming_latency.py \
  --worker <baseline-worker> --model-dir <pinned-model-directory> \
  --corpus-manifest <manifest.json> --label baseline --no-disable-nagle \
  --repetitions 2 --chunk-ms 48 --output-json <baseline-live.json>
python scripts/benchmark_streaming_latency.py \
  --worker <candidate-worker> --model-dir <pinned-model-directory> \
  --corpus-manifest <manifest.json> --label candidate --disable-nagle \
  --tail-padding-ms 1000 --repetitions 2 --chunk-ms 48 \
  --output-json <candidate-live.json>
```

The original `nodelay1000` live measurements predate the result-polling skip and
CLI override/test additions. Their tail and transport behavior match the tested
candidate; the bulk candidate includes the result-polling optimization. The live
probe is a protocol-level test using an independently paced Python client, not
a measurement of Windows microphone capture, HUD rendering or text insertion.
Model initialization is excluded. Set the native library search path for the
worker if needed on your platform. The script contacts only its loopback worker.
