# Talk Local Sherpa Models

The standalone `Talk.exe` automatically downloads and verifies its default
local Zipformer model on first startup. Users do not run a model installer and
the product release does not contain a PowerShell script.

The pinned default is:

```text
zipformer-zh-en-punct-int8-480ms
SHA-256: fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74
```

Talk stores the validated model under:

```text
%LOCALAPPDATA%\Talk\models\sherpa-onnx\zipformer-zh-en-punct-int8-480ms
```

The archive is downloaded over HTTPS into a `.partial` file while its SHA-256
is calculated. Talk rejects a digest mismatch, path traversal, links, special
archive entries, and missing model files. A successful extraction is installed
atomically and recorded in `model-manifest.json`; later launches reuse the
validated directory. A failed bootstrap never promotes a partial directory and
allows the desktop session to use its configured cloud ASR fallback.

The local Sherpa worker and native DLLs are embedded in `Talk.exe`. They are
verified and extracted automatically into
`%LOCALAPPDATA%\Talk\runtime\<payload-hash>`, so the user-facing release remains
only `Talk.exe` plus `talk.toml`.

## Engineering model tools

The commands below are for source development, CI, benchmarking, alternative
models, and offline archive testing. They are not files shipped in the product
directory.

From a Talk source checkout, an engineer can still install a catalog model
explicitly:

```powershell
.\scripts\Install-TalkSherpaModel.ps1 -ModelId paraformer-bilingual-zh-en
```

The script downloads the archive, extracts it under `.runtime\models\sherpa-onnx`,
validates the required `tokens`, `encoder`, `decoder`, and `joiner` files, then
writes:

```text
<model-dir>\talk-local-daemon.toml.snippet
```

Copy that snippet into an engineering config under the existing
`[speculative.streaming_service]` table. A source-built `talk-desktop.exe` can
then start the engineering worker in `sherpa-online` mode and pass the validated
model paths to it. Product `Talk.exe` does this resolution automatically for the
pinned default model.

## Benchmark after installation

After starting `talk-local-asr-sherpa.exe` in `sherpa-online` mode, validate the
same endpoint with the source-built benchmark tool:

```powershell
.\.internal\asr-bench.exe `
  --engine streaming_service `
  --streaming-endpoint ws://127.0.0.1:53171/asr `
  --audio-wav .\.runtime\asr-bench\sample-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --output-json .\.runtime\asr-bench\zipformer-480ms-report.json
```

Use the same WAV and reference text for each candidate model so first partial
latency, final latency, RTF, and CER are comparable.

For same-corpus model selection, use the helper script instead of manually
typing one command per model/sample:

```powershell
.\Invoke-TalkAsrCorpusRecorder.ps1 `
  -PromptManifest .\.runtime\asr-bench\real-mic-corpus\prompts.json `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus `
  -DefaultCaptureSeconds 3
```

```powershell
.\Invoke-TalkAsrCorpusBenchmark.ps1 `
  -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en') `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus\reports `
  -CloudOpenAiCompatibleEndpoint https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions `
  -CloudOpenAiCompatibleModel qwen3-asr-flash `
  -CloudOpenAiCompatibleTransport chat_completions_audio_input `
  -CloudOpenAiCompatibleApiKeyEnv TALK_PROVIDER_API_KEY
```

The recorder creates 16 kHz mono WAV files and the benchmark-ready
`corpus.json` from real microphone speech. The benchmark helper then validates
each installed model through
`Test-TalkSherpaModelInstall`, starts the local sherpa daemon once per model,
runs every manifest sample through the bundled `.internal\asr-bench.exe`, can
optionally run the same samples through an OpenAI-compatible cloud baseline,
then writes an aggregated `asr-model-comparison.json`. Run with `-PlanOnly`
first to check paths and commands without launching a daemon or calling the
cloud endpoint.

For the final default-model pass, use the end-to-end real microphone workflow
wrapper to avoid drift between recording, benchmark, selection, and
config-locking commands:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10', 'paraformer-bilingual-zh-en') `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

