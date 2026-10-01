# Talk Default Recognition Retuning Design

**Status:** Draft for review

**Goal:** Improve Talk's default recognition quality for real end users without
materially increasing the live dictation latency budget. The default product
path must perform better across wrong characters, dropped tails, multilingual
mixed speech, proper nouns, and cloud over-correction, while still shipping as
the existing two-file Windows product release.

## Product Constraints

- Keep the current product shape:
  - `Talk.exe`
  - `talk.toml`
- Keep the existing local-first runtime architecture:
  - local sherpa streaming ASR produces the first usable text;
  - cloud processing may correct or finalize text, but must not become the new
    primary latency path.
- Do not make the default user experience noticeably slower.
- Treat multilingual mixed speech as a first-class default target rather than a
  niche opt-in path.
- Use evidence from both reproducible corpus benchmarking and real microphone
  recordings before changing the product default model or default tuning.

## Current State

Talk already contains most of the building blocks required for a stronger
default recognition path:

- Local streaming ASR candidates and installer catalog:
  - `scripts/Install-TalkSherpaModel.ps1`
  - `docs/LOCAL_SHERPA_MODELS.md`
- Corpus benchmarking and model selection workflow:
  - `tools/asr-bench/src/main.rs`
  - `scripts/Invoke-TalkAsrDefaultModelWorkflow.ps1`
  - `scripts/Select-TalkDefaultAsrModel.ps1`
  - `scripts/Set-TalkDefaultAsrModel.ps1`
- Real microphone collection and benchmark workflow:
  - `scripts/Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1`
  - `examples/asr-real-mic-prompts.json`
- Product bootstrap and auto-discovery for packaged local models:
  - `crates/talk-desktop/src/model_bootstrap.rs`
  - `crates/talk-desktop/src/lib.rs`
- Cloud output preservation safeguards:
  - `crates/talk-runtime/src/voice_processing.rs`
- OpenAI-compatible transcription and text-processing prompts:
  - `crates/talk-client/src/lib.rs`

The main problem is not missing capability. The problem is that the current
default chain is not fully unified, and the evidence used to lock product
defaults is not strong enough for a multilingual, low-latency product target.

## Observed Gaps

### 1. Default-model drift

The repository currently carries multiple overlapping notions of the "default"
local ASR model:

- `default_zipformer_model_spec()` in
  `crates/talk-desktop/src/model_bootstrap.rs` points at
  `sherpa-onnx-streaming-zipformer-ar_en_id_ja_ru_th_vi_zh-2025-02-10`.
- `desktop_auto_local_asr_daemon_config()` in
  `crates/talk-desktop/src/lib.rs` prefers the packaged multilingual zipformer,
  then a legacy zh-en zipformer, then paraformer.
- `docs/LOCAL_SHERPA_MODELS.md` describes `zipformer-zh-en-punct-int8-480ms`
  as the default model.
- The release config template comments in `scripts/Publish-TalkRelease.ps1`
  still reflect the older default direction.

This means Talk can benchmark one model family, document another, auto-discover
another, and publish comments for another. That inconsistency directly weakens
any "improve the default" effort.

### 2. Default-model evidence is too narrow

The real-microphone prompt manifest in `examples/asr-real-mic-prompts.json`
contains only four samples. It currently under-represents:

- Chinese-English-Japanese mixed speech;
- proper nouns and product names;
- ASCII-heavy tokens such as paths, hotkeys, versions, and commands;
- long-form dictation stability and tail preservation;
- real-world punctuation and pause behavior beyond one short sample.

This is insufficient evidence for choosing a multilingual product default.

### 3. Default selection optimizes for a score, not a product contract

`tools/asr-bench` already computes a usable comparison score from CER, latency,
RTF, and model size. That is useful for benchmarking, but the product's default
selection needs stronger rules:

