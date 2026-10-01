use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use talk_client::LocalStreamingAsrServiceClient;
use tokio::net::TcpSocket;
use tokio_tungstenite::tungstenite::Message;

// Synthetic loopback only. Restrict the peer's TCP window so a bounded number
// of ordinary 100 ms PCM chunks reaches transport backpressure quickly.
#[tokio::test]
#[ignore = "bounded stalled-peer characterization"]
async fn characterize_stalled_local_asr_send() {
    let listener = TcpSocket::new_v4().unwrap();
    listener.set_recv_buffer_size(1024).unwrap();
    listener.bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let listener = listener.listen(1).unwrap();
    let endpoint = format!("ws://{}/asr", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        let _ = socket.next().await.unwrap().unwrap();
        socket.send(Message::Text(r#"{"type":"ready","engine":"fixture","model":"fixture","sample_rate_hz":16000,"channels":1}"#.into())).await.unwrap();
        std::future::pending::<()>().await;
        drop(socket);
    });
    let mut client = LocalStreamingAsrServiceClient::connect(&endpoint, Duration::from_secs(1))
        .await
        .unwrap();
    client
        .start("backpressure", 16000, 1, None, Duration::from_secs(1))
        .await
        .unwrap();
    let pcm = vec![0; 3200];
    let mut blocked = false;
    for sequence in 0..4096 {
        let started = Instant::now();
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            client.send_audio("backpressure", sequence, &pcm),
        )
        .await;
        if result.is_err() {
            println!("watchdog_expired sequence={sequence} prior_pcm_bytes={} chunk_bytes={} elapsed_ms={:.3}", sequence * pcm.len() as u64, pcm.len(), started.elapsed().as_secs_f64() * 1000.0);
            blocked = true;
            break;
        }
        if let Err(error) = result.unwrap() {
            println!(
                "client_returned_error sequence={sequence} elapsed_ms={:.3} error={error}",
                started.elapsed().as_secs_f64() * 1000.0
            );
            blocked = true;
            break;
        }
    }
    peer.abort();
    let _ = peer.await;
    assert!(blocked, "bounded fixture did not reach backpressure");
}
