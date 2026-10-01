use talk_client::{FrontContext, StreamingAsrEvent};
use talk_core::{TalkConfig, VoiceEvent, VoiceSession};
use talk_runtime::{
    run_local_streaming_asr_service_from_recording, run_mock_speculative_session,
    run_voice_session_from_external_asr_command_with_insert_hooks,
    run_voice_session_from_transcript_with_insert_hooks, LocalStreamingAsrLiveSession,
    RuntimeInsertDirective, SegmenterConfig, SpeculativeRuntimeEvent, SpeculativeRuntimeState,
};

#[test]
fn speculative_runtime_emits_draft_update_for_partial_asr_event() {
    let mut state = SpeculativeRuntimeState::default();
    let event = state
        .accept_asr_event(StreamingAsrEvent::partial("seg-1", "你好"))
        .unwrap();
    assert_eq!(
        event,
        SpeculativeRuntimeEvent::DraftUpdated {
            segment_id: "seg-1".to_string(),
            text: "你好".to_string(),
        }
    );
}

#[test]
fn speculative_runtime_emits_local_commit_for_final_asr_event() {
    let mut state = SpeculativeRuntimeState::default();
    let event = state
        .accept_asr_event(StreamingAsrEvent::final_segment("seg-1", "你好呀。"))
        .unwrap();
    assert_eq!(
        event,
        SpeculativeRuntimeEvent::LocalSegmentCommitted {
            segment_id: "seg-1".to_string(),
            text: "你好呀。".to_string(),
        }
    );
}

#[test]
fn mock_speculative_session_emits_draft_and_commit_events() {
    let events =
        run_mock_speculative_session(vec![(false, "seg-1", "你好"), (true, "seg-1", "你好呀")])
            .unwrap();

    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0],
        SpeculativeRuntimeEvent::DraftUpdated { .. }
    ));
    assert!(matches!(
        events[1],
        SpeculativeRuntimeEvent::LocalSegmentCommitted { .. }
    ));
}

#[test]
fn speculative_runtime_requests_text_correction_when_segment_is_stable() {
    let mut state = SpeculativeRuntimeState::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "我下午三点有空。"),
            0,
            &SegmenterConfig::default(),
        )
        .unwrap();

    assert_eq!(
        events,
        vec![
            SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: "seg-1".to_string(),
                text: "我下午三点有空。".to_string(),
            },
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: "seg-1".to_string(),
                local_text: "我下午三点有空。".to_string(),
                context_before: String::new(),
            },
        ]
    );
}

#[test]
fn speculative_runtime_treats_punctuated_idle_partial_as_correction_ready() {
    let mut state = SpeculativeRuntimeState::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "我下午三点有空。"),
            SegmenterConfig::default().punctuation_pause_ms,
            &SegmenterConfig::default(),
        )
        .unwrap();

    assert_eq!(
        events,
        vec![
            SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: "seg-1".to_string(),
                text: "我下午三点有空。".to_string(),
            },
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: "seg-1".to_string(),
                local_text: "我下午三点有空。".to_string(),
                context_before: String::new(),
            },
        ]
    );
}

#[test]
fn speculative_runtime_keeps_soft_pause_commits_unpunctuated_for_longer_natural_sentences() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "第一句话没有标点"),
            config.soft_pause_ms,
            &config,
        )
        .unwrap();

    assert_eq!(
        events,
        vec![
            SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: "seg-1".to_string(),
                text: "第一句话没有标点".to_string(),
            },
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: "seg-1".to_string(),
                local_text: "第一句话没有标点".to_string(),
                context_before: String::new(),
            },
        ]
    );
}