- a multilingual-default candidate must prove multilingual coverage;
- a higher-accuracy candidate must not become default if it breaks the latency
  budget in a user-visible way;
- proper-noun and mixed-language quality should influence default decisions
  more than a short synthetic phrase does.

### 4. Packaged local-model defaults do not fully use the new accuracy knobs

Talk now supports endpointing, endpoint reset, hotword files, raw hotword
vocabularies, and BPE/modeling metadata. However, the auto-generated daemon
configs in `crates/talk-desktop/src/lib.rs` still default to conservative
values:

- `greedy_search`
- no explicit endpoint toggle
- no endpoint reset
- no model-specific `modeling_unit`
- no model-specific `bpe_vocab`

The product therefore contains accuracy knobs that are not fully carried into
the default local-ASR path.

### 5. Cloud correction still has room to overreach on short or token-sensitive text

Talk already rejects catastrophic long-form faithful rewrites through
`validate_faithful_output()`, but the current cloud dictation prompt and output
validation are still permissive for cases such as:

- product names like `Talk`, `Neuro`, `Hook`, `Loom`, `Gateway`;
- file paths, hotkeys, command names, model names, and version strings;
- multilingual mixed phrases where one script is silently normalized into
  another;
- short utterances where one wrong token changes the whole result.

That gap causes some of the most frustrating "it was already right, then got
corrected into something wrong" failures.

## Design

The retuning work is split into six product-facing workstreams. Each workstream
has an explicit output and verification boundary.

### Workstream A: Expand the recognition evidence set

The repository will gain a stronger default-recognition evaluation set that is
explicitly designed for multilingual, low-latency desktop dictation.

The prompt/evidence set will cover at least these categories:

1. short Chinese dictation;
2. Chinese + English mixed speech;
3. Chinese + English + Japanese mixed speech;
4. proper nouns and product tokens;
5. long-form punctuation and pause handling;
6. realistic light-noise desktop speech.

This will start in:

- `examples/asr-real-mic-prompts.json`
- `docs/asr-benchmarks/corpus-manifest.example.json`

The real-microphone workflow in
`scripts/Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1` will treat these
categories as first-class evidence, not optional manual notes. Reusing a
previously recorded corpus with `-SkipRecording` will only be allowed when the
current prompt manifest and the existing corpus manifest cover the same
sample-id set; stale corpora become a hard blocker instead of silently
reusing partial evidence. To keep operator refresh work bounded, record-only
refresh runs will also support reusing the aligned subset of an existing corpus
and capturing only the newly required sample IDs before rewriting the corpus
manifest in prompt order.

The result is that future model-default changes are anchored to the same
multilingual product target instead of a minimal smoke set.

### Workstream B: Turn model selection into a product-default gate

The default model selection flow will still use `tools/asr-bench` and
`scripts/Select-TalkDefaultAsrModel.ps1`, but the selection contract will
become stricter.

Selection will happen in two stages:

#### Stage 1: Eligibility gates

A candidate cannot become the default product model unless it:

- has enough real microphone samples;
- covers all required prompt categories;
- satisfies the required local-model evidence rules;
- stays inside explicit latency and RTF ceilings appropriate for live desktop
  dictation;
- does not rely on synthetic or smoke-only evidence to lock the default.

The exact numbers can remain implementation-tunable, but the design requires
the gates to exist as product rules rather than being hidden inside one scalar
score.

#### Stage 2: Ranked selection among eligible candidates

Once candidates pass the product gates, ranking will prioritize:

1. CER;
2. multilingual-subset CER;
3. proper-noun/token-sensitive subset CER;
4. final latency;
5. RTF.

The selection JSON written by `scripts/Select-TalkDefaultAsrModel.ps1` will be
extended to explain:

- which candidates were eligible;
- which were rejected and why;
- why the winning candidate was chosen.

The goal is to make default-model changes auditable instead of opaque.

### Workstream C: Unify the packaged default local model

