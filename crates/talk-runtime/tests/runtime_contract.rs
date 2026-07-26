use std::io::Write;
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use talk_client::FrontContext;
use talk_core::{
    ClipboardBackendMode, OutputMode, ProviderKind, SessionStatus, TalkConfig, VoiceEvent,
    VoiceMode, VoiceSession,
};
use talk_insert::{InsertMethod, InsertOutcome};
use talk_runtime::{
    complete_failed_session, complete_failed_session_with_mode_override, infer_smart_voice_mode,
    process_voice_transcript_text, process_voice_transcript_text_with_diagnostics,
    provider_text_processing_credentials_available, run_voice_session,
    run_voice_session_from_audio_artifact_with_insert_hook,
    run_voice_session_from_audio_artifact_with_insert_hooks,
    run_voice_session_from_local_transcript_with_insert_hooks,
    run_voice_session_from_transcript_with_route_evidence_and_insert_hooks,
    runtime_voice_text_result, update_session_log_after_text_processing,
    FaithfulOutputFallbackReason, FaithfulOutputValidation, RuntimeInsertContext,
    RuntimeInsertDirective, RuntimePhase, RuntimeVoiceTextResult, SmartRouteEvidence,
};

fn runtime_test_root(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join("talk-runtime-contract")
        .join(name)
}

fn config_with_mock_provider(name: &str) -> TalkConfig {
    let root = runtime_test_root(name);
    let audio_dir = root.join("audio").display().to_string().replace('\\', "/");
    let log_dir = root.join("logs").display().to_string().replace('\\', "/");
    TalkConfig::from_toml_str(&format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "mock"
mock_transcript = "runtime transcript"

[output]
mode = "dry_run"
restore_clipboard = true
clipboard_backend = "fallback"

voice_mode = "dictate"

[logging]
dir = "{log_dir}"
"#,
        audio_dir = audio_dir,
        log_dir = log_dir
    ))
    .expect("runtime test config should parse")
}

fn spawn_provider_request_detector() -> (String, thread::JoinHandle<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider request detector");
    listener
        .set_nonblocking(true)
        .expect("set provider request detector nonblocking");
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("provider detector address")
    );
    let handle = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    let _ = stream.write_all(
                        b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    );
                    return true;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("provider request detector failed: {error}"),
            }
        }
        false
    });
    (endpoint, handle)
}

#[test]
fn provider_text_processing_credentials_are_checked_without_exposing_secrets() {
    let mut config = config_with_mock_provider("provider-credentials");
    config.provider.kind = ProviderKind::OpenAiCompatible;
    config.provider.api_key = Some("configured-test-key".to_string());
    assert!(provider_text_processing_credentials_available(&config));

    config.provider.api_key = None;
    config.provider.api_key_env = None;
    assert!(!provider_text_processing_credentials_available(&config));
}

#[test]
fn ambient_dashscope_file_does_not_enable_non_dashscope_provider() {
    let mut config = config_with_mock_provider("non-dashscope-credentials");
    config.provider.kind = ProviderKind::OpenAiCompatible;
    config.provider.mock_transcript = None;
    config.provider.audio_transcriptions_endpoint =
        Some("https://example.invalid/v1/audio/transcriptions".to_string());
    config.provider.chat_completions_endpoint =
        Some("https://example.invalid/v1/chat/completions".to_string());
    config.provider.api_key = None;
    config.provider.api_key_env = None;

    assert!(!provider_text_processing_credentials_available(&config));
}

#[tokio::test]
async fn missing_openai_credentials_do_not_send_provider_request() {
    let (endpoint, detector) = spawn_provider_request_detector();
    let mut config = config_with_mock_provider("missing-credentials-no-request");
    config.provider.kind = ProviderKind::OpenAiCompatible;
    config.provider.mock_transcript = None;
    config.provider.audio_transcriptions_endpoint = Some(endpoint.clone());
    config.provider.chat_completions_endpoint = Some(endpoint);
    config.provider.transcription_model = Some("test-transcription-model".to_string());
    config.provider.chat_model = Some("test-chat-model".to_string());
    config.provider.api_key = None;
    config.provider.api_key_env = Some("TALK_TEST_MISSING_PROVIDER_KEY".to_string());

    let error = process_voice_transcript_text(
        &config,
        "local transcript".to_string(),
        Some(VoiceMode::Transcribe),
        FrontContext::default(),
    )
    .await
    .expect_err("missing credentials should fail before provider I/O");

    let provider_request_observed = detector
        .join()
        .expect("provider request detector should join");
    assert!(
        !provider_request_observed,
        "missing credentials must not create an outbound provider connection"
    );
    assert!(error.to_string().contains("TALK_TEST_MISSING_PROVIDER_KEY"));
}

