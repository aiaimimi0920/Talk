use super::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use talk_audio::RecordingPcmChunk;
use talk_core::TalkError;
use tokio_tungstenite::tungstenite::Message;

// Models a callback already in flight at Stop: stopping capture joins that
// callback and makes its last PCM available. No microphone or ASR model is used.
struct ProducingSource {
    capturing: Arc<AtomicBool>,
    pending: RefCell<VecDeque<Vec<u8>>>,
    sequence: Cell<u64>,
    fail_drain: bool,
}

impl ProducingSource {
    fn new() -> Self {
        Self {
            capturing: Arc::new(AtomicBool::new(true)),
            pending: RefCell::new(VecDeque::from([vec![0, 0]])),
            sequence: Cell::new(0),
            fail_drain: false,
        }
    }
}

impl LivePcmSource for ProducingSource {
    fn stop_capture(&mut self) -> Result<(), TalkError> {
        if self.capturing.swap(false, Ordering::SeqCst) {
            self.pending.get_mut().push_back(vec![1, 0]);
        }
        Ok(())
    }

    fn drain_pcm_chunk(
        &self,
        _cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        if self.fail_drain {
            return Err(TalkError::Audio("fixture drain failure".to_string()));
        }
        Ok(self.pending.borrow_mut().pop_front().map(|bytes| {
            let sequence = self.sequence.get();
            self.sequence.set(sequence + 1);
            RecordingPcmChunk {
                sequence,
                sample_rate_hz: 16_000,
                channels: 1,
                bytes,
            }
        }))
    }
}

#[derive(Clone, Copy)]
enum Reply {
    Final,
    Error,
    Timeout,
}

async fn session_and_peer(
    capturing: Arc<AtomicBool>,
    reply: Reply,
) -> (LocalStreamingAsrLiveSession, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let start: Value =
            serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                .unwrap();
        assert_eq!(start["type"], "start");
        socket.send(Message::Text(json!({"type":"ready", "engine":"fixture", "model":"fixture", "sample_rate_hz":16000, "channels":1}).to_string().into())).await.unwrap();
        let mut audio = Vec::new();
        loop {
            let message: Value =
                serde_json::from_str(&socket.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            // In the old order capture is still active here, even though the
            // user has requested Stop. Final response timing must not control it.
            assert!(
                !capturing.load(Ordering::SeqCst),
                "capture outlived Stop's final drain"
            );
            match message["type"].as_str().unwrap() {
                "audio" => audio.push(message),
                "stop" => break,
                kind => panic!("unexpected message {kind}"),
            }
        }
        assert_eq!(
            audio.len(),
            2,
            "the joined callback tail must be sent before Stop"
        );
        assert_eq!(audio[0]["sequence"], 0);
        assert_eq!(audio[0]["pcm_base64"], "AAA=");
        assert_eq!(audio[1]["sequence"], 1);
        assert_eq!(audio[1]["pcm_base64"], "AQA=");
        match reply {
            Reply::Final | Reply::Error => {
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(!capturing.load(Ordering::SeqCst));
                let response = if matches!(reply, Reply::Final) {
                    json!({"type":"final", "session_id":"stop-test", "segment_id":"segment", "text":"complete"})
                } else {
                    json!({"type":"error", "session_id":"stop-test", "message":"fixture recognizer failure"})
                };
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .unwrap();
            }
            Reply::Timeout => {
                // Keep the peer open until the client's final-result deadline
                // closes it, instead of turning this into a disconnect test.
                let _ = socket.next().await;
            }
        }
    });
    let mut client = LocalStreamingAsrServiceClient::connect(&endpoint, Duration::from_secs(1))
        .await
        .unwrap();
    client
        .start("stop-test", 16_000, 1, None, Duration::from_secs(1))
        .await
        .unwrap();
    (
        LocalStreamingAsrLiveSession {
            client,
            cursor: RecordingPcmCursor::default(),
            events: vec![StreamingAsrEvent::partial("segment", "existing")],
            session_id: "stop-test".to_string(),
            sample_rate_hz: 16_000,
            channels: 1,
            final_timeout: if matches!(reply, Reply::Timeout) {
                Duration::from_millis(200)
            } else {
                Duration::from_secs(2)
            },
        },
        peer,
    )
}

#[tokio::test]
async fn stop_freezes_capture_and_sends_callback_tail_before_delayed_final() {
    let mut source = ProducingSource::new();
    let (session, peer) = session_and_peer(Arc::clone(&source.capturing), Reply::Final).await;
    let events = session.stop_audio_source(&mut source).await.unwrap();
    peer.await.unwrap();
    assert_eq!(
        events,
        vec![
            StreamingAsrEvent::partial("segment", "existing"),
            StreamingAsrEvent::final_segment("segment", "complete")
        ]
    );
    assert!(!source.capturing.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stop_keeps_capture_stopped_when_recognizer_reports_error() {
    let mut source = ProducingSource::new();
    let (session, peer) = session_and_peer(Arc::clone(&source.capturing), Reply::Error).await;
    let error = session.stop_audio_source(&mut source).await.unwrap_err();
    peer.await.unwrap();
    assert!(error.to_string().contains("fixture recognizer failure"));
    assert!(!source.capturing.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stop_keeps_capture_stopped_while_final_result_times_out() {
    let mut source = ProducingSource::new();
    let (session, peer) = session_and_peer(Arc::clone(&source.capturing), Reply::Timeout).await;
    let error = session.stop_audio_source(&mut source).await.unwrap_err();
    peer.await.unwrap();
    assert!(error.to_string().contains("timed out"));
    assert!(!source.capturing.load(Ordering::SeqCst));
}

#[tokio::test]
async fn stop_releases_capture_even_when_final_pcm_drain_fails() {
    let mut source = ProducingSource::new();
    source.fail_drain = true;
    let (session, peer) = session_and_peer(Arc::clone(&source.capturing), Reply::Final).await;
    let error = session.stop_audio_source(&mut source).await.unwrap_err();
    peer.abort();
    let _ = peer.await;
    assert!(error.to_string().contains("fixture drain failure"));
    assert!(!source.capturing.load(Ordering::SeqCst));
}