The workflow records the real microphone corpus from
`asr-real-mic-prompts.json`, runs the same-corpus benchmark, writes
`asr-model-evidence-status.json`, writes `selected-default-asr-model.json` when
the evidence gate passes, then applies the evidence-selected installed model to
`talk-desktop.toml`. Use `-PreflightOnly` before recording to check
the prompt manifest, executables, installed model directories, cloud API key
environment variable, and target config. The preflight object also returns
per-check `RemediationHint` text and a deduplicated `RemediationCommands` list,
so release operators can copy the missing model installer commands and the
redacted API-key environment-variable template directly from the preflight
output. When `TALK_PROVIDER_API_KEY` is not set but the release
`talk-desktop.toml` contains `[provider].api_key`, the workflow treats that
packaged key as the cloud baseline key source and temporarily exposes it only to
the nested benchmark process. DashScope-compatible runs also reuse the standard
per-user credential file at
`%USERPROFILE%\.neuro\qwen-platform\qwen-dashscope-openai\api-key\manual-live.json`
when that file exists and neither the process environment nor the config file
provides a key. Plain `-PreflightOnly` never records audio; add
`-ProbeAudio -AudioProbeSeconds 2` only when the operator also wants a short
non-silent microphone signal gate before recording the full corpus. That optional
probe adds a `microphone_signal` check and fails when the Windows backend is not
ready or the probe records silence. Use `-PlanOnly` to inspect the paths without
checking file existence, `-RecordOnly -PreflightOnly` to check only the
recording prerequisites, `-RecordOnly` to capture the corpus and stop before
benchmarking, `-SkipRecording` to reuse an existing `corpus.json`, or
`-SkipApply` to stop after selection. The intended staged operator flow is:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly -PreflightOnly
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly -PreflightOnly -ProbeAudio -AudioProbeSeconds 2
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -PromptManifest .\asr-real-mic-prompts.json `
  -CorpusRoot .\.runtime\asr-bench\real-mic-corpus `
  -RecordOnly `
  -ResumeExistingCorpus
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -SkipRecording `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

Do not combine `-RecordOnly` with `-SkipRecording`: the former is the corpus
capture stage, and the latter is the resume-from-existing-corpus stage.
After `-RecordOnly` records the corpus, inspect
`.\.runtime\asr-bench\real-mic-corpus\record-only-status.json` before resuming.
It records the reusable corpus manifest path, sample/recording counts, missing
WAV files if any, and the exact `-SkipRecording` resume command. When resuming,
`-SkipRecording -PreflightOnly` includes a `record_only_status` check. A present
status file must be ready and must point at the same `corpus.json`; otherwise
preflight and the full `-SkipRecording` workflow fail before benchmarking. A
missing status file is tolerated so older or hand-built corpus manifests can
still be benchmarked directly.

When the prompt manifest expands after a previous real-microphone pass, prefer
`-RecordOnly -ResumeExistingCorpus`. That incremental refresh reuses already
aligned WAV files, records only the newly required sample IDs, and rewrites
`corpus.json` in prompt order before the later `-SkipRecording` benchmark pass.
If you try `-SkipRecording` against a stale corpus, preflight now fails and
returns the incremental refresh command instead of silently accepting partial
evidence.

After the comparison exists, use the source-checkout selection gate:

```powershell
.\Select-TalkDefaultAsrModel.ps1 `
  -ComparisonJson .\.runtime\asr-bench\real-mic-corpus\reports\asr-model-comparison.json `
  -OutputJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json
```

If you only want to inspect whether the evidence is complete enough, add
`-StatusOnly`. This does not write `selected-default-asr-model.json`; it returns
`ready`, all `blockingReasons`, missing required local model IDs, cloud-baseline
presence, and per-candidate sample checks:

```powershell
.\Select-TalkDefaultAsrModel.ps1 `
  -ComparisonJson .\.runtime\asr-bench\real-mic-corpus\reports\asr-model-comparison.json `
  -StatusOnly
```

The one-command workflow writes the same status object to
`.\.runtime\asr-bench\real-mic-corpus\reports\asr-model-evidence-status.json`
before strict selection, so failed production selection runs still leave a full
diagnostic artifact.

This gate is what should be used before changing the packaged default. It
requires real microphone evidence, at least three samples per candidate,
the same unique sample ID set for every candidate, multilingual Zipformer,
legacy zh-en Zipformer, and Paraformer local candidates, and the cloud-only
baseline. It independently re-ranks local candidates by CER, first partial
latency, final latency, RTF, memory, and model size rather than trusting the
comparison JSON order. It intentionally rejects the current Huihui TTS smoke
reports as insufficient production evidence.