#[tokio::test]
async fn failed_session_preserves_explicit_command_mode() {
    let mut config = config_with_mock_provider("failed-session-explicit-command-mode");
    config.provider.kind = ProviderKind::OpenAiCompatible;
    config.provider.mock_transcript = None;
    config.provider.audio_transcriptions_endpoint = Some("http://127.0.0.1:9/not-used".to_string());
    config.provider.chat_completions_endpoint = Some("http://127.0.0.1:9/not-used".to_string());
    config.provider.transcription_model = Some("not-used".to_string());
    config.provider.chat_model = Some("test-chat-model".to_string());
    config.provider.api_key = None;
    config.provider.api_key_env = None;

    let mut session = VoiceSession::new("failed-session-explicit-command-mode");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_transcript_with_route_evidence_and_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        "打开记事本".to_string(),
        Some(VoiceMode::Command),
        SmartRouteEvidence {
            committed_streaming_segment_count: 3,
        },
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await
    .expect("provider credential failure should return a persisted failure report");

    assert_eq!(report.session.status(), SessionStatus::Failed);
    assert_eq!(report.requested_mode, VoiceMode::Command);
    assert_eq!(report.smart_routed_mode, None);
}

#[tokio::test]
async fn transcription_failure_preserves_explicit_command_mode_in_report_and_log() {
    let mut config = config_with_mock_provider("transcription-failure-explicit-command-mode");
    config.provider.mock_transcript = None;
    let audio_path = runtime_test_root("transcription-failure-explicit-command-mode")
        .join("audio")
        .join("captured.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio dir")).expect("create audio dir");
    std::fs::write(&audio_path, b"fake wav").expect("write fake wav");

    let mut session = VoiceSession::new("transcription-failure-explicit-command-mode");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_audio_artifact_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        None,
        Some(VoiceMode::Command),
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await
    .expect("transcription failure should return a persisted failure report");

    assert_eq!(report.session.status(), SessionStatus::Failed);
    assert_eq!(report.requested_mode, VoiceMode::Command);
    assert_eq!(report.smart_routed_mode, None);

    let log: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report.log_path).expect("read failed-session log"),
    )
    .expect("parse failed-session log");
    assert_eq!(log["processing"]["requested_mode"], "command");
    assert_eq!(log["processing"]["resolved_mode"], "command");
}

#[tokio::test]
async fn blank_transcript_preserves_explicit_document_mode_in_report_and_log() {
    let config = config_with_mock_provider("blank-transcript-explicit-document-mode");
    let mut session = VoiceSession::new("blank-transcript-explicit-document-mode");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_transcript_with_route_evidence_and_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        "   ".to_string(),
        Some(VoiceMode::Document),
        SmartRouteEvidence::default(),
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await
    .expect("blank transcript should return a persisted failure report");

    assert_eq!(report.session.status(), SessionStatus::Failed);
    assert_eq!(report.requested_mode, VoiceMode::Document);
    assert_eq!(report.smart_routed_mode, None);

    let log: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report.log_path).expect("read blank-transcript log"),
    )
    .expect("parse blank-transcript log");
    assert_eq!(log["processing"]["requested_mode"], "document");
    assert_eq!(log["processing"]["resolved_mode"], "document");
}

#[test]
fn local_transcript_completes_without_openai_credentials() {
    let mut config = config_with_mock_provider("local-transcript-without-provider-key");
    config.provider.kind = ProviderKind::OpenAiCompatible;
    config.provider.mock_transcript = None;
    config.provider.api_key = None;
    config.provider.api_key_env = Some("TALK_TEST_MISSING_PROVIDER_KEY".to_string());

    let mut session = VoiceSession::new("local-transcript-without-provider-key");
    session.apply(VoiceEvent::TriggerStart).unwrap();
    session.apply(VoiceEvent::TriggerStop).unwrap();

    let report = run_voice_session_from_local_transcript_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        "本地识别已经成功。".to_string(),
        Some(VoiceMode::Smart),
        |_| RuntimeInsertDirective::UseConfiguredOutput,
        || {},
        |_| {},
    )
    .expect("local transcript should not require provider credentials");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(report.session.output_text(), Some("本地识别已经成功。"));
}

