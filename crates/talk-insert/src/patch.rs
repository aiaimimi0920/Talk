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
    let original_changed = original_chars
        .len()
        .saturating_sub(common_prefix + common_suffix);
    let corrected_changed = corrected_chars
        .len()
        .saturating_sub(common_prefix + common_suffix);
    original_changed.max(corrected_changed) as f32 / original_chars.len().max(1) as f32
}

pub fn should_auto_apply_corrected_text(
    original: &str,
    corrected: &str,
    max_edit_ratio: f32,
) -> bool {
    if original == corrected {
        return false;
    }
    compute_patch_edit_ratio(original, corrected) <= max_edit_ratio
}