Talk will move to one authoritative notion of the packaged default local model.

That single default entry will drive:

- `default_zipformer_model_spec()` in
  `crates/talk-desktop/src/model_bootstrap.rs`;
- auto-discovery order in `crates/talk-desktop/src/lib.rs`;
- release comments in `scripts/Publish-TalkRelease.ps1`;
- guidance in `docs/LOCAL_SHERPA_MODELS.md`;
- example config expectations.

The implementation may still support legacy models and alternate installed
models, but the product default must no longer drift between bootstrap, runtime,
docs, and release comments.

The chosen packaged default will come from Workstream B rather than from a
historical hard-coded preference.

If the final packaged default is no longer a zipformer family model, the
bootstrap helper naming may be generalized during implementation so the code
does not keep a misleading "zipformer" name for a non-zipformer product
default.

### Workstream D: Promote model-specific low-latency tuning into the default path

The packaged local-ASR auto-config path in `crates/talk-desktop/src/lib.rs`
will become model-aware instead of only file-presence-aware.

For each supported default candidate family, the auto-generated daemon config
will be able to carry:

- decoding method;
- endpoint behavior;
- endpoint reset behavior;
- model family;
- model-specific BPE/modeling metadata needed for hotword tokenization.

The important rule is that default latency remains protected:

- no unconditional beam-search upgrade for every user;
- no forced high-latency correction path just to chase accuracy;
- hotword-enabled beam search only when hotword biasing is actually needed.

This lets Talk ship better default recognition behavior without turning the
local ASR path into a slower, more fragile pipeline.

### Workstream E: Add token-preserving cloud correction safeguards

The cloud dictation correction path will be tightened in two places.

#### E1. More faithful dictation instructions

The `Transcribe` / `Dictate` prompt in `crates/talk-client/src/lib.rs` will be
rewritten to make the intended behavior explicit:

- preserve the spoken language instead of translating it;
- preserve mixed-script tokens and named entities;
- preserve numbers, paths, hotkeys, model names, and ASCII terms;
- allow punctuation cleanup and obvious STT repairs only;
- forbid summarization, paraphrasing, or semantic rewriting.

#### E2. Protected-token validation

Faithful-output validation in `crates/talk-runtime/src/voice_processing.rs`
will gain an additional protected-token pass for token-sensitive faithful modes.

The validation will derive a bounded set of important tokens from the baseline
transcript, including:

- ASCII words and abbreviations;
- path-like substrings;
- hotkeys and control tokens;
- obvious version/number/time sequences;
- product names and model identifiers;
- mixed-script fragments that should not silently normalize into another script.

If cloud output introduces unsafe changes to those protected tokens, Talk will
reject the cloud correction and fall back to the local baseline or the existing
safe faithful path.

This protects exactly the high-friction failures that users notice most:

- correct proper nouns changed into wrong homophones;
- paths or commands "cleaned up" into plain prose;
- multilingual segments partially rewritten into a different language.

### Workstream F: Make the product release reflect the new default chain

The release publisher and example configs will be updated so that the packaged
product reflects the same default assumptions validated by the benchmark flow.

This includes:

- `scripts/Publish-TalkRelease.ps1`
- `examples/desktop-streaming-service-speculative-config.toml`
- related release tests and docs

The release still produces the same product shape, but the sibling `talk.toml`
comments, bootstrap expectations, and local-model notes will be aligned with
the newly selected default recognition path.

## File-Level Scope

Expected primary modifications:

- `examples/asr-real-mic-prompts.json`
- `docs/asr-benchmarks/corpus-manifest.example.json`
- `scripts/Invoke-TalkAsrRealMicDefaultModelWorkflow.ps1`
- `scripts/Invoke-TalkAsrDefaultModelWorkflow.ps1`
- `scripts/Select-TalkDefaultAsrModel.ps1`
- `scripts/Set-TalkDefaultAsrModel.ps1`
- `scripts/Publish-TalkRelease.ps1`
- `crates/talk-desktop/src/model_bootstrap.rs`
- `crates/talk-desktop/src/lib.rs`
- `crates/talk-client/src/lib.rs`
- `crates/talk-runtime/src/voice_processing.rs`
- related Rust unit/contract tests and Pester tests