#[tokio::test]
async fn runtime_runs_mock_session_and_reports_phase_sequence() {
    let config = config_with_mock_provider("phase-sequence");
    let mut phases = Vec::new();

    let report = run_voice_session(
        &config,
        Some("runtime transcript".to_string()),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
        |phase| phases.push(phase),
    )
    .await
    .expect("runtime mock session should succeed");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(report.session.output_text(), Some("runtime transcript"));
    assert_eq!(
        phases,
        vec![
            RuntimePhase::TriggerArmed,
            RuntimePhase::Recording,
            RuntimePhase::Transcribing,
            RuntimePhase::Processing,
            RuntimePhase::Inserting,
            RuntimePhase::Completed,
        ]
    );
    assert!(report.log_path.exists(), "runtime session log should exist");
}

#[tokio::test]
async fn report_exposes_transcript_and_processed_output_for_desktop_ui() {
    let config = config_with_mock_provider("desktop-text-result");

    let report = run_voice_session(
        &config,
        Some("runtime transcript".to_string()),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
        |_| {},
    )
    .await
    .expect("runtime mock session should succeed");

    assert_eq!(
        runtime_voice_text_result(&report),
        RuntimeVoiceTextResult {
            transcript: Some("runtime transcript".to_string()),
            processed_output: Some("runtime transcript".to_string()),
            smart_routed_mode: None,
        }
    );
}

#[test]
fn smart_route_infers_concrete_mode_from_transcript() {
    assert_eq!(infer_smart_voice_mode("打开记事本"), VoiceMode::Command);
    assert_eq!(
        infer_smart_voice_mode("生成一篇描述春天的散文"),
        VoiceMode::Generate
    );
    assert_eq!(
        infer_smart_voice_mode("请把这段话润色成正式公文"),
        VoiceMode::Document
    );
    assert_eq!(
        infer_smart_voice_mode("你好呀今天我们讨论项目进度"),
        VoiceMode::Transcribe
    );
}

#[tokio::test]
async fn smart_runtime_result_exposes_routed_mode_for_desktop_policy() {
    let config = config_with_mock_provider("smart-route-generation-report");

    let report = run_voice_session(
        &config,
        Some("生成一篇描述春天的散文".to_string()),
        Some(VoiceMode::Smart),
        FrontContext::default(),
        |_| {},
    )
    .await
    .expect("smart runtime session should succeed");

    assert_eq! {
        runtime_voice_text_result(&report),
        RuntimeVoiceTextResult {
            transcript: Some("生成一篇描述春天的散文".to_string()),
            processed_output: Some("生成一篇描述春天的散文".to_string()),
            smart_routed_mode: Some(VoiceMode::Generate),
        }
    };
}

#[tokio::test]
async fn smart_long_form_session_log_records_route_and_preservation_diagnostics() {
    let config = config_with_mock_provider("smart-long-form-processing-log");
    let transcript = "这是一次较长的会议记录。主持人先介绍项目背景，随后有人引用演示中的一句话打开记事本，然后继续说明风险、时间表和后续安排。参与者逐一反馈，最后确认下周再复盘。"
        .repeat(3);
    assert!(
        transcript
            .chars()
            .filter(|character| !character.is_whitespace())
            .count()
            >= 160
    );

    let report = run_voice_session(
        &config,
        Some(transcript),
        Some(VoiceMode::Smart),
        FrontContext::default(),
        |_| {},
    )
    .await
    .expect("smart long-form mock session should complete");
    let log: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report.log_path).expect("read smart processing log"),
    )
    .expect("parse smart processing log");
    let processing = log
        .get("processing")
        .expect("session log should include processing diagnostics");

    assert_eq!(processing["requested_mode"], "smart");
    assert_eq!(processing["resolved_mode"], "transcribe");
    assert_eq!(processing["route_reason"], "long_character_count");
    assert!(processing["route_input_char_count"].as_u64().unwrap() >= 160);
    assert!(processing["sentence_boundary_count"].as_u64().unwrap() >= 3);
    assert_eq!(processing["streaming_segment_count"], 0);
    assert_eq!(
        processing["faithful_input_char_count"],
        processing["faithful_output_char_count"]
    );
    assert_eq!(processing["retention_ratio"], 1.0);
    assert_eq!(processing["normalized_change_ratio"], 0.0);
    assert!(processing.get("preservation_fallback_reason").is_none());
}