#[test]
fn speculative_runtime_does_not_split_decimal_numbers_at_the_period() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    // The period in "3.5" is a decimal point, not a sentence boundary, so even a
    // final event with a pause must keep the number intact rather than commit
    // "the ratio is 3." and split off "5".
    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "the ratio is 3.5"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    let committed = events.iter().find_map(|event| match event {
        SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(committed, Some("the ratio is 3.5"), "events = {events:?}");
}

#[test]
fn speculative_runtime_does_not_split_numbers_at_comma_or_colon_separators() {
    let config = SegmenterConfig::default();

    // Thousands separator: the comma in "1,000" is not a clause boundary.
    let mut thousands = SpeculativeRuntimeState::default();
    let thousands_events = thousands
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "the total is 1,000 dollars"),
            0,
            &config,
        )
        .unwrap();
    let thousands_committed = thousands_events.iter().find_map(|event| match event {
        SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(
        thousands_committed,
        Some("the total is 1,000 dollars"),
        "events = {thousands_events:?}"
    );

    // Time separator: the colon in "3:30" is not a clause boundary.
    let mut time = SpeculativeRuntimeState::default();
    let time_events = time
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "meet at 3:30 today"),
            0,
            &config,
        )
        .unwrap();
    let time_committed = time_events.iter().find_map(|event| match event {
        SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(
        time_committed,
        Some("meet at 3:30 today"),
        "events = {time_events:?}"
    );
}

#[test]
fn speculative_runtime_length_cap_break_backs_up_to_latin_word_boundary() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    // 33 non-whitespace chars, no punctuation: the 30-char length cap fires
    // inside "close". The break must back up to the last whole word rather than
    // committing a partial token like "...and cl".
    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "please open the settings menu and close"),
            0,
            &config,
        )
        .unwrap();

    let committed = events.iter().find_map(|event| match event {
        SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
        _ => None,
    });
    assert_eq!(
        committed,
        Some("please open the settings menu and"),
        "events = {events:?}"
    );
}

#[test]
fn speculative_runtime_length_cap_keeps_backed_up_word_prefix_in_the_next_count() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig {
        max_chunk_chars: 10,
        correction_context_chars: 0,
        ..SegmenterConfig::default()
    };
    let source = "alpha beta gamma delta epsilon zeta eta theta";

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", source),
            0,
            &config,
        )
        .unwrap();
    let committed = events
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        committed,
        vec![
            "alpha beta",
            " gamma delta",
            " epsilon",
            " zeta eta",
            " theta"
        ]
    );
    assert_eq!(committed.concat(), source);
}

#[test]
fn speculative_runtime_keeps_latin_ending_soft_pause_clause_without_synthetic_ascii_comma() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "打开显卡 gpu"),
            config.soft_pause_ms,
            &config,
        )
        .unwrap();

    assert!(
        matches!(
            events.as_slice(),
            [
                SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. },
                SpeculativeRuntimeEvent::CorrectionRequested { local_text, .. },
            ] if text == "打开显卡 gpu" && local_text == "打开显卡 gpu"
        ),
        "events = {events:?}"
    );
}

#[test]
fn speculative_runtime_keeps_cjk_ending_soft_pause_clause_without_synthetic_fullwidth_comma() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "gpu 已经打开了"),
            config.soft_pause_ms,
            &config,
        )
        .unwrap();

    assert!(
        matches!(
            events.as_slice(),
            [
                SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. },
                SpeculativeRuntimeEvent::CorrectionRequested { .. },
            ] if text == "gpu 已经打开了"
        ),
        "events = {events:?}"
    );
}

#[test]
fn speculative_runtime_does_not_request_duplicate_correction_for_same_segment() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "我下午三点有空。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let second_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "我下午三点有空。"),
            0,
            &config,
        )
        .unwrap();

    assert_eq!(correction_event_count(&first_events), 1);
    assert_eq!(correction_event_count(&second_events), 0);
}

#[test]
fn speculative_runtime_does_not_recommit_same_stable_segment_after_partial_commit() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "我下午三点有空。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let second_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "我下午三点有空。"),
            0,
            &config,
        )
        .unwrap();

    assert!(matches!(
        first_events.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { .. },
            SpeculativeRuntimeEvent::CorrectionRequested { .. }
        ]
    ));
    assert_eq!(second_events, Vec::<SpeculativeRuntimeEvent>::new());
}

