use super::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::cell::Cell;
use std::time::Instant;
use talk_audio::RecordingPcmChunk;
use talk_core::TalkError;
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

// Bounded synthetic PCM, never a microphone or model. A native drain can
// include accumulated audio; the small case uses an ordinary 100 ms chunk.
struct BacklogSource {
    remaining: Cell<usize>,
    drains: Cell<u64>,
    chunk_bytes: usize,
    capturing: Cell<bool>,
}

impl BacklogSource {
    fn new(chunks: usize, chunk_bytes: usize) -> Self {
        Self {
            remaining: Cell::new(chunks),
            drains: Cell::new(0),
            chunk_bytes,
            capturing: Cell::new(true),
        }
    }
}

impl LivePcmSource for BacklogSource {
    fn stop_capture(&mut self) -> Result<(), TalkError> {
        self.capturing.set(false);
        Ok(())
    }

    fn drain_pcm_chunk(
        &self,
        _cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        if self.remaining.get() == 0 {
            return Ok(None);
        }
        self.remaining.set(self.remaining.get() - 1);
        let sequence = self.drains.get();
        self.drains.set(sequence + 1);
        Ok(Some(RecordingPcmChunk {
            sequence,
            sample_rate_hz: 16_000,
            channels: 1,
            bytes: vec![0; self.chunk_bytes],
        }))
    }
}

async fn stalled_peer(
    pump_timeout: Duration,
    final_timeout: Duration,
) -> (
    LocalStreamingAsrLiveSession,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Vec<Value>>,
) {
    stalled_peer_with_final_delay(pump_timeout, final_timeout, Duration::ZERO).await
}

async fn stalled_peer_with_final_delay(
    pump_timeout: Duration,
    final_timeout: Duration,
    final_delay: Duration,
) -> (
    LocalStreamingAsrLiveSession,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Vec<Value>>,
) {
    let listener = tokio::net::TcpSocket::new_v4().unwrap();
    // Before listen/handshake: bounds how much data a deliberately stalled
    // loopback peer accepts. This does not model a real microphone loss rate.
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
        socket.send(Message::Text(json!({"type":"ready","engine":"fixture","model":"fixture","sample_rate_hz":16000,"channels":1}).to_string().into())).await.unwrap();
        let _ = resumed.await;
        let mut received = Vec::new();
        while let Some(Ok(message)) = socket.next().await {
            if let Message::Text(text) = message {
                let value: Value = serde_json::from_str(&text).unwrap();
                let stop = value["type"] == "stop";
                received.push(value);
                if stop {
                    tokio::time::sleep(final_delay).await;
                    socket.send(Message::Text(json!({"type":"final","session_id":"backpressure","segment_id":"segment","text":"complete"}).to_string().into())).await.unwrap();
                }
            }
        }
        received
    });
    let mut client = LocalStreamingAsrServiceClient::connect(&endpoint, Duration::from_secs(2))
        .await
        .unwrap();
    client
        .start("backpressure", 16000, 1, None, Duration::from_secs(2))
        .await
        .unwrap();
    (
        LocalStreamingAsrLiveSession {
            client: Some(client),
            terminal_error: None,
            cursor: RecordingPcmCursor::default(),
            events: vec![StreamingAsrEvent::partial("segment", "unfinished")],
            session_id: "backpressure".to_string(),
            sample_rate_hz: 16000,
            channels: 1,
            final_timeout,
            pump_timeout,
        },
        resume,
        peer,
    )
}

async fn received_after_close(
    resume: oneshot::Sender<()>,
    peer: tokio::task::JoinHandle<Vec<Value>>,
) -> Vec<Value> {
    let _ = resume.send(());
    tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap()
}

fn assert_only_unique_audio(messages: &[Value], drained: u64) {
    assert!(!messages.is_empty());
    assert!(messages.len() as u64 <= drained);
    for (sequence, message) in messages.iter().enumerate() {
        assert_eq!(
            message["type"], "audio",
            "terminal sessions must not send Stop or Cancel"
        );
        assert_eq!(
            message["sequence"], sequence as u64,
            "no replay or duplicate sequence"
        );
    }
}

