# Talk Local-First Speculative Dictation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework Talk into a local-first speculative voice input system where local ASR drafts appear immediately, sentence-sized chunks are corrected asynchronously by a cloud text model, and cloud patches are applied only when the active target is still safe.

**Architecture:** The latency-critical path becomes `mic -> local streaming ASR -> live draft display/insert`; cloud correction is moved off that path. The implementation introduces segment state, correction patches, insert anchors, and patch safety decisions so cloud output can improve quality without stealing focus or corrupting user edits.

**Tech Stack:** Rust workspace under `Talk/`; Win32 desktop shell in `talk-desktop`; runtime orchestration in `talk-runtime`; audio capture in `talk-audio`; provider adapters in `talk-client`; insertion helpers in `talk-insert`; contract tests with `cargo test`; release packaging with `Talk/scripts/Publish-TalkRelease.ps1`.

---

## Baseline and target

Current baseline:

```text
RightAlt -> native recording -> WAV artifact -> provider transcription -> text processing -> insert or copy popup
```

Target:

```text
RightAlt -> live native recording -> local streaming ASR draft -> immediate preview/insert
                                      -> segment endpoint detector
                                      -> async cloud correction
                                      -> safe patch or suggestion popup
```

This plan preserves the current stable RightAlt/HUD/copy-popup behavior while adding the new speculative pipeline behind disabled-by-default config gates.

---

## File structure

- Create `Talk/crates/talk-core/src/speculative.rs`
  - Owns pure data contracts: `SpeculativeSegment`, `SpeculativeCorrectionPatch`, `SpeculativeEdit`, `SpeculativeMode`.
- Modify `Talk/crates/talk-core/src/lib.rs`
  - Exposes speculative contracts and config gates.
- Create `Talk/crates/talk-core/tests/speculative_contract.rs`
  - Tests segment states and faithful-mode patch validation.
- Create `Talk/crates/talk-runtime/src/segmenter.rs`
  - Detects stable phrase/sentence chunks from text length, punctuation, pause, and ASR finality.
- Create `Talk/crates/talk-runtime/src/speculative.rs`
  - Owns speculative runtime events and local segment state machine.
- Create `Talk/crates/talk-runtime/tests/segmenter_contract.rs`
  - Tests chunk readiness decisions.
- Create `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`
  - Tests draft and local-commit event ordering.
- Create `Talk/crates/talk-client/src/streaming_asr.rs`
  - Defines `StreamingAsrEngine`, `StreamingAsrEvent`, mock streaming ASR, and external JSON-line ASR parsing.
- Create `Talk/crates/talk-client/src/correction.rs`
  - Parses cloud correction patches from strict JSON.
- Create `Talk/crates/talk-client/tests/streaming_asr_contract.rs`
  - Tests partial/final ASR event handling.
- Create `Talk/crates/talk-client/tests/correction_contract.rs`
  - Tests cloud patch parsing and faithful-mode rejection of broad rewrites.
- Create `Talk/crates/talk-insert/src/patch.rs`
  - Owns edit-ratio and conservative auto-apply checks.
- Create `Talk/crates/talk-insert/tests/patch_contract.rs`
  - Tests patch edit ratios and auto-apply thresholds.
- Modify `Talk/crates/talk-desktop/src/lib.rs`
  - Adds pure desktop patch decisions and speculative transcript HUD models.
- Modify `Talk/crates/talk-desktop/tests/desktop_contract.rs`
  - Tests target anchor matching, stale correction rejection, and speculative HUD states.
- Modify `Talk/crates/talk-desktop/src/main.rs`
  - Integrates the speculative path behind config; preserves current batch behavior when disabled.
- Modify `Talk/README.md` and `Talk/docs/TALK_DESIGN.md`
  - Documents the new architecture.
- Create `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`
  - Operator-facing explanation of the pipeline and safety rules.

---

## Task 1: Core speculative contracts

**Files:**
- Create: `Talk/crates/talk-core/src/speculative.rs`
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Create: `Talk/crates/talk-core/tests/speculative_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-core/tests/speculative_contract.rs`:

```rust
use talk_core::{
    SpeculativeCorrectionPatch, SpeculativeEdit, SpeculativeEditKind, SpeculativeMode,
    SpeculativeSegment, SpeculativeSegmentState,
};

#[test]
fn speculative_segment_starts_partial_and_can_be_locally_committed() {
    let mut segment = SpeculativeSegment::new("seg-1", "你好").unwrap();
    assert_eq!(segment.id(), "seg-1");
    assert_eq!(segment.draft_text(), "你好");
    assert_eq!(segment.state(), SpeculativeSegmentState::Partial);

    segment.mark_local_final("你好呀").unwrap();
    assert_eq!(segment.draft_text(), "你好呀");
    assert_eq!(segment.state(), SpeculativeSegmentState::LocalFinal);
}

#[test]
fn speculative_segment_rejects_blank_drafts() {
    let error = SpeculativeSegment::new("seg-blank", "   ").unwrap_err();
    assert!(error.to_string().contains("draft text must not be blank"));
}

#[test]
fn faithful_mode_rejects_large_cloud_rewrite() {
    let patch = SpeculativeCorrectionPatch::new(
        "seg-1",
        "我下午三点有空",
        "我建议我们将会议安排在明天下午三点，这样会更加合适",
        0.93,
        vec![SpeculativeEdit::new(
            SpeculativeEditKind::Rewrite,
            "我下午三点有空",
            "我建议我们将会议安排在明天下午三点，这样会更加合适",
        )],
    )
    .unwrap();

    let error = patch
        .validate_for_mode(SpeculativeMode::FaithfulDictation)
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("faithful dictation only allows conservative edits"));
}

#[test]
fn faithful_mode_accepts_punctuation_only_patch() {
    let patch = SpeculativeCorrectionPatch::new(
        "seg-1",
        "我下午三点有空",
        "我下午三点有空。",
        0.97,
        vec![SpeculativeEdit::new(
            SpeculativeEditKind::Punctuation,
            "空",
            "空。",
        )],
    )
    .unwrap();

    patch
        .validate_for_mode(SpeculativeMode::FaithfulDictation)
        .unwrap();
}
```

- [ ] **Step 2: Verify red**

