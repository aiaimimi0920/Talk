# Talk ASR Benchmarking

Talk will choose local ASR engines by measured interaction quality, not by model popularity alone.

The benchmark harness lives at `tools/asr-bench` and writes a stable JSON report with these fields:

- `engine`: candidate engine label, for example `sherpa-onnx-zipformer`.
  Streaming service mode writes `streaming_service:<engine>:<model>` so
  multiple sherpa-onnx models remain distinguishable in comparison reports.
- `audio_duration_ms`: duration of the input WAV if provided.
- `cold_start_ms`: engine/model initialization time.
- `first_partial_ms`: time from start to first visible partial text.
- `final_latency_ms`: time from start to final local ASR text.
- `rtf`: real-time factor, computed as `final_latency_ms / audio_duration_ms`.
- `peak_rss_mb`: peak resident memory in MB.
- `model_size_mb`: optional extracted/package model size in MB when known.
- `sample_id`: optional corpus sample identifier. Use the same `sample_id`
  for the same utterance across all candidate engines.
- `text`: final recognized text.
- `cer`: character error rate against an optional reference transcript.

Current dry-run mode validates the schema and release plumbing before large model binaries are added:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p asr-bench -- `
  --engine sherpa-onnx-zipformer `
  --dry-run-text "你好" `
  --output-json Talk/target/asr-bench-smoke.json
```

## Streaming service mode

`asr-bench` can also benchmark Talk's loopback local streaming ASR service from
a real WAV file. This is the preferred path for comparing sherpa-onnx streaming
Zipformer and Paraformer models because it exercises the same WebSocket protocol
used by `talk-desktop.exe`.

Input audio must be 16-bit PCM WAV. The recommended comparison format is
16 kHz, mono, signed 16-bit PCM:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p asr-bench -- `
  --engine streaming_service `
  --streaming-endpoint ws://127.0.0.1:53171/asr `
  --audio-wav Talk/.runtime/asr-bench/sample-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --sample-id short-search-001 `
  --model-size-mb 162 `
  --output-json Talk/.runtime/asr-bench/streaming-service-report.json
```

From a packaged release, use the bundled tool:

```powershell
.\.internal\asr-bench.exe `
  --engine streaming_service `
  --streaming-endpoint ws://127.0.0.1:53171/asr `
  --audio-wav .\.runtime\asr-bench\sample-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --sample-id short-search-001 `
  --model-size-mb 162 `
  --output-json .\.runtime\asr-bench\streaming-service-report.json
```

Useful timing knobs:

- `--chunk-ms 80`: controls how many milliseconds of PCM are sent per audio
  frame. Smaller chunks can improve first-partial latency at higher overhead.
- `--connect-timeout-ms 1000`: caps loopback connection setup time.
- `--ready-timeout-ms 1000`: caps model ready wait time after `start`.
- `--partial-idle-timeout-ms 10`: controls how long the client polls for
  immediately available partials after each chunk.
- `--final-timeout-ms 7000`: caps the wait for a final transcript after `stop`.
- `--model-size-mb 162`: optionally records the extracted or packaged model
  size so report comparison can account for distribution cost.
- `--sample-id short-search-001`: records which corpus item produced the
  report. This lets comparison mode reject accidental comparisons where
  Zipformer and Paraformer were run on different utterance sets.

## Cloud-only OpenAI-compatible baseline mode

`asr-bench` can benchmark the current cloud transcription path against the same
corpus. This is intentionally a baseline, not the desired interaction model:
cloud-only transcription has no local partial text, so the report records
`first_partial_ms == final_latency_ms`. That makes the latency gap against
local-first streaming visible in the same JSON schema.

Chat Completions audio-input example:

```powershell
$env:TALK_PROVIDER_API_KEY = '<redacted>'
cargo run --manifest-path Talk/Cargo.toml -p asr-bench -- `
  --cloud-openai-compatible-endpoint https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions `
  --cloud-openai-compatible-model qwen3-asr-flash `
  --cloud-openai-compatible-transport chat_completions_audio_input `
  --cloud-openai-compatible-api-key-env TALK_PROVIDER_API_KEY `
  --audio-wav Talk/.runtime/asr-bench/real-mic-corpus/short-search-001-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --sample-id short-search-001 `
  --output-json Talk/.runtime/asr-bench/real-mic-corpus/reports/cloud-openai-compatible-short-search-001.json
