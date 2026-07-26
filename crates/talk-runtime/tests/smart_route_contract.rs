use talk_core::VoiceMode;
use talk_runtime::{
    analyze_smart_voice_mode, infer_smart_voice_mode, smart_transcribe_fallback_is_stable,
    SmartLeadingIntent, SmartRouteEvidence, SmartRouteReason,
};

fn non_whitespace_char_count(text: &str) -> usize {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .count()
}

fn command_like_text_with_non_whitespace_count(target_count: usize) -> String {
    let prefix = "打开记事本并输入以下内容：";
    let filler_count = target_count.saturating_sub(non_whitespace_char_count(prefix));
    let transcript = format!("{prefix}{}", "甲".repeat(filler_count));
    assert_eq!(non_whitespace_char_count(&transcript), target_count);
    transcript
}

fn assert_routes_to_transcription(transcript: &str) {
    assert_eq!(
        infer_smart_voice_mode(transcript),
        VoiceMode::Transcribe,
        "descriptive or status text must not become a transformation: {transcript}"
    );
}

#[test]
fn corrected_smart_fallback_waits_for_incomplete_leading_intent_prefixes() {
    for transcript in ["帮我", "帮我。", "请", "打", "打。", "please help"] {
        assert!(
            !smart_transcribe_fallback_is_stable(transcript),
            "an incomplete intent prefix must not be treated as stable transcription: {transcript}"
        );
    }

    for transcript in [
        "今天下午开会。",
        "我想喝咖啡。",
        "打游戏。",
        "open source is useful.",
    ] {
        assert!(
            smart_transcribe_fallback_is_stable(transcript),
            "ordinary completed speech should remain eligible for live transcription: {transcript}"
        );
    }
}

#[test]
fn short_direct_commands_remain_commands() {
    assert_eq!(infer_smart_voice_mode("打开记事本"), VoiceMode::Command);
    assert_eq!(infer_smart_voice_mode("Open Notepad"), VoiceMode::Command);
}

#[test]
fn natural_chinese_email_request_routes_to_generate() {
    assert_eq!(
        infer_smart_voice_mode("帮我写一封邮件。"),
        VoiceMode::Generate
    );
}

#[test]
fn english_relative_clause_can_describe_a_direct_command_object() {
    assert_eq!(
        infer_smart_voice_mode("Open the file that is on the desktop"),
        VoiceMode::Command
    );
    assert_eq!(
        infer_smart_voice_mode("Open the folder which is named Reports"),
        VoiceMode::Command
    );

    assert_routes_to_transcription("Open files are sometimes corrupted.");
    assert_routes_to_transcription("Close friends are important.");
}

#[test]
fn translate_feature_nouns_remain_transcription() {
    for transcript in [
        "把翻译功能关闭。",
        "把自动翻译功能关闭。",
        "把翻译服务暂时停用。",
    ] {
        assert_routes_to_transcription(transcript);
    }

    assert_eq!(
        infer_smart_voice_mode("把下面内容翻译成英文"),
        VoiceMode::Translate
    );
}

#[test]
fn document_feature_nouns_remain_transcription() {
    for transcript in [
        "把总结功能关闭。",
        "把自动润色功能打开。",
        "把改写建议记录下来。",
        "把整理工具卸载。",
    ] {
        assert_routes_to_transcription(transcript);
    }

    assert_eq!(
        infer_smart_voice_mode("把下面内容总结成三点"),
        VoiceMode::Document
    );
    assert_eq!(
        infer_smart_voice_mode("将这段文字润色一下"),
        VoiceMode::Document
    );
}

#[test]
fn chinese_relative_and_completed_action_clauses_remain_transcription() {
    for transcript in [
        "打开的文件已损坏。",
        "关闭的窗口仍在后台运行。",
        "删除过的文件无法恢复。",
        "移动了目录后程序报错。",
    ] {
        assert_routes_to_transcription(transcript);
    }

    assert_eq!(infer_smart_voice_mode("打开记事本"), VoiceMode::Command);
}

#[test]
fn quoted_descriptive_question_and_conditional_forms_remain_transcription() {
    let cases = [
        "他说打开记事本，然后继续演示。",
        "他说：“打开记事本。”然后继续演示。",
        "为什么打不开这个频道？",
        "如果你打开应用，就会出现这个对话框。",
        "He said to open Notepad and then continued the demonstration.",
    ];

    for transcript in cases {
        assert_eq!(
            infer_smart_voice_mode(transcript),
            VoiceMode::Transcribe,
            "descriptive transcript must not become a command: {transcript}"
        );
    }
}

#[test]
fn generative_ai_description_remains_transcription() {
    assert_routes_to_transcription("生成式人工智能正在快速发展。");
}