When that gate succeeds, apply the selected installed model to the desktop
config:

```powershell
.\Set-TalkDefaultAsrModel.ps1 `
  -SelectionJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json `
  -ConfigPath .\talk-desktop.toml `
  -ModelRoot .\.runtime\models\sherpa-onnx
```

The applier creates `talk-desktop.toml.bak` by default, validates the selected
model with the same installer validation used by the benchmark helper, and
writes the active local daemon block needed for `talk-desktop.exe` to start the
selected sherpa model instead of dry-run mode.

## Built-in model catalog

`Install-TalkSherpaModel.ps1` currently exposes these model IDs:

| Model ID | Family | Size | Use |
| --- | --- | ---: | --- |
| `paraformer-bilingual-zh-en` | paraformer | ~999 MiB | Legacy packaged fallback. Still supported for manual comparison and recovery, but no longer the preferred default. |
| `zipformer-zh-en-punct-int8-480ms` | transducer | ~128 MiB | Packaged default. Best raw CER on the aligned Chinese/English real-microphone corpus, with punctuation and the lowest package size among the compared bilingual models. |
| `sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10` | transducer | ~247 MiB | Multilingual fallback for language coverage beyond the default Chinese/English dictation target. |
| `zipformer-zh-int8-2025-06-30` | transducer | ~126 MiB | Chinese-only streaming Zipformer fallback. |
| `offline-zipformer-zh-en-int8-2023-11-22` | offline transducer | ~72 MiB int8 runtime | Benchmark-only bilingual final-rescore candidate; rejected on the aligned corpus. |
| `offline-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09` | offline SenseVoice | ~227 MiB int8 runtime | Benchmark-only multilingual final-rescore candidate; rejected on the aligned corpus. |
| `offline-whisper-base-int8` | offline Whisper | ~154 MiB int8 runtime | Benchmark-only multilingual final-rescore candidate; rejected on the aligned corpus. |
| `offline-whisper-small-int8` | offline Whisper | ~359 MiB int8 runtime | Benchmark-only multilingual final-rescore candidate; rejected on accuracy and latency. |

The default model is `zipformer-zh-en-punct-int8-480ms`.
The packaged release verifies archive SHA-256 `fa5f63d618e5a01526e275a358bb7772e403f84808a4769fba52cffd8160bf74`.

Current evidence status:

- On the six aligned real-microphone samples in
  `.runtime/asr-bench/real-mic-corpus-r17-faithful-20260729-r1`, the refreshed
  2026-08-05 baseline measured raw CER `0.2909` for
  `zipformer-zh-en-punct-int8-480ms`, versus `0.4875` for the previous
  multilingual default and `0.4591` for Paraformer. This is a 40.3% relative
  raw-error reduction from the previous default, and the selected model won all
  six non-aggregate sample comparisons.
- With product-equivalent endpoint reset enabled, the selected model measured
  first partial `432 ms`, final latency `1745 ms`, and RTF `0.213` in that run.
  The accuracy-first live gate is now
  `MaxFirstPartialMs = 750` and `MaxRtf = 0.60`; it still rejects clearly slow
  candidates while allowing the more accurate sub-second model.
- A 2026-08-07 same-corpus run rejected the Chinese-only
  `zipformer-zh-int8-2025-06-30` candidate. Its aggregate CER was `0.5358`
  versus `0.2909` for the packaged bilingual model; both mixed-language samples
  measured `0.75` CER and the proper-noun sample measured `0.8438`. It is not a
  viable default for Talk's Chinese/English dictation target.
- A live `qwen3.7-plus` provider-correction replay on the same six local reports
  produced six exact final outputs (`meanProcessedCer = 0`). The desktop patch
  gate previously classified distributed casing, spacing, and punctuation edits
  as one large contiguous rewrite, so none of the five changed samples qualified
  for live auto-apply at `0.25`. Talk now measures actual character-level edit
  distance and uses a `0.35` product limit aligned with the faithful-output guard.
  On the recorded replay, three of five changed samples qualify for live apply;
  the two broad corrections still require the unchanged-target whole-document
  path or the editable popup.
- A 2026-08-07 offline final-rescore study used the same six sample IDs, corpus
  manifest SHA-256, and per-WAV SHA-256 values. None of the four offline
  candidates beat the packaged streaming model's raw CER `0.2909`: SenseVoice
  measured `0.3623`, offline Zipformer `0.4982`, Whisper small `0.5076`, and
  Whisper base `0.5662`. Mean one-shot final latency, including process startup
  and model load, was `3.38 s`, `4.29 s`, `8.80 s`, and `3.64 s` respectively,
  versus `1.72 s` for the current streaming model. No offline candidate is wired
  into the desktop stop path. The reproducible comparison is stored at
  `.runtime/asr-bench/accuracy-optimization-20260807-r34/offline-five-model-comparison.json`.
- A 2026-08-07 performance matrix kept the selected model, endpoint reset, and
  all six aligned WAV hashes fixed. CPU `num_threads = 2` remained best at CER
  `0.2909`, first partial `417 ms`, final latency `1716 ms`, and RTF `0.2097`;
  one thread measured `492/1859/0.2255`, while four threads measured
  `494/1816/0.2226`. Client audio chunks also remain `80 ms`: `120 ms` raised
  first partial to `805 ms`, while `40 ms` raised final latency to `3505 ms`.
  The reports are under
  `.runtime/asr-bench/accuracy-optimization-20260807-r35/{num-threads,chunk-ms}`.
- The r35 partial-idle follow-up separates throughput and real-time replay.
  Burst-mode replay measured `1 ms` at `390/1578/0.1935`, `5 ms` at
  `395/1649/0.2015`, and `10 ms` at `435/1744/0.2130` (first partial/final
  latency/RTF). That mode sends the next chunk as soon as polling becomes idle,
  so its latency includes the polling value once per chunk and cannot select the
  desktop timer. The new `--streaming-realtime` mode paces chunks against the WAV
  timeline and records that mode in every report. It measured `10 ms`, `1 ms`,
  and `5 ms` at `1868/8137/0.9967`, `1869/8135/0.9964`, and
  `1872/8139/0.9969`; all three kept CER `0.2909`, and the 1-5 ms differences
  are below run-to-run noise. The desktop therefore retains its shorter `1 ms`
  idle wait rather than blocking each pump for an unproven `10 ms` benefit.
  Evidence is stored under
  `.runtime/asr-bench/accuracy-optimization-20260807-r35/{partial-idle-ms,partial-idle-realtime-ms}`.
- A follow-up 2026-08-05 parameter matrix confirmed that the product defaults
  remain the best measured configuration. `blank_penalty = -0.30` and `-0.15`
  did not change CER; `0.45` and `0.60` worsened CER to `0.3233`; changing
  endpoint rule 2 from `1.2 s` to `0.8 s` did not improve CER or latency, while
  `1.6 s` was slower. Applying the existing silence trimmer to the corpus
  worsened CER from `0.2909` to `0.2996`, so no trimming or non-default decoding
  parameter is enabled on the live streaming path.
- The same follow-up run fixed sherpa segment-boundary whitespace before strict
  WebSocket events are emitted. This prevents otherwise valid partial/final
  hypotheses from being rejected when a decoder setting returns surrounding
  whitespace; internal word spacing is preserved.
- The newer `real-mic-corpus/reports-20260804-r28` corpus is not default-model
  evidence because its WAV speech no longer matches its reference text. The
  benchmark now binds every generated report to the exact corpus manifest and
  WAV SHA-256. Explicit mismatches fail preflight, and legacy reports without
  provenance are skipped instead of interpreting their CER as current model
  quality.
- Product auto-discovery stays on `greedy_search` and does not inject a managed
  regression hotword list. Hotwords remain an explicit user configuration:
  the measured full regression list improved one domain phrase but slightly
  worsened aggregate CER and could fail tokenization on unsupported phrases.
  A second seven-phrase, BPE-only ablation also failed to beat the default:
  scores `0.5`, `1.0`, and `1.5` measured CER `0.3068`, `0.3095`, and `0.2909`
  with first-partial latency `577`, `641`, and `607 ms`, versus baseline CER
  `0.2909` and `435 ms`. Managed hotwords therefore remain disabled.
- `paraformer-bilingual-zh-en` has also been validated on the same Huihui TTS
  WAV from the source checkout. The extracted model directory measured about
  1052 MiB, first partial latency was 185 ms, final latency was 322 ms, RTF was
  0.210, and CER was 0.0 against `你好呀`.
- The Huihui TTS smoke sample still proves Paraformer can work end-to-end, but
  the packaged default now follows the stronger real-microphone evidence rather
  than the one synthetic sample.
- `Invoke-TalkAsrCorpusBenchmark.ps1` is available in the source tree so the
  same real microphone corpus can be replayed against Zipformer, Paraformer,
  future local streaming engines, and cloud-only OpenAI-compatible baselines
  without hand-maintained command drift.
- `Invoke-TalkAsrCorpusRecorder.ps1` is available in the source tree so the
  real microphone corpus itself can be captured from the same source/CI
  checkout before running the same-corpus benchmark helper. The repository
  includes `asr-real-mic-prompts.json` as a starter prompt manifest for the
  required short Chinese, mixed Chinese/English, mixed
  Chinese/English/Japanese, proper-noun, long punctuation, and realistic-noise
  samples.
- `Select-TalkDefaultAsrModel.ps1` is available in the source tree so the
  final default-model decision can be gated by evidence instead of manually
  reading the comparison JSON or over-trusting a synthetic smoke sample.
- `Set-TalkDefaultAsrModel.ps1` is available in the source tree so a successful
  selection can be applied to `talk-desktop.toml` through a repeatable,
  backup-producing config update instead of manually copying TOML snippets.
- `Invoke-TalkAsrDefaultModelWorkflow.ps1` is available in the source tree so
  the final Task 6 pass can run benchmark -> selection -> optional config apply
  from one source/CI command after the real microphone corpus is recorded.
- `Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1` is available in the source
  tree as the preferred end-to-end engineering operator entry. It chains real
  microphone recording -> same-corpus benchmark -> evidence selection ->
  optional config apply from one release-side command, and it supports
  `-PreflightOnly` so missing models, internal tools, config files, or cloud
  API key environment variables are reported with remediation hints and safe
  copyable commands before the operator spends time recording the corpus. The
  workflow also recognizes a packaged desktop `[provider].api_key` as the cloud
  baseline key source without printing the secret value. Operators can add
  `-ProbeAudio` to preflight when they want a short real microphone signal check;
  this is readiness evidence only and does not replace real dictated corpus
  samples for default-model selection.

## Offline or pre-downloaded archives

If the archive is already present, avoid another download:

```powershell
.\Install-TalkSherpaModel.ps1 `
  -ModelId zipformer-zh-en-punct-int8-480ms `
  -ArchivePath C:\models\sherpa\sherpa-onnx-x-asr-480ms-streaming-zipformer-transducer-zh-en-punct-int8-2026-06-05.tar.bz2 `
  -SkipDownload
```