#[tokio::test]
async fn stalled_pump_is_terminal_and_stop_never_replays_or_promotes_partial() {
    let (mut session, resume, peer) =
        stalled_peer(Duration::from_millis(100), Duration::from_secs(1)).await;
    let mut source = BacklogSource::new(4096, 3200);
    let mut failure = None;
    for _ in 0..4096 {
        let before = source.drains.get();
        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            session.pump_audio_source(&source, Duration::from_millis(1)),
        )
        .await
        .expect("desktop pump remained blocked past watchdog");
        assert_eq!(source.drains.get(), before + 1, "one snapshot per pump");
        if let Err(error) = result {
            println!(
                "stalled_pump chunks_drained={} chunk_bytes=3200 elapsed_ms={:.3} error={error}",
                source.drains.get(),
                started.elapsed().as_secs_f64() * 1000.0
            );
            failure = Some(error);
            break;
        }
    }
    assert!(failure.unwrap().to_string().contains("timed out pumping"));
    let drained = source.drains.get();
    assert!(session
        .pump_audio_source(&source, Duration::from_millis(1))
        .await
        .unwrap_err()
        .to_string()
        .contains("cannot resume"));
    assert_eq!(source.drains.get(), drained);
    assert!(session
        .stop_audio_source(&mut source)
        .await
        .unwrap_err()
        .to_string()
        .contains("cannot resume"));
    assert!(!source.capturing.get());
    assert_eq!(source.drains.get(), drained);
    assert_only_unique_audio(&received_after_close(resume, peer).await, drained);
}

#[tokio::test]
async fn externally_cancelled_pump_closes_socket_and_cancel_stays_local() {
    let (mut session, resume, peer) =
        stalled_peer(Duration::from_secs(5), Duration::from_secs(1)).await;
    let source = BacklogSource::new(4096, 3200);
    let mut interrupted = false;
    for _ in 0..4096 {
        if tokio::time::timeout(
            Duration::from_millis(30),
            session.pump_audio_source(&source, Duration::from_millis(1)),
        )
        .await
        .is_err()
        {
            interrupted = true;
            break;
        }
    }
    assert!(interrupted);
    let drained = source.drains.get();
    assert!(session.client.is_none());
    assert!(session
        .pump_audio_source(&source, Duration::from_millis(1))
        .await
        .unwrap_err()
        .to_string()
        .contains("interrupted"));
    assert_eq!(source.drains.get(), drained);
    tokio::time::timeout(Duration::from_millis(100), session.cancel())
        .await
        .unwrap()
        .unwrap();
    assert_only_unique_audio(&received_after_close(resume, peer).await, drained);
}

#[tokio::test]
async fn stop_deadline_covers_stalled_final_pcm_and_never_returns_partial_success() {
    let (session, resume, peer) =
        stalled_peer(Duration::from_millis(100), Duration::from_millis(150)).await;
    let mut source = BacklogSource::new(4096, 3200);
    let started = Instant::now();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        session.stop_audio_source(&mut source),
    )
    .await
    .unwrap()
    .unwrap_err();
    println!(
        "stalled_stop chunks_drained={} elapsed_ms={:.3} error={error}",
        source.drains.get(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(error.to_string().contains("timed out sending final"));
    assert!(!source.capturing.get());
    assert_only_unique_audio(
        &received_after_close(resume, peer).await,
        source.drains.get(),
    );
}

#[tokio::test]
async fn slow_valid_peer_recovers_within_configured_pump_budget() {
    let (mut session, resume, peer) =
        stalled_peer(Duration::from_secs(2), Duration::from_secs(2)).await;
    // A deliberately accumulated backlog larger than the restricted TCP window.
    let mut source = BacklogSource::new(1, 2 * 1024 * 1024);
    let resume_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        resume.send(()).unwrap();
    });
    let started = Instant::now();
    session
        .pump_audio_source(&source, Duration::from_millis(1))
        .await
        .unwrap();
    println!(
        "slow_valid_pump chunk_bytes={} elapsed_ms={:.3}",
        source.chunk_bytes,
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(session.client.is_some());
    assert_eq!(source.drains.get(), 1);
    assert!(session
        .stop_audio_source(&mut source)
        .await
        .unwrap()
        .last()
        .unwrap()
        .is_final());
    resume_task.await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0]["type"], "audio");
    assert_eq!(received[0]["sequence"], 0);
    assert_eq!(received[1]["type"], "stop");
    assert!(!source.capturing.get());
}