#[test]
fn speculative_runtime_splits_cumulative_same_asr_segment_into_tail_segments() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "第一句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let draft_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "第一句。第二"),
            0,
            &config,
        )
        .unwrap();
    let second_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "第一句。第二句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    assert!(matches!(
        first_events.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { segment_id: correction_id, local_text, .. },
        ] if segment_id == "seg-1"
            && text == "第一句。"
            && correction_id == "seg-1"
            && local_text == "第一句。"
    ));
    assert_eq!(
        draft_events,
        vec![SpeculativeRuntimeEvent::DraftUpdated {
            segment_id: "seg-1#2".to_string(),
            text: "第二".to_string(),
        }]
    );
    assert_eq!(
        second_events,
        vec![
            SpeculativeRuntimeEvent::LocalSegmentCommitted {
                segment_id: "seg-1#2".to_string(),
                text: "第二句。".to_string(),
            },
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: "seg-1#2".to_string(),
                local_text: "第二句。".to_string(),
                context_before: "第一句。".to_string(),
            },
        ]
    );
}

#[test]
fn speculative_runtime_recommits_revised_words_instead_of_dropping_them() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "我要去。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "我想去北京。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    // The revised hypothesis must be re-committed under the same id, not dropped
    // and not concatenated onto the stale text.
    assert!(
        matches!(
            revised.as_slice(),
            [
                SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
                SpeculativeRuntimeEvent::CorrectionRequested { .. },
            ] if segment_id == "seg-1" && text == "我想去北京。"
        ),
        "revised events = {revised:?}"
    );
}

#[test]
fn speculative_runtime_revision_preserves_shared_prefix_segment() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    // Commit two clauses: seg-1 = 第一句。 and seg-1#2 = 第二句。
    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "第一句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第二句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    // Revise only the second clause. The first clause must be untouched.
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第三句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    assert!(
        matches!(
            revised.as_slice(),
            [
                SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
                SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. },
            ] if segment_id == "seg-1#2" && text == "第三句。" && context_before == "第一句。"
        ),
        "revised events = {revised:?}"
    );
}

#[test]
fn speculative_runtime_revision_respects_utf8_char_boundaries() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "北京市。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    // Divergence after the first multi-byte codepoint (北) must not panic.
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "北海市。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    assert!(
        matches!(
            revised.as_slice(),
            [
                SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
                SpeculativeRuntimeEvent::CorrectionRequested { .. },
            ] if segment_id == "seg-1" && text == "北海市。"
        ),
        "revised events = {revised:?}"
    );
}

#[test]
fn speculative_runtime_reports_invalidated_segments_when_revision_shrinks() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    // Commit two clauses: seg-1 = 第一句。 and seg-1#2 = 第二句。
    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第二句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    // Revise into a single clause that diverges early. seg-1 is re-committed;
    // seg-1#2 has no replacement and must be reported as invalidated so the HUD
    // can drop it.
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第三句。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    assert!(
        revised.iter().any(|event| matches!(
            event,
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text }
                if segment_id == "seg-1" && text == "第三句。"
        )),
        "revised events = {revised:?}"
    );
    assert!(
        revised.iter().any(|event| matches!(
            event,
            SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids }
                if segment_ids == &vec!["seg-1#2".to_string()]
        )),
        "revised events = {revised:?}"
    );
}

#[test]
fn speculative_runtime_invalidates_many_revised_segments_without_dropping_other_sources() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment(
                "seg-1",
                "第一句。第二句。第三句。第四句。第五句。第六句。",
            ),
            0,
            &config,
        )
        .unwrap();
    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-other", "保留内容。"),
            0,
            &config,
        )
        .unwrap();

    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "全新句子。"),
            0,
            &config,
        )
        .unwrap();

    assert!(revised.iter().any(|event| matches!(
        event,
        SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. }
            if context_before == "保留内容。"
    )));
    assert!(revised.iter().any(|event| matches!(
        event,
        SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids }
            if segment_ids == &vec![
                "seg-1#2".to_string(),
                "seg-1#3".to_string(),
                "seg-1#4".to_string(),
                "seg-1#5".to_string(),
                "seg-1#6".to_string(),
            ]
    )));
}

