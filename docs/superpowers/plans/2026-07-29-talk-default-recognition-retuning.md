# Talk Default Recognition Retuning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Retune Talk's default recognition chain so the packaged product improves multilingual mixed-speech accuracy, proper-noun preservation, tail stability, and cloud-correction safety without materially increasing live dictation latency.

**Architecture:** Keep the existing local-first Talk pipeline. Strengthen the evidence set and default-model selection gates first, then unify the packaged default local model and model-aware daemon tuning, then tighten faithful cloud correction so it preserves mixed-language and token-sensitive content. Verification must use both reproducible corpus evidence and real microphone evidence before publishing a new Talk product release.

**Tech Stack:** Rust Talk workspace (`talk-desktop`, `talk-runtime`, `talk-client`), PowerShell/Pester benchmark and release scripts, sherpa-onnx local streaming ASR, DashScope OpenAI-compatible transcription/text processing.

---

## Product Contracts

- Product release stays exactly:
  - `Talk.exe`
  - `talk.toml`
- Default recognition remains local-first:
  - local sherpa streaming ASR produces the first usable text;
  - cloud processing may correct faithful dictation text, but must not become the primary latency path.
- Default recognition quality is evaluated against:
  - a reproducible corpus benchmark;
  - a real-microphone benchmark set.
- The packaged default local-ASR model must be chosen by evidence, not by a stale hard-coded preference.
- Default cloud faithful correction must preserve mixed-language text, proper nouns, numbers, hotkeys, paths, and model/product identifiers unless there is strong evidence that the change is safe.

---

### Task 1: Expand the default-recognition evidence set

**Files:**
- Modify: `examples/asr-real-mic-prompts.json`
- Modify: `docs/asr-benchmarks/corpus-manifest.example.json`
- Modify: `scripts/tests/Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1`
- Modify: `scripts/tests/Select-TalkDefaultAsrModel.Tests.ps1`

- [ ] **Step 1: Write the failing real-mic prompt coverage test**

Add a Pester assertion that the default prompt manifest contains category coverage for:
- short Chinese;
- Chinese-English mixed speech;
- Chinese-English-Japanese mixed speech;
- proper nouns / token-sensitive speech;
- long punctuation / pause speech;
- noise-realistic speech.

Suggested assertion shape inside `scripts/tests/Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1`:

```powershell
It 'ships a multilingual default prompt manifest for default-model locking' {
    $promptPath = Join-Path $talkRoot 'examples\asr-real-mic-prompts.json'
    $manifest = Get-Content -LiteralPath $promptPath -Raw -Encoding UTF8 | ConvertFrom-Json
    $sampleIds = @($manifest.samples | ForEach-Object { [string]$_.sampleId })

    $sampleIds | Should Contain 'short-search-001'
    $sampleIds | Should Contain 'mixed-english-001'
    $sampleIds | Should Contain 'mixed-english-japanese-001'
    $sampleIds | Should Contain 'proper-nouns-001'
    $sampleIds | Should Contain 'punctuation-longform-001'
    $sampleIds | Should Contain 'noise-realistic-001'
}
```

- [ ] **Step 2: Run the prompt coverage test and verify RED**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1 -Output Detailed
```

Expected: FAIL because the current prompt manifest does not yet contain the multilingual/default coverage set.

- [ ] **Step 3: Expand the prompt manifest and corpus example**

Update `examples/asr-real-mic-prompts.json` so it includes concrete default prompts such as:

```json
{
  "sampleId": "mixed-english-japanese-001",
  "referenceText": "请帮我打开 Talk 的 local first ASR テスト 页面。",
  "captureSeconds": 6
}
```

and:

```json
{
  "sampleId": "proper-nouns-001",
  "referenceText": "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。",
  "captureSeconds": 6
}
```

Mirror the same categories in `docs/asr-benchmarks/corpus-manifest.example.json` so the example benchmark corpus documents the new target set.

- [ ] **Step 4: Re-run the prompt coverage test and verify GREEN**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1 -Output Detailed
```

Expected: PASS for the new prompt coverage assertion and no regression in existing workflow tests.

- [ ] **Step 5: Commit the evidence-set expansion**

Run:

```powershell
git add .\examples\asr-real-mic-prompts.json .\docs\asr-benchmarks\corpus-manifest.example.json .\scripts\tests\Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1 .\scripts\tests\Select-TalkDefaultAsrModel.Tests.ps1
git commit -m "test: expand talk default recognition evidence set"
```

### Task 2: Add explicit product gates to default-model selection

