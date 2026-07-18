# Talk Local-First ASR Roadmap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Evolve Talk from cloud-only stop-then-transcribe dictation into a local-first streaming input method that shows instant local ASR drafts and later applies safe model corrections.

**Architecture:** Talk keeps the Typeless-style desktop interaction unchanged while moving recognition into a layered pipeline: native mic PCM capture, loopback local streaming ASR, immediate draft display, sentence-level text correction, and conservative patch/insert output. Local ASR is selected by measured first-partial latency, final latency, real-time factor, memory, and Chinese dictation CER rather than by model reputation alone.

**Tech Stack:** Rust workspace under `Talk`, `tokio-tungstenite` WebSocket streaming protocol, `sherpa-onnx` local ASR daemon, PowerShell release/model scripts, Cargo/Pester validation, Windows desktop hotkey/HUD/output path.

---

## File structure

- Existing: `Talk/crates/talk-audio/src/lib.rs` captures native Windows PCM and exposes live drain helpers.
- Existing: `Talk/crates/talk-client/src/streaming_asr.rs` implements the loopback streaming ASR WebSocket client.
- Existing: `Talk/crates/talk-runtime/src/lib.rs` bridges recording, speculative local ASR, provider correction, and final session logging.
- Existing: `Talk/crates/talk-desktop/src/main.rs` owns the RightAlt interaction loop, non-activating HUD, insertion target capture, and popup fallback.
- Existing: `Talk/tools/talk-local-asr-sherpa/src/main.rs` hosts dry-run and `sherpa-online` local streaming ASR.
- Existing: `Talk/scripts/Install-TalkSherpaModel.ps1` installs validated sherpa model bundles into release/runtime directories.
- Modify: `Talk/tools/asr-bench/src/main.rs` to benchmark real streaming service endpoints from WAV PCM.
- Modify: `Talk/scripts/Publish-TalkRelease.ps1` to package `asr-bench.exe` with the desktop release.
- Add: `Talk/scripts/Invoke-TalkAsrCorpusRecorder.ps1` to record a stable real microphone WAV corpus and benchmark manifest.
- Add: `Talk/scripts/Select-TalkDefaultAsrModel.ps1` to reject weak benchmark evidence and write the selected default local ASR model record.
- Add: `Talk/scripts/Set-TalkDefaultAsrModel.ps1` to apply an evidence-selected installed sherpa model to `talk-desktop.toml`.
- Add: `Talk/scripts/Invoke-TalkAsrDefaultModelWorkflow.ps1` to run benchmark, selection, and optional config locking as one release-side command.
- Add: `Talk/scripts/Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1` to run real microphone corpus recording, benchmark, selection, and optional config locking as one release-side Task 6 command.
- Modify: `Talk/docs/ASR_BENCHMARKING.md` and `Talk/docs/LOCAL_SHERPA_MODELS.md` to document model validation and benchmark commands.

## Task 1: Keep the local streaming foundation stable

**Files:**
- Already modified: `Talk/crates/talk-client/src/streaming_asr.rs`
- Already modified: `Talk/crates/talk-runtime/src/lib.rs`
- Already modified: `Talk/crates/talk-desktop/src/main.rs`
- Already modified: `Talk/tools/talk-local-asr-sherpa/src/main.rs`

- [x] **Step 1: Define a stable streaming ASR protocol**

Use JSON WebSocket messages for `start`, `ready`, `audio`, `partial`, `final`, `error`, and `stop`. Keep PCM payloads base64-encoded so the protocol can be inspected in logs and tests.

- [x] **Step 2: Pump live PCM into the local service**

During recording, drain captured PCM chunks into the streaming ASR client and collect currently available partial events without blocking the HUD timer.

- [x] **Step 3: Render partial drafts without stealing focus**

Show the latest partial transcript inside the listening HUD while preserving the non-activating Typeless-style overlay and the existing insertion/fallback rules.

- [x] **Step 4: Package the local ASR daemon**

Publish `.internal\talk-local-asr-sherpa.exe` and the required `sherpa-onnx`/ONNX Runtime DLLs with `talk-desktop.exe`.

## Task 2: Add a real streaming ASR benchmark/probe tool

**Files:**
- Modify: `Talk/tools/asr-bench/src/main.rs`
- Modify: `Talk/tools/asr-bench/Cargo.toml`

