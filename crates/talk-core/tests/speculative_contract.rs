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