**Files:**
- Modify: `scripts/Select-TalkDefaultAsrModel.ps1`
- Modify: `scripts/Invoke-TalkAsrDefaultModelWorkflow.ps1`
- Modify: `scripts/tests/Select-TalkDefaultAsrModel.Tests.ps1`
- Modify: `scripts/tests/Invoke-TalkAsrDefaultModelWorkflow.Tests.ps1`

- [ ] **Step 1: Write failing selector tests for coverage and latency gates**

Add Pester tests proving:
- a candidate missing the multilingual sample id set is rejected;
- a candidate over the default latency ceiling is rejected even if it has the best CER;
- the written selection JSON records rejection reasons.

Use helper candidates similar to:

```powershell
It 'rejects a low-CER candidate that exceeds the live final-latency budget' {
    $selection = Select-TalkDefaultAsrModel `
        -ComparisonJson $comparisonPath `
        -OutputJson $outputPath `
        -PassThru

    $selection.selectedModelId | Should Be 'zipformer-zh-en-punct-int8-480ms'
    ($selection.rejectedCandidates | ConvertTo-Json -Depth 8) | Should Match 'latency'
}
```

- [ ] **Step 2: Run selector tests and verify RED**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Select-TalkDefaultAsrModel.Tests.ps1 -Output Detailed
```

Expected: FAIL because the current selector mostly ranks by benchmark metrics and does not yet enforce explicit multilingual coverage and latency gates.

- [ ] **Step 3: Implement product gates in the selector**

In `scripts/Select-TalkDefaultAsrModel.ps1`, extend candidate evidence status with explicit product checks:

```powershell
$requiredDefaultSampleIds = @(
    'short-search-001',
    'mixed-english-001',
    'mixed-english-japanese-001',
    'proper-nouns-001',
    'punctuation-longform-001',
    'noise-realistic-001'
)
```

and bounded latency gates such as:

```powershell
$maxDefaultFirstPartialMs = 350
$maxDefaultFinalLatencyMs = 650
$maxDefaultRtf = 0.60
```

If a candidate fails any product gate, record the reason in a structured rejection list and exclude it from ranked default selection.

- [ ] **Step 4: Re-run selector tests and verify GREEN**

Run:

```powershell
Invoke-Pester -Path .\scripts\tests\Select-TalkDefaultAsrModel.Tests.ps1 -Output Detailed
Invoke-Pester -Path .\scripts\tests\Invoke-TalkAsrDefaultModelWorkflow.Tests.ps1 -Output Detailed
```

Expected: PASS. The workflow tests should still produce a selection JSON, but now with stricter eligibility logic and explicit rejection reasons.

- [ ] **Step 5: Commit the selection-gate work**

Run:

```powershell
git add .\scripts\Select-TalkDefaultAsrModel.ps1 .\scripts\Invoke-TalkAsrDefaultModelWorkflow.ps1 .\scripts\tests\Select-TalkDefaultAsrModel.Tests.ps1 .\scripts\tests\Invoke-TalkAsrDefaultModelWorkflow.Tests.ps1
git commit -m "feat: gate talk default model selection by product evidence"
```

### Task 3: Unify the packaged default local model

**Files:**
- Modify: `crates/talk-desktop/src/model_bootstrap.rs`
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `docs/LOCAL_SHERPA_MODELS.md`
- Modify: `scripts/Publish-TalkRelease.ps1`
- Modify: `scripts/tests/Publish-TalkRelease.Tests.ps1`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write failing consistency tests**

Add tests proving:
- the packaged bootstrap default model id matches the documented product default;
- the release config comments document the same default model id;
- the desktop auto-discovery order prefers the same default first.

Suggested release-side assertion in `Publish-TalkRelease.Tests.ps1`:

```powershell
It 'documents the same packaged default model id used by bootstrap' {
    $config = New-TalkReleaseDesktopConfigContent
    $config | Should Match 'selected packaged default model'
    $config | Should Match 'sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10'
}
```

- [ ] **Step 2: Run consistency tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-desktop
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1 -Output Detailed
```

Expected: FAIL if the packaged default model references are still drifting between bootstrap, auto-discovery, and release comments.

- [ ] **Step 3: Implement one authoritative packaged default entry**

Refactor the packaged default model definition so:
- `crates/talk-desktop/src/model_bootstrap.rs` exposes the actual product default model spec;
- `crates/talk-desktop/src/lib.rs` prefers that same product default first during auto-discovery;
- `docs/LOCAL_SHERPA_MODELS.md` describes that same product default;
- `scripts/Publish-TalkRelease.ps1` comments reference that same default id and file set.

If the final chosen packaged default is not a zipformer family model, rename helper functions/constants so the code no longer uses misleading `zipformer`-specific names for the packaged default.

- [ ] **Step 4: Re-run consistency tests and verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-desktop
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1 -Output Detailed
```