#[test]
fn speculative_runtime_reuses_truncated_boundaries_after_revision() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第二句。第三句。"),
            0,
            &config,
        )
        .unwrap();
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "新第一句。"),
            0,
            &config,
        )
        .unwrap();
    let second = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "新第一句。新第二句。"),
            0,
            &config,
        )
        .unwrap();
    let third = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "新第一句。新第二句。新第三句。"),
            0,
            &config,
        )
        .unwrap();

    assert!(matches!(
        revised.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { segment_id: correction_id, .. },
            SpeculativeRuntimeEvent::LocalSegmentsInvalidated { segment_ids },
        ] if segment_id == "seg-1"
            && text == "新第一句。"
            && correction_id == "seg-1"
            && segment_ids == &vec!["seg-1#2".to_string(), "seg-1#3".to_string()]
    ));
    assert!(matches!(
        second.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { segment_id: correction_id, .. },
        ] if segment_id == "seg-1#2"
            && text == "新第二句。"
            && correction_id == "seg-1#2"
    ));
    assert!(matches!(
        third.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { segment_id: correction_id, .. },
        ] if segment_id == "seg-1#3"
            && text == "新第三句。"
            && correction_id == "seg-1#3"
    ));
}

#[test]
fn speculative_runtime_single_revision_preserves_interleaved_context_order() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第二句。"),
            0,
            &config,
        )
        .unwrap();
    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-other", "后续来源。"),
            0,
            &config,
        )
        .unwrap();

    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句。第三句。"),
            0,
            &config,
        )
        .unwrap();

    assert!(matches!(
        revised.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id: correction_id,
                context_before,
                ..
            },
        ] if segment_id == "seg-1#2"
            && text == "第三句。"
            && correction_id == "seg-1#2"
            && context_before == "第一句。\n后续来源。"
    ));
}

#[test]
fn speculative_runtime_ignores_stale_shorter_prefix_hypothesis() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "你好世界。"),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    // A shorter hypothesis that is a prefix of the committed text is stale and
    // must not disturb the committed segment.
    let stale = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "你好世"),
            0,
            &config,
        )
        .unwrap();

    assert!(
        stale.is_empty(),
        "stale shorter hypothesis events = {stale:?}"
    );
}

#[test]
fn speculative_runtime_normalizes_padded_hypotheses_across_fast_paths() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "  第一句。  "),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let identical = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", " 第一句。 "),
            0,
            &config,
        )
        .unwrap();
    let stale = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "  第一句  "),
            0,
            &config,
        )
        .unwrap();
    let appended = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "  第一句。第二句。  "),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();
    let revised = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "  第一句。第三句。  "),
            config.punctuation_pause_ms,
            &config,
        )
        .unwrap();

    assert!(matches!(
        first.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { .. },
        ] if segment_id == "seg-1" && text == "第一句。"
    ));
    assert!(identical.is_empty());
    assert!(stale.is_empty());
    assert!(matches!(
        appended.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { .. },
        ] if segment_id == "seg-1#2" && text == "第二句。"
    ));
    assert!(matches!(
        revised.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { segment_id, text },
            SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. },
        ] if segment_id == "seg-1#2"
            && text == "第三句。"
            && context_before == "第一句。"
    ));
}

