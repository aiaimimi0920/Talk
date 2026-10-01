#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegmenterConfig {
    pub punctuation_pause_ms: u64,
    pub soft_pause_ms: u64,
    pub min_clause_chars: usize,
    pub min_final_chars: usize,
    pub max_chunk_chars: usize,
    pub correction_context_chars: usize,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            punctuation_pause_ms: 280,
            soft_pause_ms: 520,
            min_clause_chars: 2,
            min_final_chars: 6,
            max_chunk_chars: 30,
            correction_context_chars: 80,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmenterInput<'a> {
    pub text: &'a str,
    pub trailing_silence_ms: u64,
    pub asr_marked_final: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentReadiness {
    Wait,
    Ready,
}

pub fn evaluate_segment_readiness(
    config: &SegmenterConfig,
    input: &SegmenterInput<'_>,
) -> SegmentReadiness {
    let text_analysis = analyze_segment_text(input.text);
    if text_analysis.char_count == 0 {
        return SegmentReadiness::Wait;
    }
    if text_analysis.char_count >= config.max_chunk_chars {
        return SegmentReadiness::Ready;
    }
    if input.asr_marked_final && text_analysis.char_count >= config.min_final_chars {
        return SegmentReadiness::Ready;
    }
    if text_analysis
        .last_non_whitespace
        .is_some_and(is_clause_punctuation)
        && text_analysis.content_char_count >= config.min_clause_chars
    {
        return SegmentReadiness::Ready;
    }
    if text_analysis
        .last_non_whitespace
        .is_some_and(is_sentence_punctuation)
        && input.trailing_silence_ms >= config.punctuation_pause_ms
    {
        return SegmentReadiness::Ready;
    }
    if text_analysis.char_count >= config.min_final_chars
        && input.trailing_silence_ms >= config.soft_pause_ms
    {
        return SegmentReadiness::Ready;
    }
    SegmentReadiness::Wait
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SegmentTextAnalysis {
    char_count: usize,
    content_char_count: usize,
    last_non_whitespace: Option<char>,
}

fn analyze_segment_text(text: &str) -> SegmentTextAnalysis {
    let mut analysis = SegmentTextAnalysis {
        char_count: 0,
        content_char_count: 0,
        last_non_whitespace: None,
    };
    for character in text.chars().filter(|character| !character.is_whitespace()) {
        analysis.char_count += 1;
        if !is_segment_punctuation(character) {
            analysis.content_char_count += 1;
        }
        analysis.last_non_whitespace = Some(character);
    }
    analysis
}

/// Clause-boundary punctuation (comma/semicolon/colon, CJK and ASCII).
pub(crate) fn is_clause_punctuation(character: char) -> bool {
    matches!(character, '，' | ',' | '；' | ';' | '：' | ':')
}

/// Sentence-boundary punctuation (period/exclamation/question, CJK and ASCII).
pub(crate) fn is_sentence_punctuation(character: char) -> bool {
    matches!(character, '。' | '！' | '？' | '.' | '!' | '?')
}

/// Any segmentation punctuation (clause or sentence). Single source of truth so
/// the segmenter and the runtime splitter cannot drift apart.
pub(crate) fn is_segment_punctuation(character: char) -> bool {
    is_clause_punctuation(character) || is_sentence_punctuation(character)
}

#[cfg(test)]
mod tests {
    use super::{analyze_segment_text, SegmentTextAnalysis};

    #[test]
    fn segment_text_analysis_counts_content_and_tail_in_one_pass() {
        assert_eq!(
            analyze_segment_text(" 你，好！ \t\n"),
            SegmentTextAnalysis {
                char_count: 4,
                content_char_count: 2,
                last_non_whitespace: Some('！'),
            }
        );
        assert_eq!(
            analyze_segment_text(" \t\r\n"),
            SegmentTextAnalysis {
                char_count: 0,
                content_char_count: 0,
                last_non_whitespace: None,
            }
        );
        assert_eq!(
            analyze_segment_text("\u{00a0}你，\u{2003}好？\u{202f}"),
            SegmentTextAnalysis {
                char_count: 4,
                content_char_count: 2,
                last_non_whitespace: Some('？'),
            }
        );
    }
}
