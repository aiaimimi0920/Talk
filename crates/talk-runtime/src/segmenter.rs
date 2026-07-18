#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmenterConfig {
    pub punctuation_pause_ms: u64,
    pub soft_pause_ms: u64,
    pub min_final_chars: usize,
    pub max_chunk_chars: usize,
    pub correction_context_chars: usize,
}

impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            punctuation_pause_ms: 280,
            soft_pause_ms: 520,
            min_final_chars: 6,
            max_chunk_chars: 30,
            correction_context_chars: 80,
        }
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

pub fn evaluate_segment_readiness(
    config: &SegmenterConfig,
    input: &SegmenterInput,
) -> SegmentReadiness {
    let char_count = input
        .text
        .chars()
        .filter(|item| !item.is_whitespace())
        .count();
    if char_count == 0 {
        return SegmentReadiness::Wait;
    }
    if char_count >= config.max_chunk_chars {
        return SegmentReadiness::Ready;
    }
    if input.asr_marked_final && char_count >= config.min_final_chars {
        return SegmentReadiness::Ready;
    }
    if ends_with_sentence_punctuation(&input.text)
        && input.trailing_silence_ms >= config.punctuation_pause_ms
    {
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
