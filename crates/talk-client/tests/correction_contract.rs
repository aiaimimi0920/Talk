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
