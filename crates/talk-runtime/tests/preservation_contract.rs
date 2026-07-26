use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use talk_client::FrontContext;
use talk_core::{SessionStatus, TalkConfig, VoiceEvent, VoiceMode, VoiceSession};
use talk_runtime::{
    run_voice_session_from_transcript_with_insert_hooks, validate_faithful_output,
    RuntimeInsertDirective,
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
