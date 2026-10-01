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
    assert!((ratio - (1.0 / 3.0)).abs() < f32::EPSILON);
}

#[test]
fn distributed_small_corrections_use_actual_edit_distance() {
    let original = "现在办公室里有一点空调和键盘声， 请继续记录 talk 的多语言识别测试结果";
    let corrected = "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。";

    let ratio = compute_patch_edit_ratio(original, corrected);

    assert!((ratio - (3.0 / 38.0)).abs() < f32::EPSILON);
    assert!(should_auto_apply_corrected_text(original, corrected, 0.25,));
}

#[test]
fn empty_text_edit_ratios_are_explicit() {
    assert_eq!(compute_patch_edit_ratio("", ""), 0.0);
    assert_eq!(compute_patch_edit_ratio("", "text"), 1.0);
    assert_eq!(compute_patch_edit_ratio("text", ""), 1.0);
}

#[test]
fn faithful_threshold_accepts_moderate_replay_corrections_only() {
    assert!(should_auto_apply_corrected_text(
        "打开 talk 的 localfosterasr 测试",
        "打开 Talk 的 local first ASR 测试",
        0.35,
    ));
    assert!(should_auto_apply_corrected_text(
        "今天下午三点半， 我们掀开项目例会， 确认 talk 的默认识别模型， 然后把多语言测试结果同步给",
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。",
        0.35,
    ));
    assert!(!should_auto_apply_corrected_text(
        "请把 neo talk 的千问三 ASR flush 结果保存到 SIPA 的 user",
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。",
        0.35,
    ));
}

#[test]
fn bounded_auto_apply_matches_exact_edit_ratio_for_small_strings() {
    fn strings(alphabet: &[char], max_len: usize) -> Vec<String> {
        let mut values = vec![String::new()];
        for _ in 0..max_len {
            let prefixes = values.clone();
            for prefix in prefixes {
                for character in alphabet {
                    let mut value = prefix.clone();
                    value.push(*character);
                    values.push(value);
                }
            }
        }
        values.sort();
        values.dedup();
        values
    }

    let values = strings(&['a', '界'], 4);
    for original in &values {
        for corrected in &values {
            for threshold in [0.0, 0.25, 0.35, 0.5, 1.0, 2.0] {
                let expected = original != corrected
                    && compute_patch_edit_ratio(original, corrected) <= threshold;
                assert_eq!(
                    should_auto_apply_corrected_text(original, corrected, threshold),
                    expected,
                    "original={original:?} corrected={corrected:?} threshold={threshold}"
                );
            }
        }
    }
}

#[test]
fn bounded_auto_apply_rejects_large_equal_length_rewrite_without_full_matrix_work() {
    let original = "a".repeat(20_000);
    let corrected = "界".repeat(20_000);

    assert!(!should_auto_apply_corrected_text(
        &original, &corrected, 0.001,
    ));
}

#[test]
fn bounded_auto_apply_trims_large_common_prefix_and_suffix() {
    let prefix = "Talk 长文本上下文。".repeat(2_000);
    let original = format!("{prefix}误{prefix}");
    let corrected = format!("{prefix}正{prefix}");

    assert!(should_auto_apply_corrected_text(
        &original, &corrected, 0.001,
    ));
}

#[test]
fn auto_apply_threshold_handles_non_finite_values_explicitly() {
    assert!(!should_auto_apply_corrected_text(
        "original",
        "corrected",
        f32::NAN,
    ));
    assert!(!should_auto_apply_corrected_text(
        "original",
        "corrected",
        f32::NEG_INFINITY,
    ));
    assert!(should_auto_apply_corrected_text(
        "original",
        "corrected",
        f32::INFINITY,
    ));
}