If the model directory already exists and you want to replace it:

```powershell
.\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms -Force
```

## Validate an extracted model manually

The helper exposes a validation function for tests and manual diagnostics:

```powershell
. .\Install-TalkSherpaModel.ps1
Test-TalkSherpaModelInstall `
  -ModelId zipformer-zh-en-punct-int8-480ms `
  -ModelDir .\.runtime\models\sherpa-onnx\zipformer-zh-en-punct-int8-480ms
```

For transducer models, validation requires:

- `tokens.txt`
- `encoder*.onnx`
- `decoder*.onnx`
- `joiner*.onnx`

For Paraformer models, validation requires:

- `tokens.txt`
- `encoder*.onnx`
- `decoder*.onnx`

## Product bootstrap versus engineering installation

The product bootstrap is intentionally limited to the pinned Zipformer model so
first-run behavior is deterministic and its digest can be reviewed in source.
Download and extraction happen on a background worker after the Windows shell
initializes; the UI remains available while the model is prepared. If network,
digest, extraction, or required-file validation fails, Talk keeps the failure
reason in its status/evidence and uses cloud ASR for the affected session when
configured.

The PowerShell installer remains useful for engineering-only model comparison,
offline archives, Paraformer experiments, and corpus benchmarks. Its `.runtime`
output is deliberately separate from the product cache and must not be copied
into a user release directory.