#[test]
fn speculative_segmentation_hot_path_borrows_event_strings_until_owned_state_boundaries() {
    let source = include_str!("../src/speculative.rs");
    let accept_start = source
        .find("    pub fn accept_asr_event_with_segmentation")
        .expect("segmented ASR event handler");
    let reconcile_start = source[accept_start..]
        .find("    fn reconcile_source_for_segmentation")
        .map(|offset| accept_start + offset)
        .expect("source reconciliation helper");
    let accept_source = &source[accept_start..reconcile_start];

    assert!(accept_source.contains("let source_segment_id = event.segment_id();"));
    assert!(accept_source.contains("let source_text = event.text().trim();"));
    assert!(accept_source.contains("let text = &source_text[tail_start_byte..];"));
    assert!(!accept_source.contains("event.segment_id().to_string()"));
    assert!(!accept_source.contains("event.text().trim().to_string()"));
    assert!(!accept_source.contains("source_text[tail_start_byte..].to_string()"));
    assert!(accept_source.contains("let mut events = Vec::with_capacity(event_capacity);"));
    assert!(accept_source.contains(".saturating_mul(2)"));
    assert!(accept_source.contains("usize::from(!invalidated_ids.is_empty())"));
    assert!(accept_source.contains("(!invalidated_ids.is_empty()).then(HashSet::new)"));
    assert!(accept_source.contains("if let Some(reemitted_ids) = reemitted_ids.as_mut()"));
    assert!(!accept_source.contains("let mut reemitted_ids: HashSet<String> = HashSet::new()"));

    let invalidate_start = source[reconcile_start..]
        .find("    fn invalidate_revised_source_segments")
        .map(|offset| reconcile_start + offset)
        .expect("revision invalidation helper");
    let reconcile_source = &source[reconcile_start..invalidate_start];
    let append_check = reconcile_source
        .find("source_text.strip_prefix(committed_text)")
        .expect("borrowed append fast path");
    let revision_ownership = reconcile_source
        .find("committed_text.to_owned()")
        .expect("revision-only ownership boundary");

    assert!(append_check < revision_ownership);
    assert!(!reconcile_source.contains("committed_text.trim().to_string()"));

    let next_id_start = source[invalidate_start..]
        .find("    fn next_runtime_segment_id")
        .map(|offset| invalidate_start + offset)
        .expect("runtime segment id helper");
    let invalidate_source = &source[invalidate_start..next_id_start];
    assert!(!invalidate_source.contains(".cloned()"));
    assert!(invalidate_source.contains("boundary_count"));
    assert!(invalidate_source.contains(".truncate(retained_count)"));
    assert!(invalidate_source.contains("if let [invalidated_id] = invalidated_ids.as_slice()"));
    assert!(invalidate_source.contains(".rposition("));
    assert!(invalidate_source.contains("self.committed_segment_ids.remove(index)"));
    assert!(invalidate_source.contains("collect::<HashSet<_>>()"));
}

#[test]
fn speculative_runtime_requests_three_ordered_clause_corrections_for_example_sentence() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "你好，"),
            0,
            &config,
        )
        .unwrap();
    let second = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "你好，今天我们去北京玩，"),
            0,
            &config,
        )
        .unwrap();
    let third = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "你好，今天我们去北京玩，明天我们去上海玩。"),
            0,
            &config,
        )
        .unwrap();

    let corrections = first
        .into_iter()
        .chain(second)
        .chain(third)
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id,
                local_text,
                ..
            } => Some((segment_id, local_text)),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        corrections,
        vec![
            ("seg-1".to_string(), "你好，".to_string()),
            ("seg-1#2".to_string(), "今天我们去北京玩，".to_string()),
            ("seg-1#3".to_string(), "明天我们去上海玩。".to_string()),
        ]
    );
}

#[test]
fn speculative_runtime_does_not_recommit_earlier_internal_clause_boundaries_from_partial_growth() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let first = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "你好，"),
            0,
            &config,
        )
        .unwrap();
    let second = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "你好，今天我们去北京玩，"),
            0,
            &config,
        )
        .unwrap();

    let first_corrections = first
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested { local_text, .. } => {
                Some(local_text.as_str())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let second_corrections = second
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id,
                local_text,
                ..
            } => Some((segment_id.as_str(), local_text.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(first_corrections, vec!["你好，"]);
    assert_eq!(second_corrections, vec![("seg-1#2", "今天我们去北京玩，")]);
}

#[test]
fn speculative_runtime_splits_single_final_event_into_three_clause_corrections() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "你好，今天我们去北京玩，明天我们去上海玩。"),
            0,
            &config,
        )
        .unwrap();

    let corrections = events
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id,
                local_text,
                ..
            } => Some((segment_id.as_str(), local_text.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        corrections,
        vec![
            ("seg-1", "你好，"),
            ("seg-1#2", "今天我们去北京玩，"),
            ("seg-1#3", "明天我们去上海玩。"),
        ]
    );
}

#[test]
fn speculative_runtime_preserves_ascii_separator_when_aggregating_clause_events() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();
    let first_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::partial("seg-1", "Hello,"),
            0,
            &config,
        )
        .unwrap();
    let second_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "Hello, world."),
            0,
            &config,
        )
        .unwrap();
    let events = first_events
        .into_iter()
        .chain(second_events)
        .collect::<Vec<_>>();

    let committed_text = events
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::LocalSegmentCommitted { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();
    let correction_text = events
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested { local_text, .. } => {
                Some(local_text.as_str())
            }
            _ => None,
        })
        .collect::<String>();

    assert_eq!(committed_text, "Hello, world.");
    assert_eq!(correction_text, "Hello, world.");
}