Run:

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-core speculative -- --nocapture
```

Expected: compile failure because the speculative types do not exist.

- [ ] **Step 3: Implement minimal contracts**

Create `Talk/crates/talk-core/src/speculative.rs` with:

```rust
use crate::TalkError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativeMode {
    FaithfulDictation,
    CleanDictation,
    Polish,
    Translate,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativeSegmentState {
    Partial,
    LocalFinal,
    CloudCorrectionPending,
    CloudCorrected,
    CloudCorrectionDeferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativeEditKind {
    Punctuation,
    Spacing,
    Casing,
    DictionaryCorrection,
    FillerRemoval,
    Rewrite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativeEdit {
    pub kind: SpeculativeEditKind,
    pub before: String,
    pub after: String,
}

impl SpeculativeEdit {
    pub fn new(kind: SpeculativeEditKind, before: impl Into<String>, after: impl Into<String>) -> Self {
        Self { kind, before: before.into(), after: after.into() }
    }

    fn is_conservative(&self) -> bool {
        matches!(
            self.kind,
            SpeculativeEditKind::Punctuation
                | SpeculativeEditKind::Spacing
                | SpeculativeEditKind::Casing
                | SpeculativeEditKind::DictionaryCorrection
                | SpeculativeEditKind::FillerRemoval
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpeculativeCorrectionPatch {
    segment_id: String,
    original_text: String,
    corrected_text: String,
    confidence: f32,
    edits: Vec<SpeculativeEdit>,
}

impl SpeculativeCorrectionPatch {
    pub fn new(
        segment_id: impl Into<String>,
        original_text: impl Into<String>,
        corrected_text: impl Into<String>,
        confidence: f32,
        edits: Vec<SpeculativeEdit>,
    ) -> Result<Self, TalkError> {
        let segment_id = segment_id.into();
        let original_text = original_text.into();
        let corrected_text = corrected_text.into();
        if segment_id.trim().is_empty() {
            return Err(TalkError::InvalidConfig("segment id must not be blank".to_string()));
        }
        if original_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig("original text must not be blank".to_string()));
        }
        if corrected_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig("corrected text must not be blank".to_string()));
        }
        if !(0.0..=1.0).contains(&confidence) {
            return Err(TalkError::InvalidConfig("confidence must be between 0 and 1".to_string()));
        }
        Ok(Self { segment_id, original_text, corrected_text, confidence, edits })
    }

    pub fn corrected_text(&self) -> &str {
        &self.corrected_text
    }

    pub fn validate_for_mode(&self, mode: SpeculativeMode) -> Result<(), TalkError> {
        if mode == SpeculativeMode::FaithfulDictation
            && self.edits.iter().any(|edit| !edit.is_conservative())
        {
            return Err(TalkError::InvalidConfig(
                "faithful dictation only allows conservative edits".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativeSegment {
    id: String,
    draft_text: String,
    state: SpeculativeSegmentState,
}

impl SpeculativeSegment {
    pub fn new(id: impl Into<String>, draft_text: impl Into<String>) -> Result<Self, TalkError> {
        let id = id.into();
        let draft_text = draft_text.into();
        if id.trim().is_empty() {
            return Err(TalkError::InvalidConfig("segment id must not be blank".to_string()));
        }
        if draft_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig("draft text must not be blank".to_string()));
        }
        Ok(Self { id, draft_text, state: SpeculativeSegmentState::Partial })
    }

    pub fn id(&self) -> &str { &self.id }
    pub fn draft_text(&self) -> &str { &self.draft_text }
    pub fn state(&self) -> SpeculativeSegmentState { self.state }

    pub fn mark_local_final(&mut self, final_text: impl Into<String>) -> Result<(), TalkError> {
        let final_text = final_text.into();
        if final_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig("local final text must not be blank".to_string()));
        }
        self.draft_text = final_text;
        self.state = SpeculativeSegmentState::LocalFinal;
        Ok(())
    }
}
```

Modify `Talk/crates/talk-core/src/lib.rs`:

```rust
pub mod speculative;
pub use speculative::{
    SpeculativeCorrectionPatch, SpeculativeEdit, SpeculativeEditKind, SpeculativeMode,
    SpeculativeSegment, SpeculativeSegmentState,
};
```

- [ ] **Step 4: Verify green**

Run the same `cargo test ... -p talk-core speculative` command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-core/src/lib.rs" "Talk/crates/talk-core/src/speculative.rs" "Talk/crates/talk-core/tests/speculative_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add speculative transcript core contracts"
```

---

## Task 2: Segment endpoint detector

**Files:**
- Create: `Talk/crates/talk-runtime/src/segmenter.rs`
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Create: `Talk/crates/talk-runtime/tests/segmenter_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-runtime/tests/segmenter_contract.rs`:

```rust
use talk_runtime::{evaluate_segment_readiness, SegmentReadiness, SegmenterConfig, SegmenterInput};

#[test]
fn segmenter_commits_sentence_punctuation_after_short_pause() {
    let input = SegmenterInput {
        text: "我明天下午三点有空。".to_string(),
        trailing_silence_ms: 320,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Ready
    );
}

#[test]
fn segmenter_waits_for_short_text_without_pause_or_punctuation() {
    let input = SegmenterInput {
        text: "我明天".to_string(),
        trailing_silence_ms: 40,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Wait
    );
}

#[test]
fn segmenter_forces_long_chunks_even_without_punctuation() {
    let input = SegmenterInput {
        text: "这是一段已经超过最大本地等待长度但是用户还没有明确停顿的中文语音内容".to_string(),
        trailing_silence_ms: 0,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Ready
    );
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-runtime segmenter -- --nocapture
```

Expected: compile failure because segmenter types do not exist.

- [ ] **Step 3: Implement segmenter**

Create `Talk/crates/talk-runtime/src/segmenter.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmenterConfig {
    pub punctuation_pause_ms: u64,
    pub soft_pause_ms: u64,
    pub min_final_chars: usize,
    pub max_chunk_chars: usize,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self { punctuation_pause_ms: 280, soft_pause_ms: 520, min_final_chars: 6, max_chunk_chars: 30 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmenterInput {
    pub text: String,
    pub trailing_silence_ms: u64,
    pub asr_marked_final: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentReadiness {
    Wait,
    Ready,
}

pub fn evaluate_segment_readiness(config: &SegmenterConfig, input: &SegmenterInput) -> SegmentReadiness {
    let char_count = input.text.chars().filter(|item| !item.is_whitespace()).count();
    if char_count == 0 {
        return SegmentReadiness::Wait;
    }
    if char_count >= config.max_chunk_chars {
        return SegmentReadiness::Ready;
    }
    if input.asr_marked_final && char_count >= config.min_final_chars {
        return SegmentReadiness::Ready;
    }
    if ends_with_sentence_punctuation(&input.text) && input.trailing_silence_ms >= config.punctuation_pause_ms {
        return SegmentReadiness::Ready;
    }
    if char_count >= config.min_final_chars && input.trailing_silence_ms >= config.soft_pause_ms {
        return SegmentReadiness::Ready;
    }
    SegmentReadiness::Wait
}

fn ends_with_sentence_punctuation(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .is_some_and(|item| matches!(item, '。' | '！' | '？' | '.' | '!' | '?'))
}
```

Modify `Talk/crates/talk-runtime/src/lib.rs`:

```rust
mod segmenter;
pub use segmenter::{evaluate_segment_readiness, SegmentReadiness, SegmenterConfig, SegmenterInput};
```

- [ ] **Step 4: Verify green**

Run the same runtime segmenter test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-runtime/src/lib.rs" "Talk/crates/talk-runtime/src/segmenter.rs" "Talk/crates/talk-runtime/tests/segmenter_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add speculative segment endpoint detector"
```

---

## Task 3: Streaming ASR event abstraction

**Files:**
- Create: `Talk/crates/talk-client/src/streaming_asr.rs`
- Modify: `Talk/crates/talk-client/src/lib.rs`
- Create: `Talk/crates/talk-client/tests/streaming_asr_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-client/tests/streaming_asr_contract.rs`:

```rust
use talk_client::{MockStreamingAsrEngine, StreamingAsrEngine, StreamingAsrEvent};

#[test]
fn mock_streaming_asr_emits_partial_then_final_events() {
    let mut engine = MockStreamingAsrEngine::new(vec![
        StreamingAsrEvent::partial("seg-1", "你好"),
        StreamingAsrEvent::partial("seg-1", "你好呀"),
        StreamingAsrEvent::final_segment("seg-1", "你好呀。"),
    ]);

    assert_eq!(engine.next_event().unwrap(), StreamingAsrEvent::partial("seg-1", "你好"));
    assert_eq!(engine.next_event().unwrap(), StreamingAsrEvent::partial("seg-1", "你好呀"));
    assert_eq!(engine.next_event().unwrap(), StreamingAsrEvent::final_segment("seg-1", "你好呀。"));
    assert!(engine.next_event().is_none());
}

#[test]
fn streaming_asr_event_rejects_blank_text() {
    let error = StreamingAsrEvent::try_partial("seg-1", "   ").unwrap_err();
    assert!(error.to_string().contains("streaming ASR text must not be blank"));
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-client streaming_asr -- --nocapture
```

Expected: compile failure because streaming ASR types do not exist.

- [ ] **Step 3: Implement mock streaming ASR**

Create `Talk/crates/talk-client/src/streaming_asr.rs`:

```rust
use talk_core::TalkError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingAsrEvent {
    Partial { segment_id: String, text: String },
    Final { segment_id: String, text: String },
}

impl StreamingAsrEvent {
    pub fn partial(segment_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::try_partial(segment_id, text).expect("valid static partial ASR event")
    }

    pub fn final_segment(segment_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::try_final(segment_id, text).expect("valid static final ASR event")
    }

    pub fn try_partial(segment_id: impl Into<String>, text: impl Into<String>) -> Result<Self, TalkError> {
        Self::new(segment_id, text, false)
    }

    pub fn try_final(segment_id: impl Into<String>, text: impl Into<String>) -> Result<Self, TalkError> {
        Self::new(segment_id, text, true)
    }

    fn new(segment_id: impl Into<String>, text: impl Into<String>, final_segment: bool) -> Result<Self, TalkError> {
        let segment_id = segment_id.into();
        let text = text.into();
        if segment_id.trim().is_empty() {
            return Err(TalkError::InvalidConfig("streaming ASR segment id must not be blank".to_string()));
        }
        if text.trim().is_empty() {
            return Err(TalkError::InvalidConfig("streaming ASR text must not be blank".to_string()));
        }
        Ok(if final_segment {
            Self::Final { segment_id, text }
        } else {
            Self::Partial { segment_id, text }
        })
    }

    pub fn segment_id(&self) -> &str {
        match self {
            Self::Partial { segment_id, .. } | Self::Final { segment_id, .. } => segment_id,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Partial { text, .. } | Self::Final { text, .. } => text,
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(self, Self::Final { .. })
    }
}

pub trait StreamingAsrEngine {
    fn next_event(&mut self) -> Option<StreamingAsrEvent>;
}

pub struct MockStreamingAsrEngine {
    events: std::collections::VecDeque<StreamingAsrEvent>,
}

impl MockStreamingAsrEngine {
    pub fn new(events: Vec<StreamingAsrEvent>) -> Self {
        Self { events: events.into() }
    }
}

impl StreamingAsrEngine for MockStreamingAsrEngine {
    fn next_event(&mut self) -> Option<StreamingAsrEvent> {
        self.events.pop_front()
    }
}
```

Modify `Talk/crates/talk-client/src/lib.rs`:

```rust
mod streaming_asr;
pub use streaming_asr::{MockStreamingAsrEngine, StreamingAsrEngine, StreamingAsrEvent};
```

- [ ] **Step 4: Verify green**

Run the same streaming ASR test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-client/src/lib.rs" "Talk/crates/talk-client/src/streaming_asr.rs" "Talk/crates/talk-client/tests/streaming_asr_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add streaming ASR event abstraction"
```

---

## Task 4: Cloud correction patch parsing

**Files:**
- Create: `Talk/crates/talk-client/src/correction.rs`
- Modify: `Talk/crates/talk-client/src/lib.rs`
- Create: `Talk/crates/talk-client/tests/correction_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-client/tests/correction_contract.rs`:

```rust
use talk_client::parse_cloud_correction_patch;
use talk_core::SpeculativeMode;

#[test]
fn parses_conservative_cloud_correction_patch() {
    let json = r#"
    {
      "segment_id": "seg-1",
      "original_text": "我下午三点有空",
      "corrected_text": "我下午三点有空。",
      "confidence": 0.98,
      "edits": [
        { "kind": "punctuation", "before": "空", "after": "空。" }
      ]
    }
    "#;

    let patch = parse_cloud_correction_patch(json, SpeculativeMode::FaithfulDictation).unwrap();
    assert_eq!(patch.corrected_text(), "我下午三点有空。");
}

#[test]
fn rejects_rewrite_patch_in_faithful_mode() {
    let json = r#"
    {
      "segment_id": "seg-1",
      "original_text": "我下午三点有空",
      "corrected_text": "我建议我们明天下午三点开会",
      "confidence": 0.91,
      "edits": [
        { "kind": "rewrite", "before": "我下午三点有空", "after": "我建议我们明天下午三点开会" }
      ]
    }
    "#;

    let error = parse_cloud_correction_patch(json, SpeculativeMode::FaithfulDictation).unwrap_err();
    assert!(error
        .to_string()
        .contains("faithful dictation only allows conservative edits"));
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-client correction -- --nocapture
```

Expected: compile failure because correction parser functions do not exist.

- [ ] **Step 3: Implement parser**

Create `Talk/crates/talk-client/src/correction.rs`:

```rust
use serde::Deserialize;
use talk_core::{
    SpeculativeCorrectionPatch, SpeculativeEdit, SpeculativeEditKind, SpeculativeMode, TalkError,
};

#[derive(Debug, Deserialize)]
struct CloudPatchPayload {
    segment_id: String,
    original_text: String,
    corrected_text: String,
    confidence: f32,
    edits: Vec<CloudEditPayload>,
}

#[derive(Debug, Deserialize)]
struct CloudEditPayload {
    kind: String,
    before: String,
    after: String,
}

fn parse_edit_kind(value: &str) -> Result<SpeculativeEditKind, TalkError> {
    match value {
        "punctuation" => Ok(SpeculativeEditKind::Punctuation),
        "spacing" => Ok(SpeculativeEditKind::Spacing),
        "casing" => Ok(SpeculativeEditKind::Casing),
        "dictionary_correction" => Ok(SpeculativeEditKind::DictionaryCorrection),
        "filler_removal" => Ok(SpeculativeEditKind::FillerRemoval),
        "rewrite" => Ok(SpeculativeEditKind::Rewrite),
        other => Err(TalkError::Provider(format!("unknown speculative edit kind: {other}"))),
    }
}

pub fn parse_cloud_correction_patch(
    json: &str,
    mode: SpeculativeMode,
) -> Result<SpeculativeCorrectionPatch, TalkError> {
    let payload: CloudPatchPayload = serde_json::from_str(json)
        .map_err(|error| TalkError::Provider(format!("invalid correction patch json: {error}")))?;
    let edits = payload
        .edits
        .into_iter()
        .map(|item| {
            Ok(SpeculativeEdit::new(
                parse_edit_kind(&item.kind)?,
                item.before,
                item.after,
            ))
        })
        .collect::<Result<Vec<_>, TalkError>>()?;
    let patch = SpeculativeCorrectionPatch::new(
        payload.segment_id,
        payload.original_text,
        payload.corrected_text,
        payload.confidence,
        edits,
    )?;
    patch.validate_for_mode(mode)?;
    Ok(patch)
}
```

Modify `Talk/crates/talk-client/src/lib.rs`:

```rust
mod correction;
pub use correction::parse_cloud_correction_patch;
```

If `talk-client` lacks `serde_json`, add to `Talk/crates/talk-client/Cargo.toml`:

```toml
serde_json = { workspace = true }
```

- [ ] **Step 4: Verify green**

Run the same correction test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-client/Cargo.toml" "Talk/crates/talk-client/src/lib.rs" "Talk/crates/talk-client/src/correction.rs" "Talk/crates/talk-client/tests/correction_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): parse cloud correction patches"
```

---

## Task 5: Text patch safety helpers

**Files:**
- Create: `Talk/crates/talk-insert/src/patch.rs`
- Modify: `Talk/crates/talk-insert/src/lib.rs`
- Create: `Talk/crates/talk-insert/tests/patch_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-insert/tests/patch_contract.rs`:

```rust
use talk_insert::{compute_patch_edit_ratio, should_auto_apply_corrected_text};

#[test]
fn punctuation_only_change_is_safe_to_auto_apply() {
    assert!(should_auto_apply_corrected_text(
        "我下午三点有空",
        "我下午三点有空。",
        0.25,
    ));
}

#[test]
fn broad_rewrite_is_not_safe_to_auto_apply() {
    assert!(!should_auto_apply_corrected_text(
        "我下午三点有空",
        "我建议我们把会议安排在明天下午三点这样比较合适",
        0.25,
    ));
}

#[test]
fn edit_ratio_counts_changed_characters_against_original_length() {
    let ratio = compute_patch_edit_ratio("你好呀", "你好呀。");
    assert!(ratio > 0.0);
    assert!(ratio < 0.5);
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-insert patch -- --nocapture
```

Expected: compile failure because patch helpers do not exist.

- [ ] **Step 3: Implement helpers**

Create `Talk/crates/talk-insert/src/patch.rs`:

```rust
pub fn compute_patch_edit_ratio(original: &str, corrected: &str) -> f32 {
    let original_chars: Vec<char> = original.chars().collect();
    let corrected_chars: Vec<char> = corrected.chars().collect();
    if original_chars.is_empty() {
        return if corrected_chars.is_empty() { 0.0 } else { 1.0 };
    }
    let common_prefix = original_chars
        .iter()
        .zip(corrected_chars.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let common_suffix = original_chars[common_prefix..]
        .iter()
        .rev()
        .zip(corrected_chars[common_prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let original_changed = original_chars.len().saturating_sub(common_prefix + common_suffix);
    let corrected_changed = corrected_chars.len().saturating_sub(common_prefix + common_suffix);
    original_changed.max(corrected_changed) as f32 / original_chars.len().max(1) as f32
}

pub fn should_auto_apply_corrected_text(original: &str, corrected: &str, max_edit_ratio: f32) -> bool {
    if original == corrected {
        return false;
    }
    compute_patch_edit_ratio(original, corrected) <= max_edit_ratio
}
```

Modify `Talk/crates/talk-insert/src/lib.rs`:

```rust
mod patch;
pub use patch::{compute_patch_edit_ratio, should_auto_apply_corrected_text};
```

- [ ] **Step 4: Verify green**

Run the same patch test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-insert/src/lib.rs" "Talk/crates/talk-insert/src/patch.rs" "Talk/crates/talk-insert/tests/patch_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add safe correction patch primitives"
```

---

## Task 6: Desktop patch decision model

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Write failing tests**

Append to `Talk/crates/talk-desktop/tests/desktop_contract.rs`:

```rust
use talk_desktop::{
    decide_speculative_patch_application, SpeculativeInsertAnchor,
    SpeculativePatchApplication, SpeculativePatchCandidate,
};

#[test]
fn speculative_patch_applies_when_anchor_matches_and_edit_is_small() {
    let anchor = SpeculativeInsertAnchor::new(100, Some(200), "seg-1", "我下午三点有空", 1_000).unwrap();
    let candidate = SpeculativePatchCandidate::new(100, Some(200), "seg-1", "我下午三点有空。", 1_400).unwrap();
    assert_eq!(
        decide_speculative_patch_application(&anchor, &candidate, 2_000, 0.25),
        SpeculativePatchApplication::Apply
    );
}

#[test]
fn speculative_patch_defers_when_focus_changed() {
    let anchor = SpeculativeInsertAnchor::new(100, Some(200), "seg-1", "我下午三点有空", 1_000).unwrap();
    let candidate = SpeculativePatchCandidate::new(100, Some(201), "seg-1", "我下午三点有空。", 1_400).unwrap();
    assert_eq!(
        decide_speculative_patch_application(&anchor, &candidate, 2_000, 0.25),
        SpeculativePatchApplication::DeferToPopup
    );
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop speculative_patch -- --nocapture
```

Expected: compile failure because desktop patch decision types do not exist.

- [ ] **Step 3: Implement pure decision helpers**

Add to `Talk/crates/talk-desktop/src/lib.rs`:

```rust
use talk_insert::should_auto_apply_corrected_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativeInsertAnchor {
    pub window_handle: isize,
    pub focus_handle: Option<isize>,
    pub segment_id: String,
    pub inserted_text: String,
    pub inserted_at_ms: u64,
}

impl SpeculativeInsertAnchor {
    pub fn new(
        window_handle: isize,
        focus_handle: Option<isize>,
        segment_id: impl Into<String>,
        inserted_text: impl Into<String>,
        inserted_at_ms: u64,
    ) -> Result<Self, String> {
        let segment_id = segment_id.into();
        let inserted_text = inserted_text.into();
        if window_handle == 0 {
            return Err("window handle must not be zero".to_string());
        }
        if segment_id.trim().is_empty() {
            return Err("segment id must not be blank".to_string());
        }
        if inserted_text.trim().is_empty() {
            return Err("inserted text must not be blank".to_string());
        }
        Ok(Self { window_handle, focus_handle, segment_id, inserted_text, inserted_at_ms })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeculativePatchCandidate {
    pub current_window_handle: isize,
    pub current_focus_handle: Option<isize>,
    pub segment_id: String,
    pub corrected_text: String,
    pub received_at_ms: u64,
}

impl SpeculativePatchCandidate {
    pub fn new(
        current_window_handle: isize,
        current_focus_handle: Option<isize>,
        segment_id: impl Into<String>,
        corrected_text: impl Into<String>,
        received_at_ms: u64,
    ) -> Result<Self, String> {
        let segment_id = segment_id.into();
        let corrected_text = corrected_text.into();
        if current_window_handle == 0 {
            return Err("current window handle must not be zero".to_string());
        }
        if segment_id.trim().is_empty() {
            return Err("segment id must not be blank".to_string());
        }
        if corrected_text.trim().is_empty() {
            return Err("corrected text must not be blank".to_string());
        }
        Ok(Self { current_window_handle, current_focus_handle, segment_id, corrected_text, received_at_ms })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeculativePatchApplication {
    Apply,
    KeepLocalText,
    DeferToPopup,
}

pub fn decide_speculative_patch_application(
    anchor: &SpeculativeInsertAnchor,
    candidate: &SpeculativePatchCandidate,
    max_age_ms: u64,
    max_edit_ratio: f32,
) -> SpeculativePatchApplication {
    if anchor.segment_id != candidate.segment_id {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.window_handle != candidate.current_window_handle {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.focus_handle != candidate.current_focus_handle {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if candidate.received_at_ms.saturating_sub(anchor.inserted_at_ms) > max_age_ms {
        return SpeculativePatchApplication::DeferToPopup;
    }
    if anchor.inserted_text == candidate.corrected_text {
        return SpeculativePatchApplication::KeepLocalText;
    }
    if should_auto_apply_corrected_text(&anchor.inserted_text, &candidate.corrected_text, max_edit_ratio) {
        SpeculativePatchApplication::Apply
    } else {
        SpeculativePatchApplication::DeferToPopup
    }
}
```

If required, add to `Talk/crates/talk-desktop/Cargo.toml`:

```toml
talk-insert = { path = "../talk-insert" }
```

- [ ] **Step 4: Verify green**

Run the same desktop speculative patch test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-desktop/Cargo.toml" "Talk/crates/talk-desktop/src/lib.rs" "Talk/crates/talk-desktop/tests/desktop_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add desktop speculative patch decisions"
```

---

## Task 7: Runtime speculative event state

**Files:**
- Create: `Talk/crates/talk-runtime/src/speculative.rs`
- Modify: `Talk/crates/talk-runtime/src/lib.rs`
- Create: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`

- [ ] **Step 1: Write failing tests**

Create `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`:

```rust
use talk_client::StreamingAsrEvent;
use talk_runtime::{SpeculativeRuntimeEvent, SpeculativeRuntimeState};

#[test]
fn speculative_runtime_emits_draft_update_for_partial_asr_event() {
    let mut state = SpeculativeRuntimeState::default();
    let event = state
        .accept_asr_event(StreamingAsrEvent::partial("seg-1", "你好"))
        .unwrap();
    assert_eq!(
        event,
        SpeculativeRuntimeEvent::DraftUpdated {
            segment_id: "seg-1".to_string(),
            text: "你好".to_string(),
        }
    );
}

#[test]
fn speculative_runtime_emits_local_commit_for_final_asr_event() {
    let mut state = SpeculativeRuntimeState::default();
    let event = state
        .accept_asr_event(StreamingAsrEvent::final_segment("seg-1", "你好呀。"))
        .unwrap();
    assert_eq!(
        event,
        SpeculativeRuntimeEvent::LocalSegmentCommitted {
            segment_id: "seg-1".to_string(),
            text: "你好呀。".to_string(),
        }
    );
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-runtime speculative_runtime -- --nocapture
```

Expected: compile failure because runtime speculative types do not exist.

- [ ] **Step 3: Implement runtime state**

Create `Talk/crates/talk-runtime/src/speculative.rs`:

```rust
use std::collections::HashMap;
use talk_client::StreamingAsrEvent;
use talk_core::{SpeculativeSegment, TalkError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeculativeRuntimeEvent {
    DraftUpdated { segment_id: String, text: String },
    LocalSegmentCommitted { segment_id: String, text: String },
}

#[derive(Debug, Default)]
pub struct SpeculativeRuntimeState {
    segments: HashMap<String, SpeculativeSegment>,
}

impl SpeculativeRuntimeState {
    pub fn accept_asr_event(&mut self, event: StreamingAsrEvent) -> Result<SpeculativeRuntimeEvent, TalkError> {
        let segment_id = event.segment_id().to_string();
        let text = event.text().to_string();
        if event.is_final() {
            let segment = self
                .segments
                .entry(segment_id.clone())
                .or_insert(SpeculativeSegment::new(segment_id.clone(), text.clone())?);
            segment.mark_local_final(text.clone())?;
            Ok(SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text })
        } else {
            self.segments.insert(segment_id.clone(), SpeculativeSegment::new(segment_id.clone(), text.clone())?);
            Ok(SpeculativeRuntimeEvent::DraftUpdated { segment_id, text })
        }
    }
}
```

Modify `Talk/crates/talk-runtime/src/lib.rs`:

```rust
mod speculative;
pub use speculative::{SpeculativeRuntimeEvent, SpeculativeRuntimeState};
```

- [ ] **Step 4: Verify green**

Run the same runtime speculative test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-runtime/src/lib.rs" "Talk/crates/talk-runtime/src/speculative.rs" "Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add speculative runtime event state"
```

---

## Task 8: Config gates

**Files:**
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Modify: `Talk/crates/talk-core/tests/config_contract.rs`
- Modify: `Talk/examples/desktop-qwen-audio-input-live-config.toml`

- [ ] **Step 1: Add failing config test**

Append to `Talk/crates/talk-core/tests/config_contract.rs`:

```rust
#[test]
fn parses_disabled_speculative_dictation_config() {
    let config: TalkConfig = toml::from_str(r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"

[speculative]
enabled = false
local_asr = "mock"
cloud_correction = "disabled"
max_patch_age_ms = 2000
max_auto_patch_edit_ratio = 0.25
"#).unwrap();

    assert!(!config.speculative.enabled);
    assert_eq!(config.speculative.max_patch_age_ms, 2000);
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-core parses_disabled_speculative_dictation_config -- --nocapture
```

Expected: compile failure or parse failure because `TalkConfig.speculative` does not exist.

- [ ] **Step 3: Add config with disabled default**

Add to `TalkConfig` in `Talk/crates/talk-core/src/lib.rs`:

```rust
#[serde(default)]
pub speculative: SpeculativeConfig,
```

Add:

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SpeculativeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_speculative_local_asr")]
    pub local_asr: String,
    #[serde(default = "default_speculative_cloud_correction")]
    pub cloud_correction: String,
    #[serde(default = "default_speculative_max_patch_age_ms")]
    pub max_patch_age_ms: u64,
    #[serde(default = "default_speculative_max_auto_patch_edit_ratio")]
    pub max_auto_patch_edit_ratio: f32,
}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_asr: default_speculative_local_asr(),
            cloud_correction: default_speculative_cloud_correction(),
            max_patch_age_ms: default_speculative_max_patch_age_ms(),
            max_auto_patch_edit_ratio: default_speculative_max_auto_patch_edit_ratio(),
        }
    }
}

fn default_speculative_local_asr() -> String { "mock".to_string() }
fn default_speculative_cloud_correction() -> String { "disabled".to_string() }
fn default_speculative_max_patch_age_ms() -> u64 { 2_000 }
fn default_speculative_max_auto_patch_edit_ratio() -> f32 { 0.25 }
```

Add validation:

```rust
if self.speculative.max_patch_age_ms == 0 {
    problems.push("speculative.max_patch_age_ms must be greater than 0".to_string());
}
if !(0.0..=1.0).contains(&self.speculative.max_auto_patch_edit_ratio) {
    problems.push("speculative.max_auto_patch_edit_ratio must be between 0 and 1".to_string());
}
```

Add disabled example to `Talk/examples/desktop-qwen-audio-input-live-config.toml`:

```toml
[speculative]
enabled = false
local_asr = "mock"
cloud_correction = "disabled"
max_patch_age_ms = 2000
max_auto_patch_edit_ratio = 0.25
```

- [ ] **Step 4: Verify green**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-core speculative -- --nocapture
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-core parses_desktop_qwen_audio_input_live_example_config -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-core/src/lib.rs" "Talk/crates/talk-core/tests/config_contract.rs" "Talk/examples/desktop-qwen-audio-input-live-config.toml"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add speculative dictation config gates"
```

---

## Task 9: Desktop speculative HUD model

**Files:**
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add failing HUD tests**

Append to `Talk/crates/talk-desktop/tests/desktop_contract.rs`:

```rust
use talk_desktop::{desktop_speculative_transcript_view_model, DesktopSpeculativeTranscriptState};

#[test]
fn speculative_hud_marks_partial_text_as_draft() {
    let model = desktop_speculative_transcript_view_model(
        DesktopSpeculativeTranscriptState::Partial,
        "你好",
    );
    assert_eq!(model.text, "你好");
    assert_eq!(model.opacity_percent, 62);
    assert!(!model.show_cloud_corrected_mark);
}

#[test]
fn speculative_hud_marks_cloud_corrected_text_as_stable() {
    let model = desktop_speculative_transcript_view_model(
        DesktopSpeculativeTranscriptState::CloudCorrected,
        "你好呀。",
    );
    assert_eq!(model.text, "你好呀。");
    assert_eq!(model.opacity_percent, 100);
    assert!(model.show_cloud_corrected_mark);
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop speculative_hud -- --nocapture
```

Expected: compile failure because HUD speculative types do not exist.

- [ ] **Step 3: Implement HUD view model**

Add to `Talk/crates/talk-desktop/src/lib.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopSpeculativeTranscriptState {
    Partial,
    LocalFinal,
    CloudCorrecting,
    CloudCorrected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSpeculativeTranscriptViewModel {
    pub text: String,
    pub opacity_percent: u8,
    pub show_cloud_corrected_mark: bool,
}

pub fn desktop_speculative_transcript_view_model(
    state: DesktopSpeculativeTranscriptState,
    text: &str,
) -> DesktopSpeculativeTranscriptViewModel {
    let opacity_percent = match state {
        DesktopSpeculativeTranscriptState::Partial => 62,
        DesktopSpeculativeTranscriptState::LocalFinal => 88,
        DesktopSpeculativeTranscriptState::CloudCorrecting => 88,
        DesktopSpeculativeTranscriptState::CloudCorrected => 100,
    };
    DesktopSpeculativeTranscriptViewModel {
        text: text.to_string(),
        opacity_percent,
        show_cloud_corrected_mark: state == DesktopSpeculativeTranscriptState::CloudCorrected,
    }
}
```

- [ ] **Step 4: Verify green**

Run the same speculative HUD test command. Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-desktop/src/lib.rs" "Talk/crates/talk-desktop/tests/desktop_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add speculative transcript HUD model"
```

---

## Task 10: Desktop integration behind config

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add disabled-by-default tests**

Append to `Talk/crates/talk-desktop/tests/desktop_contract.rs`:

```rust
use talk_desktop::{desktop_speculative_pipeline_enabled, DesktopSpeculativePipelineConfig};

#[test]
fn desktop_speculative_pipeline_is_disabled_by_default() {
    assert!(!desktop_speculative_pipeline_enabled(&DesktopSpeculativePipelineConfig::default()));
}

#[test]
fn desktop_speculative_pipeline_enables_only_when_local_asr_is_configured() {
    let config = DesktopSpeculativePipelineConfig {
        enabled: true,
        local_asr: "mock".to_string(),
        cloud_correction: "disabled".to_string(),
    };
    assert!(desktop_speculative_pipeline_enabled(&config));
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop desktop_speculative_pipeline -- --nocapture
```

Expected: compile failure because pipeline config helper does not exist.

- [ ] **Step 3: Implement pure enablement helper**

Add to `Talk/crates/talk-desktop/src/lib.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopSpeculativePipelineConfig {
    pub enabled: bool,
    pub local_asr: String,
    pub cloud_correction: String,
}

impl Default for DesktopSpeculativePipelineConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_asr: "mock".to_string(),
            cloud_correction: "disabled".to_string(),
        }
    }
}

pub fn desktop_speculative_pipeline_enabled(config: &DesktopSpeculativePipelineConfig) -> bool {
    config.enabled && !config.local_asr.trim().is_empty()
}
```

- [ ] **Step 4: Wire main.rs without changing default behavior**

In `Talk/crates/talk-desktop/src/main.rs`, preserve the current batch path when disabled:

```rust
if !effective_config.speculative.enabled {
    // Existing run_voice_session_from_audio_artifact_with_insert_hooks path remains active.
}
```

Add an enabled-path function that returns a clear error until Tasks 11 and 12 finish integration:

```rust
fn run_speculative_desktop_session() -> Result<()> {
    anyhow::bail!(
        "speculative dictation is enabled but local-first desktop runtime is not active in this build"
    )
}
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop desktop_speculative_pipeline -- --nocapture
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" --workspace
```

Expected: pass. The known negative-case message `Talk capability server host must be loopback, got 0.0.0.0` is acceptable when the final exit code is 0.

- [ ] **Step 6: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-desktop/src/lib.rs" "Talk/crates/talk-desktop/src/main.rs" "Talk/crates/talk-desktop/tests/desktop_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): gate speculative desktop pipeline"
```

---

## Task 11: Mock speculative desktop preview

**Files:**
- Modify: `Talk/crates/talk-runtime/src/speculative.rs`
- Modify: `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`
- Modify: `Talk/crates/talk-desktop/src/main.rs`

- [ ] **Step 1: Add runtime mock session test**

Append to `Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs`:

```rust
use talk_runtime::run_mock_speculative_session;

#[test]
fn mock_speculative_session_emits_draft_and_commit_events() {
    let events = run_mock_speculative_session(vec![
        (false, "seg-1", "你好"),
        (true, "seg-1", "你好呀"),
    ])
    .unwrap();

    assert_eq!(events.len(), 2);
    assert!(matches!(events[0], SpeculativeRuntimeEvent::DraftUpdated { .. }));
    assert!(matches!(events[1], SpeculativeRuntimeEvent::LocalSegmentCommitted { .. }));
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-runtime mock_speculative_session -- --nocapture
```

Expected: compile failure because helper does not exist.

- [ ] **Step 3: Implement helper**

Add to `Talk/crates/talk-runtime/src/speculative.rs`:

```rust
pub fn run_mock_speculative_session(
    inputs: Vec<(bool, &str, &str)>,
) -> Result<Vec<SpeculativeRuntimeEvent>, TalkError> {
    let mut state = SpeculativeRuntimeState::default();
    let mut events = Vec::new();
    for (is_final, segment_id, text) in inputs {
        let event = if is_final {
            StreamingAsrEvent::try_final(segment_id, text)?
        } else {
            StreamingAsrEvent::try_partial(segment_id, text)?
        };
        events.push(state.accept_asr_event(event)?);
    }
    Ok(events)
}
```

- [ ] **Step 4: Wire preview-only desktop behavior**

In `Talk/crates/talk-desktop/src/main.rs`, when `speculative.enabled = true` and `local_asr = "mock"`, route mock events to the HUD and keep final insertion on the existing batch path:

```rust
match event {
    SpeculativeRuntimeEvent::DraftUpdated { text, .. } => {
        show_hud_text(hwnd, &text, None)?;
    }
    SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => {
        show_hud_text(hwnd, &text, None)?;
    }
}
```

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-runtime mock_speculative_session -- --nocapture
cargo check --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop --all-targets
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-runtime/src/speculative.rs" "Talk/crates/talk-runtime/tests/speculative_runtime_contract.rs" "Talk/crates/talk-desktop/src/main.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add mock speculative desktop preview"
```

---

## Task 12: Safe replacement mechanics

**Files:**
- Modify: `Talk/crates/talk-desktop/src/main.rs`
- Modify: `Talk/crates/talk-desktop/src/lib.rs`
- Modify: `Talk/crates/talk-desktop/tests/desktop_contract.rs`

- [ ] **Step 1: Add replacement count tests**

Append to `Talk/crates/talk-desktop/tests/desktop_contract.rs`:

```rust
use talk_desktop::desktop_speculative_replacement_selection_count;

#[test]
fn replacement_selection_count_counts_unicode_scalars() {
    assert_eq!(desktop_speculative_replacement_selection_count("你好呀。"), 4);
}

#[test]
fn replacement_selection_count_ignores_empty_text() {
    assert_eq!(desktop_speculative_replacement_selection_count(""), 0);
}
```

- [ ] **Step 2: Verify red**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop replacement_selection_count -- --nocapture
```

Expected: compile failure because helper does not exist.

- [ ] **Step 3: Implement helper**

Add to `Talk/crates/talk-desktop/src/lib.rs`:

```rust
pub fn desktop_speculative_replacement_selection_count(text: &str) -> usize {
    text.chars().count()
}
```

- [ ] **Step 4: Add guarded patch application in main.rs**

When `decide_speculative_patch_application(...) == Apply`, the desktop shell applies correction only if current foreground target still matches the insert anchor:

```text
1. Select the previous local segment with Shift+Left repeated N Unicode scalar positions.
2. Put corrected text on the clipboard using WindowsClipboardBackend.
3. Paste with the same preferred paste shortcut path used by normal insertion.
4. Restore clipboard according to current output config.
5. If any target check fails, show the corrected text in the copy popup instead of modifying the external app.
```

The code must reuse existing insertion and clipboard helpers; it must not introduce a second raw paste implementation.

- [ ] **Step 5: Verify**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop speculative_patch replacement_selection_count -- --nocapture
cargo check --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-desktop --all-targets
```

Expected: pass.

- [ ] **Step 6: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-desktop/src/lib.rs" "Talk/crates/talk-desktop/src/main.rs" "Talk/crates/talk-desktop/tests/desktop_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): apply safe speculative correction patches"
```

---

## Task 13: External local ASR JSON-line adapter contract

**Files:**
- Modify: `Talk/crates/talk-client/src/streaming_asr.rs`
- Modify: `Talk/crates/talk-client/tests/streaming_asr_contract.rs`
- Modify: `Talk/crates/talk-core/src/lib.rs`
- Modify: `Talk/crates/talk-core/tests/config_contract.rs`

- [ ] **Step 1: Add config and parser tests**

Append to `Talk/crates/talk-client/tests/streaming_asr_contract.rs`:

```rust
use talk_client::parse_streaming_asr_json_line;

#[test]
fn parses_external_asr_json_line() {
    let event = parse_streaming_asr_json_line(
        r#"{"type":"partial","segment_id":"seg-1","text":"你好"}"#,
    )
    .unwrap();
    assert_eq!(event, StreamingAsrEvent::partial("seg-1", "你好"));
}
```

Append to `Talk/crates/talk-core/tests/config_contract.rs`:

```rust
#[test]
fn parses_speculative_external_local_asr_command() {
    let config: TalkConfig = toml::from_str(r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"

[speculative]
enabled = true
local_asr = "external_command"
cloud_correction = "disabled"
external_asr_command = "local-asr.exe --jsonl"
"#).unwrap();

    assert_eq!(config.speculative.local_asr, "external_command");
    assert_eq!(config.speculative.external_asr_command.as_deref(), Some("local-asr.exe --jsonl"));
}
```

- [ ] **Step 2: Verify red**

Run both new tests. Expected: failures because external command config and parser are not implemented.

- [ ] **Step 3: Implement parser and config field**

Extend `SpeculativeConfig`:

```rust
#[serde(default)]
pub external_asr_command: Option<String>,
```

Add validation:

```rust
if self.speculative.enabled
    && self.speculative.local_asr == "external_command"
    && self
        .speculative
        .external_asr_command
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
{
    problems.push(
        "speculative.external_asr_command must be set when local_asr is external_command".to_string(),
    );
}
```

Add to `Talk/crates/talk-client/src/streaming_asr.rs`:

```rust
#[derive(Debug, serde::Deserialize)]
struct ExternalAsrJsonLine {
    #[serde(rename = "type")]
    kind: String,
    segment_id: String,
    text: String,
}

pub fn parse_streaming_asr_json_line(line: &str) -> Result<StreamingAsrEvent, TalkError> {
    let item: ExternalAsrJsonLine = serde_json::from_str(line)
        .map_err(|error| TalkError::Provider(format!("invalid streaming ASR json line: {error}")))?;
    match item.kind.as_str() {
        "partial" => StreamingAsrEvent::try_partial(item.segment_id, item.text),
        "final" => StreamingAsrEvent::try_final(item.segment_id, item.text),
        other => Err(TalkError::Provider(format!("unknown streaming ASR event type: {other}"))),
    }
}
```

Expose `parse_streaming_asr_json_line` from `Talk/crates/talk-client/src/lib.rs`.

- [ ] **Step 4: Verify green**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-core parses_speculative_external_local_asr_command -- --nocapture
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" -p talk-client parses_external_asr_json_line -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/crates/talk-core/src/lib.rs" "Talk/crates/talk-core/tests/config_contract.rs" "Talk/crates/talk-client/src/lib.rs" "Talk/crates/talk-client/src/streaming_asr.rs" "Talk/crates/talk-client/tests/streaming_asr_contract.rs"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "feat(talk): add external streaming ASR adapter contract"
```

---

## Task 14: Documentation

**Files:**
- Modify: `Talk/README.md`
- Modify: `Talk/docs/TALK_DESIGN.md`
- Create: `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`

- [ ] **Step 1: Create architecture document**

Create `Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`:

```markdown
# Local-First Speculative Dictation

Talk's long-term voice input architecture is local-first. Local ASR is responsible for immediate text visibility. Cloud correction is asynchronous and never blocks the first visible text.

## Pipeline

```text
Mic -> local streaming ASR -> partial/local-final text -> segment detector
                                             -> async cloud correction -> safe patch or popup
```

## Safety Rules

1. Cloud correction never blocks local text display.
2. Ordinary dictation applies only conservative cloud edits automatically.
3. Broad rewrites require polish/translate/ask mode or user confirmation.
4. A correction is auto-applied only when the original window and focused control still match the insert anchor.
5. Stale corrections fall back to a copy/suggestion popup.
```

- [ ] **Step 2: Link from README**

Add to `Talk/README.md`:

```markdown
### Local-first speculative dictation

Talk is being prepared for a local-first speculative dictation pipeline. The live input path is designed to show local ASR text immediately, then apply cloud correction only when the original target is still safe. See [`docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md`](docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md).
```

- [ ] **Step 3: Update design doc**

Add to `Talk/docs/TALK_DESIGN.md`:

```markdown
## Local-first speculative dictation direction

The default dictation path should not wait for cloud inference before the user sees text. Local streaming ASR owns the latency budget. Cloud correction is asynchronous, conservative by default, and applied through the same target-identity safety rules used by desktop insertion.
```

- [ ] **Step 4: Verify**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" --workspace
```

Expected: pass.

- [ ] **Step 5: Commit**

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk/README.md" "Talk/docs/TALK_DESIGN.md" "Talk/docs/LOCAL_FIRST_SPECULATIVE_DICTATION.md"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "docs(talk): document local-first speculative dictation"
```

---

## Task 15: Full verification and release

**Files:**
- Modify only files changed by formatting.
- Release output: `C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk`

- [ ] **Step 1: Format**

```powershell
cargo fmt --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" --all
```

Expected: exit code 0.

- [ ] **Step 2: Full check**

```powershell
cargo check --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" --workspace --all-targets
```

Expected: exit code 0.

- [ ] **Step 3: Full tests**

```powershell
cargo test --manifest-path "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\Cargo.toml" --workspace
```

Expected: exit code 0. The known negative-case message `Error: Talk capability server host must be loopback, got 0.0.0.0` is acceptable when the final exit code is 0.

- [ ] **Step 4: Publish release**

```powershell
& "C:\Users\Public\nas_home\AI\GameEditor\Neuro\Talk\scripts\Publish-TalkRelease.ps1" -VersionId "desktop-shell-local-first-speculative-v1" -SkipSmoke
```

Expected user-facing executable:

```text
C:\Users\Public\nas_home\AI\GameEditor\Neuro\release\Talk\desktop-shell-local-first-speculative-v1\talk-desktop.exe
```

- [ ] **Step 5: Commit formatting changes if any**

If `cargo fmt` changed tracked files:

```powershell
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" add -- "Talk"
git -C "C:\Users\Public\nas_home\AI\GameEditor\Neuro" commit -m "chore(talk): format speculative dictation changes"
```

If no files changed, this commit is skipped.

---

## Self-review

### Spec coverage

- Local-first text visibility: Tasks 3, 7, 9, 10, 11.
- Segment completion: Task 2.
- Async cloud correction patch format: Task 4.
- Patch safety: Tasks 5, 6, 12.
- Disabled-by-default rollout: Tasks 8, 10.
- External local ASR boundary: Task 13.
- Documentation: Task 14.
- Verification and release: Task 15.

### Scope control

This plan builds the product architecture and provider boundary. It does not choose or vendor a specific RNN-T/Conformer model. The concrete local model can be connected through the external JSON-line ASR adapter after this structure is merged and verified.

### Type consistency

The speculative core types introduced in Task 1 are reused by Tasks 4, 7, and 8. The patch helpers introduced in Task 5 are reused by the desktop decision helper in Task 6. The desktop anchor and patch candidate names are consistent between tests and implementation snippets.
