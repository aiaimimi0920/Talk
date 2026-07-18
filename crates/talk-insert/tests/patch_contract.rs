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
