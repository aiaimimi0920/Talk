# ASR accuracy, latency and resource baseline (2026-10-02)

This round establishes a reproducible baseline and exposes an optional cloud
correction control. **It does not change the model, decoder or thread defaults.**
The measured options have tradeoffs; none improves every requested dimension.

## Results

The broader corpus has **85 utterances, 535.328 seconds**: 11 Mandarin AISHELL
clips (141 reference characters) and 74 English LibriSpeech clips (5,260
characters / 1,180 words). One measured pass follows a separate warmup for each
broader configuration. The worker, model weights, input bytes and 48 ms input
chunks are identical. Bulk input is sent without intentional pacing or idle
polls, with a concurrent receive task.

| Configuration | Mandarin CER | English CER | English WER | Mean bulk final | RTF | Worker CPU time | Peak RSS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Current: greedy, 2 threads | 0/141 | 112/5260 (2.13%) | 70/1180 (5.93%) | 633.8 ms | 0.1006 | 177.23 s | 301.8 MB |
| Greedy, 1 thread | 0/141 | 114/5260 (2.17%) | 69/1180 (5.85%) | 612.9 ms | 0.0973 | 50.97 s | 300.4 MB |
| Modified beam, 1 thread | 0/141 | 104/5260 (1.98%) | 65/1180 (5.51%) | 697.8 ms | 0.1108 | 59.03 s | 299.8 MB |

Lower thread count substantially reduces measured CPU work on this host, but
does **not** guarantee identical recognition. Greedy/one-thread changes three
raw transcripts: two punctuation-only changes and one `mantelboard` → `mantle
board` change. That last case adds two character errors while reducing the word
error count by one. Modified beam improves character errors in seven clips and
worsens two against the current default. Its aggregate gain is specific to this
small corpus; it also takes longer to finish. Per-clip transcripts and error
counts are retained, including regressions, in the [raw report](asr-resources-20261002.json).

The original 10-utterance subset was also fed at speaking speed, once per
configuration after warmup. Total audio duration was 92.101 s:

| Configuration | Mean first partial | Mean Stop-to-final | Worker CPU time | Peak RSS |
| --- | --- | --- | --- | --- |
| Greedy, 2 threads | 1480.1 ms | 86.9 ms | 37.98 s | 299.4 MB |
| Greedy, 1 thread | 1482.8 ms | 92.0 ms | 9.02 s | 297.8 MB |

Here one thread uses about **76% less worker CPU time**, with about **5 ms more
mean Stop-to-final latency**. All ten transcripts match between these two paced
runs. This is a protocol-level result, not a Windows microphone, HUD or insertion
measurement, and CPU-seconds are not measured electrical energy.

Timing varies across sequential cloud runs. On two bulk passes of the original
10 clips, one thread averaged 821.0 ms versus 759.9 ms for two threads, the
opposite ordering from the broader pass. Consequently **no general speedup is
claimed**. Bulk Stop-to-final includes queued audio inference and must not be
confused with the paced tail latency above. Paced RTF is approximately 1.01
because it includes the time spent delivering speech at its original speed.

Disabling the ONNX CPU memory arena through Sherpa's existing CPU provider config
was rejected: on the same 10 clips/two bulk passes it raised peak RSS to 313.1 MB
from 299.2 MB, with similar timing and unchanged text. No memory-saving default
is justified by this experiment.

## Measurement contract and limits

- Baseline source: `0dc0f828f928f1c72b43026b0657e8dd250e5da5`; exact worker and model
  file SHA-256 values are in the raw report. No recognition-runtime code changes
  were made for the comparisons.
- Linux cloud, AMD EPYC 9V74, nine visible logical CPUs; Rust debug protocol
  code and optimized Sherpa 1.13.4 native prebuilt inference. Model benchmarks
  ran sequentially, without concurrent compilation. Shared host scheduling and
  non-random run order limit timing conclusions.
- CPU is the worker process's user + system CPU time across all threads, sampled
  before connection and on final receipt. Linux counters have 10 ms resolution.
  Client CPU and activity after the final sample snapshot are excluded.
- Peak RSS is the worker's lifetime high-water mark (Linux `VmHWM`), including
  initialization and preceding utterances. It is not a per-clip allocation peak;
  MB means 1,000,000 bytes. Windows uses `peak_wset` when available; unmeasured
  peaks are null rather than zero.
