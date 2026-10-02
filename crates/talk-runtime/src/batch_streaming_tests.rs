use super::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::cell::Cell;
use std::sync::Arc;
use std::time::Instant;
use talk_audio::RecordingPcmChunk;
use talk_core::TalkError;
use tokio::sync::{oneshot, Notify};
use tokio_tungstenite::tungstenite::Message;

struct BufferedSource {
    remaining: Cell<usize>,
    drained: Cell<u64>,
    channels: u16,
    preparation_delay: Duration,
    transfer_started: Option<Arc<Notify>>,
}

impl BufferedSource {
    fn new(chunks: usize) -> Self {
        Self {
            remaining: Cell::new(chunks),
            drained: Cell::new(0),
            channels: 1,
            preparation_delay: Duration::ZERO,
            transfer_started: None,
        }
    }
}

impl LivePcmSource for BufferedSource {
    fn stop_capture(&mut self) -> Result<(), TalkError> {
        panic!("the batch helper must preserve caller-owned capture lifetime")
    }

    fn drain_pcm_chunk(
        &self,
        _cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        if self.remaining.get() == 0 {
            return Ok(None);
        }
        let sequence = self.drained.get();
        if sequence == 0 {
            if let Some(started) = &self.transfer_started {
                started.notify_one();
            }
        }
        if sequence == 0 && !self.preparation_delay.is_zero() {
            // Controlled work in the transfer phase, independent of OS buffers.
            std::thread::sleep(self.preparation_delay);
        }
        self.remaining.set(self.remaining.get() - 1);
        self.drained.set(sequence + 1);
        Ok(Some(RecordingPcmChunk {
            sequence,
            sample_rate_hz: 16000,
            channels: self.channels,
            bytes: vec![0; 3200],
        }))
    }
}

async fn peer_and_config(
    final_budget: Duration,
    final_delay: Option<Duration>,
) -> (
    TalkConfig,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Vec<Value>>,
) {
    let listener = tokio::net::TcpSocket::new_v4().unwrap();
    listener.set_recv_buffer_size(1024).unwrap();
    listener.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = listener.listen(1).unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let (resume, resumed) = oneshot::channel();
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let start: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        assert_eq!(start["type"], "start");
        assert_eq!(start["sample_rate_hz"], 16000);
        assert_eq!(start["channels"], 1);
        socket.send(Message::Text(json!({"type":"ready","engine":"fixture","model":"fixture","sample_rate_hz":16000,"channels":1}).to_string().into())).await.unwrap();
        let _ = resumed.await;
        let mut received = vec![start];
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let value: Value = serde_json::from_str(&text).unwrap();
            let stop = value["type"] == "stop";
            received.push(value);
            if stop {
                if socket.send(Message::Text(json!({"type":"partial","session_id":"batch-test","segment_id":"segment","text":"unfinished"}).to_string().into())).await.is_err() { break; }
                if let Some(delay) = final_delay {
                    tokio::time::sleep(delay).await;
                    if socket.send(Message::Text(json!({"type":"final","session_id":"batch-test","segment_id":"segment","text":"complete"}).to_string().into())).await.is_err() { break; }
                }
            }
        }
        received
    });
    let mut config = TalkConfig::from_toml_str(include_str!(
        "../../../examples/desktop-streaming-service-speculative-config.toml"
    ))
    .unwrap();
    let service = config.speculative.streaming_service.as_mut().unwrap();
    service.endpoint = endpoint;
    service.final_timeout_ms = final_budget.as_millis() as u64;
    (config, resume, peer)
}

async fn finish_peer(
    resume: Option<oneshot::Sender<()>>,
    peer: tokio::task::JoinHandle<Vec<Value>>,
) -> Vec<Value> {
    if let Some(resume) = resume {
        let _ = resume.send(());
    }
    tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap()
}

fn assert_audio_prefix_only(messages: &[Value], drained: u64) {
    assert_eq!(messages[0]["type"], "start");
    assert!(messages.len() as u64 <= drained + 1);
    for (sequence, message) in messages[1..].iter().enumerate() {
        assert_eq!(
            message["type"], "audio",
            "no Stop or replay after failed transfer"
        );
        assert_eq!(message["sequence"], sequence as u64);
    }
}

#[tokio::test]
async fn batch_transfer_timeout_closes_connection_without_stop_or_replay() {
    let (config, resume, peer) = peer_and_config(Duration::from_millis(150), None).await;
    let source = BufferedSource::new(4096);
    let started = Instant::now();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        run_local_streaming_asr_service_from_source(&config, "batch-test", &source, None),
    )
    .await
    .expect("batch transfer ignored its deadline")
    .unwrap_err();
    println!(
        "batch_transfer_timeout drained_chunks={} elapsed_ms={:.3} error={error}",
        source.drained.get(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(error.to_string().contains("timed out sending batch"));
    assert!(source.remaining.get() > 0);
    let messages = finish_peer(Some(resume), peer).await;
    assert!(
        messages.len() > 1,
        "fixture must deliver some PCM before stalling"
    );
    assert_audio_prefix_only(&messages, source.drained.get());
}

#[tokio::test]
async fn batch_success_keeps_audio_format_order_language_and_events() {
    let (config, resume, peer) =
        peer_and_config(Duration::from_secs(1), Some(Duration::ZERO)).await;
    resume.send(()).unwrap();
    let source = BufferedSource::new(2);
    let events =
        run_local_streaming_asr_service_from_source(&config, "batch-test", &source, Some("zh"))
            .await
            .unwrap();
    assert_eq!(
        events,
        vec![
            StreamingAsrEvent::partial("segment", "unfinished"),
            StreamingAsrEvent::final_segment("segment", "complete")
        ]
    );
    let messages = finish_peer(None, peer).await;
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["language"], "zh");
    for (sequence, message) in messages[1..3].iter().enumerate() {
        assert_eq!(message["type"], "audio");
        assert_eq!(message["sequence"], sequence as u64);
        assert_eq!(message["pcm_base64"].as_str().unwrap().len(), 4268);
    }
    assert_eq!(messages[3]["type"], "stop");
}