```

Audio Transcriptions endpoint example:

```powershell
$env:TALK_PROVIDER_API_KEY = '<redacted>'
.\.internal\asr-bench.exe `
  --cloud-openai-compatible-endpoint https://example.invalid/v1/audio/transcriptions `
  --cloud-openai-compatible-model whisper-compatible-model `
  --cloud-openai-compatible-transport audio_transcriptions `
  --cloud-openai-compatible-api-key-env TALK_PROVIDER_API_KEY `
  --audio-wav .\.runtime\asr-bench\real-mic-corpus\short-search-001-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --sample-id short-search-001 `
  --output-json .\.runtime\asr-bench\real-mic-corpus\reports\cloud-openai-compatible-short-search-001.json
```

The benchmark reads the bearer token from the named environment variable and
does not print the key. Keep API keys out of command histories, reports, and
docs.

## Comparing candidate reports

Use comparison mode after producing JSON reports for each candidate on the same
WAV set. The tool automatically groups reports by `engine`, averages latency,
RTF, and CER across that engine's samples, uses max memory/model-size values,
and then ranks the aggregated candidates. The current selector intentionally
prioritizes correctness first, then interaction latency:

1. lower `cer`,
2. lower `first_partial_ms`,
3. lower `final_latency_ms`,
4. lower `rtf`,
5. lower `peak_rss_mb`,
6. lower optional `model_size_mb`.

Example:

```powershell
cargo run --manifest-path Talk/Cargo.toml -p asr-bench -- `
  --compare-report Talk/.runtime/asr-bench/zipformer-short-search-001.json `
  --compare-report Talk/.runtime/asr-bench/zipformer-mixed-english-001.json `
  --compare-report Talk/.runtime/asr-bench/paraformer-short-search-001.json `
  --compare-report Talk/.runtime/asr-bench/paraformer-mixed-english-001.json `
  --output-json Talk/.runtime/asr-bench/asr-model-comparison.json
```

From a packaged release:

```powershell
.\.internal\asr-bench.exe `
  --compare-report .\.runtime\asr-bench\zipformer-short-search-001.json `
  --compare-report .\.runtime\asr-bench\zipformer-mixed-english-001.json `
  --compare-report .\.runtime\asr-bench\paraformer-short-search-001.json `
  --compare-report .\.runtime\asr-bench\paraformer-mixed-english-001.json `
  --output-json .\.runtime\asr-bench\asr-model-comparison.json
```

If any report includes `sample_id`, all compared reports must include it and
each engine must cover the same `sample_id` set. If reports omit `sample_id`,
comparison mode still requires each engine to have the same number of samples,
but it cannot prove the utterance sets match.

The selected engine is only as reliable as the corpus behind the reports. A
single synthetic TTS file is useful for smoke-testing the runtime path, but it
is not enough to lock a production default. Use multiple real microphone clips
covering short commands, search-bar input, punctuation, mixed Chinese/English,
and noisy-room speech before changing Talk's default model.

## Recording a real microphone corpus

Use `Invoke-TalkAsrCorpusRecorder.ps1` to create the real microphone WAV set
that drives model selection. The recorder reads a prompt manifest, calls
`talk.exe probe-audio --json` for each sample, copies the captured 16 kHz mono
WAV into a stable corpus directory, and writes a benchmark-ready `corpus.json`.

Prompt manifest example:

```json
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "short-search-001",
      "referenceText": "你好呀",
      "captureSeconds": 2
    },
    {
      "sampleId": "mixed-english-001",
      "referenceText": "打开 Talk 的 local first ASR 测试",
      "captureSeconds": 5
    }
  ]
}
```

From a source checkout:

```powershell
.\Talk\scripts\Invoke-TalkAsrCorpusRecorder.ps1 `
  -PromptManifest .\Talk\.runtime\asr-bench\real-mic-corpus\prompts.json `
  -OutputRoot .\Talk\.runtime\asr-bench\real-mic-corpus `
  -TalkExe .\Talk\target\release\talk.exe `
  -InputDevice '<optional exact input device>' `
  -DefaultCaptureSeconds 3
```

From a packaged release:

```powershell
.\Invoke-TalkAsrCorpusRecorder.ps1 `
  -PromptManifest .\asr-real-mic-prompts.json `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus `
  -InputDevice '<optional exact input device>' `
  -DefaultCaptureSeconds 3
```