#[test]
fn open_rate_question_remains_transcription() {
    assert_routes_to_transcription("打开率为什么下降？");
}

#[test]
fn write_access_status_remains_transcription() {
    assert_routes_to_transcription("Write access is disabled.");
}

#[test]
fn english_rewrite_keyword_after_first_clause_remains_transcription() {
    assert_routes_to_transcription("This text is only an example. Please rewrite it.");
}

#[test]
fn later_clause_rewrite_keyword_cannot_select_document_mode() {
    assert_routes_to_transcription("这只是一个示例。请改写后面的内容。");
}

#[test]
fn long_transcript_with_quoted_action_deep_in_body_remains_transcription() {
    let mut transcript =
        "这是一次较长的会议转录，发言人逐项说明背景、约束、进度和后续安排。".repeat(16);
    let action_offset = transcript.chars().count();
    transcript.push_str("随后有人引用演示中的一句话：“打开记事本”，这只是对现场内容的复述。");
    while transcript.chars().count() < 5_703 {
        transcript.push_str(
            "与会者继续讨论需求、风险、验证结果和下一阶段计划，没有提出新的直接操作要求。",
        );
    }

    assert!(action_offset >= 400, "action must occur deep in the body");
    assert!(
        transcript.chars().count() >= 5_703,
        "regression fixture must model the production transcript length"
    );
    assert_eq!(infer_smart_voice_mode(&transcript), VoiceMode::Transcribe);
}

#[test]
fn leading_summary_instruction_can_transform_a_long_body() {
    let body = "会议首先回顾了项目背景，随后讨论风险、验证结果和下一阶段计划。".repeat(12);
    let transcript = format!("帮我总结下面的内容：{body}");

    assert!(non_whitespace_char_count(&transcript) >= 160);
    assert_eq!(infer_smart_voice_mode(&transcript), VoiceMode::Document);
}

