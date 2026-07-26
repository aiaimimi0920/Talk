use serde::Serialize;
use talk_core::VoiceMode;

const LEADING_INTENT_MAX_NON_WHITESPACE: usize = 96;
const SHORT_DIRECT_MAX_NON_WHITESPACE: usize = 80;
const LONG_FORM_MIN_NON_WHITESPACE: usize = 160;
const LONG_FORM_MIN_SENTENCE_BOUNDARIES: usize = 3;
const LONG_FORM_MIN_STREAMING_SEGMENTS: usize = 3;
const FAITHFUL_LONG_INPUT_MIN_CHARS: usize = 120;
const FAITHFUL_MIN_RETENTION_RATIO: f64 = 0.60;
const FAITHFUL_MAX_CHANGE_RATIO: f64 = 0.35;

const SMART_POLITE_PREFIXES: [&str; 17] = [
    "please help me ",
    "please help me",
    "could you ",
    "could you",
    "can you ",
    "can you",
    "help me ",
    "help me",
    "please ",
    "please",
    "请帮我",
    "请你帮我",
    "麻烦帮我",
    "麻烦你",
    "帮我",
    "请你",
    "请",
];

const SMART_CHINESE_INTENT_ACTIONS: [&str; 24] = [
    "总结",
    "概括",
    "归纳",
    "润色",
    "改写",
    "整理",
    "翻译",
    "生成",
    "创作",
    "起草",
    "写一篇",
    "写一段",
    "写一个",
    "写一份",
    "写一封",
    "写封",
    "打开",
    "关闭",
    "启动",
    "运行",
    "执行",
    "删除",
    "复制",
    "移动",
];

