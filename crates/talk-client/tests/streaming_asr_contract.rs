use talk_client::{
    final_transcript_from_streaming_asr_events, local_streaming_server_message_to_asr_event,
    parse_local_streaming_asr_server_message, parse_streaming_asr_json_line,
    serialize_local_streaming_asr_client_message, LocalStreamingAsrClientMessage,
    LocalStreamingAsrServerMessage, LocalStreamingAsrServiceClient, MockStreamingAsrEngine,
    StreamingAsrEngine, StreamingAsrEvent,
};

#[test]
fn mock_streaming_asr_emits_partial_then_final_events() {
    let mut engine = MockStreamingAsrEngine::new(vec![
        StreamingAsrEvent::partial("seg-1", "你好"),
        StreamingAsrEvent::partial("seg-1", "你好呀"),
        StreamingAsrEvent::final_segment("seg-1", "你好呀。"),
    ]);

    assert_eq!(
        engine.next_event().unwrap(),
        StreamingAsrEvent::partial("seg-1", "你好")
    );
    assert_eq!(
        engine.next_event().unwrap(),
        StreamingAsrEvent::partial("seg-1", "你好呀")
    );
    assert_eq!(
        engine.next_event().unwrap(),
        StreamingAsrEvent::final_segment("seg-1", "你好呀。")
    );
    assert!(engine.next_event().is_none());
}

#[test]
fn streaming_asr_event_rejects_blank_text() {
    let error = StreamingAsrEvent::try_partial("seg-1", "   ").unwrap_err();
    assert!(error
        .to_string()
        .contains("streaming ASR text must not be blank"));
}

#[test]
fn parses_external_asr_json_line() {
    let event =
        parse_streaming_asr_json_line(r#"{"type":"partial","segment_id":"seg-1","text":"你好"}"#)
            .unwrap();
    assert_eq!(event, StreamingAsrEvent::partial("seg-1", "你好"));
}

#[test]
fn final_transcript_prefers_last_final_event() {
    let transcript = final_transcript_from_streaming_asr_events(&[
        StreamingAsrEvent::partial("seg-1", "你好"),
        StreamingAsrEvent::final_segment("seg-1", "你好。"),
        StreamingAsrEvent::partial("seg-2", "后续草稿"),
    ])
    .unwrap();

    assert_eq!(transcript, "你好。");
}

#[test]
fn final_transcript_falls_back_to_latest_partial_when_no_final_exists() {
    let transcript = final_transcript_from_streaming_asr_events(&[
        StreamingAsrEvent::partial("seg-1", "你"),
        StreamingAsrEvent::partial("seg-1", "你好"),
    ])
    .unwrap();

    assert_eq!(transcript, "你好");
}

#[test]
fn local_streaming_start_message_serializes_session_audio_shape_and_language() {
    let message =
        LocalStreamingAsrClientMessage::start("session-1", 16_000, 1, Some("zh")).unwrap();

    let json = serialize_local_streaming_asr_client_message(&message).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["type"], "start");
    assert_eq!(value["session_id"], "session-1");
    assert_eq!(value["sample_rate_hz"], 16_000);
    assert_eq!(value["channels"], 1);
    assert_eq!(value["language"], "zh");
}

#[test]
fn local_streaming_audio_message_base64_encodes_raw_pcm() {
    let message = LocalStreamingAsrClientMessage::audio("session-1", 7, &[0x00, 0x01, 0xff])
        .expect("valid PCM chunk");

    let json = serialize_local_streaming_asr_client_message(&message).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["type"], "audio");
    assert_eq!(value["session_id"], "session-1");
    assert_eq!(value["sequence"], 7);
    assert_eq!(value["pcm_base64"], "AAH/");
}

