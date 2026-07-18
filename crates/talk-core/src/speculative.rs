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
    pub fn new(
        kind: SpeculativeEditKind,
        before: impl Into<String>,
        after: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            before: before.into(),
            after: after.into(),
        }
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
            return Err(TalkError::InvalidConfig(
                "segment id must not be blank".to_string(),
            ));
        }
        if original_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "original text must not be blank".to_string(),
            ));
        }
        if corrected_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "corrected text must not be blank".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&confidence) {
            return Err(TalkError::InvalidConfig(
                "confidence must be between 0 and 1".to_string(),
            ));
        }
        Ok(Self {
            segment_id,
            original_text,
            corrected_text,
            confidence,
            edits,
        })
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
            return Err(TalkError::InvalidConfig(
                "segment id must not be blank".to_string(),
            ));
        }
        if draft_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "draft text must not be blank".to_string(),
            ));
        }
        Ok(Self {
            id,
            draft_text,
            state: SpeculativeSegmentState::Partial,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn draft_text(&self) -> &str {
        &self.draft_text
    }

    pub fn state(&self) -> SpeculativeSegmentState {
        self.state
    }

    pub fn mark_local_final(&mut self, final_text: impl Into<String>) -> Result<(), TalkError> {
        let final_text = final_text.into();
        if final_text.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "local final text must not be blank".to_string(),
            ));
        }
        self.draft_text = final_text;
        self.state = SpeculativeSegmentState::LocalFinal;
        Ok(())
    }
}