#[tokio::test]
async fn batch_slow_preparation_preserves_full_final_response_budget() {
    let (config, resume, peer) =
        peer_and_config(Duration::from_secs(2), Some(Duration::from_millis(1500))).await;
    resume.send(()).unwrap();
    let mut source = BufferedSource::new(1);
    source.preparation_delay = Duration::from_millis(1100);
    let started = Instant::now();
    let events = run_local_streaming_asr_service_from_source(&config, "batch-test", &source, None)
        .await
        .unwrap();
    println!(
        "batch_separate_final_budget elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(started.elapsed() > Duration::from_secs(2));
    assert!(events.last().unwrap().is_final());
    let messages = finish_peer(None, peer).await;
    assert_eq!(messages.last().unwrap()["type"], "stop");
}

#[tokio::test]
async fn batch_final_timeout_does_not_return_partial_only_success() {
    let (config, resume, peer) = peer_and_config(Duration::from_millis(150), None).await;
    resume.send(()).unwrap();
    let source = BufferedSource::new(1);
    let error = run_local_streaming_asr_service_from_source(&config, "batch-test", &source, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"));
    let messages = finish_peer(None, peer).await;
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[2]["type"], "stop",
        "fixture must reach final-result collection"
    );
}

#[tokio::test]
async fn batch_empty_source_still_rejects_before_stop() {
    let (config, resume, peer) =
        peer_and_config(Duration::from_secs(1), Some(Duration::ZERO)).await;
    resume.send(()).unwrap();
    let source = BufferedSource::new(0);
    let error = run_local_streaming_asr_service_from_source(&config, "batch-test", &source, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("produced no PCM chunks"));
    assert_eq!(finish_peer(None, peer).await.len(), 1);
}

#[tokio::test]
async fn batch_format_mismatch_still_rejects_without_audio_or_stop() {
    let (config, resume, peer) =
        peer_and_config(Duration::from_secs(1), Some(Duration::ZERO)).await;
    resume.send(()).unwrap();
    let mut source = BufferedSource::new(1);
    source.channels = 2;
    let error = run_local_streaming_asr_service_from_source(&config, "batch-test", &source, None)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("does not match streaming_service"));
    assert_eq!(finish_peer(None, peer).await.len(), 1);
}

#[tokio::test]
async fn cancelling_batch_transfer_closes_the_connection_without_replay() {
    let (config, resume, peer) = peer_and_config(Duration::from_secs(5), None).await;
    let started = Arc::new(Notify::new());
    let mut source = BufferedSource::new(4096);
    source.transfer_started = Some(Arc::clone(&started));
    {
        let mut transfer = Box::pin(run_local_streaming_asr_service_from_source(
            &config,
            "batch-test",
            &source,
            None,
        ));
        tokio::select! {
            result = &mut transfer => panic!("transfer finished before cancellation setup: {result:?}"),
            ready = tokio::time::timeout(Duration::from_secs(2), started.notified()) => ready.expect("fixture did not begin PCM transfer"),
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut transfer)
                .await
                .is_err()
        );
        // Dropping the owned future here closes the socket before the peer resumes.
    }
    assert!(source.drained.get() > 0);
    assert!(source.remaining.get() > 0);
    assert_audio_prefix_only(&finish_peer(Some(resume), peer).await, source.drained.get());
}

#[tokio::test]
async fn public_batch_api_returns_error_for_stalled_buffered_recording() {
    let (config, resume, peer) = peer_and_config(Duration::from_millis(100), None).await;
    let recording = talk_audio::start_recording(&AudioCaptureRequest {
        backend: talk_core::AudioBackendMode::Silent,
        temp_dir: std::env::temp_dir().join("talk-batch-liveness-test"),
        session_id: "batch-test".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 90,
        silent_samples: 90 * 16_000,
    })
    .unwrap();
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        run_local_streaming_asr_service_from_recording(&config, "batch-test", &recording, None),
    )
    .await
    .expect("public batch API remained blocked");
    println!(
        "public_batch_pcm_bytes=2880000 elapsed_ms={:.3} error={}",
        started.elapsed().as_secs_f64() * 1000.0,
        result.as_ref().unwrap_err()
    );
    assert!(result.unwrap_err().to_string().contains("timed out"));
    let messages = finish_peer(Some(resume), peer).await;
    // The OS may buffer this single large frame completely. Then the existing
    // final-response deadline fires instead; that is also a bounded API result.
    assert!(messages.len() <= 3);
    assert_eq!(messages[0]["type"], "start");
    if messages.len() >= 2 {
        assert_eq!(messages[1]["type"], "audio");
        assert_eq!(messages[1]["sequence"], 0);
    }
    if messages.len() == 3 {
        assert_eq!(messages[2]["type"], "stop");
    }
    recording.cancel().unwrap();
}