#[test]
fn speculative_runtime_requests_correction_for_short_final_tail_clause() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig::default();

    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句，好的"),
            0,
            &config,
        )
        .unwrap();

    let corrections = events
        .iter()
        .filter_map(|event| match event {
            SpeculativeRuntimeEvent::CorrectionRequested {
                segment_id,
                local_text,
                ..
            } => Some((segment_id.as_str(), local_text.as_str())),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        corrections,
        vec![("seg-1", "第一句，"), ("seg-1#2", "好的")]
    );
}

#[test]
fn speculative_runtime_includes_bounded_previous_local_context_for_correction() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig {
        correction_context_chars: 3,
        ..SegmenterConfig::default()
    };

    state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-1", "第一句话很长。"),
            0,
            &config,
        )
        .unwrap();
    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-2", "第二句话完整。"),
            0,
            &config,
        )
        .unwrap();

    assert_eq!(
        events[1],
        SpeculativeRuntimeEvent::CorrectionRequested {
            segment_id: "seg-2".to_string(),
            local_text: "第二句话完整。".to_string(),
            context_before: "很长。".to_string(),
        }
    );
}

#[test]
fn speculative_runtime_bounded_context_preserves_tail_newline_boundaries() {
    let mut state = SpeculativeRuntimeState::default();
    let config = SegmenterConfig {
        correction_context_chars: 4,
        ..SegmenterConfig::default()
    };

    for (segment_id, text) in [("seg-1", "甲。"), ("seg-2", "乙。")] {
        state
            .accept_asr_event_with_segmentation(
                StreamingAsrEvent::final_segment(segment_id, text),
                0,
                &config,
            )
            .unwrap();
    }
    let events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-3", "丙。"),
            0,
            &config,
        )
        .unwrap();

    assert!(matches!(
        events.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { .. },
            SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. },
        ] if context_before == "。\n乙。"
    ));

    let one_char_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-4", "丁。"),
            0,
            &SegmenterConfig {
                correction_context_chars: 1,
                ..SegmenterConfig::default()
            },
        )
        .unwrap();
    assert!(matches!(
        one_char_events.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { .. },
            SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. },
        ] if context_before == "。"
    ));

    let no_context_events = state
        .accept_asr_event_with_segmentation(
            StreamingAsrEvent::final_segment("seg-5", "戊。"),
            0,
            &SegmenterConfig {
                correction_context_chars: 0,
                ..SegmenterConfig::default()
            },
        )
        .unwrap();
    assert!(matches!(
        no_context_events.as_slice(),
        [
            SpeculativeRuntimeEvent::LocalSegmentCommitted { .. },
            SpeculativeRuntimeEvent::CorrectionRequested { context_before, .. },
        ] if context_before.is_empty()
    ));
}

fn correction_event_count(events: &[SpeculativeRuntimeEvent]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, SpeculativeRuntimeEvent::CorrectionRequested { .. }))
        .count()
}

#[tokio::test]
async fn local_transcript_session_processes_and_inserts_without_provider_transcription() {
    let root = std::env::temp_dir().join(format!(
        "talk-local-transcript-runtime-{}",
        std::process::id()
    ));
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    let config = TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "mock"
mock_transcript = "this provider transcript must not be used"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"
"#
    ))
    .unwrap();
    let mut session = VoiceSession::new("local-transcript-session");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_transcript_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        "你好。".to_string(),
        None,
        FrontContext::default(),
        |_| RuntimeInsertDirective::UseConfiguredOutput,
        || {},
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(report.session.transcript(), Some("你好。"));
    assert_eq!(report.session.output_text(), Some("你好。"));
    assert!(report.log_path.exists());
}