#[tokio::test]
async fn smart_insert_context_exposes_routed_mode_before_insert() {
    let config = config_with_mock_provider("smart-route-insert-context");
    let audio_path = runtime_test_root("smart-route-insert-context")
        .join("audio")
        .join("captured.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio dir")).expect("create audio dir");
    std::fs::write(&audio_path, b"fake wav").expect("write fake wav");

    let captured_context = Arc::new(Mutex::new(None::<RuntimeInsertContext>));
    let captured_context_for_hook = Arc::clone(&captured_context);

    let mut session = VoiceSession::new("smart-route-insert-context-session");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("session should stop");

    let report = run_voice_session_from_audio_artifact_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        Some("打开记事本".to_string()),
        Some(VoiceMode::Smart),
        FrontContext::default(),
        |context| {
            *captured_context_for_hook.lock().expect("insert context") = Some(context.clone());
            RuntimeInsertDirective::DryRunOnly
        },
        || {},
        |_| {},
    )
    .await
    .expect("smart runtime run with insert context should succeed");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(report.smart_routed_mode, Some(VoiceMode::Command));
    assert_eq!(
        captured_context.lock().expect("insert context").clone(),
        Some(RuntimeInsertContext {
            requested_mode: VoiceMode::Smart,
            smart_routed_mode: Some(VoiceMode::Command),
            transcript: "打开记事本".to_string(),
            output_text: "打开记事本".to_string(),
        })
    );
}

#[tokio::test]
async fn runtime_processes_transcript_text_without_insert_side_effects() {
    let config = config_with_mock_provider("process-transcript-text");

    let output = process_voice_transcript_text(
        &config,
        "本地识别文本".to_string(),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
    )
    .await
    .unwrap();

    assert_eq!(output, "本地识别文本");
}

#[tokio::test]
async fn transcript_processing_diagnostics_are_available_to_non_session_callers() {
    let config = config_with_mock_provider("process-transcript-text-diagnostics");
    let transcript = "这是一段最终整篇转录，桌面端需要同时拿到忠实度诊断。".repeat(4);

    let output = process_voice_transcript_text_with_diagnostics(
        &config,
        transcript.clone(),
        Some(VoiceMode::Transcribe),
        FrontContext::default(),
    )
    .await
    .expect("runtime processing with diagnostics should succeed");

    assert_eq!(output.text, transcript);
    let validation = output
        .faithful_validation
        .expect("transcribe processing should expose faithful validation");
    assert!(validation.accepted);
    assert_eq!(validation.input_char_count, validation.output_char_count);
    assert_eq!(validation.normalized_change_ratio, 0.0);
}

#[test]
fn final_text_processing_updates_output_and_faithful_metrics_without_losing_route_diagnostics() {
    let root = runtime_test_root("update-final-processing-log");
    std::fs::create_dir_all(&root).expect("create final processing log root");
    let log_path = root.join("session.json");
    std::fs::write(
        &log_path,
        serde_json::to_string_pretty(&serde_json::json!({
            "id": "session",
            "status": "completed",
            "transcript": "完整原始转录",
            "output_text": "本地完整转录",
            "trigger_mode": "toggle",
            "trigger_events": ["trigger_start", "trigger_stop"],
            "processing": {
                "requested_mode": "smart",
                "resolved_mode": "transcribe",
                "route_reason": "multiple_streaming_segments",
                "streaming_segment_count": 4,
                "preservation_fallback_reason": "stale_reason"
            }
        }))
        .expect("serialize final processing fixture"),
    )
    .expect("write final processing fixture");

    update_session_log_after_text_processing(
        &log_path,
        "完整原始转录",
        Some(FaithfulOutputValidation {
            accepted: false,
            fallback_reason: Some(FaithfulOutputFallbackReason::CatastrophicCompression),
            input_char_count: 5_703,
            output_char_count: 28,
            retention_ratio: 28.0 / 5_703.0,
            normalized_change_ratio: 1.0,
        }),
    )
    .expect("update final processing diagnostics");

    let updated: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&log_path).expect("read updated final processing log"),
    )
    .expect("parse updated final processing log");
    assert_eq!(updated["transcript"], "完整原始转录");
    assert_eq!(updated["output_text"], "完整原始转录");
    assert_eq!(updated["processing"]["requested_mode"], "smart");
    assert_eq!(updated["processing"]["resolved_mode"], "transcribe");
    assert_eq!(
        updated["processing"]["route_reason"],
        "multiple_streaming_segments"
    );
    assert_eq!(updated["processing"]["streaming_segment_count"], 4);
    assert_eq!(updated["processing"]["faithful_input_char_count"], 5_703);
    assert_eq!(updated["processing"]["faithful_output_char_count"], 28);
    assert_eq!(
        updated["processing"]["preservation_fallback_reason"],
        "catastrophic_compression"
    );

    update_session_log_after_text_processing(
        &log_path,
        "完整原始转录，已修正标点。",
        Some(FaithfulOutputValidation {
            accepted: true,
            fallback_reason: None,
            input_char_count: 5_703,
            output_char_count: 5_710,
            retention_ratio: 1.0,
            normalized_change_ratio: 0.01,
        }),
    )
    .expect("replace stale fallback reason with accepted validation");
    let accepted: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&log_path).expect("read accepted final processing log"),
    )
    .expect("parse accepted final processing log");
    assert_eq!(accepted["output_text"], "完整原始转录，已修正标点。");
    assert!(accepted["processing"]
        .get("preservation_fallback_reason")
        .is_none());
}

