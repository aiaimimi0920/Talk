pub fn compute_patch_edit_ratio(original: &str, corrected: &str) -> f32 {
    let original_chars: Vec<char> = original.chars().collect();
    let corrected_chars: Vec<char> = corrected.chars().collect();
    if original_chars.is_empty() {
        return if corrected_chars.is_empty() { 0.0 } else { 1.0 };
    }

    // A shared prefix/suffix contributes zero edits, so trimming it before the
    // O(n*m) DP leaves the distance unchanged while shrinking the DP to the
    // actually-edited middle. The ratio still divides by the full original
    // length, so the result is identical to the untrimmed computation.
    let common_prefix = original_chars
        .iter()
        .zip(&corrected_chars)
        .take_while(|(original_char, corrected_char)| original_char == corrected_char)
        .count();
    let trimmed_original = &original_chars[common_prefix..];
    let trimmed_corrected = &corrected_chars[common_prefix..];
    let common_suffix = trimmed_original
        .iter()
        .rev()
        .zip(trimmed_corrected.iter().rev())
        .take_while(|(original_char, corrected_char)| original_char == corrected_char)
        .count();
    let trimmed_original = &trimmed_original[..trimmed_original.len() - common_suffix];
    let trimmed_corrected = &trimmed_corrected[..trimmed_corrected.len() - common_suffix];

    let distance = levenshtein_distance(trimmed_original, trimmed_corrected);
    distance as f32 / original_chars.len() as f32
}

fn levenshtein_distance(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution_cost = usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[right.len()]
}

pub fn should_auto_apply_corrected_text(
    original: &str,
    corrected: &str,
    max_edit_ratio: f32,
) -> bool {
    if original == corrected {
        return false;
    }
    if max_edit_ratio.is_nan() || max_edit_ratio < 0.0 {
        return false;
    }
    if max_edit_ratio == f32::INFINITY {
        return true;
    }

    let original_chars = original.chars().collect::<Vec<_>>();
    let corrected_chars = corrected.chars().collect::<Vec<_>>();
    if original_chars.is_empty() {
        return 1.0 <= max_edit_ratio;
    }

    let max_distance = (max_edit_ratio * original_chars.len() as f32).floor() as usize;
    levenshtein_distance_with_limit(&original_chars, &corrected_chars, max_distance).is_some()
}

fn levenshtein_distance_with_limit(
    left: &[char],
    right: &[char],
    max_distance: usize,
) -> Option<usize> {
    let common_prefix = left
        .iter()
        .zip(right)
        .take_while(|(left, right)| left == right)
        .count();
    let left = &left[common_prefix..];
    let right = &right[common_prefix..];
    let common_suffix = left
        .iter()
        .rev()
        .zip(right.iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let left = &left[..left.len() - common_suffix];
    let right = &right[..right.len() - common_suffix];

    if left.len().abs_diff(right.len()) > max_distance {
        return None;
    }
    if left.is_empty() {
        return (right.len() <= max_distance).then_some(right.len());
    }
    if right.is_empty() {
        return (left.len() <= max_distance).then_some(left.len());
    }

    let max_distance = max_distance.min(left.len().max(right.len()));
    let unreachable = max_distance.saturating_add(1);
    let band_capacity = max_distance
        .saturating_mul(2)
        .saturating_add(1)
        .min(right.len() + 1);
    let mut previous = vec![unreachable; band_capacity];
    let mut previous_start = 0;
    let mut previous_end = max_distance.min(right.len());
    for column in previous_start..=previous_end {
        previous[column - previous_start] = column;
    }
    let mut current = vec![unreachable; band_capacity];

    for (left_index, left_char) in left.iter().enumerate() {
        let row = left_index + 1;
        let start = row.saturating_sub(max_distance);
        let end = row.saturating_add(max_distance).min(right.len());
        if start > end {
            return None;
        }
        let mut row_min = unreachable;
        for column in start..=end {
            let current_index = column - start;
            let distance = if column == 0 {
                row
            } else {
                let deletion = if (previous_start..=previous_end).contains(&column) {
                    previous[column - previous_start].saturating_add(1)
                } else {
                    unreachable
                };
                let insertion = if column > start {
                    current[current_index - 1].saturating_add(1)
                } else {
                    unreachable
                };
                let diagonal = column - 1;
                let substitution = if (previous_start..=previous_end).contains(&diagonal) {
                    previous[diagonal - previous_start]
                        .saturating_add(usize::from(left_char != &right[column - 1]))
                } else {
                    unreachable
                };
                deletion.min(insertion).min(substitution)
            };
            current[current_index] = distance;
            row_min = row_min.min(distance);
        }
        if row_min > max_distance {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
        previous_start = start;
        previous_end = end;
    }

    if !(previous_start..=previous_end).contains(&right.len()) {
        return None;
    }
    let distance = previous[right.len() - previous_start];
    (distance <= max_distance).then_some(distance)
}