Packaged releases include `asr-real-mic-prompts.json` as a starter manifest
with short search, mixed Chinese/English, punctuation, and natural/noisy speech
samples. Copy or edit it before recording if you need a different corpus, but
keep the same `sampleId` set when comparing Zipformer, Paraformer, and the
cloud baseline.

The recorder rejects silent captures by default. Use `-AllowSilent` only for
diagnosing device routing, not for production model-selection corpora. Use
`-PlanOnly` first to inspect output names without touching the microphone:

```powershell
.\Invoke-TalkAsrCorpusRecorder.ps1 `
  -PromptManifest .\.runtime\asr-bench\real-mic-corpus\prompts.json `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus `
  -PlanOnly
```

Recorder output:

```text
<output-root>\recording-config.toml
<output-root>\<sample-id>-16k-mono-s16.wav
<output-root>\corpus.json
<output-root>\.captures\
<output-root>\logs\
```

## Same-corpus helper script

For real model selection, prefer the release/source helper script over ad-hoc
one-off commands. It enforces the important rule that every candidate model is
run against the same `sample_id` set, records model size, writes per-sample JSON
reports, and then calls `asr-bench --compare-report`.

Corpus manifest schema:

```json
{
  "schemaVersion": 1,
  "samples": [
    {
      "sampleId": "short-search-001",
      "audioWav": "short-search-001-16k-mono-s16.wav",
      "referenceText": "你好呀"
    }
  ]
}
```

Rules:

- `sampleId` must be stable across engines and use only letters, numbers,
  `.`, `_`, or `-`.
- `audioWav` may be absolute or relative to the manifest file.
- `referenceText` is the human transcript used for CER.
- The WAV should be 16 kHz, mono, signed 16-bit PCM.

From a source checkout:

```powershell
.\Talk\scripts\Invoke-TalkAsrCorpusBenchmark.ps1 `
  -CorpusManifest .\Talk\.runtime\asr-bench\real-mic-corpus\corpus.json `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -ModelRoot .\Talk\.runtime\models\sherpa-onnx `
  -OutputRoot .\Talk\.runtime\asr-bench\real-mic-corpus\reports `
  -CloudOpenAiCompatibleEndpoint https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions `
  -CloudOpenAiCompatibleModel qwen3-asr-flash `
  -CloudOpenAiCompatibleTransport chat_completions_audio_input `
  -CloudOpenAiCompatibleApiKeyEnv TALK_PROVIDER_API_KEY
```

From a packaged release:

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

Use `-PlanOnly` first if you want to validate model paths, output paths, and
benchmark arguments without starting any daemon process:

```powershell
.\Invoke-TalkAsrCorpusBenchmark.ps1 `
  -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -PlanOnly
```

The helper starts `talk-local-asr-sherpa.exe` once per model, waits for the
loopback endpoint, runs every corpus sample through `.internal\asr-bench.exe`,
stops the daemon, and finally writes:

```text
<output-root>\<model-id>-<sample-id>.json
<output-root>\cloud-openai-compatible-<transport>-<sample-id>.json
<output-root>\asr-model-comparison.json
<output-root>\logs\<model-id>-daemon.out.log
<output-root>\logs\<model-id>-daemon.err.log
```

## End-to-end real microphone default-model workflow

For the final release-side Task 6 pass, prefer
`Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1`. It chains the full operator
flow:

1. real microphone corpus recording from `asr-real-mic-prompts.json`,
2. same-corpus Zipformer/Paraformer/cloud benchmarking,
3. evidence-gated local default selection,
4. optional `talk-desktop.toml` config locking.

From a packaged release, install the candidate sherpa models first, set
`TALK_PROVIDER_API_KEY` for the cloud baseline, then run:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

By default the wrapper uses:

- prompt manifest: `.\asr-real-mic-prompts.json`,
- corpus root: `.\.runtime\asr-bench\real-mic-corpus`,
- reports root: `.\.runtime\asr-bench\real-mic-corpus\reports`,
- cloud endpoint:
  `https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`,
- cloud model: `qwen3-asr-flash`,
- cloud API key environment variable: `TALK_PROVIDER_API_KEY`.

Run `-PreflightOnly` before asking an operator to record the corpus. It checks
the prompt manifest, `talk.exe`, `.internal\asr-bench.exe`, the packaged local
ASR daemon, every requested installed sherpa model, the cloud baseline API key
environment variable, and the target `talk-desktop.toml` without recording
audio or starting a benchmark. If the process environment does not contain the
cloud baseline API key, the workflow can use the packaged desktop
`[provider].api_key` as the key source for the cloud baseline run. The real key
is never printed in the preflight object:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml `
  -PreflightOnly
