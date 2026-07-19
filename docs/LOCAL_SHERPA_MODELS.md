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
.\scripts\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms
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
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
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
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
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
the nested benchmark process. Plain `-PreflightOnly` never records audio; add
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
the same unique sample ID set for every candidate, Zipformer and Paraformer
local candidates, and the cloud-only baseline. It independently re-ranks local
candidates by CER, first partial latency, final latency, RTF, memory, and model
size rather than trusting the comparison JSON order. It intentionally rejects
the current Huihui TTS smoke reports as insufficient production evidence.

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
| `zipformer-zh-en-punct-int8-480ms` | transducer | ~128 MiB | Recommended first real local model. Low-latency streaming Chinese/English with punctuation. |
| `zipformer-zh-int8-2025-06-30` | transducer | ~126 MiB | Chinese-only streaming Zipformer fallback. |
| `paraformer-bilingual-zh-en` | paraformer | ~999 MiB | Larger bilingual streaming Paraformer comparison target. |

The default model is `zipformer-zh-en-punct-int8-480ms`.

Current evidence status:

- `zipformer-zh-en-punct-int8-480ms` has been validated end-to-end through the
  source-built daemon and `asr-bench` on a short Microsoft Huihui Chinese TTS
  WAV. The extracted model directory in that release measured about 162 MiB,
  first partial latency was 255 ms, final latency was 317 ms, RTF was 0.207, and
  CER was 0.333 against the reference `你好呀` because the recognized text was
  `你好`.
- `paraformer-bilingual-zh-en` has also been validated on the same Huihui TTS
  WAV from the source checkout. The extracted model directory measured about
  1052 MiB, first partial latency was 185 ms, final latency was 322 ms, RTF was
  0.210, and CER was 0.0 against `你好呀`.
- On this one synthetic smoke sample, `asr-bench --compare-report` selects
  Paraformer because accuracy is prioritized over the much smaller package size.
- This is a runtime smoke result, not a final accuracy result. It proves the
  sherpa-online paths work, but it does not replace real microphone benchmarks.
  Do not promote Paraformer or reject Zipformer until both are benchmarked on
  the same real microphone clips with the same JSON schema.
- `Invoke-TalkAsrCorpusBenchmark.ps1` is available in the source tree so the
  same real microphone corpus can be replayed against Zipformer, Paraformer,
  future local streaming engines, and cloud-only OpenAI-compatible baselines
  without hand-maintained command drift.
- `Invoke-TalkAsrCorpusRecorder.ps1` is available in the source tree so the
  real microphone corpus itself can be captured from the same source/CI
  checkout before running the same-corpus benchmark helper. The repository
  includes `asr-real-mic-prompts.json` as a starter prompt manifest for the required
  short search, mixed Chinese/English, punctuation, and natural/noisy samples.
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