#[cfg(windows)]
#[tokio::test]
async fn external_asr_command_session_processes_final_event_without_provider_transcription() {
    let root =
        std::env::temp_dir().join(format!("talk-external-asr-runtime-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    let audio_path = root.join("audio.wav");
    std::fs::write(&audio_path, b"fake wav").unwrap();
    let script_path = root.join("emit-asr.ps1");
    std::fs::write(
        &script_path,
        concat!(
            "\u{feff}",
            r#"
Write-Output '{"type":"partial","segment_id":"seg-1","text":"你好"}'
Write-Output '{"type":"final","segment_id":"seg-1","text":"你好。"}'
"#
        ),
    )
    .unwrap();
    let script_bytes = std::fs::read(&script_path).unwrap();
    assert!(
        script_bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "Windows PowerShell 5.1 requires a UTF-8 BOM for non-ASCII scripts"
    );
    let command = format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File {}",
        script_path.display()
    );
    let config = TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "http"
endpoint = "http://127.0.0.1:9/talk-runtime-test-should-not-be-called"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"
"#
    ))
    .unwrap();
    let mut session = VoiceSession::new("external-asr-session");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_external_asr_command_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        command,
        None,
        FrontContext::default(),
        |_| RuntimeInsertDirective::UseConfiguredOutput,
        || {},
        |_| {},
    )
    .await
    .unwrap();

    assert_eq!(report.session.transcript(), Some("你好。"));
    assert_eq!(report.session.output_text(), Some("你好。"));
    assert!(report.log_path.exists());
}

#[tokio::test]
async fn streaming_service_runtime_drains_recording_pcm_and_returns_asr_events() {
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::time::Duration;
    use talk_audio::{start_recording, AudioCaptureRequest, WavSettings};
    use talk_core::AudioBackendMode;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut received = Vec::<Value>::new();

        let start = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&start).unwrap());
        websocket
            .send(Message::Text(
                r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer-streaming-zh","sample_rate_hz":16000,"channels":1}"#
                    .into(),
            ))
            .await
            .unwrap();

        let audio = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&audio).unwrap());

        let stop = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&stop).unwrap());
        websocket
            .send(Message::Text(
                r#"{"type":"final","session_id":"streaming-runtime-session","segment_id":"seg-1","text":"你好。"}"#
                    .into(),
            ))
            .await
            .unwrap();

        received
    });

    let root = std::env::temp_dir().join(format!(
        "talk-runtime-streaming-service-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    let config = TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "mock"
mock_transcript = "provider should not be used"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"

[speculative.streaming_service]
endpoint = "{endpoint}"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 1000
final_timeout_ms = 1000
"#
    ))
    .unwrap();
    let recording = start_recording(&AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: root.join("audio"),
        session_id: "streaming-runtime-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 15,
        silent_samples: 320,
    })
    .unwrap();

    let events = run_local_streaming_asr_service_from_recording(
        &config,
        "streaming-runtime-session",
        &recording,
        Some("zh"),
    )
    .await
    .unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        events,
        vec![StreamingAsrEvent::final_segment("seg-1", "你好。")]
    );
    assert_eq!(received[0]["type"], "start");
    assert_eq!(received[1]["type"], "audio");
    assert_eq!(received[1]["pcm_base64"].as_str().unwrap().len(), 856);
    assert_eq!(received[2]["type"], "stop");
}

#[tokio::test]
async fn live_streaming_service_session_pumps_partial_events_before_stop() {
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::time::Duration;
    use talk_audio::{start_recording, AudioCaptureRequest, WavSettings};
    use talk_core::AudioBackendMode;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();
        let mut received = Vec::<Value>::new();

        let start = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&start).unwrap());
        websocket
            .send(Message::Text(
                r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer-streaming-zh","sample_rate_hz":16000,"channels":1}"#
                    .into(),
            ))
            .await
            .unwrap();

        let audio = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&audio).unwrap());
        websocket
            .send(Message::Text(
                r#"{"type":"partial","session_id":"live-streaming-runtime-session","segment_id":"seg-1","text":"你好"}"#
                    .into(),
            ))
            .await
            .unwrap();

        let stop = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        received.push(serde_json::from_str::<Value>(&stop).unwrap());
        websocket
            .send(Message::Text(
                r#"{"type":"final","session_id":"live-streaming-runtime-session","segment_id":"seg-1","text":"你好。"}"#
                    .into(),
            ))
            .await
            .unwrap();

        received
    });

    let root = std::env::temp_dir().join(format!(
        "talk-runtime-live-streaming-service-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    let config = TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "mock"
mock_transcript = "provider should not be used"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"

[speculative.streaming_service]
endpoint = "{endpoint}"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 1000
final_timeout_ms = 1000
"#
    ))
    .unwrap();
    let recording = start_recording(&AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: root.join("audio"),
        session_id: "live-streaming-runtime-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 15,
        silent_samples: 320,
    })
    .unwrap();

    let mut live_session =
        LocalStreamingAsrLiveSession::start(&config, "live-streaming-runtime-session", Some("zh"))
            .await
            .unwrap();
    let partial_events = live_session
        .pump_available_audio(&recording, Duration::from_millis(100))
        .await
        .unwrap();
    assert_eq!(
        partial_events,
        vec![StreamingAsrEvent::partial("seg-1", "你好")]
    );

    let final_events = live_session.stop(&recording).await.unwrap();
    let artifact = recording.finish().unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_events,
        vec![
            StreamingAsrEvent::partial("seg-1", "你好"),
            StreamingAsrEvent::final_segment("seg-1", "你好。")
        ]
    );
    assert_eq!(received[0]["type"], "start");
    assert_eq!(received[1]["type"], "audio");
    assert_eq!(received[2]["type"], "stop");
    assert!(
        artifact.path.exists(),
        "finalized recording should be persisted"
    );
}