#[tokio::test]
async fn slow_recognition_does_not_fail_the_default_live_pump() {
    let (mut session, resume, peer) = stalled_peer_with_final_delay(
        Duration::from_millis(100),
        Duration::from_secs(1),
        Duration::from_millis(150),
    )
    .await;
    let mut source = BacklogSource::new(2, 3200);
    resume.send(()).unwrap();
    let started = Instant::now();
    for _ in 0..2 {
        assert!(session
            .pump_audio_source(&source, Duration::from_millis(1))
            .await
            .unwrap()
            .is_empty());
    }
    println!(
        "healthy_pumps count=2 chunk_bytes=3200 elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(session.client.is_some());
    assert_eq!(source.drains.get(), 2);
    let events = session.stop_audio_source(&mut source).await.unwrap();
    assert!(events.last().unwrap().is_final());
    let received = tokio::time::timeout(Duration::from_secs(3), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.len(), 3);
    assert_eq!(received[0]["sequence"], 0);
    assert_eq!(received[1]["sequence"], 1);
    assert_eq!(received[2]["type"], "stop");
}

#[tokio::test]
async fn stop_preserves_full_final_response_window_after_slow_transfer() {
    let (session, resume, peer) = stalled_peer_with_final_delay(
        Duration::from_millis(100),
        Duration::from_secs(2),
        Duration::from_millis(1700),
    )
    .await;
    let mut source = BacklogSource::new(1, 2 * 1024 * 1024);
    let resume_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        resume.send(()).unwrap();
    });
    let started = Instant::now();
    let events = session.stop_audio_source(&mut source).await.unwrap();
    println!(
        "slow_stop_with_full_final_window elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(events.last().unwrap().is_final());
    assert!(
        started.elapsed() > Duration::from_secs(2),
        "fixture must exceed a shared two-second budget"
    );
    assert!(!source.capturing.get());
    resume_task.await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(5), peer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(received[1]["type"], "stop");
}

#[tokio::test]
async fn dropping_pending_stop_keeps_capture_frozen_and_closes_transport() {
    let (session, resume, peer) =
        stalled_peer(Duration::from_millis(100), Duration::from_secs(5)).await;
    let mut source = BacklogSource::new(4096, 3200);
    let result = tokio::time::timeout(
        Duration::from_millis(100),
        session.stop_audio_source(&mut source),
    )
    .await;
    assert!(result.is_err(), "fixture must interrupt a pending Stop");
    assert!(!source.capturing.get());
    assert_only_unique_audio(
        &received_after_close(resume, peer).await,
        source.drains.get(),
    );
}

#[tokio::test]
async fn continuous_partial_messages_cannot_extend_live_pump_forever() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        socket.next().await.unwrap().unwrap();
        socket.send(Message::Text(json!({"type":"ready","engine":"fixture","model":"fixture","sample_rate_hz":16000,"channels":1}).to_string().into())).await.unwrap();
        socket.next().await.unwrap().unwrap();
        loop {
            if socket.send(Message::Text(json!({"type":"partial","session_id":"backpressure","segment_id":"segment","text":"unfinished"}).to_string().into())).await.is_err() { break; }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    });
    let mut config = TalkConfig::from_toml_str(include_str!(
        "../../../examples/desktop-streaming-service-speculative-config.toml"
    ))
    .unwrap();
    config.speculative.enabled = true;
    config.speculative.local_asr = "streaming_service".to_string();
    config
        .speculative
        .streaming_service
        .as_mut()
        .unwrap()
        .endpoint = endpoint;
    let mut session = LocalStreamingAsrLiveSession::start(&config, "backpressure", None)
        .await
        .unwrap();
    let mut source = BacklogSource::new(1, 3200);
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        session.pump_audio_source(&source, Duration::from_millis(50)),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("timed out pumping"));
    assert!(
        session.events.is_empty(),
        "incomplete pump must not publish partial events"
    );
    assert!(session.stop_audio_source(&mut source).await.is_err());
    assert!(!source.capturing.get());
    tokio::time::timeout(Duration::from_secs(2), peer)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancel_deadline_drops_a_live_client_with_a_blocked_write() {
    let (mut session, resume, peer) =
        stalled_peer(Duration::from_millis(100), Duration::from_secs(1)).await;
    // Test the control-send guard directly: deliberately interrupt a low-level
    // send, leaving the socket's existing write buffer blocked. Live pump
    // ownership prevents this state from being reused in production.
    let pcm = vec![0; 4 * 1024 * 1024];
    assert!(tokio::time::timeout(
        Duration::from_millis(100),
        session
            .client
            .as_mut()
            .unwrap()
            .send_audio("backpressure", 0, &pcm)
    )
    .await
    .is_err());
    let started = Instant::now();
    let error = tokio::time::timeout(Duration::from_secs(2), session.cancel())
        .await
        .unwrap()
        .unwrap_err();
    println!(
        "stalled_cancel elapsed_ms={:.3} error={error}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    assert!(error.to_string().contains("timed out cancelling"));
    let received = received_after_close(resume, peer).await;
    assert!(received.iter().all(|message| message["type"] == "audio"));
    assert!(received.len() <= 1, "no replay after cancellation");
}