#[tokio::test]
async fn runtime_persists_failed_session_log_when_provider_fails() {
    let mut config = config_with_mock_provider("provider-fails");
    config.provider.kind = ProviderKind::Http;
    config.provider.mock_transcript = None;
    config.provider.endpoint = Some("http://127.0.0.1:9/transcribe".to_string());
    config.output.mode = OutputMode::DryRun;
    let mut phases = Vec::new();

    let report = run_voice_session(&config, None, None, FrontContext::default(), |phase| {
        phases.push(phase)
    })
    .await
    .expect("runtime should persist failure report instead of bubbling raw error");

    assert_eq!(report.session.status(), SessionStatus::Failed);
    assert!(
        report.session.error().is_some(),
        "failed runtime session should retain error"
    );
    assert!(
        report.log_path.exists(),
        "failed runtime session should still write log"
    );
    assert_eq!(
        phases,
        vec![
            RuntimePhase::TriggerArmed,
            RuntimePhase::Recording,
            RuntimePhase::Transcribing,
            RuntimePhase::Failed,
        ]
    );
}

#[tokio::test]
async fn runtime_runs_before_insert_hook_between_inserting_and_completed() {
    let config = config_with_mock_provider("before-insert-hook");
    let audio_path = runtime_test_root("before-insert-hook")
        .join("audio")
        .join("captured.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio dir")).expect("create audio dir");
    std::fs::write(&audio_path, b"fake wav").expect("write fake wav");

    let markers = Arc::new(Mutex::new(Vec::<String>::new()));
    let phase_markers = Arc::clone(&markers);
    let hook_markers = Arc::clone(&markers);

    let mut session = VoiceSession::new("before-insert-hook-session");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("session should stop");

    let report = run_voice_session_from_audio_artifact_with_insert_hook(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        Some("runtime transcript".to_string()),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
        |_| {
            hook_markers
                .lock()
                .expect("hook markers")
                .push("hook:before_insert".to_string());
            RuntimeInsertDirective::UseConfiguredOutput
        },
        |phase| {
            phase_markers
                .lock()
                .expect("phase markers")
                .push(format!("phase:{phase:?}"));
        },
    )
    .await
    .expect("runtime run with insert hook should succeed");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(
        markers.lock().expect("markers").clone(),
        vec![
            "phase:Processing".to_string(),
            "phase:Inserting".to_string(),
            "hook:before_insert".to_string(),
            "phase:Completed".to_string(),
        ]
    );
}

#[tokio::test]
async fn runtime_runs_before_and_after_insert_hooks_around_inserting() {
    let config = config_with_mock_provider("around-insert-hooks");
    let audio_path = runtime_test_root("around-insert-hooks")
        .join("audio")
        .join("captured.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio dir")).expect("create audio dir");
    std::fs::write(&audio_path, b"fake wav").expect("write fake wav");

    let markers = Arc::new(Mutex::new(Vec::<String>::new()));
    let phase_markers = Arc::clone(&markers);
    let before_markers = Arc::clone(&markers);
    let after_markers = Arc::clone(&markers);

    let mut session = VoiceSession::new("around-insert-hooks-session");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("session should stop");

    let report = run_voice_session_from_audio_artifact_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        Some("runtime transcript".to_string()),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
        |_| {
            before_markers
                .lock()
                .expect("before markers")
                .push("hook:before_insert".to_string());
            RuntimeInsertDirective::UseConfiguredOutput
        },
        || {
            after_markers
                .lock()
                .expect("after markers")
                .push("hook:after_insert".to_string());
        },
        |phase| {
            phase_markers
                .lock()
                .expect("phase markers")
                .push(format!("phase:{phase:?}"));
        },
    )
    .await
    .expect("runtime run with around insert hooks should succeed");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(
        markers.lock().expect("markers").clone(),
        vec![
            "phase:Processing".to_string(),
            "phase:Inserting".to_string(),
            "hook:before_insert".to_string(),
            "hook:after_insert".to_string(),
            "phase:Completed".to_string(),
        ]
    );
}