Expected: PASS. Bootstrap, auto-discovery, release comments, and docs now describe the same packaged default model.

- [ ] **Step 5: Commit the default-model unification**

Run:

```powershell
git add .\crates\talk-desktop\src\model_bootstrap.rs .\crates\talk-desktop\src\lib.rs .\docs\LOCAL_SHERPA_MODELS.md .\scripts\Publish-TalkRelease.ps1 .\scripts\tests\Publish-TalkRelease.Tests.ps1 .\crates\talk-desktop\tests\desktop_contract.rs
git commit -m "feat: unify talk packaged default local model"
```

### Task 4: Carry model-aware low-latency tuning into the packaged local-ASR path

**Files:**
- Modify: `crates/talk-desktop/src/lib.rs`
- Modify: `crates/talk-core/src/lib.rs` only if new model metadata fields are required
- Modify: `examples/desktop-streaming-service-speculative-config.toml`
- Modify: `crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write failing desktop tuning tests**

Add tests proving that the packaged auto-daemon config can carry model-aware defaults without forcing beam search globally:

```rust
#[test]
fn packaged_default_local_asr_prefers_low_latency_defaults_without_hotwords() {
    let config = desktop_auto_local_asr_daemon_config(&model_root).unwrap();
    assert_eq!(config.decoding_method.as_deref(), Some("greedy_search"));
    assert!(config.modeling_unit.is_some() || config.bpe_vocab.is_some());
}
```

Add a second test proving that hotword-enabled configs still upgrade to
`modified_beam_search` only when hotword biasing is actually configured.

- [ ] **Step 2: Run desktop tuning tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-desktop --test desktop_contract
```

Expected: FAIL because the auto-generated daemon configs still omit model-specific metadata and conservative endpoint defaults.

- [ ] **Step 3: Implement model-aware daemon defaults**

In `crates/talk-desktop/src/lib.rs`, move the packaged model defaults into a model-aware helper that can return:

```rust
SpeculativeLocalAsrDaemonConfig {
    decoding_method: Some("greedy_search".to_string()),
    enable_endpoint: Some(true),
    endpoint_reset: Some(true),
    modeling_unit: Some("cjkchar+bpe".to_string()),
    bpe_vocab: Some(model_dir.join("bpe.vocab")),
    ..
}
```

Only set `modeling_unit` / `bpe_vocab` for models that actually support those files. Do not force `modified_beam_search` unless a hotwords source is configured.

- [ ] **Step 4: Re-run desktop tuning tests and verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-desktop --test desktop_contract
```

Expected: PASS. Packaged local-ASR auto-config is now model-aware and still low latency by default.

- [ ] **Step 5: Commit the tuning work**

Run:

```powershell
git add .\crates\talk-desktop\src\lib.rs .\crates\talk-core\src\lib.rs .\examples\desktop-streaming-service-speculative-config.toml .\crates\talk-desktop\tests\desktop_contract.rs
git commit -m "feat: tune talk packaged local asr defaults"
```

### Task 5: Tighten faithful cloud correction for multilingual and token-sensitive dictation

**Files:**
- Modify: `crates/talk-client/src/lib.rs`
- Modify: `crates/talk-runtime/src/voice_processing.rs`
- Modify: `crates/talk-runtime/src/lib.rs` if diagnostics need extension
- Modify: `crates/talk-client/tests/client_contract.rs`
- Modify: `crates/talk-runtime/tests/runtime_contract.rs`

- [ ] **Step 1: Write failing provider/runtime tests**

Add tests proving:
- dictation prompts explicitly forbid translation and token rewriting;
- cloud output that changes a product/path/token-sensitive substring is rejected in faithful mode;
- punctuation-only and obvious STT cleanup are still accepted.

Suggested runtime contract shape:

```rust
#[test]
fn faithful_output_rejects_protected_ascii_token_rewrite() {
    let validation = validate_faithful_output(
        "请打开 Talk 的 qwen3 asr flash 日志。",
        "请打开 talk 的千问日志。"
    );
    assert!(!validation.accepted);
}
```

- [ ] **Step 2: Run provider/runtime tests and verify RED**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-client --test client_contract
cargo test --manifest-path .\Cargo.toml -p talk-runtime --test runtime_contract
```