Expected non-goals for this task:

- replacing the local-first architecture with a cloud-first architecture;
- adding new external providers;
- changing the Talk product bundle format;
- broad UI redesign unrelated to recognition quality;
- redesigning the Smart routing feature beyond what is necessary to preserve
  faithful transcription quality.

## Testing Strategy

The implementation must be test-first and must prove the following behavior.

### Selection and evidence contracts

- multilingual/default prompt coverage is required before a model can become the
  product default;
- latency and RTF limits are enforced as explicit gates;
- the default selection result records why a candidate won or was rejected;
- a slower but more accurate model cannot silently become the product default if
  it fails the live-latency contract.

Primary tests:

- `scripts/tests/Select-TalkDefaultAsrModel.Tests.ps1`
- `scripts/tests/Invoke-TalkAsrDefaultModelWorkflow.Tests.ps1`
- `scripts/tests/Invoke-TalkAsrRealMicDefaultModelWorkflow.Tests.ps1`

### Runtime and provider preservation contracts

- faithful dictation prompts keep the original language and token-sensitive
  content intact;
- protected-token validation rejects unsafe cloud rewrites;
- long-form faithful validation still blocks catastrophic compression and
  excessive sequence change;
- safe cloud corrections still pass for punctuation and obvious STT cleanup.

Primary tests:

- `crates/talk-runtime/tests/runtime_contract.rs`
- `crates/talk-runtime/tests/smart_route_contract.rs`
- `crates/talk-client/tests/client_contract.rs`

### Packaged default-model consistency

- bootstrap default model, auto-discovery default model, release comments, and
  docs point to the same product default;
- auto-generated daemon args preserve the expected low-latency defaults;
- model-aware metadata required for hotwords can be supplied without forcing
  every user onto a slower decode path.

Primary tests:

- `crates/talk-desktop/tests/desktop_contract.rs`
- `scripts/tests/Publish-TalkRelease.Tests.ps1`
- `scripts/tests/Install-TalkSherpaModel.Tests.ps1`

## Verification Strategy

The feature is only complete when both evidence tracks are fresh.

### 1. Reproducible corpus benchmark

Run the benchmark/model-selection workflow against the strengthened corpus set
and inspect:

- candidate CER;
- multilingual subset quality;
- proper-noun/token-sensitive subset quality;
- first partial latency;
- final latency;
- RTF;
- final selected model and selection explanation.

### 2. Real microphone benchmark

Run the real-microphone workflow with the expanded prompt set and inspect:

- recorded samples and sample categories;
- candidate comparison output;
- selected model;
- evidence status JSON;
- any category-specific regressions.

### 3. Product release validation

After the selected default path is applied, publish a Talk product build into:

`C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk`

The produced release must still contain exactly:

```text
Talk.exe
talk.toml
```

and its default comments/config behavior must match the newly selected default
recognition chain.

## Acceptance Criteria

This task is accepted when all of the following are true:

1. Talk has a stronger multilingual real-microphone evidence set than the
   current four-sample manifest.
2. Default model selection uses explicit product gates for evidence coverage and
   live-latency safety, not just one aggregate score.
3. Packaged bootstrap, runtime auto-discovery, release config comments, and
   docs agree on the same default local-ASR model.
4. Model-aware low-latency tuning is carried into the packaged default local
   ASR path.
5. Faithful cloud correction is stricter about preserving mixed-language and
   token-sensitive content.
6. The final product release remains low-friction and low-latency for default
   users.
7. A fresh release artifact is produced under
   `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk` after the
   implementation milestone is complete.
