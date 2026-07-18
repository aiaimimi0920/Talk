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
        r#"
Write-Output '{"type":"partial","segment_id":"seg-1","text":"你好"}'
Write-Output '{"type":"final","segment_id":"seg-1","text":"你好。"}'
"#,
    )
    .unwrap();
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

    let final_events = live_session.stop(recording).await.unwrap();
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
}
