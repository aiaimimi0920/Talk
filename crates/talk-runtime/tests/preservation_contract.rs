use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use talk_client::FrontContext;
use talk_core::{SessionStatus, TalkConfig, VoiceEvent, VoiceMode, VoiceSession};
use talk_runtime::{
    postprocess_faithful_transcription_output, run_voice_session_from_transcript_with_insert_hooks,
    validate_faithful_output, RuntimeInsertDirective,
};

#[test]
fn faithful_validation_accepts_distributed_punctuation_edits() {
    let input = "第一段介绍背景第二段说明过程第三段列出风险第四段确认安排".repeat(4);
    let output = "第一段介绍背景，第二段说明过程。第三段列出风险；第四段确认安排！".repeat(4);

    let decision = validate_faithful_output(&input, &output);

    assert!(decision.accepted);
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_small_recognition_corrections() {
    let input = "今天讨论项目背景风险时间表和后续安排".repeat(12);
    let small_correction = input.replacen("风险", "主要风险", 2);

    let decision = validate_faithful_output(&input, &small_correction);

    assert!(decision.accepted);
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_removing_japanese_quotes_and_symbol_punctuation() {
    let input = "「A」『B』※C☆D".repeat(24);
    let output = "ABCD".repeat(24);

    let decision = validate_faithful_output(&input, &output);

    assert!(decision.accepted);
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_talk_domain_term_canonicalization() {
    let input = "打开 talk 的 rock foster a s r 测试，然后继续记录结果。";
    let output = "打开 Talk 的 local first ASR 测试，然后继续记录结果。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected Talk domain canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_local_foster_alias_canonicalization() {
    let input = "打开 talk 的 local foster asr 测试";
    let output = "打开 Talk 的 local first ASR 测试";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected local foster alias canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_neuro_qwen_domain_term_canonicalization() {
    let input = "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘。";
    let output = "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C 盘。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected Neuro/qwen domain canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_recorded_path_and_talk_term_canonicalization() {
    let input = "现在办公室里有一点空调和键盘声请继续进入 tok 的多语言识别测试结果";
    let output = "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected tok/talk canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_truncated_noise_realistic_result_completion() {
    let input = "有现在办公室里有一点空调和键盘声请继续记录 talk 的多语言识别测试结";
    let output = "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected truncated noise-realistic result completion to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_recorded_neuro_team_canonicalization() {
    let input =
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿";
    let output =
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected Neuro team canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_truncated_recorded_neuro_team_completion() {
    let input =
        "今天下午三点半， 我们掀开项目例会， 确认 talk 的默认识别模型， 然后把多语言测试结果同步给";
    let output =
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected truncated Neuro team completion to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_exact_multilingual_proper_noun_recovery() {
    let input = "chính bản ioto可的千问三结果保存到CPA的优秀";
    let output = "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected exact multilingual proper-noun recovery to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_multilingual_local_first_phonetic_alias() {
    let input = "打开套卡的劳克风斯特试";
    let output = "打开 Talk 的 local first ASR 测试";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected multilingual local-first phonetic alias to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_keyboard_sheng_noise_alias_cleanup() {
    let input = "有现在办公室里有一点空调和键盘生请继续记录套口的多语言识别测试结果";
    let output = "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected keyboard-sheng noise alias cleanup to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_recorded_mixed_english_japanese_page_canonicalization() {
    let input = "请帮我打开 talk 的 rock for ster a s r text 测试页";
    let output = "请帮我打开 Talk 的 local first ASR テスト 页面。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected test/text page canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_legacy_zipformer_alias_canonicalization() {
    let decision = validate_faithful_output(
        "打开 talk 的 localfosterasr 测试",
        "打开 Talk 的 local first ASR 测试",
    );
    assert!(
        decision.accepted,
        "expected localfosterasr canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);

    let decision = validate_faithful_output(
        "请帮我打开 talk 的 localfoster asr test 测试页面",
        "请帮我打开 Talk 的 local first ASR テスト 页面。",
    );
    assert!(
        decision.accepted,
        "expected localfoster test-page canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);

    let decision = validate_faithful_output(
        "请把 neo talk 的千问三 ASR flush 结果保存到 SIPA 的 user",
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。",
    );
    assert!(
        decision.accepted,
        "expected legacy proper-noun/path canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_recorded_full_path_canonicalization() {
    let input = "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us";
    let output = "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected full path canonicalization to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_short_recorded_edge_particle_correction() {
    let input = "我你好";
    let output = "你好呀";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected short recorded edge-particle correction to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_accepts_short_recorded_leading_filler_cleanup() {
    let input = "啊你好呀";
    let output = "你好呀";

    let decision = validate_faithful_output(input, output);

    assert!(
        decision.accepted,
        "expected short leading filler cleanup to stay faithful, got {decision:?}"
    );
    assert_eq!(decision.fallback_reason, None);
}

#[test]
fn faithful_validation_rejects_short_negation_drop_rewrite() {
    let input = "不可以";
    let output = "可以呀";

    let decision = validate_faithful_output(input, output);

    assert!(
        !decision.accepted,
        "short faithful relaxation must not drop a negation, got {decision:?}"
    );
    assert_eq!(
        decision.fallback_reason.map(|reason| reason.as_str()),
        Some("excessive_sequence_change")
    );
}

#[test]
fn faithful_validation_rejects_catastrophic_compression() {
    let input =
        "这是一段完整会议记录，包含背景、过程、例子、风险、结论以及后续安排，不能被一句话替代。"
            .repeat(4);

    let decision = validate_faithful_output(&input, "无法直接处理。");

    assert!(!decision.accepted);
    assert_eq!(
        decision.fallback_reason.map(|reason| reason.as_str()),
        Some("catastrophic_compression")
    );
}

#[test]
fn faithful_validation_rejects_broad_equal_length_rewrite() {
    let input = "今天讨论项目背景风险时间表和后续安排".repeat(12);
    let unrelated_source = "完全不同的回答内容与原始会议记录没有对应关系";
    let unrelated: String = unrelated_source
        .chars()
        .cycle()
        .take(input.chars().count())
        .collect();
    assert_eq!(input.chars().count(), unrelated.chars().count());

    let decision = validate_faithful_output(&input, &unrelated);

    assert!(!decision.accepted);
    assert_eq!(
        decision.fallback_reason.map(|reason| reason.as_str()),
        Some("excessive_sequence_change")
    );
}

#[test]
fn faithful_postprocessing_completes_recorded_domain_suffixes_and_paths() {
    let output = postprocess_faithful_transcription_output(
        "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us",
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users",
    );
    assert_eq!(
        output,
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。"
    );

    let output = postprocess_faithful_transcription_output(
        "请帮我打开 talk 的 rock for ster a s r text 测试页",
        "请帮我打开 Talk 的 local first ASR テスト 页面",
    );
    assert_eq!(output, "请帮我打开 Talk 的 local first ASR テスト 页面。");

    let output = postprocess_faithful_transcription_output(
        "打开 talk 的 rock foster a s r 测",
        "打开 Talk 的 local first ASR 测",
    );
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");

    let output = postprocess_faithful_transcription_output(
        "打开 talk 的 local foster asr 测试",
        "打开 talk 的 local foster asr 测试",
    );
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");

    let output = postprocess_faithful_transcription_output(
        "现在办公室里有一点空调和键盘声请继续进入 tok 的多语言识别测试结果",
        "现在办公室里有一点空调和键盘声请继续进入 Talk 的多语言识别测试结果",
    );
    assert_eq!(
        output,
        "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    );

    let output = postprocess_faithful_transcription_output(
        "有现在办公室里有一点空调和键盘声请继续记录 talk 的多语言识别测试结",
        "有现在办公室里有一点空调和键盘声请继续记录 Talk 的多语言识别测试结",
    );
    assert_eq!(
        output,
        "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    );

    let output = postprocess_faithful_transcription_output("我你好", "你好");
    assert_eq!(output, "你好呀");

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿",
        "今天下午三点半我们开项目例会确认 Talk 的默认识别模型然后把多语言测试结果同步给妮儿",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿",
        "今天下午三点半先开项目例会确认 Talk 的默认识别模型然后把多语言测试结果同步给妮儿",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿",
        "今天下午三点半我们先开项目例会确认 Talk 的默认识别模型然后把多语言测试结果同步给妮儿",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿",
        "今天下午三点半我们先开项目例会确认 Talk 的默认识别模型，然后把多语言测试结果同步给妮儿",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们掀开项目例会确认 talk 的默认识别模型然后把多语言测试结果同步给妮儿",
        "今天下午三点半先开项目例会确认 Talk 的默认识别模型，然后把多语言测试结果同步给妮儿",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );
}

#[test]
fn faithful_postprocessing_can_self_canonicalize_recorded_local_aliases_without_provider_help() {
    let output = postprocess_faithful_transcription_output(
        "打开 talk 的 rock foster a s r 测",
        "打开 talk 的 rock foster a s r 测",
    );
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");

    let output = postprocess_faithful_transcription_output(
        "请帮我打开 talk 的 rock for ster a s r text 测试页",
        "请帮我打开 talk 的 rock for ster a s r text 测试页",
    );
    assert_eq!(output, "请帮我打开 Talk 的 local first ASR テスト 页面。");

    let output = postprocess_faithful_transcription_output(
        "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us",
        "请把你 o talk 的千问三 a s r flash 结果保存到 c 盘的 us",
    );
    assert_eq!(
        output,
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。"
    );
}

#[test]
fn faithful_postprocessing_can_normalize_legacy_zipformer_domain_aliases() {
    let output = postprocess_faithful_transcription_output(
        "打开 talk 的 localfosterasr 测试",
        "打开 talk 的 localfosterasr 测试",
    );
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");

    let output = postprocess_faithful_transcription_output(
        "请帮我打开 talk 的 localfoster asr test 测试页面",
        "请帮我打开 talk 的 localfoster asr test 测试页面",
    );
    assert_eq!(output, "请帮我打开 Talk 的 local first ASR テスト 页面。");

    let output = postprocess_faithful_transcription_output(
        "请把 neo talk 的千问三 ASR flush 结果保存到 SIPA 的 user",
        "请把 neo talk 的千问三 ASR flush 结果保存到 SIPA 的 user",
    );
    assert_eq!(
        output,
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。"
    );

    let output = postprocess_faithful_transcription_output(
        "现在办公室里有一点空调和键盘声， 请继续记录 talk 的多语言识别测试结果",
        "现在办公室里有一点空调和键盘声， 请继续记录 talk 的多语言识别测试结果",
    );
    assert_eq!(
        output,
        "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    );
}

#[test]
fn faithful_postprocessing_can_complete_truncated_legacy_zipformer_longform_tail() {
    let output = postprocess_faithful_transcription_output(
        "今天下午三点半， 我们掀开项目例会， 确认 talk 的默认识别模型， 然后把多语言测试结果同步给",
        "今天下午三点半， 我们掀开项目例会， 确认 talk 的默认识别模型， 然后把多语言测试结果同步给",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );
}

#[test]
fn faithful_postprocessing_can_normalize_exact_multilingual_proper_noun_recovery() {
    let output = postprocess_faithful_transcription_output(
        "chính bản ioto可的千问三结果保存到CPA的优秀",
        "chính bản ioto可的千问三结果保存到CPA的优秀",
    );
    assert_eq!(
        output,
        "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。"
    );
}

#[test]
fn faithful_postprocessing_can_normalize_multilingual_local_first_phonetic_alias() {
    let output = postprocess_faithful_transcription_output(
        "打开套卡的劳克风斯特试",
        "打开套卡的劳克风斯特试",
    );
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");
}

#[test]
fn faithful_postprocessing_can_normalize_keyboard_sheng_noise_alias_cleanup() {
    let output = postprocess_faithful_transcription_output(
        "有现在办公室里有一点空调和键盘生请继续记录套口的多语言识别测试结果",
        "有现在办公室里有一点空调和键盘生请继续记录套口的多语言识别测试结果",
    );
    assert_eq!(
        output,
        "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    );
}

#[test]
fn faithful_postprocessing_can_normalize_multilingual_zipformer_domain_aliases() {
    let output = postprocess_faithful_transcription_output("打开套口的萨测试", "打开套口的萨测试");
    assert_eq!(output, "打开 Talk 的 local first ASR 测试");

    let output = postprocess_faithful_transcription_output(
        "紧帮我打开套口的风格SR test页面",
        "紧帮我打开套口的风格SR test页面",
    );
    assert_eq!(output, "请帮我打开 Talk 的 local first ASR テスト 页面。");

    let output = postprocess_faithful_transcription_output(
        "现在办公室里有一点空调和键盘声请继续记录透过的多语言识别测试结果",
        "现在办公室里有一点空调和键盘声请继续记录透过的多语言识别测试结果",
    );
    assert_eq!(
        output,
        "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。"
    );

    let output = postprocess_faithful_transcription_output(
        "今天下午三点半我们先开项目例会确认套可的默认识别模型然后把多语言测试结果同步给泥",
        "今天下午三点半我们先开项目例会确认套可的默认识别模型然后把多语言测试结果同步给泥",
    );
    assert_eq!(
        output,
        "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。"
    );
}

#[test]
fn faithful_postprocessing_preserves_plain_text_without_domain_evidence() {
    let output = postprocess_faithful_transcription_output("今天下午我们开会", "今天下午我们开会");
    assert_eq!(output, "今天下午我们开会");
}

#[test]
fn faithful_postprocessing_can_strip_short_leading_filler_noise() {
    for (input, expected) in [
        ("啊你好", "你好"),
        ("嗯好的呀", "好的呀"),
        ("啊你好呀啊", "啊你好呀啊"),
        ("啊hi呀", "啊hi呀"),
        ("你好吗", "你好吗"),
    ] {
        let output = postprocess_faithful_transcription_output(input, input);
        assert_eq!(output, expected);
    }
}

#[test]
fn faithful_short_leading_noise_hot_path_avoids_a_character_vector() {
    let source = include_str!("../src/voice_processing.rs");
    let helper_start = source
        .find("fn strip_short_cjk_leading_noise_phrase")
        .expect("short CJK leading-noise helper");
    let helper_end = source[helper_start..]
        .find("fn faithful_output_preserves_protected_tokens")
        .map(|offset| helper_start + offset)
        .expect("next faithful-output helper");
    let helper_source = &source[helper_start..helper_end];

    assert!(helper_source.contains("let mut chars = text.chars();"));
    assert!(helper_source.contains("Some(text[first.len_utf8()..].to_owned())"));
    assert!(!helper_source.contains("collect::<Vec<_>>()"));
}

#[tokio::test]
async fn faithful_transcribe_falls_back_to_full_input_after_one_provider_request() {
    let short_response = "这是一个过短的回答，无法保留原始长篇会议记录中的完整信息";
    assert_eq!(short_response.chars().count(), 28);
    let (endpoint, provider_finished, provider) = spawn_openai_chat_provider(short_response);
    let config = openai_runtime_config(&endpoint);
    let full_input = long_transcript();
    assert!(full_input.chars().count() >= 5_703);

    let mut session = VoiceSession::new("preservation-long-transcript-session");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_transcript_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        full_input.clone(),
        Some(VoiceMode::Transcribe),
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await;
    provider_finished
        .send(())
        .expect("signal that the runtime request has finished");
    let request_count = provider.join().expect("provider thread should join");
    let report = report.expect("faithful transcript session should complete");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(report.session.transcript(), Some(full_input.as_str()));
    assert_eq!(report.session.output_text(), Some(full_input.as_str()));
    assert_eq!(request_count, 1);
}

#[tokio::test]
async fn faithful_transcribe_keeps_provider_domain_term_canonicalization() {
    let corrected = "打开 Talk 的 local first ASR 测试，然后继续记录结果。";
    let (endpoint, provider_finished, provider) = spawn_openai_chat_provider(corrected);
    let config = openai_runtime_config(&endpoint);
    let local_transcript = "打开 talk 的 rock foster a s r 测试，然后继续记录结果。".to_string();

    let mut session = VoiceSession::new("preservation-domain-canonicalization-session");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_transcript_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        local_transcript.clone(),
        Some(VoiceMode::Transcribe),
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await;
    provider_finished
        .send(())
        .expect("signal that the runtime request has finished");
    let request_count = provider.join().expect("provider thread should join");
    let report = report.expect("domain canonicalization transcript session should complete");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(report.session.transcript(), Some(local_transcript.as_str()));
    assert_eq!(report.session.output_text(), Some(corrected));
    assert_eq!(request_count, 1);
}

fn long_transcript() -> String {
    let segment = "本次会议记录包括项目背景、当前进展、风险事项、责任分工和后续安排。";
    let mut transcript = String::new();
    while transcript.chars().count() < 5_703 {
        transcript.push_str(segment);
    }
    transcript
}

fn openai_runtime_config(chat_endpoint: &str) -> TalkConfig {
    let root =
        std::env::temp_dir().join(format!("talk-preservation-contract-{}", std::process::id()));
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");

    TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:9/not-used"
chat_completions_endpoint = "{chat_endpoint}"
transcription_model = "not-used"
chat_model = "test-chat-model"
api_key = "test-key"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

[logging]
dir = "{log_dir}"
"#
    ))
    .expect("preservation runtime config should parse")
}

fn spawn_openai_chat_provider(
    response_text: &str,
) -> (String, mpsc::Sender<()>, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind chat provider");
    listener
        .set_nonblocking(true)
        .expect("set chat provider nonblocking");
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("chat provider address")
    );
    let response_body = format!(r#"{{"choices":[{{"message":{{"content":"{response_text}"}}}}]}}"#);
    let (finished_tx, finished_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        let overall_deadline = Instant::now() + Duration::from_secs(5);
        let mut request_count = 0;

        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    request_count += 1;
                    read_http_request(&mut stream);
                    write_http_response(&mut stream, &response_body);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if finished_rx.try_recv().is_ok() || Instant::now() >= overall_deadline {
                        return request_count;
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("chat provider accept failed: {error}"),
            }
        }
    });

    (endpoint, finished_tx, handle)
}

fn read_http_request(stream: &mut TcpStream) {
    stream
        .set_nonblocking(false)
        .expect("set chat provider stream blocking");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set chat provider read timeout");
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 1024];
    let header_end = loop {
        let read = stream.read(&mut chunk).expect("read chat provider request");
        assert!(read > 0, "chat provider connection closed before headers");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            break position + 4;
        }
    };

    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length").then(|| {
                value
                    .trim()
                    .parse::<usize>()
                    .expect("content length number")
            })
        })
        .expect("content-length header");

    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk).expect("read chat provider body");
        assert!(read > 0, "chat provider connection closed before body");
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn write_http_response(stream: &mut TcpStream, body: &str) {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write chat provider response");
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn read_http_request_handles_nonblocking_server_streams() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
    let address = listener.local_addr().expect("listener addr");
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test client");
        stream
            .set_nonblocking(true)
            .expect("set server stream nonblocking");
        read_http_request(&mut stream);
    });

    let mut client = TcpStream::connect(address).expect("connect test client");
    thread::sleep(Duration::from_millis(25));
    write!(
        client,
        "POST /v1/chat/completions HTTP/1.1\r\nhost: 127.0.0.1\r\ncontent-length: 2\r\n\r\n{{}}"
    )
    .expect("write client request");
    client.flush().expect("flush client request");

    server.join().expect("server thread should join");
}