- [x] **Step 1: Write the failing streaming_service benchmark test**

Add `streaming_service_benchmark_sends_wav_and_records_partial_and_final`, which starts a fake loopback ASR WebSocket server, sends a WAV through the benchmark tool, emits a `partial` event, emits a `final` event, and verifies the JSON report records text, CER, first partial latency, final latency, and audio duration.

- [x] **Step 2: Verify RED**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p asr-bench streaming_service_benchmark_sends_wav_and_records_partial_and_final -- --nocapture
```

Expected before implementation: unresolved streaming benchmark API/dependency failures.

- [x] **Step 3: Implement WAV-to-streaming-service benchmarking**

Add `--streaming-endpoint`, chunk 16-bit PCM WAV into configurable millisecond windows, send `start/audio/stop`, collect `partial` and `final`, calculate CER/RTF/latencies, and write the same JSON schema as dry-run mode.

- [x] **Step 4: Verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Talk\Cargo.toml -p asr-bench streaming_service_benchmark_sends_wav_and_records_partial_and_final -- --nocapture
```

Expected after implementation: the focused test passes and prints a `streaming_service:fake` report.

## Task 3: Package benchmark tooling in Talk releases

**Files:**
- Modify: `Talk/scripts/Publish-TalkRelease.ps1`
- Modify: `Talk/scripts/tests/Publish-TalkRelease.Tests.ps1`

- [x] **Step 1: Write the failing release packaging test**

Add `packages the ASR benchmark tool for release-side local ASR validation`. It publishes a mocked release and expects `.internal\asr-bench.exe` plus a manifest support file with `kind = "asr-benchmark-tool"` and `path = ".internal/asr-bench.exe"`.

- [x] **Step 2: Verify RED**

Run:

```powershell
Invoke-Pester -Script .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected before implementation: the new test fails because `.internal\asr-bench.exe` is missing.

- [x] **Step 3: Implement release packaging**

Build `-p asr-bench`, require `target\release\asr-bench.exe`, copy it into `.internal`, add it to BUILD_INFO artifacts, and add the manifest support file entry.

- [x] **Step 4: Verify GREEN**

Run:

```powershell
cargo build --manifest-path .\Talk\Cargo.toml --release -p asr-bench
Invoke-Pester -Script .\Talk\scripts\tests\Publish-TalkRelease.Tests.ps1
```

Expected after implementation: the full publish test file passes with the ASR benchmark tool packaged.

## Task 4: Run real model validation

**Files:**
- Use: `Talk/scripts/Install-TalkSherpaModel.ps1`
- Use: `Talk/tools/asr-bench/src/main.rs`
- Output: `Talk/.runtime/asr-bench/*.json` or release-side `.runtime\asr-bench\*.json`

- [x] **Step 1: Install the recommended first local model**

Run from source or a release directory:

```powershell
.\Install-TalkSherpaModel.ps1 -ModelId zipformer-zh-en-punct-int8-480ms
```

Expected: the installer validates `tokens.txt`, encoder, decoder, joiner, and writes `talk-local-daemon.toml.snippet`.

- [x] **Step 2: Start the sherpa daemon in real mode**

Run `.internal\talk-local-asr-sherpa.exe` or `target\release\talk-local-asr-sherpa.exe` with the generated snippet values and `--mode sherpa-online`.

Expected: the daemon binds `127.0.0.1:53171` and reports a ready model when a client connects.

- [x] **Step 3: Benchmark a controlled Chinese TTS WAV**

Generate a local `Microsoft Huihui Desktop` Chinese WAV, resample it to
16 kHz mono signed 16-bit PCM, start the real `sherpa-online` daemon, and run
the release-bundled `asr-bench.exe`.

Observed release-side report:

```json
{
  "engine": "streaming_service:sherpa-onnx",
  "audio_duration_ms": 1530,
  "first_partial_ms": 255,
  "final_latency_ms": 317,
  "rtf": 0.20718954248366014,
  "text": "你好",
  "cer": 0.3333333333333333
}
```

This validates the real local inference path. It is not a final accuracy
measurement because the source was synthetic TTS and the target phrase was
`你好呀`.

- [x] **Step 4: Benchmark real human speech WAVs**

Use the packaged/source recorder helper to create real microphone samples:

```powershell
.\Invoke-TalkAsrCorpusRecorder.ps1 `
  -PromptManifest .\.runtime\asr-bench\real-mic-corpus\prompts.json `
  -OutputRoot .\.runtime\asr-bench\real-mic-corpus `
  -DefaultCaptureSeconds 3
```

Then run:

```powershell
.\.internal\asr-bench.exe `
  --engine streaming_service `
  --streaming-endpoint ws://127.0.0.1:53171/asr `
  --audio-wav .\.runtime\asr-bench\real-mic-corpus\short-search-001-16k-mono-s16.wav `
  --reference-text "你好呀" `
  --sample-id short-search-001 `
  --output-json .\.runtime\asr-bench\real-mic-corpus\reports\zipformer-480ms-short-search-001.json
```

Expected: each report contains a non-empty transcript, first partial latency,
final latency, RTF, and CER for real microphone speech.

## Task 5: Add sentence-level correction patches

**Files:**
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Modify: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/tools/talk-local-asr-sherpa/src/main.rs`
- Modify: `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`

- [x] **Step 1: Detect stable sentence chunks**

Treat a chunk as correction-ready when local ASR produces final punctuation, a stable segment boundary, or a configurable minimum length with idle time.

- [x] **Step 2: Send text-only correction requests**

Send local ASR text plus bounded context to the configured text correction provider, not the raw audio, so the user gets immediate local text while quality improves asynchronously.

- [x] **Step 3: Patch only safe visible text**

Apply cloud/local LLM corrections only when the edit ratio is below the configured threshold and the active target/session still matches the original dictation target.

- [x] **Step 4: Fall back to editable popup when unsafe**

If the target changed, the control cannot be patched, or the correction is too large, show the editable copy popup with the corrected candidate instead of stealing focus or modifying another application.

Current implementation note: runtime now emits
`SpeculativeRuntimeEvent::CorrectionRequested { segment_id, local_text,
context_before }` from stable local ASR chunks and de-duplicates requests per
segment id, and it no longer emits duplicate local commits for the same stable
segment. Desktop now pumps live streaming ASR events during active recording,
inserts stable local chunks only when the original editable target is still
active, sends text-only correction jobs with bounded `context_before`, and
applies corrections only when both the focus anchor and live-tail guard remain
safe. Stop-time full-final insertion is suppressed after live insertion to avoid
duplicates; an uncommitted final tail is inserted separately when safe or shown
in the editable copy popup. The packaged local sherpa daemon now keeps
partial/final segment ids stable within one utterance.

## Task 6: Select the default local ASR engine by evidence

**Files:**
- Modify: `Talk/docs/ASR_BENCHMARKING.md`
- Modify: `Talk/docs/LOCAL_SHERPA_MODELS.md`
- Add if needed: `Talk/docs/asr-benchmarks/<model-id>.json`

Current implementation note: `asr-bench` now supports report comparison with
`--compare-report`, keeps old reports compatible through optional
`model_size_mb` and `sample_id`, groups multiple reports by engine, rejects
mismatched sample-id sets, and ranks aggregated candidates by mean CER first,
then mean first partial latency, mean final latency, mean RTF, max memory, and
max package size. A Zipformer Huihui TTS smoke report and a Paraformer Huihui
TTS smoke report are recorded under `docs/asr-benchmarks`; the synthetic
comparison selects Paraformer on accuracy. Task 6 remains open because the
production default still needs same-corpus real microphone measurements for
Zipformer, Paraformer, and cloud-only baseline.

Follow-up implementation note: `Invoke-TalkAsrCorpusBenchmark.ps1` now provides
the repeatable same-corpus runner for Task 6. It reads a manifest with stable
`sampleId` values, validates installed sherpa models, starts the local daemon
once per model, runs every WAV through `asr-bench`, records `model_size_mb`, and
produces the final `asr-model-comparison.json`. The script is also packaged in
desktop releases as `asr-corpus-benchmark-helper`. This removes command drift
from Zipformer/Paraformer selection, but it still needs real microphone WAVs
and cloud-only baseline reports before the default can be locked.

Follow-up implementation note: `asr-bench` now also supports an
OpenAI-compatible cloud-only baseline through
`--cloud-openai-compatible-endpoint`, `--cloud-openai-compatible-model`, and
`--cloud-openai-compatible-transport`. The same-corpus helper can include that
baseline in the final comparison without starting another local daemon. Cloud
reports intentionally set `first_partial_ms` to `final_latency_ms` because the
cloud-only path has no streaming local partials. This makes the local-first
latency advantage measurable, but real microphone WAVs and actual provider
responses are still required before the production default is locked.

Follow-up implementation note: `Invoke-TalkAsrCorpusRecorder.ps1` now provides
the repeatable real microphone corpus capture path for Task 4 and Task 6. It
reads a prompt manifest, records each sample through `talk.exe probe-audio`,
rejects silent captures by default, copies 16 kHz mono WAV files to stable
`<sample-id>-16k-mono-s16.wav` names, and writes a benchmark-ready `corpus.json`.
It is packaged in desktop releases as `asr-corpus-recorder-helper`. The helper
removes manual WAV naming drift, but it does not replace the actual operator
recording pass that is still needed before locking the production default.

Follow-up implementation note: `Select-TalkDefaultAsrModel.ps1` now provides the
evidence gate for Task 6 Step 4. It reads `asr-model-comparison.json`, requires
at least three real microphone samples per candidate by default, rejects
synthetic/smoke sample ids, requires every candidate to use the same unique
sample-id set, requires both Zipformer and Paraformer local candidates, requires
the cloud-only baseline, re-ranks local candidates by evidence metrics, and writes
`selected-default-asr-model.json` with the selected local model id and ranked
local candidates. This makes the final default-model update auditable, but the
selector will intentionally reject the current Huihui TTS comparison until real
microphone reports exist.

Follow-up implementation note: `Set-TalkDefaultAsrModel.ps1` now provides the
final config-locking step after selection succeeds. It reads
`selected-default-asr-model.json`, rejects non-evidence-ready records, validates
the selected installed model through `Test-TalkSherpaModelInstall`, backs up the
target desktop config by default, and replaces or appends the active
`[speculative.streaming_service.local_daemon]` block. It is packaged in desktop
releases as `asr-default-model-applier`, so the release-side Task 6 workflow no
longer depends on manually copying TOML snippets.

Follow-up implementation note: the release-side applier smoke has been validated
against `desktop-shell-local-first-asr-default-applier-v2`. The packaged
`Set-TalkDefaultAsrModel.ps1` replaces an active local daemon block, creates the
default backup, rejects `evidenceReady = false`, and preserves LF-only TOML
newline style instead of mixing CRLF into source-controlled configs.

Follow-up implementation note: releases now package `asr-real-mic-prompts.json`
as a starter corpus prompt manifest. It covers short search input, mixed
Chinese/English, punctuation, and natural/noisy speech so the final Task 6
recording pass can start from the release directory without hand-writing the
prompt JSON.

Follow-up implementation note: `desktop-shell-local-first-asr-task6-ready-v4`
has release-side plan-only validation for both corpus recording and corpus
benchmarking. `Invoke-TalkAsrCorpusRecorder.ps1` and
`Invoke-TalkAsrCorpusBenchmark.ps1` now resolve explicit relative paths against
the current PowerShell FileSystem location instead of the process working
directory, so release commands like `.\asr-real-mic-prompts.json` and
`.\.runtime\asr-bench\real-mic-corpus\corpus.json` remain stable when launched
from wrappers.

Follow-up implementation note: `Invoke-TalkAsrDefaultModelWorkflow.ps1` now
provides the release-side one-command Task 6 wrapper. It runs the same-corpus
benchmark, passes the comparison JSON through `Select-TalkDefaultAsrModel.ps1`,
and optionally applies the selected installed model through
`Set-TalkDefaultAsrModel.ps1`. It is packaged as
`asr-default-model-workflow` and also resolves explicit relative paths against
the current PowerShell FileSystem location so release-directory commands remain
stable under wrapper hosts.

Follow-up implementation note: `Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1`
now provides the preferred end-to-end release-side Task 6 operator entry. It
chains `Invoke-TalkAsrCorpusRecorder.ps1` with the default-model workflow, uses
the packaged real microphone prompts and Qwen OpenAI-compatible cloud baseline
defaults, supports `-PreflightOnly`, `-PlanOnly`, `-SkipRecording`, and `-SkipApply`, and is
packaged as `asr-real-mic-default-model-workflow`. This removes the final
manual gap between corpus recording and evidence-gated config locking, while
still requiring actual real microphone speech before the selector will accept a
production default.

Follow-up implementation note: the real microphone Task 6 workflow now has a
preflight-only mode that checks the prompt manifest, recording `talk.exe`, the
release ASR benchmark tool, local ASR daemon, installed sherpa model directories,
cloud baseline API key environment variable, and target desktop config before
recording starts. This keeps operator time focused on collecting speech rather
than discovering missing prerequisites after the corpus is captured.

Follow-up implementation note: the same preflight result now carries
per-check `RemediationHint` text and a deduplicated `RemediationCommands` list.
Missing or invalid sherpa candidates point to the exact
`Install-TalkSherpaModel.ps1` command for the selected model root, and missing
cloud baseline keys return only a redacted environment-variable template. This
makes the release-side real microphone pass more self-service without exposing
provider secrets.

Follow-up implementation note: the real microphone default-model workflow now
uses a packaged desktop `[provider].api_key` as the cloud baseline key source
when the configured API-key environment variable is blank. Preflight reports the
key as available from desktop config without printing it, and the full workflow
temporarily injects that value only for the nested benchmark/selection call
before restoring the process environment. This removes a redundant operator step
for releases that already package the Qwen/DashScope key.

Follow-up implementation note: the cloud baseline default model was corrected
from `qwen-audio-asr-latest` to `qwen3-asr-flash`. A non-production Huihui TTS
smoke through DashScope compatible chat-audio input returned HTTP 404 for the
old model and succeeded for `qwen3-asr-flash`, matching the current desktop
Qwen transcription config. This validates the cloud baseline transport path but
does not replace the required real microphone corpus.

Follow-up implementation note: the real microphone Task 6 preflight now supports
an optional short microphone signal gate through `-ProbeAudio` and
`-AudioProbeSeconds`. Plain `-PreflightOnly` still avoids recording audio. When
the operator opts in, the workflow runs `talk.exe probe-audio --json`, adds a
`microphone_signal` check, and blocks if the Windows microphone backend is not
ready or the probe is silent. A release-side manual probe on the current host
captured a non-silent `麦克风` signal with peak about 0.1499 and RMS about
0.0177, but that is only readiness evidence; it does not replace the required
same-corpus real human speech benchmarks for Zipformer, Paraformer, and the
cloud baseline.

Follow-up implementation note: `Select-TalkDefaultAsrModel.ps1` now supports
`-StatusOnly` for post-benchmark evidence diagnostics. It reads the same
`asr-model-comparison.json` but does not write a selection file; instead it
returns `ready`, all current `blockingReasons`, missing required local model
IDs, cloud-baseline presence, per-candidate sample checks, and ranked local
candidates. This keeps the production selector strict while making incomplete
Task 6 evidence easy to inspect after partial real microphone runs.

Follow-up implementation note: `Invoke-TalkAsrDefaultModelWorkflow.ps1` now
writes `asr-model-evidence-status.json` immediately after same-corpus
benchmarking and before the strict selector writes
`selected-default-asr-model.json`. If production selection rejects incomplete
Task 6 evidence, the workflow still leaves a structured blocker artifact with
the same information as `Select-TalkDefaultAsrModel.ps1 -StatusOnly`.

Follow-up implementation note: `Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1`
now supports staged operator recording through `-RecordOnly`. In this mode
`-PreflightOnly` checks only the prompt manifest, `talk.exe`, and planned corpus
output while skipping benchmark tools, installed sherpa models, cloud baseline
keys, and desktop config apply. A full `-RecordOnly` run records the real
microphone corpus and returns without starting Zipformer/Paraformer/cloud
benchmarking. Operators can then resume the evidence gate later with
`-SkipRecording` against the existing `corpus.json`. `-RecordOnly
-PreflightOnly -ProbeAudio` uses the planned recorder config for a short
microphone signal gate without requiring benchmark prerequisites. `-RecordOnly`
and `-SkipRecording` are mutually exclusive so staged runs cannot silently turn
into a no-op.

Follow-up implementation note: staged `-RecordOnly` runs now write
`record-only-status.json` next to the recorded `corpus.json`. The status artifact
checks that the corpus manifest exists, counts manifest samples, counts existing
WAV files, lists missing WAV paths, records how many samples the recorder
returned, and includes the copyable `-SkipRecording` resume command. This gives
operators an auditable handoff between the real microphone capture stage and the
later Zipformer/Paraformer/cloud benchmark stage.

Follow-up implementation note: `-SkipRecording` now consumes that handoff when
it exists. `-SkipRecording -PreflightOnly` adds a `record_only_status` check and
requires a present `record-only-status.json` to be ready and to match the current
`corpus.json`. Full `-SkipRecording` runs apply the same guard before starting
the same-corpus benchmark. Missing status files remain non-blocking so older or
manually assembled corpus manifests can still be benchmarked.

Completed 2026-07-19. The normalized corpus manifest contains four genuine
microphone recordings: `short-search-001`, `mixed-english-001`,
`punctuation-001`, and `natural-noise-001`. The recorder validates the emitted
artifact header as 16 kHz, mono, 16-bit PCM.

- [x] **Step 1: Benchmark Zipformer**

Measure `zipformer-zh-en-punct-int8-480ms` on real short Chinese dictation WAVs.

Completed 2026-07-19. The same-corpus report contains four Zipformer samples
with aggregated CER, first-partial latency, final latency, RTF, and model size.

- [x] **Step 2: Benchmark Paraformer**

Measure the larger bilingual Paraformer model with the same WAV set and JSON schema.

Completed 2026-07-19. The same four sample IDs were benchmarked with the
Paraformer model and included in `asr-model-comparison.json`.

- [x] **Step 3: Compare against cloud-only baseline**

Use the same utterances to compare local-first latency against the current cloud provider flow.

Completed 2026-07-19. The Qwen `qwen3-asr-flash` cloud baseline covers the same
four sample IDs and passes the strict selector's shared-corpus checks.

- [x] **Step 4: Lock the default**

Choose the default model by first partial latency, final latency, RTF, memory footprint, Chinese CER, and packaging size. Keep alternatives configurable rather than hard-coded by process name or host app.

Completed 2026-07-19. Strict evidence selection chose
`zipformer-zh-en-punct-int8-480ms`; the result was applied to the ignored
runtime desktop configuration copy at
`.runtime/asr-bench/real-mic-corpus/talk-desktop-default-locked.toml`.
The packaged release keeps the same Zipformer model as its local ASR
auto-discovery default while retaining Paraformer as an installable option.

After installing the candidate models, prefer the end-to-end real microphone
workflow:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -ModelId @('zipformer-zh-en-punct-int8-480ms', 'paraformer-bilingual-zh-en') `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

For staged operator runs, first capture only the real microphone corpus:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly -PreflightOnly
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly -PreflightOnly -ProbeAudio -AudioProbeSeconds 2
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 -RecordOnly
```

Then continue with the existing corpus:

```powershell
.\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -SkipRecording `
  -CorpusRoot .\.runtime\asr-bench\real-mic-corpus `
  -ModelRoot .\.runtime\models\sherpa-onnx `
  -ConfigPath .\talk-desktop.toml
```

Or, after recording the real microphone corpus, rerun only the lower-level
benchmark/selection/apply workflow:

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

Or, after the same-corpus benchmark writes `asr-model-comparison.json`, run the
selection and apply steps manually:

```powershell
.\Select-TalkDefaultAsrModel.ps1 `
  -ComparisonJson .\.runtime\asr-bench\real-mic-corpus\reports\asr-model-comparison.json `
  -OutputJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json
```

Then apply the selected installed model to the desktop config:

```powershell
.\Set-TalkDefaultAsrModel.ps1 `
  -SelectionJson .\.runtime\asr-bench\real-mic-corpus\reports\selected-default-asr-model.json `
  -ConfigPath .\talk-desktop.toml `
  -ModelRoot .\.runtime\models\sherpa-onnx
```

Expected: the selector rejects weak/synthetic evidence and only writes a
selection record when the real-microphone Zipformer, Paraformer, and cloud
baseline matrix is complete enough to justify a packaged default change. The
applier then validates the selected local model installation before changing
the desktop runtime config.

## Self-review

- Spec coverage: This plan covers the approved local ASR first, text correction second architecture; it also covers the current implementation batch that adds real streaming benchmark tooling and release packaging.
- Placeholder scan: No placeholder-only steps are present; unchecked steps have concrete commands or measurable acceptance criteria.
- Type consistency: File paths, package names, script names, and manifest support kind names match the current Talk workspace.