Expected: FAIL because faithful mode does not yet protect mixed-language/token-sensitive content tightly enough.

- [ ] **Step 3: Tighten prompts and add protected-token validation**

Update `system_prompt_for_mode()` in `crates/talk-client/src/lib.rs` so `Transcribe` / `Dictate` explicitly says:

```text
Preserve the original language, mixed-language tokens, product names, paths, hotkeys, numbers, and ASCII terms. Only fix obvious speech-to-text mistakes and punctuation. Do not translate, summarize, paraphrase, or rewrite.
```

Then extend `validate_faithful_output()` in `crates/talk-runtime/src/voice_processing.rs` with a protected-token check that extracts bounded token-sensitive substrings and rejects unsafe rewrites before accepting the cloud output.

- [ ] **Step 4: Re-run provider/runtime tests and verify GREEN**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml -p talk-client --test client_contract
cargo test --manifest-path .\Cargo.toml -p talk-runtime --test runtime_contract
```

Expected: PASS. Faithful mode is stricter on unsafe mixed-language/token-sensitive rewrites while still allowing punctuation cleanup and safe corrections.

- [ ] **Step 5: Commit the faithful-correction safeguards**

Run:

```powershell
git add .\crates\talk-client\src\lib.rs .\crates\talk-runtime\src\voice_processing.rs .\crates\talk-runtime\src\lib.rs .\crates\talk-client\tests\client_contract.rs .\crates\talk-runtime\tests\runtime_contract.rs
git commit -m "fix: preserve talk faithful multilingual dictation output"
```

### Task 6: Re-run evidence workflows and publish a new Talk release

**Files:**
- Existing Talk workspace files only
- Generate: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\<version-id>\Talk.exe`
- Generate: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\<version-id>\talk.toml`

- [ ] **Step 1: Format and compile verification**

Run:

```powershell
cargo fmt --manifest-path .\Cargo.toml --all -- --check
cargo check --manifest-path .\Cargo.toml --workspace --all-targets
```

Expected: both commands exit 0.

- [ ] **Step 2: Workspace test verification**

Run:

```powershell
cargo test --manifest-path .\Cargo.toml --workspace
Invoke-Pester -Path .\scripts\tests\Select-TalkDefaultAsrModel.Tests.ps1 -Output Detailed
Invoke-Pester -Path .\scripts\tests\Invoke-TalkAsrDefaultModelWorkflow.Tests.ps1 -Output Detailed
Invoke-Pester -Path .\scripts\tests\Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1 -Output Detailed
Invoke-Pester -Path .\scripts\tests\Publish-TalkRelease.Tests.ps1 -Output Detailed
```

Expected: all commands exit 0.

- [ ] **Step 3: Run the reproducible corpus benchmark workflow**

Run:

```powershell
.\scripts\Invoke-TalkAsrDefaultModelWorkflow.ps1 `
  -CorpusManifest .\docs\asr-benchmarks\corpus-manifest.example.json `
  -OutputRoot .\.runtime\asr-bench\default-model-benchmark `
  -SkipApply `
  -PassThru
```

Expected: selection/evidence JSON is produced and the product gate explanations are visible in the result.

- [ ] **Step 4: Run the real-microphone default-model workflow**

Run:

```powershell
.\scripts\Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1 `
  -PromptManifest .\examples\asr-real-mic-prompts.json `
  -CorpusRoot .\.runtime\asr-bench\real-mic-corpus `
  -ReportsRoot .\.runtime\asr-bench\real-mic-corpus\reports `
  -PassThru
```

Expected: recorded samples, evidence status, and selected default model are produced from the expanded multilingual prompt set.

- [ ] **Step 5: Publish the improved Talk product release**

Run:

```powershell
.\scripts\Publish-TalkRelease.ps1 `
  -VersionId talk-accuracy-20260729-r1 `
  -ReleaseRoot C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk `
  -ProductProfile
```

Expected: the publisher exits 0 and produces:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r1\Talk.exe
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r1\talk.toml
```

- [ ] **Step 6: Verify release contents**

Run:

```powershell
$release = 'C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\talk-accuracy-20260729-r1'
Get-ChildItem -LiteralPath $release -Force | Select-Object Name,Length,Mode
```

Expected: only `Talk.exe` and `talk.toml` are present in the product directory.

- [ ] **Step 7: Commit the release-facing final changes**

Run:

```powershell
git add .
git commit -m "feat: retune talk default recognition chain"
```