#[test]
fn local_streaming_partial_and_final_messages_convert_to_asr_events() {
    let partial = parse_local_streaming_asr_server_message(
        r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":"你好"}"#,
    )
    .unwrap();
    let final_message = parse_local_streaming_asr_server_message(
        r#"{"type":"final","session_id":"session-1","segment_id":"seg-1","text":"你好。"}"#,
    )
    .unwrap();

    assert_eq!(
        local_streaming_server_message_to_asr_event(partial).unwrap(),
        Some(StreamingAsrEvent::partial("seg-1", "你好"))
    );
    assert_eq!(
        local_streaming_server_message_to_asr_event(final_message).unwrap(),
        Some(StreamingAsrEvent::final_segment("seg-1", "你好。"))
    );
}

#[test]
fn local_streaming_ready_message_exposes_engine_model_and_audio_shape() {
    let message = parse_local_streaming_asr_server_message(
        r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer-streaming-zh","sample_rate_hz":16000,"channels":1}"#,
    )
    .unwrap();

    match message {
        LocalStreamingAsrServerMessage::Ready(ready) => {
            assert_eq!(ready.engine, "sherpa-onnx");
            assert_eq!(ready.model, "zipformer-streaming-zh");
            assert_eq!(ready.sample_rate_hz, 16_000);
            assert_eq!(ready.channels, 1);
        }
        other => panic!("expected ready message, got {other:?}"),
    }
}

#[test]
fn local_streaming_server_error_message_converts_to_provider_error() {
    let message = parse_local_streaming_asr_server_message(
        r#"{"type":"error","session_id":"session-1","message":"model is not loaded"}"#,
    )
    .unwrap();

    let error = local_streaming_server_message_to_asr_event(message)
        .expect_err("server error must fail the ASR event conversion");

    assert!(
        error.to_string().contains(
            "local streaming ASR service error for session session-1: model is not loaded"
        ),
        "error={error}"
    );
}

#[tokio::test]
async fn local_streaming_service_client_sends_start_audio_stop_and_collects_events() {
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::time::Duration;
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
                r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":"你好"}"#
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
                r#"{"type":"final","session_id":"session-1","segment_id":"seg-1","text":"你好。"}"#
                    .into(),
            ))
            .await
            .unwrap();

        received
    });

    let mut client = LocalStreamingAsrServiceClient::connect(&endpoint, Duration::from_secs(1))
        .await
        .unwrap();
    let ready = client
        .start("session-1", 16_000, 1, Some("zh"), Duration::from_secs(1))
        .await
        .unwrap();
    client
        .send_audio("session-1", 1, &[0x00, 0x01, 0xff])
        .await
        .unwrap();
    client.stop("session-1").await.unwrap();
    let events = client
        .collect_asr_events_until_final(Duration::from_secs(1))
        .await
        .unwrap();
    let received = server.await.unwrap();

    assert_eq!(ready.engine, "sherpa-onnx");
    assert_eq!(ready.model, "zipformer-streaming-zh");
    assert_eq!(
        events,
        vec![
            StreamingAsrEvent::partial("seg-1", "你好"),
            StreamingAsrEvent::final_segment("seg-1", "你好。"),
        ]
    );
    assert_eq!(received[0]["type"], "start");
    assert_eq!(received[0]["session_id"], "session-1");
    assert_eq!(received[0]["language"], "zh");
    assert_eq!(received[1]["type"], "audio");
    assert_eq!(received[1]["sequence"], 1);
    assert_eq!(received[1]["pcm_base64"], "AAH/");
    assert_eq!(received[2]["type"], "stop");
    assert_eq!(received[2]["session_id"], "session-1");
}

#[cfg(windows)]
#[test]
fn runs_external_asr_command_and_collects_json_line_events() {
    use talk_client::run_external_streaming_asr_command;

    let root = std::env::temp_dir().join(format!("talk-external-asr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
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

    let events = run_external_streaming_asr_command(&command, &audio_path).unwrap();

    assert_eq!(
        events,
        vec![
            StreamingAsrEvent::partial("seg-1", "你好"),
            StreamingAsrEvent::final_segment("seg-1", "你好。"),
        ]
    );
}