```

The preflight result includes `Checks[]` with `RemediationHint` fields and a
deduplicated `RemediationCommands[]` list for common blockers. Typical commands
include model installers such as:

```powershell
.\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms -DestinationRoot '<release>\.runtime\models\sherpa-onnx'
```

and a safe API-key placeholder:

```powershell
$env:TALK_PROVIDER_API_KEY = '<redacted>'
```

The placeholder is intentional; do not write real provider keys into docs or
benchmark reports.

During the full workflow, a packaged desktop config key is injected only into
the current PowerShell process for the nested benchmark/selection call and is
restored afterward. If the environment variable is already set, the workflow
uses the environment value and does not overwrite it.

By default `-PreflightOnly` does not record audio. If an operator wants to prove
the selected Windows microphone is ready before spending time on the full corpus,
add `-ProbeAudio`. This runs one short `talk.exe probe-audio --json` capture,
adds a `microphone_signal` check, and blocks only when the backend is not ready
or the probe records silence:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml `
  -PreflightOnly `
  -ProbeAudio `
  -AudioProbeSeconds 2
```

Run `-PlanOnly` if you only want to check the recording, report, model, and
config paths without checking file existence:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml `
  -PlanOnly
```

If the operator needs to capture real speech first and run the expensive
Zipformer/Paraformer/cloud matrix later, stage the workflow with
`-RecordOnly`. Its preflight checks only the recording prerequisites and skips
benchmark executables, model installs, cloud API keys, and config apply:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -RecordOnly `
  -PreflightOnly
```

Add `-ProbeAudio` when that staged preflight should also capture a short
non-silent microphone signal through the recorder config, still without
requiring benchmark prerequisites:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -RecordOnly `
  -PreflightOnly `
  -ProbeAudio `
  -AudioProbeSeconds 2
```

Then record the corpus and stop immediately after `corpus.json` is written:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -RecordOnly
```

After a successful `-RecordOnly` run, the workflow also writes:

```text
.\.runtime\asr-bench\real-mic-corpus\record-only-status.json
```

This status artifact records whether the staged corpus is ready to reuse. It
includes the prompt manifest path, `corpus.json` path, planned sample count,
recorded sample count, existing WAV count, missing WAV paths if any, and a
copyable `nextCommand` that resumes the Task 6 evidence gate with
`-SkipRecording`.

When resuming with `-SkipRecording -PreflightOnly`, the workflow reports a
`record_only_status` check. If `record-only-status.json` is present, it must be
ready and its `corpusManifest` must match the current `CorpusRoot\corpus.json`;
otherwise preflight blocks the run. A full `-SkipRecording` run applies the same
guard before starting Zipformer/Paraformer/cloud benchmarking, so an incomplete
staged corpus cannot accidentally consume operator or provider time. If the
status file is absent, the check is marked `skipped` rather than blocking; this
keeps older or manually supplied corpus manifests usable.

`-RecordOnly` and `-SkipRecording` are intentionally mutually exclusive:
`-RecordOnly` means "capture a corpus now and stop", while `-SkipRecording`
means "reuse an existing corpus and continue to benchmark/selection".

When the operator is ready to benchmark and lock the default, reuse that corpus
with `-SkipRecording`:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -SkipRecording `
  -CorpusRoot .\.runtime\asr-bench\real-mic-corpus `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

The workflow intentionally still fails if the real-microphone evidence is weak:
the default selector requires real sample IDs, enough samples, the same sample
set for all candidates, both local model families, and a cloud baseline unless
diagnostic-only override switches are explicitly passed.

## Existing-corpus default-model workflow

Once a real microphone `corpus.json` already exists and the candidate sherpa
models are installed, the lower-level workflow wrapper can run only the final
benchmark/selection/apply pass. It chains:

1. same-corpus Zipformer/Paraformer/cloud benchmarking,
2. evidence-gated local default selection,
3. optional `talk-desktop.toml` config locking.

From a packaged release:

```powershell
.\Invoke-TalkAsrDefaultModelWorkflow.ps1 `
  -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus\reports `
  -CloudOpenAiCompatibleEndpoint https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions `
  -CloudOpenAiCompatibleModel qwen3-asr-flash `
  -CloudOpenAiCompatibleTransport chat_completions_audio_input `
  -CloudOpenAiCompatibleApiKeyEnv TALK_PROVIDER_API_KEY `
  -ConfigPath .\talk-desktop.toml
```

Use `-PlanOnly` first to verify release-relative paths without starting the
local daemon or calling the cloud endpoint:

```powershell
.\Invoke-TalkAsrDefaultModelWorkflow.ps1 `
  -CorpusManifest .\.runtime\asr-bench\real-mic-corpus\corpus.json `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus\reports `
  -ConfigPath .\talk-desktop.toml `
  -PlanOnly
```

Use `-SkipApply` when you want the benchmark and
`selected-default-asr-model.json` but do not want the workflow to edit
`talk-desktop.toml`. Diagnostic-only switches such as
`-AllowMissingCloudBaseline` and `-AllowSyntheticSampleIds` are still passed to
the selector; they should not be used when locking a production default.
After benchmarking, the workflow now also writes:

```text
<output-root>\asr-model-evidence-status.json
```

This status file is produced before the strict selection step. If the production
selector rejects incomplete evidence, the status file remains available and
contains the full blocker list instead of only the first thrown error.

## Evidence-gated default model selection

After producing `asr-model-comparison.json`, run the selector helper before
changing Talk's default local ASR model:

```powershell
.\Select-TalkDefaultAsrModel.ps1 `
  -ComparisonJson .\.runtime\asr-bench\real-mic-corpus\reports\asr-model-comparison.json `
  -OutputJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json
```

If the comparison is still incomplete, inspect the gate without writing a
selection file:

```powershell
.\Select-TalkDefaultAsrModel.ps1 `
  -ComparisonJson .\.runtime\asr-bench\real-mic-corpus\reports\asr-model-comparison.json `
  -StatusOnly
```

`-StatusOnly` returns `ready`, `blockingReasons`, `missingLocalModelIds`,
`cloudBaselinePresent`, per-candidate sample checks, and the ranked local
candidates when enough evidence exists. This is the preferred diagnostic after
recording or benchmarking because it reports all current evidence blockers at
once instead of stopping at the first thrown selector error.

The selector is deliberately stricter than `asr-bench --compare-report`.
`asr-bench` can compare any report set, including one-off smoke tests; the
selector decides whether the evidence is strong enough to lock Talk's default
local model. By default it requires:

- at least 3 samples per candidate,
- real sample IDs rather than `huihui`, `tts`, `synthetic`, or `smoke` IDs,
- the same unique `sample_id` set for every local and cloud candidate,
- both required local candidates:
  - `zipformer-zh-en-punct-int8-480ms`,
  - `paraformer-bilingual-zh-en`,
- an OpenAI-compatible cloud baseline candidate.

The output file records the selected local model ID, selected engine, global
comparison winner, cloud-baseline presence, and ranked local candidates:

```text
<output-root>\selected-default-asr-model.json
```

If the cloud baseline globally wins the raw comparison, the selector still
chooses the best local candidate for Talk's local-first runtime and records the
cloud result as baseline evidence. The selector re-ranks local candidates by
the same evidence metrics instead of trusting the input JSON order. Use
`-AllowMissingCloudBaseline` or
`-AllowSyntheticSampleIds` only for diagnostics; do not use those flags to lock
the production default.

After the selector succeeds, apply the selected installed sherpa model to the
desktop config:

```powershell
.\Set-TalkDefaultAsrModel.ps1 `
  -SelectionJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json `
  -ConfigPath .\talk-desktop.toml `
  -ModelRoot .\.runtime\models\sherpa-onnx
```

The applier validates the selected model with `Test-TalkSherpaModelInstall`,
backs up the config by default, and writes an active
`[speculative.streaming_service.local_daemon]` block. It replaces an existing
active block when present and leaves commented example blocks untouched.

Concrete adapters should keep the same report shape so sherpa-onnx streaming
Zipformer, sherpa-onnx Paraformer, FunASR/SenseVoice, faster-whisper, and cloud
baselines can be compared by:

1. Chinese dictation CER before cloud correction,
2. first partial latency,
3. final latency,
4. real-time factor,
5. memory footprint,
6. CPU/GPU cost and package size.
