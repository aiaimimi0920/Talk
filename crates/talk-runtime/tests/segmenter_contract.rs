use talk_runtime::{evaluate_segment_readiness, SegmentReadiness, SegmenterConfig, SegmenterInput};

#[test]
fn segmenter_commits_sentence_punctuation_after_short_pause() {
    let input = SegmenterInput {
        text: "我明天下午三点有空。".to_string(),
        trailing_silence_ms: 320,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Ready
    );
}

#[test]
fn segmenter_waits_for_short_text_without_pause_or_punctuation() {
    let input = SegmenterInput {
        text: "我明天".to_string(),
        trailing_silence_ms: 40,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Wait
    );
}

#[test]
fn segmenter_forces_long_chunks_even_without_punctuation() {
    let input = SegmenterInput {
        text: "这是一段已经超过最大本地等待长度但是用户还没有明确停顿的中文语音内容".to_string(),
        trailing_silence_ms: 0,
        asr_marked_final: false,
    };
    assert_eq!(
        evaluate_segment_readiness(&SegmenterConfig::default(), &input),
        SegmentReadiness::Ready
    );
}