const SMART_ENGLISH_INTENT_ACTIONS: [&str; 16] = [
    "summarize",
    "summarise",
    "polish",
    "rewrite",
    "translate",
    "write",
    "draft",
    "compose",
    "generate",
    "open",
    "launch",
    "run",
    "close",
    "delete",
    "copy",
    "move",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartLeadingIntent {
    Document,
    Translate,
    Generate,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartRouteReason {
    Empty,
    ExplicitDocumentInstruction,
    ExplicitTranslateInstruction,
    ExplicitGenerateInstruction,
    ShortDirectCommand,
    LongCharacterCount,
    MultipleSentenceBoundaries,
    MultipleStreamingSegments,
    TranscribeFallback,
}

impl SmartRouteReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::ExplicitDocumentInstruction => "explicit_document_instruction",
            Self::ExplicitTranslateInstruction => "explicit_translate_instruction",
            Self::ExplicitGenerateInstruction => "explicit_generate_instruction",
            Self::ShortDirectCommand => "short_direct_command",
            Self::LongCharacterCount => "long_character_count",
            Self::MultipleSentenceBoundaries => "multiple_sentence_boundaries",
            Self::MultipleStreamingSegments => "multiple_streaming_segments",
            Self::TranscribeFallback => "transcribe_fallback",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SmartRouteEvidence {
    pub committed_streaming_segment_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SmartVoiceRouteAnalysis {
    pub non_whitespace_char_count: usize,
    pub sentence_boundary_count: usize,
    pub committed_streaming_segment_count: usize,
    pub long_form_evidence: bool,
    pub leading_intent: Option<SmartLeadingIntent>,
    pub resolved_mode: VoiceMode,
    pub reason: SmartRouteReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FaithfulOutputFallbackReason {
    CatastrophicCompression,
    ExcessiveSequenceChange,
}

impl FaithfulOutputFallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CatastrophicCompression => "catastrophic_compression",
            Self::ExcessiveSequenceChange => "excessive_sequence_change",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FaithfulOutputValidation {
    pub accepted: bool,
    pub fallback_reason: Option<FaithfulOutputFallbackReason>,
    pub input_char_count: usize,
    pub output_char_count: usize,
    pub retention_ratio: f64,
    pub normalized_change_ratio: f64,
}

pub fn infer_smart_voice_mode(transcript: &str) -> VoiceMode {
    analyze_smart_voice_mode(transcript, SmartRouteEvidence::default()).resolved_mode
}

pub fn analyze_smart_voice_mode(
    transcript: &str,
    evidence: SmartRouteEvidence,
) -> SmartVoiceRouteAnalysis {
    let non_whitespace_char_count = count_non_whitespace(transcript);
    let sentence_boundary_count = count_sentence_boundaries(transcript);
    let committed_streaming_segment_count = evidence.committed_streaming_segment_count;
    let long_form_reason = if non_whitespace_char_count >= LONG_FORM_MIN_NON_WHITESPACE {
        Some(SmartRouteReason::LongCharacterCount)
    } else if sentence_boundary_count >= LONG_FORM_MIN_SENTENCE_BOUNDARIES {
        Some(SmartRouteReason::MultipleSentenceBoundaries)
    } else if committed_streaming_segment_count >= LONG_FORM_MIN_STREAMING_SEGMENTS {
        Some(SmartRouteReason::MultipleStreamingSegments)
    } else {
        None
    };
    let long_form_evidence = long_form_reason.is_some();

    let leading_intent = leading_intent(transcript);
    let (resolved_mode, reason) = if non_whitespace_char_count == 0 {
        (VoiceMode::Transcribe, SmartRouteReason::Empty)
    } else if let Some(intent) = leading_intent {
        match intent {
            SmartLeadingIntent::Document => (
                VoiceMode::Document,
                SmartRouteReason::ExplicitDocumentInstruction,
            ),
            SmartLeadingIntent::Translate => (
                VoiceMode::Translate,
                SmartRouteReason::ExplicitTranslateInstruction,
            ),
            SmartLeadingIntent::Generate => (
                VoiceMode::Generate,
                SmartRouteReason::ExplicitGenerateInstruction,
            ),
            SmartLeadingIntent::Command
                if non_whitespace_char_count <= SHORT_DIRECT_MAX_NON_WHITESPACE =>
            {
                (VoiceMode::Command, SmartRouteReason::ShortDirectCommand)
            }
            SmartLeadingIntent::Command => (
                VoiceMode::Transcribe,
                long_form_reason.unwrap_or(SmartRouteReason::TranscribeFallback),
            ),
        }
    } else if long_form_evidence {
        (
            VoiceMode::Transcribe,
            long_form_reason.expect("long_form_evidence implies a route reason"),
        )
    } else {
        (VoiceMode::Transcribe, SmartRouteReason::TranscribeFallback)
    };

    SmartVoiceRouteAnalysis {
        non_whitespace_char_count,
        sentence_boundary_count,
        committed_streaming_segment_count,
        long_form_evidence,
        leading_intent,
        resolved_mode,
        reason,
    }
}

/// Returns whether a fallback transcript is complete enough to lock Smart to
/// live transcription. A short corrected segment can still be the beginning of
/// a command or transformation request (for example, "帮我" or "打").
pub fn smart_transcribe_fallback_is_stable(transcript: &str) -> bool {
    let leading_intent_span = leading_intent_span(transcript);
    let leading_span = trim_smart_intent_boundaries(&leading_intent_span);
    if leading_span.is_empty() {
        return false;
    }

    let normalized = leading_span.to_lowercase();
    if SMART_POLITE_PREFIXES
        .iter()
        .any(|prefix| prefix.starts_with(&normalized) && prefix != &normalized)
    {
        return false;
    }

    let command_text = trim_smart_intent_boundaries(strip_polite_prefix(&normalized));
    if command_text.is_empty() {
        return false;
    }

    !(SMART_CHINESE_INTENT_ACTIONS
        .iter()
        .any(|action| action.starts_with(command_text) && action != &command_text)
        || SMART_ENGLISH_INTENT_ACTIONS
            .iter()
            .any(|action| action.starts_with(command_text) && action != &command_text)
        || SMART_ENGLISH_INTENT_ACTIONS
            .iter()
            .any(|action| action == &command_text))
}

fn trim_smart_intent_boundaries(text: &str) -> &str {
    text.trim_matches(|character: char| character.is_whitespace() || is_clause_boundary(character))
}

pub fn count_non_whitespace(text: &str) -> usize {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

pub fn count_sentence_boundaries(text: &str) -> usize {
    let mut count = 0;
    let mut inside_boundary_run = false;
    for character in text.chars() {
        if is_sentence_boundary(character) {
            if !inside_boundary_run {
                count += 1;
            }
            inside_boundary_run = true;
        } else {
            inside_boundary_run = false;
        }
    }
    count
}

pub fn validate_faithful_output(input: &str, output: &str) -> FaithfulOutputValidation {
    let input_chars = normalize_faithful_text(input);
    let output_chars = normalize_faithful_text(output);
    let input_char_count = input_chars.len();
    let output_char_count = output_chars.len();
    let retention_ratio = if input_char_count == 0 {
        1.0
    } else {
        output_char_count as f64 / input_char_count as f64
    };
    let max_char_count = input_char_count.max(output_char_count);

    if input_char_count >= FAITHFUL_LONG_INPUT_MIN_CHARS
        && retention_ratio < FAITHFUL_MIN_RETENTION_RATIO
    {
        return FaithfulOutputValidation {
            accepted: false,
            fallback_reason: Some(FaithfulOutputFallbackReason::CatastrophicCompression),
            input_char_count,
            output_char_count,
            retention_ratio,
            normalized_change_ratio: normalized_length_difference(
                input_char_count,
                output_char_count,
            ),
        };
    }

    if max_char_count == 0 {
        return FaithfulOutputValidation {
            accepted: true,
            fallback_reason: None,
            input_char_count,
            output_char_count,
            retention_ratio,
            normalized_change_ratio: 0.0,
        };
    }

    let max_distance = ((max_char_count as f64) * FAITHFUL_MAX_CHANGE_RATIO).floor() as usize;
    let distance = bounded_levenshtein_distance(&input_chars, &output_chars, max_distance);
    let (accepted, fallback_reason, normalized_change_ratio) = match distance {
        Some(distance) => (true, None, distance as f64 / max_char_count as f64),
        None => (
            false,
            Some(FaithfulOutputFallbackReason::ExcessiveSequenceChange),
            (max_distance.saturating_add(1) as f64 / max_char_count as f64).min(1.0),
        ),
    };

    FaithfulOutputValidation {
        accepted,
        fallback_reason,
        input_char_count,
        output_char_count,
        retention_ratio,
        normalized_change_ratio,
    }
}

pub fn voice_mode_requires_faithful_output(mode: VoiceMode) -> bool {
    matches!(mode, VoiceMode::Transcribe | VoiceMode::Dictate)
}

fn normalize_faithful_text(text: &str) -> Vec<char> {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalized_length_difference(left: usize, right: usize) -> f64 {
    let maximum = left.max(right);
    if maximum == 0 {
        0.0
    } else {
        left.abs_diff(right) as f64 / maximum as f64
    }
}

fn bounded_levenshtein_distance(
    left: &[char],
    right: &[char],
    max_distance: usize,
) -> Option<usize> {
    let mut prefix_len = 0;
    while prefix_len < left.len()
        && prefix_len < right.len()
        && left[prefix_len] == right[prefix_len]
    {
        prefix_len += 1;
    }

    let mut left_end = left.len();
    let mut right_end = right.len();
    while left_end > prefix_len
        && right_end > prefix_len
        && left[left_end - 1] == right[right_end - 1]
    {
        left_end -= 1;
        right_end -= 1;
    }

    let left = &left[prefix_len..left_end];
    let right = &right[prefix_len..right_end];
    if left.len().abs_diff(right.len()) > max_distance {
        return None;
    }
    if left.is_empty() {
        return (right.len() <= max_distance).then_some(right.len());
    }
    if right.is_empty() {
        return (left.len() <= max_distance).then_some(left.len());
    }

    let sentinel = max_distance.saturating_add(1);
    let mut previous = vec![sentinel; right.len() + 1];
    let mut current = vec![sentinel; right.len() + 1];
    let initial_end = right.len().min(max_distance);
    for (column, value) in previous.iter_mut().enumerate().take(initial_end + 1) {
        *value = column;
    }

    for row in 1..=left.len() {
        current.fill(sentinel);
        if row <= max_distance {
            current[0] = row;
        }
        let column_start = row.saturating_sub(max_distance).max(1);
        let column_end = right.len().min(row.saturating_add(max_distance));
        if column_start > column_end {
            return None;
        }

        let mut row_minimum = sentinel;
        for column in column_start..=column_end {
            let substitution_cost = usize::from(left[row - 1] != right[column - 1]);
            let deletion = previous[column].saturating_add(1);
            let insertion = current[column - 1].saturating_add(1);
            let substitution = previous[column - 1].saturating_add(substitution_cost);
            let distance = deletion.min(insertion).min(substitution);
            current[column] = distance;
            row_minimum = row_minimum.min(distance);
        }
        if row_minimum > max_distance {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }

    (previous[right.len()] <= max_distance).then_some(previous[right.len()])
}

fn leading_intent(transcript: &str) -> Option<SmartLeadingIntent> {
    let span = leading_intent_span(transcript);
    if span.trim().is_empty() {
        return None;
    }
    let normalized = span.trim().to_lowercase();
    let command_text = strip_polite_prefix(&normalized);

    if is_document_instruction(command_text) {
        return Some(SmartLeadingIntent::Document);
    }

    if is_translate_instruction(command_text) {
        return Some(SmartLeadingIntent::Translate);
    }

    if is_generate_instruction(command_text) {
        return Some(SmartLeadingIntent::Generate);
    }

    if is_direct_command_prefix(command_text) {
        return Some(SmartLeadingIntent::Command);
    }

    None
}

fn leading_intent_span(transcript: &str) -> String {
    let mut span = String::new();
    let mut non_whitespace_count = 0;
    for character in transcript.trim_start().chars() {
        if non_whitespace_count >= LEADING_INTENT_MAX_NON_WHITESPACE {
            break;
        }
        span.push(character);
        if !character.is_whitespace() {
            non_whitespace_count += 1;
        }
        if is_clause_boundary(character) {
            break;
        }
    }
    span
}

fn strip_polite_prefix(text: &str) -> &str {
    SMART_POLITE_PREFIXES
        .iter()
        .find_map(|prefix| text.strip_prefix(prefix))
        .unwrap_or(text)
        .trim_start()
}

fn is_document_instruction(text: &str) -> bool {
    ["总结", "概括", "归纳", "润色", "改写", "整理"]
        .iter()
        .any(|action| chinese_object_action_instruction(text, action, &["一下", "成", "为", "好"]))
        || chinese_action_instruction(
            text,
            "总结",
            &["以下", "下面", "这", "上述", "后面", "内容", "一下"],
        )
        || chinese_action_instruction(
            text,
            "概括",
            &["以下", "下面", "这", "上述", "后面", "内容", "一下"],
        )
        || chinese_action_instruction(
            text,
            "归纳",
            &["以下", "下面", "这", "上述", "后面", "内容", "一下"],
        )
        || chinese_action_instruction(
            text,
            "润色",
            &[
                "以下", "下面", "这", "上述", "后面", "内容", "文本", "一下", "成",
            ],
        )
        || chinese_action_instruction(
            text,
            "改写",
            &[
                "以下", "下面", "这", "上述", "后面", "内容", "文本", "一下", "成",
            ],
        )
        || chinese_action_instruction(
            text,
            "整理",
            &[
                "以下", "下面", "这", "上述", "后面", "内容", "文本", "一下", "成",
            ],
        )
        || english_action_instruction(
            text,
            "summarize",
            &[
                "the",
                "this",
                "these",
                "following",
                "my",
                "our",
                "it",
                "text",
                "transcript",
                "content",
                "document",
                "notes",
            ],
        )
        || english_action_instruction(
            text,
            "summarise",
            &[
                "the",
                "this",
                "these",
                "following",
                "my",
                "our",
                "it",
                "text",
                "transcript",
                "content",
                "document",
                "notes",
            ],
        )
        || english_action_instruction(
            text,
            "polish",
            &[
                "the",
                "this",
                "these",
                "following",
                "my",
                "our",
                "it",
                "text",
                "transcript",
                "content",
                "document",
                "paragraph",
                "sentence",
            ],
        )
        || english_action_instruction(
            text,
            "rewrite",
            &[
                "the",
                "this",
                "these",
                "following",
                "my",
                "our",
                "it",
                "text",
                "transcript",
                "content",
                "document",
                "paragraph",
                "sentence",
            ],
        )
}

fn is_translate_instruction(text: &str) -> bool {
    chinese_object_action_instruction(text, "翻译", &["成", "为", "一下"])
        || chinese_action_instruction(
            text,
            "翻译",
            &[
                "以下", "下面", "这", "上述", "后面", "内容", "文本", "成", "为",
            ],
        )
        || english_action_instruction(
            text,
            "translate",
            &[
                "the",
                "this",
                "these",
                "following",
                "my",
                "our",
                "it",
                "text",
                "transcript",
                "content",
                "document",
                "into",
                "to",
            ],
        )
}

fn is_generate_instruction(text: &str) -> bool {
    ["写一篇", "写一段", "写一个", "写一份", "写一封", "写封"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        || chinese_action_instruction(
            text,
            "生成",
            &[
                "一", "个", "这", "以下", "下面", "关于", "基于", "根据", "内容",
            ],
        )
        || chinese_action_instruction(
            text,
            "创作",
            &[
                "一", "个", "这", "以下", "下面", "关于", "基于", "根据", "内容",
            ],
        )
        || chinese_action_instruction(
            text,
            "起草",
            &[
                "一", "个", "这", "以下", "下面", "关于", "基于", "根据", "内容",
            ],
        )
        || english_action_instruction(
            text,
            "write",
            &[
                "a",
                "an",
                "the",
                "this",
                "my",
                "our",
                "me",
                "some",
                "about",
                "based",
                "from",
                "article",
                "paragraph",
                "story",
                "email",
                "letter",
            ],
        )
        || english_action_instruction(
            text,
            "draft",
            &[
                "a",
                "an",
                "the",
                "this",
                "my",
                "our",
                "some",
                "about",
                "based",
                "from",
                "article",
                "paragraph",
                "story",
                "email",
                "letter",
                "document",
            ],
        )
        || english_action_instruction(
            text,
            "compose",
            &[
                "a",
                "an",
                "the",
                "this",
                "my",
                "our",
                "some",
                "about",
                "based",
                "from",
                "article",
                "paragraph",
                "story",
                "email",
                "letter",
            ],
        )
        || english_action_instruction(
            text,
            "generate",
            &[
                "a",
                "an",
                "the",
                "this",
                "my",
                "our",
                "some",
                "about",
                "based",
                "from",
                "article",
                "paragraph",
                "story",
                "email",
                "letter",
                "document",
                "content",
            ],
        )
}

fn chinese_object_action_instruction(text: &str, action: &str, continuations: &[&str]) -> bool {
    let Some(remainder) = text.strip_prefix("把").or_else(|| text.strip_prefix("将")) else {
        return false;
    };
    remainder.match_indices(action).any(|(action_index, _)| {
        if remainder[..action_index].trim().is_empty() {
            return false;
        }
        let action_remainder = remainder[action_index + action.len()..].trim_start();
        action_remainder.is_empty()
            || action_remainder
                .chars()
                .next()
                .is_some_and(is_clause_boundary)
            || continuations
                .iter()
                .any(|continuation| action_remainder.starts_with(continuation))
    })
}

fn chinese_action_instruction(text: &str, action: &str, continuations: &[&str]) -> bool {
    let Some(remainder) = text.strip_prefix(action) else {
        return false;
    };
    let remainder = remainder.trim_start();
    remainder.is_empty()
        || remainder.chars().next().is_some_and(is_clause_boundary)
        || continuations
            .iter()
            .any(|continuation| remainder.starts_with(continuation))
}

fn english_action_instruction(text: &str, action: &str, next_words: &[&str]) -> bool {
    let Some(remainder) = strip_english_action(text, action) else {
        return false;
    };
    let remainder = remainder.trim_start();
    if remainder.is_empty() || remainder.chars().next().is_some_and(is_clause_boundary) {
        return true;
    }
    let next_word = remainder
        .split(|character: char| !character.is_ascii_alphabetic())
        .find(|word| !word.is_empty())
        .unwrap_or_default();
    next_words.contains(&next_word)
}

fn strip_english_action<'a>(text: &'a str, action: &str) -> Option<&'a str> {
    let remainder = text.strip_prefix(action)?;
    if remainder
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        None
    } else {
        Some(remainder)
    }
}

fn is_direct_command_prefix(text: &str) -> bool {
    if text
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '"' | '\'' | '“' | '‘' | '`'))
    {
        return false;
    }
    if looks_like_question(text) {
        return false;
    }
    for (action, descriptive_suffixes) in [
        (
            "打开",
            &["率", "方式", "状态", "原因", "问题", "权限", "时间", "速度"][..],
        ),
        (
            "关闭",
            &["率", "方式", "状态", "原因", "问题", "时间", "速度"][..],
        ),
        (
            "启动",
            &["率", "方式", "状态", "原因", "问题", "时间", "速度"][..],
        ),
        (
            "运行",
            &[
                "率", "方式", "状态", "原因", "问题", "时间", "速度", "成本", "模式",
            ][..],
        ),
        (
            "执行",
            &["率", "方式", "状态", "原因", "问题", "时间", "结果", "成本"][..],
        ),
        ("删除", &["率", "方式", "状态", "原因", "问题", "时间"][..]),
        (
            "复制",
            &["率", "方式", "状态", "原因", "问题", "时间", "速度"][..],
        ),
        (
            "移动",
            &["率", "方式", "状态", "原因", "问题", "时间", "速度"][..],
        ),
    ] {
        if let Some(remainder) = text.strip_prefix(action) {
            let remainder = remainder.trim_start();
            if ["的", "了", "过的"]
                .iter()
                .any(|marker| remainder.starts_with(marker))
            {
                return false;
            }
            return !descriptive_suffixes
                .iter()
                .any(|suffix| remainder.starts_with(suffix));
        }
    }

    for action in ["open", "launch", "run", "close", "delete", "copy", "move"] {
        let Some(remainder) = strip_english_action(text, action) else {
            continue;
        };
        let next_word = remainder
            .split(|character: char| !character.is_ascii_alphabetic())
            .find(|word| !word.is_empty())
            .unwrap_or_default();
        let descriptive = [
            "access",
            "source",
            "rate",
            "rates",
            "status",
            "time",
            "times",
            "mode",
            "modes",
            "question",
            "questions",
            "issue",
            "issues",
            "permission",
            "permissions",
        ];
        return !next_word.is_empty()
            && !descriptive.contains(&next_word)
            && !has_descriptive_english_copula(text);
    }
    false
}

fn looks_like_question(text: &str) -> bool {
    text.ends_with(['?', '？'])
        || ["为什么", "为何", "怎么", "如何", "是否", "能否"]
            .iter()
            .any(|marker| text.contains(marker))
}

fn has_descriptive_english_copula(text: &str) -> bool {
    let words = text
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let Some(copula_index) = words
        .iter()
        .position(|word| matches!(*word, "is" | "are" | "was" | "were" | "means" | "remains"))
    else {
        return false;
    };
    !words[..copula_index]
        .iter()
        .any(|word| matches!(*word, "that" | "which" | "who" | "whose"))
}

fn is_clause_boundary(character: char) -> bool {
    matches!(
        character,
        '.' | ',' | '，' | ':' | '：' | ';' | '；' | '。' | '!' | '！' | '?' | '？' | '\n'
    )
}

fn is_sentence_boundary(character: char) -> bool {
    matches!(
        character,
        '.' | '。' | '!' | '！' | '?' | '？' | ';' | '；' | '\n'
    )
}