#[tokio::test]
async fn live_streaming_service_session_catches_up_multiple_buffered_chunks_per_pump() {
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};
    use talk_audio::{start_recording, AudioCaptureRequest, WavSettings};
    use talk_core::AudioBackendMode;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let audio_messages_before_stop = Arc::new(AtomicUsize::new(0));
    let server_audio_messages_before_stop = Arc::clone(&audio_messages_before_stop);
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut websocket = accept_async(stream).await.unwrap();

        let start = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&start).unwrap()["type"],
            "start"
        );
        websocket
            .send(Message::Text(
                r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer-streaming-zh","sample_rate_hz":16000,"channels":1}"#
                    .into(),
            ))
            .await
            .unwrap();

        loop {
            let message = websocket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            let message = serde_json::from_str::<Value>(&message).unwrap();
            match message["type"].as_str().unwrap() {
                "audio" => {
                    server_audio_messages_before_stop.fetch_add(1, Ordering::SeqCst);
                }
                "stop" => {
                    websocket
                        .send(Message::Text(
                            r#"{"type":"final","session_id":"live-streaming-runtime-session-catch-up","segment_id":"seg-1","text":"你好。"}"#
                                .into(),
                        ))
                        .await
                        .unwrap();
                    break;
                }
                other => panic!("unexpected streaming message type: {other}"),
            }
        }
    });

    let root = std::env::temp_dir().join(format!(
        "talk-runtime-live-streaming-service-catch-up-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    let config = TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "mock"
mock_transcript = "provider should not be used"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"

[speculative.streaming_service]
endpoint = "{endpoint}"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 1000
final_timeout_ms = 1000
"#
    ))
    .unwrap();
    let recording = start_recording(&AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: root.join("audio"),
        session_id: "live-streaming-runtime-session-catch-up".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 15,
        // Five 80 ms streaming chunks worth of PCM are immediately available.
        silent_samples: 1_280 * 5,
    })
    .unwrap();

    let mut live_session = LocalStreamingAsrLiveSession::start(
        &config,
        "live-streaming-runtime-session-catch-up",
        Some("zh"),
    )
    .await
    .unwrap();
    let partial_events = live_session
        .pump_available_audio(&recording, Duration::from_millis(100))
        .await
        .unwrap();
    assert!(partial_events.is_empty());

    let wait_deadline = Instant::now() + Duration::from_millis(250);
    while audio_messages_before_stop.load(Ordering::SeqCst) < 4 && Instant::now() < wait_deadline {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert_eq!(
        audio_messages_before_stop.load(Ordering::SeqCst),
        4,
        "one live pump should catch up multiple buffered PCM chunks instead of leaking seconds of backlog"
    );

    let final_events = live_session.stop(&recording).await.unwrap();
    let artifact = recording.finish().unwrap();
    tokio::time::timeout(Duration::from_secs(1), server)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        final_events,
        vec![StreamingAsrEvent::final_segment("seg-1", "你好。")]
    );
    assert!(
        artifact.path.exists(),
        "finalized recording should be persisted"
    );
}
