use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use talk_audio::{start_recording, AudioCaptureRequest, WavSettings};
use talk_core::{AudioBackendMode, TalkConfig};
use talk_runtime::run_local_streaming_asr_service_from_recording;
use tokio::net::TcpSocket;
use tokio_tungstenite::tungstenite::Message;

// Public API characterization only; no microphone/model/provider is involved.
#[tokio::test]
#[ignore = "bounded batch-transfer liveness characterization"]
async fn characterize_batch_transfer_before_final_deadline() {
    let listener = TcpSocket::new_v4().unwrap();
    listener.set_recv_buffer_size(1024).unwrap();
    listener.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = listener.listen(1).unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let start = socket.next().await.unwrap().unwrap().into_text().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&start).unwrap()["type"],
            "start"
        );
        socket.send(Message::Text(r#"{"type":"ready","engine":"fixture","model":"fixture","sample_rate_hz":16000,"channels":1}"#.into())).await.unwrap();
        std::future::pending::<()>().await;
        drop(socket);
    });
    let mut config = TalkConfig::from_toml_str(include_str!(
        "../../../examples/desktop-streaming-service-speculative-config.toml"
    ))
    .unwrap();
    let service = config.speculative.streaming_service.as_mut().unwrap();
    service.endpoint = endpoint;
    service.connect_timeout_ms = 1000;
    service.idle_timeout_ms = 1000;
    service.final_timeout_ms = 100;
    // Ninety seconds is the checked-in streaming example's recording limit.
    let recording = start_recording(&AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: std::env::temp_dir().join("talk-batch-liveness-probe"),
        session_id: "batch-probe".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 90,
        silent_samples: 90 * 16_000,
    })
    .unwrap();
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_millis(750),
        run_local_streaming_asr_service_from_recording(&config, "batch-probe", &recording, None),
    )
    .await;
    println!("batch_probe pcm_bytes=2880000 final_budget_ms=100 outer_watchdog_expired={} elapsed_ms={:.3}", result.is_err(), started.elapsed().as_secs_f64() * 1000.0);
    peer.abort();
    let _ = peer.await;
    recording.cancel().unwrap();
    match result {
        Err(_) => println!("outcome=outer_watchdog"),
        Ok(Err(error)) => println!("outcome=operation_error error={error}"),
        Ok(Ok(events)) => panic!("stalled peer unexpectedly returned success: {events:?}"),
    }
}