#[test]
fn short_direct_command_boundary_is_inclusive_at_80_non_whitespace_characters() {
    let below_limit = command_like_text_with_non_whitespace_count(79);
    let at_limit = command_like_text_with_non_whitespace_count(80);
    let over_limit = command_like_text_with_non_whitespace_count(81);

    let below_limit_analysis =
        analyze_smart_voice_mode(&below_limit, SmartRouteEvidence::default());
    assert_eq!(below_limit_analysis.non_whitespace_char_count, 79);
    assert_eq!(
        below_limit_analysis.leading_intent,
        Some(SmartLeadingIntent::Command)
    );
    assert_eq!(below_limit_analysis.resolved_mode, VoiceMode::Command);
    assert_eq!(
        below_limit_analysis.reason,
        SmartRouteReason::ShortDirectCommand
    );

    let at_limit_analysis = analyze_smart_voice_mode(&at_limit, SmartRouteEvidence::default());
    assert_eq!(at_limit_analysis.non_whitespace_char_count, 80);
    assert_eq!(
        at_limit_analysis.leading_intent,
        Some(SmartLeadingIntent::Command)
    );
    assert_eq!(at_limit_analysis.resolved_mode, VoiceMode::Command);
    assert_eq!(
        at_limit_analysis.reason,
        SmartRouteReason::ShortDirectCommand
    );

    let over_limit_analysis = analyze_smart_voice_mode(&over_limit, SmartRouteEvidence::default());
    assert_eq!(over_limit_analysis.non_whitespace_char_count, 81);
    assert_eq!(over_limit_analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(
        over_limit_analysis.reason,
        SmartRouteReason::TranscribeFallback
    );
}

#[test]
fn long_character_evidence_starts_at_160_non_whitespace_characters() {
    let below_threshold = "甲".repeat(159);
    let at_threshold = "甲".repeat(160);

    let below_analysis = analyze_smart_voice_mode(&below_threshold, SmartRouteEvidence::default());
    assert_eq!(below_analysis.non_whitespace_char_count, 159);
    assert!(!below_analysis.long_form_evidence);
    assert_eq!(below_analysis.reason, SmartRouteReason::TranscribeFallback);

    let threshold_analysis = analyze_smart_voice_mode(&at_threshold, SmartRouteEvidence::default());
    assert_eq!(threshold_analysis.non_whitespace_char_count, 160);
    assert!(threshold_analysis.long_form_evidence);
    assert_eq!(threshold_analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(
        threshold_analysis.reason,
        SmartRouteReason::LongCharacterCount
    );
}

#[test]
fn sentence_boundaries_become_long_form_evidence_at_three() {
    let two_boundaries = "第一句。第二句！第三句";
    let three_boundaries = "第一句。第二句！第三句？第四句";
    let four_boundaries = "第一句。第二句！第三句？第四句。";

    let two_analysis = analyze_smart_voice_mode(two_boundaries, SmartRouteEvidence::default());
    assert_eq!(two_analysis.sentence_boundary_count, 2);
    assert!(!two_analysis.long_form_evidence);
    assert_eq!(two_analysis.reason, SmartRouteReason::TranscribeFallback);

    let three_analysis = analyze_smart_voice_mode(three_boundaries, SmartRouteEvidence::default());
    assert_eq!(three_analysis.sentence_boundary_count, 3);
    assert!(three_analysis.long_form_evidence);
    assert_eq!(three_analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(
        three_analysis.reason,
        SmartRouteReason::MultipleSentenceBoundaries
    );

    let four_analysis = analyze_smart_voice_mode(four_boundaries, SmartRouteEvidence::default());
    assert_eq!(four_analysis.sentence_boundary_count, 4);
    assert!(four_analysis.long_form_evidence);
    assert_eq!(four_analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(
        four_analysis.reason,
        SmartRouteReason::MultipleSentenceBoundaries
    );
}

#[test]
fn committed_streaming_segments_become_long_form_evidence_at_three() {
    let transcript = "持续讨论项目进展和后续安排";
    let two_segment_analysis = analyze_smart_voice_mode(
        transcript,
        SmartRouteEvidence {
            committed_streaming_segment_count: 2,
        },
    );
    assert_eq!(two_segment_analysis.committed_streaming_segment_count, 2);
    assert!(!two_segment_analysis.long_form_evidence);
    assert_eq!(
        two_segment_analysis.reason,
        SmartRouteReason::TranscribeFallback
    );

    let three_segment_analysis = analyze_smart_voice_mode(
        transcript,
        SmartRouteEvidence {
            committed_streaming_segment_count: 3,
        },
    );
    assert_eq!(three_segment_analysis.committed_streaming_segment_count, 3);
    assert!(three_segment_analysis.long_form_evidence);
    assert_eq!(three_segment_analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(
        three_segment_analysis.reason,
        SmartRouteReason::MultipleStreamingSegments
    );
}

#[test]
fn transforming_instruction_after_leading_intent_span_does_not_select_document_mode() {
    let body = "会议继续讨论背景、风险、验证结果和下一阶段计划。".repeat(8);
    let transcript = format!("{}帮我总结下面的内容：{body}", "甲".repeat(96));
    let analysis = analyze_smart_voice_mode(&transcript, SmartRouteEvidence::default());

    assert!(analysis.non_whitespace_char_count >= 160);
    assert_eq!(analysis.leading_intent, None);
    assert!(analysis.long_form_evidence);
    assert_eq!(analysis.resolved_mode, VoiceMode::Transcribe);
    assert_eq!(analysis.reason, SmartRouteReason::LongCharacterCount);
}

#[test]
fn leading_translate_instruction_can_transform_a_long_body() {
    let body = "会议回顾了项目背景、风险、验证结果和下一阶段计划。".repeat(10);
    let transcript = format!("帮我把下面的内容翻译成英文：{body}");
    let analysis = analyze_smart_voice_mode(&transcript, SmartRouteEvidence::default());

    assert!(analysis.non_whitespace_char_count >= 160);
    assert_eq!(analysis.leading_intent, Some(SmartLeadingIntent::Translate));
    assert_eq!(analysis.resolved_mode, VoiceMode::Translate);
    assert_eq!(
        analysis.reason,
        SmartRouteReason::ExplicitTranslateInstruction
    );
}

#[test]
fn leading_generate_instruction_can_transform_a_long_body() {
    let body = "材料介绍了项目背景、主要冲突、人物关系和预期结局。".repeat(10);
    let transcript = format!("帮我写一篇基于下面材料的文章：{body}");
    let analysis = analyze_smart_voice_mode(&transcript, SmartRouteEvidence::default());

    assert!(analysis.non_whitespace_char_count >= 160);
    assert_eq!(analysis.leading_intent, Some(SmartLeadingIntent::Generate));
    assert_eq!(analysis.resolved_mode, VoiceMode::Generate);
    assert_eq!(
        analysis.reason,
        SmartRouteReason::ExplicitGenerateInstruction
    );
}

#[test]
fn english_leading_summary_instruction_can_transform_a_long_body() {
    let body =
        "The meeting reviewed the project background, risks, validation results, and next steps. "
            .repeat(8);
    let transcript = format!("Summarize the following content: {body}");
    let analysis = analyze_smart_voice_mode(&transcript, SmartRouteEvidence::default());

    assert!(analysis.non_whitespace_char_count >= 160);
    assert_eq!(analysis.leading_intent, Some(SmartLeadingIntent::Document));
    assert_eq!(analysis.resolved_mode, VoiceMode::Document);
    assert_eq!(
        analysis.reason,
        SmartRouteReason::ExplicitDocumentInstruction
    );
}