- Fresh process startup includes model initialization before the TCP listener.
  It took roughly 3–4 seconds in these runs. Model files were already in the OS
  cache, so this is **process-cold**, not storage-cold. The first decoded utterance
  is recorded separately as warmup and excluded from aggregates.
- Reported end-to-end latency runs from loopback connection to final transcript.
  WAV loading, microphone capture, UI rendering, insertion and model startup are
  separate. It is not a full desktop interaction measurement.
- Accuracy lowercases Unicode, removes punctuation and collapses whitespace.
  CER additionally excludes spaces; WER uses English whitespace words only.
  Counts are pooled over reference characters/words, not averaged over clips.
  Casing and punctuation quality are not scored.
- The 85 clips are a regression set, with limited speakers and domains. No
  noisy-room, accent or mixed-language coverage is established. Renamed Mandarin
  fixtures do not preserve speaker IDs. Zero errors on 141 Mandarin characters
  must not be generalized into a product accuracy claim.
- No user recordings, microphone access, paid requests or new credentials were
  used. The existing real-model tail/silence regression test also passes, but
  does not establish silence/noise behavior for every optional decoder setting.

## Reproduce

Use Python 3.12 and install the optional pinned dependencies:

```sh
python -m pip install -r scripts/asr-benchmark-requirements.txt
python scripts/prepare_public_asr_corpus.py \
  --manifest docs/asr-benchmarks/public-corpus-85.json \
  --output-dir /path/to/public-corpus
cargo build -p talk-local-asr-sherpa --locked
```

The preparation script verifies every downloaded file and decoded WAV hash,
checks LibriSpeech references, rejects path escapes and symbolic links, and
refuses to overwrite differing files. Downloads are bounded to 16 MiB each and
64 MiB total; manifests are limited to 1,000 entries. These guards are intended
for cooperative local use; they do not guarantee safety against a concurrent
actor replacing paths in a shared output directory. The
85-item manifest is copied to the output directory as `corpus.json`.

Use the existing official model archive linked in the provenance section below,
verify its SHA-256, then extract it. Run the probe with the extracted model:

```sh
python scripts/benchmark_asr_resources.py \
  --worker target/debug/talk-local-asr-sherpa \
  --model-dir /path/to/extracted-model \
  --corpus-manifest /path/to/public-corpus/corpus.json \
  --code-revision 0dc0f828f928f1c72b43026b0657e8dd250e5da5 \
  --label greedy-two-threads --num-threads 2 --chunk-ms 48 \
  --repetitions 1 --pacing bulk --output-json /path/to/result.json
```

Set the native library search path as required by the Sherpa prebuilt package.
Change only `--num-threads 1` for the thread comparison. Add
`--decoding-method modified_beam_search` for the beam experiment. Use
`--pacing realtime` to include speaking-time delivery. `--sample-id` can be
repeated for the original ten IDs listed in the raw report. The probe only
starts and contacts a loopback worker, records model/input/binary hashes, and
keeps completed results if a later sample fails. Choose a new output filename
for each run: existing reports/logs are preserved. WAV inputs are bounded to
32 MiB and 15 minutes each. Optional-dependency-free metric/integrity tests run
on both CI platforms; the symlink test skips if the OS denies creating symlinks.