#[tokio::test]
async fn runtime_can_switch_insert_stage_to_dry_run_only_after_processing() {
    let mut config = config_with_mock_provider("insert-stage-dry-run-only");
    config.output.mode = OutputMode::ClipboardPaste;
    config.output.clipboard_backend = ClipboardBackendMode::Fallback;
    let audio_path = runtime_test_root("insert-stage-dry-run-only")
        .join("audio")
        .join("captured.wav");
    std::fs::create_dir_all(audio_path.parent().expect("audio dir")).expect("create audio dir");
    std::fs::write(&audio_path, b"fake wav").expect("write fake wav");

    let mut session = VoiceSession::new("insert-stage-dry-run-only-session");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("session should stop");

    let report = run_voice_session_from_audio_artifact_with_insert_hooks(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        audio_path,
        Some("runtime transcript".to_string()),
        Some(VoiceMode::Dictate),
        FrontContext::default(),
        |_| RuntimeInsertDirective::DryRunOnly,
        || {},
        |_| {},
    )
    .await
    .expect("runtime run with dynamic dry-run insert directive should succeed");

    assert_eq!(report.session.status(), SessionStatus::Completed);
    assert_eq!(
        report.outcome,
        Some(InsertOutcome::Inserted {
            method: InsertMethod::DryRun
        })
    );
}

#[test]
fn complete_failed_session_persists_error_for_pre_recorded_runs() {
    let config = config_with_mock_provider("pre-recorded-fails");
    let mut session = VoiceSession::new("pre-recorded-session");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("pre-recorded session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("pre-recorded session should stop into transcribing");
    let trigger_events = vec!["trigger_start", "trigger_stop"];
    let mut phases = Vec::new();

    let report = complete_failed_session(
        &config,
        session,
        trigger_events,
        anyhow::anyhow!("desktop recording failed"),
        false,
        |phase| phases.push(phase),
    )
    .expect("pre-recorded failure should persist report");

    assert_eq!(report.session.status(), SessionStatus::Failed);
    assert_eq!(report.session.error(), Some("desktop recording failed"));
    assert!(
        report.log_path.exists(),
        "pre-recorded failure should write log"
    );
    assert_eq!(phases, vec![RuntimePhase::Failed]);
}

#[test]
fn externally_completed_failure_can_preserve_explicit_document_mode() {
    let config = config_with_mock_provider("externally-failed-explicit-document-mode");
    let mut session = VoiceSession::new("externally-failed-explicit-document-mode");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("pre-recorded session should start");
    session
        .apply(VoiceEvent::TriggerStop)
        .expect("pre-recorded session should stop into transcribing");

    let report = complete_failed_session_with_mode_override(
        &config,
        session,
        vec!["trigger_start", "trigger_stop"],
        Some(VoiceMode::Document),
        anyhow::anyhow!("desktop recording failed"),
        false,
        |_| {},
    )
    .expect("external failure should persist explicit mode");

    assert_eq!(report.requested_mode, VoiceMode::Document);
    assert_eq!(report.smart_routed_mode, None);
    let log: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&report.log_path).expect("read external failure log"),
    )
    .expect("parse external failure log");
    assert_eq!(log["processing"]["requested_mode"], "document");
    assert_eq!(log["processing"]["resolved_mode"], "document");
}

#[test]
fn complete_cancelled_session_persists_cancelled_status() {
    let config = config_with_mock_provider("cancelled-session");
    let mut session = VoiceSession::new("cancelled-session-id");
    session
        .apply(VoiceEvent::TriggerStart)
        .expect("cancelled session should start");
    let trigger_events = vec!["trigger_start"];
    let mut phases = Vec::new();

    let report =
        talk_runtime::complete_cancelled_session(&config, session, trigger_events, |phase| {
            phases.push(phase)
        })
        .expect("cancelled session should persist report");

    assert_eq!(report.session.status(), SessionStatus::Cancelled);
    assert!(
        report.log_path.exists(),
        "cancelled session should write log"
    );
    assert_eq!(phases, vec![RuntimePhase::Cancelled]);
}