To reproduce the rejected arena experiment, write `EnableCpuMemArena=0` to a
file and pass `--provider cpu:/absolute/path/to/file`. This is supported by
[Sherpa 1.13.4's session configuration](https://github.com/k2-fsa/sherpa-onnx/blob/v1.13.4/sherpa-onnx/csrc/session.cc).
ONNX Runtime documents the [CPU cost of thread spinning](https://onnxruntime.ai/docs/performance/tune-performance/threading.html);
that is a possible contributor, not a measured attribution of all CPU savings.

For an explicit lower-CPU experiment in Talk, its existing configuration already
supports the following setting; no new profile alias or automatic override is
needed:

```toml
[speculative.streaming_service.local_daemon]
num_threads = 1
decoding_method = "greedy_search"
```

Keep the other model paths/settings from the current configuration. Change
`decoding_method` to `"modified_beam_search"` only when deliberately evaluating
the accuracy/latency tradeoff. Restore `num_threads = 2` and `greedy_search` to
return to the current default. Neither optional combination is selected
automatically from this small corpus.

## Paid paths and optional correction control

Talk's packaged path uses local ASR with cloud fallback and separate cloud text
correction. ASR tariffs and correction-token charges are different costs.
Official prices checked **2026-10-02**:

| Service/catalog | Published ASR rate |
| --- | --- |
| Qwen3-ASR-Flash, China account, Beijing | ¥0.00022/audio-second = ¥0.0132/minute |
| Qwen3-ASR-Flash, international account, Beijing | $0.000032/audio-second = $0.00192/minute |
| Qwen3-ASR-Flash, international account, Singapore | $0.00210/minute |
| OpenAI gpt-4o-mini-transcribe | Estimated $0.003/audio-minute |
| OpenAI gpt-4o-transcribe | Estimated $0.006/audio-minute |

Sources: [Alibaba China pricing](https://help.aliyun.com/zh/model-studio/model-pricing),
[Alibaba international pricing](https://www.alibabacloud.com/help/en/model-studio/model-pricing),
[OpenAI pricing](https://developers.openai.com/api/docs/pricing).
These are separate account/region catalogs, not currency conversions or a
quality-adjusted ranking. Gateway charges, taxes, billing rules and promotions
can differ. No account bill was inspected.

Qwen3.7-plus correction has a Beijing list price of ¥2/million input tokens and
¥8/million output tokens; the alias showed a temporary 20% discount on the
checked date. There is no reliable per-audio-minute correction estimate without
actual usage and correction frequency, so no savings amount is asserted.

The new optional setting can avoid Qwen's default reasoning step for light
transcript correction:

```toml
[provider]
transcription_correction_enable_thinking = false
```

It is omitted by default, and is sent only for resolved Transcribe/Dictate
correction, never for audio recognition or other modes. This proprietary field
requires a supporting model/endpoint; generic OpenAI configurations remain
unset. Qwen3.7-plus support and default thinking behavior are documented in
the [request schema](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)
and [thinking guide](https://help.aliyun.com/zh/model-studio/deep-thinking).
Offline request tests establish the contract. Cloud correction accuracy,
latency and billed savings were **not measured**. The option does not change
the ASR unit tariff.

## Corpus and model provenance

- Model: [official pinned Zipformer archive](https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2),
  SHA-256 `fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74`.
  [Model license](https://huggingface.co/GilgameshWind/X-ASR-zh-en): Apache-2.0.
- Two original WAVs and references: [pinned WeNet fixtures](https://github.com/wenet-e2e/wenet/tree/d17059667d6afe0680d19b3a4948ab825ef25105/test/resources).
- 73 English clips: [pinned LibriSpeech subset](https://huggingface.co/datasets/hf-internal-testing/librispeech_asr_dummy/tree/5be91486e11a2d616f4ec5db8d3fd248585ac07a).
  The earlier ten-clip set used eight of these rows; this set adds the remaining
  65. LibriSpeech is [CC BY 4.0](https://www.openslr.org/12/), by Vassil Panayotov,
  Daniel Povey and collaborators, derived from LibriVox recordings.
- Ten additional Mandarin clips: [pinned Speech-Transformer fixtures](https://github.com/foamliu/Speech-Transformer/tree/cf917db8c219e837e9392177a5d385c9f2b60b0d/audios).
  The demo selects AISHELL test WAVs and exports independent `gt_N` references
  in [results.json](https://github.com/foamliu/Speech-Transformer/blob/cf917db8c219e837e9392177a5d385c9f2b60b0d/results.json).
  Only terminal `<eos>` markers are removed. AISHELL-1 is
  [Apache-2.0](https://www.openslr.org/33/). This is a software fixture mirror,
  with original utterance/speaker IDs omitted.

Audio/model binaries are not committed. Every input hash, gold reference and
immutable source is recorded in [the preparation manifest](public-corpus-85.json).

## Validation after dependency security update

The implementation was rebased onto security merge
`b6812ad6f60aef39ca4d0d77b2368ab77309c9c0`, retaining its dependency fixes.
A freshly rebuilt worker at source `5bcd5fcd6704c7041b2d1daf490ef243567841d8`
correctly transcribed the original Mandarin and English clips again. Its new
binary hash and raw results are recorded separately under
`security_rebase_smoke`. This is functional validation of the updated build;
the full accuracy/resource comparison above remains historical evidence for
source `0dc0f828` and its recorded binary, not a performance claim for the new
dependency build. The full 505-test workspace suite, check, clippy, formatting
and 11 metric/corpus tests pass after rebase.
