use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use talk_audio::{read_wav_info, write_silent_wav, AudioArtifact, WavSettings};

fn unique_temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-cli-contract-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn toml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

fn write_test_config(root: &Path) -> PathBuf {
    write_test_config_with_output_mode(root, "dry_run")
}

fn write_test_config_with_output_mode(root: &Path, output_mode: &str) -> PathBuf {
    write_test_config_with_output_mode_trigger_and_clipboard_backend(
        root,
        output_mode,
        "toggle",
        None,
    )
}

fn write_test_config_with_output_mode_and_trigger(
    root: &Path,
    output_mode: &str,
    trigger_mode: &str,
) -> PathBuf {
    write_test_config_with_output_mode_trigger_and_clipboard_backend(
        root,
        output_mode,
        trigger_mode,
        None,
    )
}

fn write_test_config_with_output_mode_trigger_and_clipboard_backend(
    root: &Path,
    output_mode: &str,
    trigger_mode: &str,
    clipboard_backend: Option<&str>,
) -> PathBuf {
    write_test_config_with_output_mode_trigger_clipboard_and_audio_backend(
        root,
        output_mode,
        trigger_mode,
        clipboard_backend,
        None,
    )
}

fn write_test_config_with_output_mode_trigger_clipboard_and_audio_backend(
    root: &Path,
    output_mode: &str,
    trigger_mode: &str,
    clipboard_backend: Option<&str>,
    audio_backend: Option<&str>,
) -> PathBuf {
    let audio_dir = root.join("audio");
    let log_dir = root.join("logs");
    let config_path = root.join("config.toml");
    let clipboard_backend_line = clipboard_backend
        .map(|backend| format!("clipboard_backend = \"{backend}\"\n"))
        .unwrap_or_default();
    let audio_backend_line = audio_backend
        .map(|backend| format!("backend = \"{backend}\"\n"))
        .unwrap_or_default();
    fs::write(
        &config_path,
        format!(
            r#"[trigger]
mode = "{}"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
{}max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{}"

[provider]
kind = "mock"
mock_transcript = "hello from config"

[output]
mode = "{}"
restore_clipboard = true
{}

[logging]
dir = "{}"
"#,
            trigger_mode,
            audio_backend_line,
            toml_path(&audio_dir),
            output_mode,
            clipboard_backend_line,
            toml_path(&log_dir)
        ),
    )
    .expect("write config");
    config_path
}

fn talk_config_json(transcript: &str) -> serde_json::Value {
    json!({
        "trigger": { "mode": "toggle", "toggle_shortcut": "Ctrl+Alt+Space" },
        "audio": {
            "backend": "silent",
            "max_recording_seconds": 60,
            "sample_rate_hz": 16000,
            "channels": 1,
            "temp_dir": ".runtime/talk-test/audio"
        },
        "provider": {
            "kind": "mock",
            "mock_transcript": transcript,
            "endpoint": null
        },
        "output": {
            "mode": "dry_run",
            "restore_clipboard": true,
            "clipboard_backend": "fallback"
        },
        "logging": { "dir": ".runtime/talk-test/logs" },
        "voice_mode": "dictate"
    })
}

struct FakeLoomServer {
    base_url: String,
}

impl FakeLoomServer {
    fn base_url(&self) -> &str {
        &self.base_url
    }
}

fn spawn_fake_loom_talk_config_server(transcript: &str, created: bool) -> FakeLoomServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Loom config server");
    listener.set_nonblocking(false).expect("fake Loom blocking");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("fake Loom local addr")
    );
    let transcript = transcript.to_string();
    thread::spawn(move || {
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .expect("fake Loom stream timeout");
            let (method, path, body) = read_fake_http_request(&mut stream);
            match (method.as_str(), path.as_str()) {
                ("GET", "/v1/configuration/claims?app=talk") => {
                    write_json_response(&mut stream, r#"{"managed":true}"#);
                }
                ("GET", "/v1/configuration/apps/talk") => {
                    write_json_response(
                        &mut stream,
                        &json!({
                            "created": created,
                            "document": { "revision": 1 },
                            "config": talk_config_json(&transcript)
                        })
                        .to_string(),
                    );
                    if !created {
                        return;
                    }
                }
                ("PUT", "/v1/configuration/apps/talk") => {
                    let request: serde_json::Value =
                        serde_json::from_str(&body).expect("fake Loom PUT json");
                    write_json_response(
                        &mut stream,
                        &json!({
                            "created": false,
                            "document": { "revision": 2 },
                            "config": request["config"].clone()
                        })
                        .to_string(),
                    );
                    return;
                }
                _ => write_http_response(&mut stream, 500, r#"{"error":"unexpected request"}"#),
            }
        }
    });
    FakeLoomServer { base_url }
}

fn spawn_fake_loom_server_rejecting_authorization(transcript: &str) -> FakeLoomServer {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("bind fake Loom authorization-sensitive server");
    listener.set_nonblocking(false).expect("fake Loom blocking");
    let base_url = format!(
        "http://{}",
        listener.local_addr().expect("fake Loom local addr")
    );
    let transcript = transcript.to_string();
    thread::spawn(move || {
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .expect("fake Loom stream timeout");
            let (method, path, headers, _body) = read_fake_http_request_with_headers(&mut stream);
            if headers
                .lines()
                .any(|line| line.to_ascii_lowercase().starts_with("authorization:"))
            {
                write_http_response(
                    &mut stream,
                    401,
                    r#"{"error":"blank auth token should not be sent"}"#,
                );
                continue;
            }
            match (method.as_str(), path.as_str()) {
                ("GET", "/v1/configuration/claims?app=talk") => {
                    write_json_response(&mut stream, r#"{"managed":true}"#);
                }
                ("GET", "/v1/configuration/apps/talk") => {
                    write_json_response(
                        &mut stream,
                        &json!({
                            "created": false,
                            "document": { "revision": 1 },
                            "config": talk_config_json(&transcript)
                        })
                        .to_string(),
                    );
                    return;
                }
                _ => write_http_response(&mut stream, 500, r#"{"error":"unexpected request"}"#),
            }
        }
    });
    FakeLoomServer { base_url }
}

fn read_fake_http_request(stream: &mut TcpStream) -> (String, String, String) {
    let (method, path, _headers, body) = read_fake_http_request_with_headers(stream);
    (method, path, body)
}

fn read_fake_http_request_with_headers(stream: &mut TcpStream) -> (String, String, String, String) {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let header_end;
    loop {
        let read = stream.read(&mut temp).expect("read fake Loom request");
        assert!(read > 0, "connection closed before fake Loom headers");
        buffer.extend_from_slice(&temp[..read]);
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }
    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut request_line = headers.lines().next().unwrap_or("").split_whitespace();
    let method = request_line.next().unwrap_or("").to_string();
    let path = request_line.next().unwrap_or("").to_string();
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .or_else(|| line.strip_prefix("Content-Length:"))
        })
        .map(str::trim)
        .map(|value| value.parse::<usize>().expect("fake Loom content length"))
        .unwrap_or(0);
    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut temp).expect("read fake Loom body");
        assert!(read > 0, "connection closed before fake Loom body");
        buffer.extend_from_slice(&temp[..read]);
    }
    let body = String::from_utf8(buffer[header_end..header_end + content_length].to_vec())
        .expect("fake Loom body utf8");
    (method, path, headers, body)
}

fn write_http_provider_config(root: &Path, endpoint: &str) -> PathBuf {
    write_http_provider_config_with_voice_mode(root, endpoint, None)
}

fn write_http_provider_config_with_voice_mode(
    root: &Path,
    endpoint: &str,
    voice_mode: Option<&str>,
) -> PathBuf {
    let audio_dir = root.join("audio");
    let log_dir = root.join("logs");
    let config_path = root.join("config.toml");
    let voice_mode_line = voice_mode
        .map(|mode| format!("voice_mode = \"{mode}\"\n\n"))
        .unwrap_or_default();
    fs::write(
        &config_path,
        format!(
            r#"{}[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{}"

[provider]
kind = "http"
endpoint = "{}"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{}"
"#,
            voice_mode_line,
            toml_path(&audio_dir),
            endpoint,
            toml_path(&log_dir)
        ),
    )
    .expect("write http config");
    config_path
}

fn write_http_provider_config_without_endpoint(root: &Path) -> PathBuf {
    let audio_dir = root.join("audio");
    let log_dir = root.join("logs");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{}"

[provider]
kind = "http"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{}"
"#,
            toml_path(&audio_dir),
            toml_path(&log_dir)
        ),
    )
    .expect("write invalid http config");
    config_path
}

fn write_openai_compatible_provider_config(
    root: &Path,
    transcription_endpoint: &str,
    chat_endpoint: &str,
    voice_mode: &str,
) -> PathBuf {
    let audio_dir = root.join("audio");
    let log_dir = root.join("logs");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
voice_mode = "{voice_mode}"

[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "{transcription_endpoint}"
chat_completions_endpoint = "{chat_endpoint}"
transcription_model = "gpt-4o-mini-transcribe"
chat_model = "gpt-4o-mini"
api_key = "talk-test-key"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"
"#,
            voice_mode = voice_mode,
            audio_dir = toml_path(&audio_dir),
            transcription_endpoint = transcription_endpoint,
            chat_endpoint = chat_endpoint,
            log_dir = toml_path(&log_dir)
        ),
    )
    .expect("write openai-compatible config");
    config_path
}

fn write_openai_compatible_chat_audio_input_provider_config(
    root: &Path,
    endpoint: &str,
    voice_mode: &str,
) -> PathBuf {
    let audio_dir = root.join("audio");
    let log_dir = root.join("logs");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        format!(
            r#"
voice_mode = "{voice_mode}"

[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "{audio_dir}"

[provider]
kind = "openai_compatible"
transcription_transport = "chat_completions_audio_input"
audio_transcriptions_endpoint = "{endpoint}"
chat_completions_endpoint = "{endpoint}"
transcription_model = "qwen3-asr-flash"
chat_model = "qwen3.7-plus"
api_key = "talk-test-key"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "{log_dir}"
"#,
            voice_mode = voice_mode,
            audio_dir = toml_path(&audio_dir),
            endpoint = endpoint,
            log_dir = toml_path(&log_dir)
        ),
    )
    .expect("write openai-compatible chat audio input config");
    config_path
}

fn spawn_http_provider() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
    listener
        .set_nonblocking(false)
        .expect("mock provider blocking");
    let endpoint = format!(
        "http://{}/provider",
        listener.local_addr().expect("mock provider addr")
    );
    let handle = thread::spawn(move || {
        let mut bodies = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .expect("provider stream read timeout");
            let body = read_http_body(&mut stream);
            let request_index = bodies.len();
            bodies.push(body.clone());
            let text = if request_index == 0 {
                "transcribed via http"
            } else {
                "processed via http"
            };
            write_json_response(&mut stream, &json!({ "text": text }).to_string());
        }
        bodies
    });
    (endpoint, handle)
}

fn spawn_failing_http_transcriber() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind failing provider");
    listener
        .set_nonblocking(false)
        .expect("failing provider blocking");
    let endpoint = format!(
        "http://{}/provider",
        listener.local_addr().expect("failing provider addr")
    );
    let handle = thread::spawn(move || {
        let mut bodies = Vec::new();
        if let Ok((mut stream, _)) = listener.accept() {
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .expect("failing provider stream read timeout");
            bodies.push(read_http_body(&mut stream));
            write_http_response(&mut stream, 500, r#"{"error":"transcribe failed"}"#);
        }
        bodies
    });
    (endpoint, handle)
}

fn spawn_openai_compatible_provider() -> (
    String,
    String,
    thread::JoinHandle<String>,
    thread::JoinHandle<String>,
) {
    let transcription_listener =
        TcpListener::bind("127.0.0.1:0").expect("bind openai-compatible transcription provider");
    transcription_listener
        .set_nonblocking(false)
        .expect("transcription provider blocking");
    let transcription_endpoint = format!(
        "http://{}/v1/audio/transcriptions",
        transcription_listener
            .local_addr()
            .expect("transcription provider addr")
    );
    let transcription_handle = thread::spawn(move || {
        let (mut stream, _) = transcription_listener
            .accept()
            .expect("accept transcription request");
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("transcription stream read timeout");
        let request = read_http_request(&mut stream);
        let response_body = r#"{"text":"transcribed via openai-compatible"}"#;
        write_http_response(&mut stream, 200, response_body);
        request
    });

    let chat_listener =
        TcpListener::bind("127.0.0.1:0").expect("bind openai-compatible chat provider");
    chat_listener
        .set_nonblocking(false)
        .expect("chat provider blocking");
    let chat_endpoint = format!(
        "http://{}/v1/chat/completions",
        chat_listener.local_addr().expect("chat provider addr")
    );
    let chat_handle = thread::spawn(move || {
        let (mut stream, _) = chat_listener.accept().expect("accept chat request");
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .expect("chat stream read timeout");
        let request = read_http_request(&mut stream);
        let response_body =
            r#"{"choices":[{"message":{"content":"assistant reply from openai-compatible"}}]}"#;
        write_http_response(&mut stream, 200, response_body);
        request
    });

    (
        transcription_endpoint,
        chat_endpoint,
        transcription_handle,
        chat_handle,
    )
}

fn spawn_openai_audio_input_provider() -> (String, thread::JoinHandle<Vec<String>>) {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("bind openai-compatible chat audio provider");
    listener
        .set_nonblocking(false)
        .expect("chat audio provider blocking");
    let endpoint = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().expect("chat audio provider addr")
    );
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().expect("accept chat audio request");
            stream
                .set_read_timeout(Some(Duration::from_secs(30)))
                .expect("chat audio stream read timeout");
            let request = read_http_request(&mut stream);
            requests.push(request.clone());
            let response_body = if request.contains("\"model\":\"qwen3-asr-flash\"") {
                r#"{"choices":[{"message":{"content":"transcribed via audio input chat"}}]}"#
            } else {
                r#"{"choices":[{"message":{"content":"assistant reply from audio input chat"}}]}"#
            };
            write_http_response(&mut stream, 200, response_body);
        }
        requests
    });
    (endpoint, handle)
}

fn read_http_body(stream: &mut TcpStream) -> String {
    read_http_request(stream)
}

fn read_http_request(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut temp = [0u8; 1024];
    let header_end;
    loop {
        let read = stream.read(&mut temp).expect("read provider request");
        assert!(read > 0, "connection closed before headers");
        buffer.extend_from_slice(&temp[..read]);
        if let Some(position) = find_subsequence(&buffer, b"\r\n\r\n") {
            header_end = position + 4;
            break;
        }
    }

    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            line.strip_prefix("content-length:")
                .or_else(|| line.strip_prefix("Content-Length:"))
        })
        .expect("content-length header")
        .trim()
        .parse::<usize>()
        .expect("content length number");

    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut temp).expect("read provider body");
        assert!(read > 0, "connection closed before body");
        buffer.extend_from_slice(&temp[..read]);
    }

    String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).to_string()
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn write_json_response(stream: &mut TcpStream, body: &str) {
    write_http_response(stream, 200, body);
}

fn write_http_response(stream: &mut TcpStream, status: u16, body: &str) {
    let status_text = match status {
        200 => "OK",
        500 => "Internal Server Error",
        _ => "Unknown",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {status_text}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .expect("write http response");
}

fn wait_for_manifest(manifest_path: &Path) -> serde_json::Value {
    wait_for_manifest_with_timeout(manifest_path, Duration::from_secs(10))
}

fn wait_for_manifest_with_timeout(manifest_path: &Path, timeout: Duration) -> serde_json::Value {
    let deadline = Instant::now() + timeout;
    loop {
        if manifest_path.exists() {
            let raw = fs::read_to_string(manifest_path).expect("read manifest");
            return serde_json::from_str(&raw).expect("valid manifest json");
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for manifest {}",
            manifest_path.display()
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn spawn_talk_server(root: &Path) -> (Child, serde_json::Value) {
    let config_path = write_test_config(root);
    spawn_talk_server_with_config(root, &config_path)
}

fn spawn_talk_server_with_config(root: &Path, config_path: &Path) -> (Child, serde_json::Value) {
    let manifest_dir = root.join("capabilities");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let exe = env!("CARGO_BIN_EXE_talk");
    let child = Command::new(exe)
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--manifest-dir",
            manifest_dir.to_str().expect("utf8 manifest dir"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .expect("spawn talk server");
    let manifest = wait_for_manifest(&manifest_dir.join("talk.json"));
    (child, manifest)
}

fn shared_local_capability_example_path(name: &str) -> PathBuf {
    let talk_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let standalone_path = talk_root
        .join("contracts")
        .join("local-capability")
        .join("examples")
        .join(name);
    if standalone_path.is_file() {
        return standalone_path;
    }

    talk_root
        .join("..")
        .join("contracts")
        .join("local-capability")
        .join("examples")
        .join(name)
}

fn shared_local_capability_example(name: &str) -> String {
    fs::read_to_string(shared_local_capability_example_path(name))
        .expect("read root local capability fixture")
}

#[test]
fn shared_local_capability_example_prefers_standalone_talk_contracts() {
    let resolved = shared_local_capability_example_path("talk-manifest.json");
    let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("contracts")
        .join("local-capability")
        .join("examples")
        .join("talk-manifest.json");

    assert_eq!(
        fs::canonicalize(resolved).expect("canonical resolved Talk contract fixture"),
        fs::canonicalize(expected).expect("canonical standalone Talk contract fixture")
    );
}

fn stop_child(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn endpoint_from_manifest(manifest: &serde_json::Value) -> (String, u16, String) {
    let base_url = manifest["transport"]["baseUrl"]
        .as_str()
        .expect("manifest transport baseUrl");
    let authority = base_url
        .strip_prefix("http://")
        .expect("http base url")
        .split('/')
        .next()
        .expect("authority");
    let (host, port) = authority.rsplit_once(':').expect("host:port");
    let token = manifest["transport"]["authToken"]
        .as_str()
        .expect("manifest auth token")
        .to_string();
    (host.to_string(), port.parse().expect("port number"), token)
}

fn http_request(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: Option<&str>,
    bearer: Option<&str>,
) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect talk server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set client timeout");
    let body = body.unwrap_or("");
    let auth = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}Connection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write talk request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read talk response");
    response
}

fn http_request_with_raw_authorization(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    body: &str,
    authorization: &str,
) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect talk server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set client timeout");
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAuthorization: {authorization}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .expect("write talk request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read talk response");
    response
}

fn http_request_with_declared_content_length(
    host: &str,
    port: u16,
    method: &str,
    path: &str,
    content_length: usize,
    bearer: Option<&str>,
) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect talk server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set client timeout");
    let auth = bearer
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {content_length}\r\n{auth}Connection: close\r\n\r\n",
    )
    .expect("write talk request");
    stream.shutdown(Shutdown::Write).expect("shutdown write");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read talk response");
    response
}

fn raw_http_request(host: &str, port: u16, request: &str) -> String {
    raw_http_request_bytes(host, port, request.as_bytes())
}

fn raw_http_request_bytes(host: &str, port: u16, request: &[u8]) -> String {
    let mut stream = TcpStream::connect((host, port)).expect("connect talk server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set client timeout");
    stream.write_all(request).expect("write raw talk request");
    stream.shutdown(Shutdown::Write).expect("shutdown write");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read talk response");
    response
}

fn http_body(response: &str) -> &str {
    response.split_once("\r\n\r\n").expect("response body").1
}

#[test]
fn check_command_accepts_dev_config() {
    let exe = env!("CARGO_BIN_EXE_talk");
    let output = Command::new(exe)
        .args(["check", "--config", "../../examples/dev-config.toml"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk check");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("config ok"));
}

#[test]
fn help_text_uses_talk_product_boundary_not_hook_mvp_branding() {
    let exe = env!("CARGO_BIN_EXE_talk");
    let output = Command::new(exe)
        .arg("--help")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk help");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Talk"), "stdout={stdout}");
    assert!(
        !stdout.contains("Hook voice"),
        "Talk CLI help must not describe itself as Hook voice, stdout={stdout}"
    );
    assert!(
        !stdout.contains("Hook voice input MVP"),
        "Talk CLI help must not use old Hook MVP branding, stdout={stdout}"
    );
}

#[test]
fn serve_command_writes_talk_manifest_and_serves_health_and_capabilities() {
    let temp_dir = unique_temp_dir("serve-health-capabilities");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);

    assert_eq!(manifest["schemaVersion"], 1);
    assert_eq!(manifest["appId"], "talk");
    assert_eq!(manifest["displayName"], "Talk");
    assert_eq!(manifest["transport"]["type"], "http");
    assert_eq!(manifest["transport"]["auth"], "bearer");
    assert!(manifest["capabilities"]
        .as_array()
        .expect("capabilities")
        .iter()
        .any(|capability| capability == "voice.capture.once"));

    let health = http_request(&host, port, "GET", "/v1/health", None, None);
    assert!(health.starts_with("HTTP/1.1 200 OK"), "health={health}");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(http_body(&health)).expect("health json")
            ["status"],
        "ready"
    );

    let capabilities = http_request(&host, port, "GET", "/v1/capabilities", None, None);
    assert!(
        capabilities.starts_with("HTTP/1.1 200 OK"),
        "capabilities={capabilities}"
    );
    let capabilities_json: serde_json::Value =
        serde_json::from_str(http_body(&capabilities)).expect("capabilities json");
    assert_eq!(capabilities_json["appId"], "talk");
    assert!(
        capabilities_json.get("authToken").is_none(),
        "public capabilities response must not leak authToken"
    );
    assert!(
        capabilities_json.get("transport").is_none(),
        "public capabilities response must not leak manifest transport"
    );
    assert!(capabilities_json["capabilities"]
        .as_array()
        .expect("capability array")
        .iter()
        .any(|capability| capability["id"] == "voice.capture.once"));

    stop_child(child);
}

#[test]
fn root_contract_talk_manifest_fixture_matches_current_invokable_capabilities() {
    let fixture: serde_json::Value =
        serde_json::from_str(&shared_local_capability_example("talk-manifest.json"))
            .expect("root contract Talk manifest fixture json");
    let capabilities = fixture["capabilities"]
        .as_array()
        .expect("fixture capabilities")
        .iter()
        .map(|capability| capability.as_str().expect("capability string").to_owned())
        .collect::<Vec<_>>();

    assert_eq!(fixture["schemaVersion"], 1);
    assert_eq!(fixture["appId"], "talk");
    assert_eq!(fixture["displayName"], "Talk");
    assert_eq!(fixture["transport"]["type"], "http");
    assert_eq!(fixture["transport"]["auth"], "bearer");
    assert!(
        fixture["transport"]["authToken"]
            .as_str()
            .expect("fixture auth token")
            .trim()
            .len()
            > 0
    );
    assert_eq!(
        capabilities,
        vec!["voice.capture.once".to_owned(), "voice.dictate".to_owned()]
    );
}

#[test]
fn serve_command_only_advertises_invokable_capabilities() {
    let temp_dir = unique_temp_dir("serve-advertised-capabilities");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);

    let manifest_capabilities = manifest["capabilities"]
        .as_array()
        .expect("manifest capabilities")
        .iter()
        .map(|capability| capability.as_str().expect("capability string").to_string())
        .collect::<Vec<_>>();

    let capabilities = http_request(&host, port, "GET", "/v1/capabilities", None, None);
    assert!(
        capabilities.starts_with("HTTP/1.1 200 OK"),
        "capabilities={capabilities}"
    );
    let capabilities_json: serde_json::Value =
        serde_json::from_str(http_body(&capabilities)).expect("capabilities json");
    let advertised_ids = capabilities_json["capabilities"]
        .as_array()
        .expect("capability array")
        .iter()
        .map(|capability| capability["id"].as_str().expect("id string").to_string())
        .collect::<Vec<_>>();

    let mut invoke_results = Vec::new();
    for capability in advertised_ids {
        let request = format!(
            r#"{{
                "requestId": "advertised-{capability}",
                "caller": "hook",
                "capability": "{capability}",
                "input": {{"mode": "dictation"}}
            }}"#
        );
        let response = http_request(
            &host,
            port,
            "POST",
            "/v1/invoke",
            Some(&request),
            Some(&token),
        );
        let json: serde_json::Value =
            serde_json::from_str(http_body(&response)).expect("invoke json");
        invoke_results.push((capability, response, json));
    }

    stop_child(child);

    assert_eq!(
        manifest_capabilities,
        vec![
            "voice.capture.once".to_string(),
            "voice.dictate".to_string()
        ]
    );
    assert_eq!(
        invoke_results
            .iter()
            .map(|(capability, _, _)| capability.clone())
            .collect::<Vec<_>>(),
        manifest_capabilities
    );
    for (capability, response, json) in invoke_results {
        assert!(
            response.starts_with("HTTP/1.1 200 OK"),
            "response={response}"
        );
        assert_ne!(
            json["error"]["code"], "unknown_capability",
            "advertised capability {capability} returned unknown_capability"
        );
    }
}

#[test]
fn serve_command_rejects_malformed_local_capability_envelopes() {
    let temp_dir = unique_temp_dir("serve-invalid-envelope");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);

    let invalid_requests = [
        (
            "empty-request-id",
            r#"{
                "requestId": "",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::Null,
            "requestId is required",
        ),
        (
            "request-id-with-whitespace",
            r#"{
                "requestId": " request-whitespace-1 ",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::Null,
            "requestId must not have leading or trailing whitespace",
        ),
        (
            "request-id-with-embedded-whitespace",
            r#"{
                "requestId": "request with space",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::Null,
            "requestId must not contain whitespace",
        ),
        (
            "empty-caller",
            r#"{
                "requestId": "invalid-caller-1",
                "caller": "",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::String("invalid-caller-1".to_string()),
            "caller is required",
        ),
        (
            "caller-with-whitespace",
            r#"{
                "requestId": "invalid-caller-1b",
                "caller": " hook ",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::String("invalid-caller-1b".to_string()),
            "caller must not have leading or trailing whitespace",
        ),
        (
            "unknown-caller",
            r#"{
                "requestId": "invalid-caller-2",
                "caller": "rogue",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::String("invalid-caller-2".to_string()),
            "caller is invalid",
        ),
        (
            "capability-with-whitespace",
            r#"{
                "requestId": "capability-whitespace-1",
                "caller": "hook",
                "capability": " voice.capture.once ",
                "input": {}
            }"#,
            serde_json::Value::String("capability-whitespace-1".to_string()),
            "capability must not have leading or trailing whitespace",
        ),
        (
            "malformed-capability-id",
            r#"{
                "requestId": "malformed-capability-1",
                "caller": "hook",
                "capability": "voice capture once",
                "input": {}
            }"#,
            serde_json::Value::String("malformed-capability-1".to_string()),
            "capability must be a dot-separated id",
        ),
        (
            "missing-input",
            r#"{
                "requestId": "missing-input-1",
                "caller": "hook",
                "capability": "voice.capture.once"
            }"#,
            serde_json::Value::String("missing-input-1".to_string()),
            "input is required",
        ),
        (
            "non-object-input",
            r#"{
                "requestId": "non-object-input-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": "not-an-object"
            }"#,
            serde_json::Value::String("non-object-input-1".to_string()),
            "input must be an object",
        ),
        (
            "invalid-mode-type",
            r#"{
                "requestId": "invalid-mode-type-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mode": 123 }
            }"#,
            serde_json::Value::String("invalid-mode-type-1".to_string()),
            "input.mode must be a string",
        ),
        (
            "blank-mode",
            r#"{
                "requestId": "blank-mode-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mode": "   " }
            }"#,
            serde_json::Value::String("blank-mode-1".to_string()),
            "input.mode must not be blank",
        ),
        (
            "mode-with-surrounding-whitespace",
            r#"{
                "requestId": "mode-whitespace-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mode": " translate " }
            }"#,
            serde_json::Value::String("mode-whitespace-1".to_string()),
            "input.mode must not have leading or trailing whitespace",
        ),
        (
            "unsupported-mode",
            r#"{
                "requestId": "unsupported-mode-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mode": "summarize" }
            }"#,
            serde_json::Value::String("unsupported-mode-1".to_string()),
            "input.mode is not supported: summarize",
        ),
        (
            "invalid-context",
            r#"{
                "requestId": "invalid-context-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "context": "not-an-object" }
            }"#,
            serde_json::Value::String("invalid-context-1".to_string()),
            "input.context is invalid",
        ),
        (
            "blank-context-source",
            r#"{
                "requestId": "blank-context-source-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "context": { "source": "   " } }
            }"#,
            serde_json::Value::String("blank-context-source-1".to_string()),
            "input.context.source must not be blank",
        ),
        (
            "context-source-with-surrounding-whitespace",
            r#"{
                "requestId": "context-source-whitespace-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "context": { "source": " hook-panel " } }
            }"#,
            serde_json::Value::String("context-source-whitespace-1".to_string()),
            "input.context.source must not have leading or trailing whitespace",
        ),
        (
            "blank-context-app-name",
            r#"{
                "requestId": "blank-context-app-name-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "context": { "appName": "   " } }
            }"#,
            serde_json::Value::String("blank-context-app-name-1".to_string()),
            "input.context.appName must not be blank",
        ),
        (
            "context-window-title-with-surrounding-whitespace",
            r#"{
                "requestId": "context-window-title-whitespace-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "context": { "windowTitle": " Neuro editor " } }
            }"#,
            serde_json::Value::String("context-window-title-whitespace-1".to_string()),
            "input.context.windowTitle must not have leading or trailing whitespace",
        ),
        (
            "blank-mock-text",
            r#"{
                "requestId": "blank-mock-text-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mockText": "   " }
            }"#,
            serde_json::Value::String("blank-mock-text-1".to_string()),
            "input.mockText must not be blank",
        ),
        (
            "mock-text-with-surrounding-whitespace",
            r#"{
                "requestId": "mock-text-whitespace-1",
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": { "mockText": " hello " }
            }"#,
            serde_json::Value::String("mock-text-whitespace-1".to_string()),
            "input.mockText must not have leading or trailing whitespace",
        ),
    ];

    let mut responses = Vec::new();
    for (case, request, expected_request_id, expected_message) in invalid_requests {
        let response = http_request(
            &host,
            port,
            "POST",
            "/v1/invoke",
            Some(request),
            Some(&token),
        );
        let json: serde_json::Value =
            serde_json::from_str(http_body(&response)).expect("invalid response json");
        responses.push((case, response, json, expected_request_id, expected_message));
    }

    stop_child(child);

    for (case, response, json, expected_request_id, expected_message) in responses {
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "case={case} response={response}"
        );
        assert_eq!(json["requestId"], expected_request_id, "case={case}");
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error"]["code"], "invalid_request");
        assert_eq!(json["error"]["message"], expected_message, "case={case}");
    }
}

#[test]
fn serve_command_rejects_non_string_local_capability_envelope_fields() {
    let temp_dir = unique_temp_dir("serve-invalid-envelope-types");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);

    let invalid_requests = [
        (
            "request-id-type",
            r#"{
                "requestId": 123,
                "caller": "hook",
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::Null,
            "requestId must be a string",
        ),
        (
            "caller-type",
            r#"{
                "requestId": "caller-type-1",
                "caller": 123,
                "capability": "voice.capture.once",
                "input": {}
            }"#,
            serde_json::Value::String("caller-type-1".to_string()),
            "caller must be a string",
        ),
        (
            "capability-type",
            r#"{
                "requestId": "capability-type-1",
                "caller": "hook",
                "capability": ["voice.capture.once"],
                "input": {}
            }"#,
            serde_json::Value::String("capability-type-1".to_string()),
            "capability must be a string",
        ),
    ];

    let mut responses = Vec::new();
    for (case, request, expected_request_id, expected_message) in invalid_requests {
        let response = http_request(
            &host,
            port,
            "POST",
            "/v1/invoke",
            Some(request),
            Some(&token),
        );
        let json: serde_json::Value =
            serde_json::from_str(http_body(&response)).expect("invalid field type response json");
        responses.push((case, response, json, expected_request_id, expected_message));
    }

    stop_child(child);

    for (case, response, json, expected_request_id, expected_message) in responses {
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request"),
            "case={case} response={response}"
        );
        assert_eq!(json["requestId"], expected_request_id, "case={case}");
        assert_eq!(json["status"], "failed", "case={case}");
        assert_eq!(json["error"]["code"], "invalid_request", "case={case}");
        assert_eq!(json["error"]["message"], expected_message, "case={case}");
    }
}

#[test]
fn serve_command_rejects_malformed_http_request_line_before_invoke() {
    let temp_dir = unique_temp_dir("serve-malformed-request-line");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "malformed-line-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "malformed HTTP request line must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("malformed request line response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_unsupported_http_version_before_invoke() {
    let temp_dir = unique_temp_dir("serve-unsupported-http-version");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "unsupported-http-version-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/2.0\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "unsupported HTTP version must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("unsupported HTTP version response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_request_target_without_leading_slash_before_routing() {
    let temp_dir = unique_temp_dir("serve-target-without-leading-slash");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "target-without-slash-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "request target without leading slash must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("request target response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_request_target_with_fragment_before_routing() {
    let temp_dir = unique_temp_dir("serve-target-with-fragment");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health#fragment HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "request target fragment must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("fragment target response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_invalid_http_method_token_before_routing() {
    let temp_dir = unique_temp_dir("serve-invalid-method-token");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request =
        format!("G@T /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "invalid HTTP method token must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("invalid method response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_http_1_1_request_without_host_before_routing() {
    let temp_dir = unique_temp_dir("serve-http11-missing-host");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = "GET /v1/health HTTP/1.1\r\nConnection: close\r\n\r\n";

    let response = raw_http_request(&host, port, request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "HTTP/1.1 request without Host must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("missing host response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_lowercase_http_1_1_without_host_before_routing() {
    let temp_dir = unique_temp_dir("serve-lowercase-http11-missing-host");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = "GET /v1/health http/1.1\r\nConnection: close\r\n\r\n";

    let response = raw_http_request(&host, port, request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "lowercase HTTP/1.1 without Host must not bypass Host validation, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("lowercase missing host response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_http_1_1_host_header_with_multiple_values_before_routing() {
    let temp_dir = unique_temp_dir("serve-http11-multi-host-value");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}, rogue.example\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "HTTP/1.1 request with multiple Host values must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("multi host response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_http_1_1_host_header_with_embedded_whitespace_before_routing() {
    let temp_dir = unique_temp_dir("serve-http11-host-whitespace");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port} rogue.example\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "HTTP/1.1 request with whitespace in Host value must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("host whitespace response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_non_json_content_type_before_invoke() {
    let temp_dir = unique_temp_dir("serve-non-json-content-type");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "content-type-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: text/plain\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "non-JSON Content-Type must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("non json content-type response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_duplicate_content_type_before_invoke() {
    let temp_dir = unique_temp_dir("serve-duplicate-content-type");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "content-type-duplicate-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Type: text/plain\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "duplicate Content-Type must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("duplicate content-type response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_comma_combined_content_type_before_invoke() {
    let temp_dir = unique_temp_dir("serve-comma-content-type");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "content-type-comma-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json; charset=utf-8, text/plain\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "comma-combined Content-Type must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("comma content-type response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_missing_content_length_before_invoke() {
    let temp_dir = unique_temp_dir("serve-missing-content-length");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "missing-content-length-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n{body}"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "missing Content-Length must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("missing content-length response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_duplicate_authorization_before_invoke() {
    let temp_dir = unique_temp_dir("serve-duplicate-authorization");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "duplicate-authorization-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nAuthorization: Bearer rogue\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "duplicate Authorization must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("duplicate authorization response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_comma_combined_authorization_before_invoke() {
    let temp_dir = unique_temp_dir("serve-comma-authorization");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "authorization-comma-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}, Bearer rogue\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "comma-combined Authorization must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("comma authorization response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_invalid_content_length_before_routing() {
    let temp_dir = unique_temp_dir("serve-invalid-content-length");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: nope\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "invalid Content-Length must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("invalid content-length response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_signed_content_length_before_routing() {
    let temp_dir = unique_temp_dir("serve-signed-content-length");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: +0\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "signed Content-Length must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("signed content-length response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_duplicate_content_length_before_routing() {
    let temp_dir = unique_temp_dir("serve-duplicate-content-length");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "duplicate Content-Length must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("duplicate content-length response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_get_request_body_before_routing() {
    let temp_dir = unique_temp_dir("serve-get-body");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let body = "unexpected";
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "GET request body must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("GET body response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_truncated_declared_request_body_before_invoke() {
    let temp_dir = unique_temp_dir("serve-truncated-body");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "truncated-body-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len() + 5
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "truncated declared body must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("truncated body response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_body_larger_than_declared_content_length_before_invoke() {
    let temp_dir = unique_temp_dir("serve-oversent-body");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "oversent-body-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n{body}"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "body larger than declared Content-Length must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("oversent body response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_incomplete_http_headers_before_routing() {
    let temp_dir = unique_temp_dir("serve-incomplete-headers");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!("GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\n");

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "incomplete HTTP headers must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("incomplete headers response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_transfer_encoding_before_routing() {
    let temp_dir = unique_temp_dir("serve-transfer-encoding");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let request = format!(
        "GET /v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "transfer-encoding must be rejected before routing, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("transfer-encoding response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_non_utf8_request_before_invoke() {
    let temp_dir = unique_temp_dir("serve-non-utf8-request");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let mut body = br#"{
        "requestId": "non-utf8-"#
        .to_vec();
    body.push(0xff);
    body.extend_from_slice(
        br#"",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#,
    );
    let mut request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    request.extend_from_slice(&body);

    let response = raw_http_request_bytes(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "non-UTF-8 request must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("non-UTF-8 response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
    assert_eq!(json["error"]["message"], "HTTP request must be UTF-8");
}

#[test]
fn serve_command_rejects_malformed_http_header_line_before_invoke() {
    let temp_dir = unique_temp_dir("serve-malformed-header-line");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "malformed-header-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nMalformed-Header-Line\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "malformed header line must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("malformed header line response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_empty_http_header_name_before_invoke() {
    let temp_dir = unique_temp_dir("serve-empty-header-name");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "empty-header-name-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\n: missing-name\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "empty HTTP header name must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("empty header name response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_http_header_name_whitespace_before_invoke() {
    let temp_dir = unique_temp_dir("serve-header-name-whitespace");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "header-name-whitespace-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type : application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "HTTP header name whitespace must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("header name whitespace response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_invalid_http_header_name_token_before_invoke() {
    let temp_dir = unique_temp_dir("serve-invalid-header-name-token");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let body = r#"{
        "requestId": "invalid-header-token-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": { "mode": "dictation" }
    }"#;
    let request = format!(
        "POST /v1/invoke HTTP/1.1\r\nHost: {host}:{port}\r\nBad Header: ignored\r\nContent-Type: application/json\r\nAuthorization: Bearer {token}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    let response = raw_http_request(&host, port, &request);

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "invalid HTTP header name token must be rejected before invoke, response={response}"
    );
    let json: serde_json::Value =
        serde_json::from_str(http_body(&response)).expect("invalid header token response json");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
}

#[test]
fn serve_command_rejects_oversized_declared_request_body() {
    let temp_dir = unique_temp_dir("serve-oversized-body");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);

    let response = http_request_with_declared_content_length(
        &host,
        port,
        "POST",
        "/v1/invoke",
        2 * 1024 * 1024,
        Some(&token),
    );

    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "response={response}"
    );
}

#[test]
fn serve_command_uses_stable_appdata_manifest_dir_by_default() {
    let temp_dir = unique_temp_dir("serve-default-manifest-dir");
    let config_path = write_test_config(&temp_dir);
    let appdata = temp_dir.join("appdata");
    fs::create_dir_all(&appdata).expect("create appdata dir");
    let manifest_path = appdata.join("Neuro").join("capabilities").join("talk.json");
    let exe = env!("CARGO_BIN_EXE_talk");
    let child = Command::new(exe)
        .env("APPDATA", &appdata)
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .expect("spawn talk server with default manifest dir");

    let manifest = wait_for_manifest_with_timeout(&manifest_path, Duration::from_secs(2));

    assert_eq!(manifest["appId"], "talk");
    assert_eq!(
        manifest["transport"]["baseUrl"]
            .as_str()
            .unwrap_or_default()
            .starts_with("http://127.0.0.1:"),
        true
    );

    stop_child(child);
}

#[test]
fn serve_command_uses_runtime_manifest_dir_when_appdata_is_blank() {
    let temp_dir = unique_temp_dir("serve-blank-appdata-manifest-dir");
    let config_path = write_test_config(&temp_dir);
    let manifest_path = temp_dir
        .join(".runtime")
        .join("neuro")
        .join("capabilities")
        .join("talk.json");
    let exe = env!("CARGO_BIN_EXE_talk");
    let child = Command::new(exe)
        .env("APPDATA", "   ")
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "127.0.0.1",
            "--port",
            "0",
        ])
        .current_dir(&temp_dir)
        .spawn()
        .expect("spawn talk server with blank APPDATA");

    let manifest = wait_for_manifest_with_timeout(&manifest_path, Duration::from_secs(2));

    assert_eq!(manifest["appId"], "talk");
    assert!(
        manifest["transport"]["baseUrl"]
            .as_str()
            .unwrap_or_default()
            .starts_with("http://127.0.0.1:"),
        "manifest={manifest}"
    );

    stop_child(child);
}

#[test]
fn serve_command_accepts_ipv6_loopback_host_and_serves_health() {
    let temp_dir = unique_temp_dir("serve-ipv6-loopback");
    let config_path = write_test_config(&temp_dir);
    let manifest_dir = temp_dir.join("capabilities");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let exe = env!("CARGO_BIN_EXE_talk");
    let child = Command::new(exe)
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "::1",
            "--port",
            "0",
            "--manifest-dir",
            manifest_dir.to_str().expect("utf8 manifest dir"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .expect("spawn ipv6 loopback talk server");

    let manifest =
        wait_for_manifest_with_timeout(&manifest_dir.join("talk.json"), Duration::from_secs(2));
    assert_eq!(
        manifest["transport"]["baseUrl"]
            .as_str()
            .unwrap_or_default()
            .starts_with("http://["),
        true
    );

    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let health = http_request(&host, port, "GET", "/v1/health", None, None);
    assert!(health.starts_with("HTTP/1.1 200 OK"), "health={health}");

    stop_child(child);
}

#[test]
fn serve_command_accepts_bracketed_ipv6_loopback_host_and_serves_health() {
    let temp_dir = unique_temp_dir("serve-bracketed-ipv6-loopback");
    let config_path = write_test_config(&temp_dir);
    let manifest_dir = temp_dir.join("capabilities");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let exe = env!("CARGO_BIN_EXE_talk");
    let child = Command::new(exe)
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "[::1]",
            "--port",
            "0",
            "--manifest-dir",
            manifest_dir.to_str().expect("utf8 manifest dir"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .expect("spawn bracketed ipv6 loopback talk server");

    let manifest =
        wait_for_manifest_with_timeout(&manifest_dir.join("talk.json"), Duration::from_secs(2));
    assert_eq!(
        manifest["transport"]["baseUrl"]
            .as_str()
            .unwrap_or_default()
            .starts_with("http://["),
        true
    );

    let (host, port, _token) = endpoint_from_manifest(&manifest);
    let health = http_request(&host, port, "GET", "/v1/health", None, None);
    assert!(health.starts_with("HTTP/1.1 200 OK"), "health={health}");

    stop_child(child);
}

#[test]
fn serve_command_rejects_non_loopback_hosts_before_writing_manifest() {
    let temp_dir = unique_temp_dir("serve-non-loopback");
    let config_path = write_test_config(&temp_dir);
    let manifest_dir = temp_dir.join("capabilities");
    fs::create_dir_all(&manifest_dir).expect("create manifest dir");
    let manifest_path = manifest_dir.join("talk.json");
    let exe = env!("CARGO_BIN_EXE_talk");
    let mut child = Command::new(exe)
        .args([
            "serve",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--host",
            "0.0.0.0",
            "--port",
            "0",
            "--manifest-dir",
            manifest_dir.to_str().expect("utf8 manifest dir"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .spawn()
        .expect("spawn non-loopback talk server");

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if manifest_path.exists() {
            stop_child(child);
            panic!("non-loopback serve wrote a manifest before rejecting host");
        }
        if let Some(status) = child.try_wait().expect("poll child") {
            assert!(
                !status.success(),
                "non-loopback serve unexpectedly succeeded"
            );
            return;
        }
        if Instant::now() >= deadline {
            stop_child(child);
            panic!("non-loopback serve did not fail within deadline");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn serve_command_invokes_voice_capture_once_with_bearer_auth() {
    let temp_dir = unique_temp_dir("serve-invoke");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = r#"{
        "requestId": "invoke-test-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": {
            "mode": "dictation",
            "context": { "source": "hook-panel" }
        }
    }"#;

    let unauthorized = http_request(&host, port, "POST", "/v1/invoke", Some(request), None);
    assert!(
        unauthorized.starts_with("HTTP/1.1 401 Unauthorized"),
        "unauthorized={unauthorized}"
    );

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(request),
        Some(&token),
    );
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "invoke-test-1");
    assert_eq!(json["status"], "succeeded");
    assert_eq!(json["output"]["text"], "hello from config");
    assert_eq!(json["output"]["transcript"], "hello from config");
    let evidence_path = PathBuf::from(
        json["output"]["evidencePath"]
            .as_str()
            .expect("evidence path"),
    );
    assert!(evidence_path.exists(), "evidence_path={evidence_path:?}");

    let unknown_request = r#"{
        "requestId": "invoke-test-2",
        "caller": "hook",
        "capability": "voice.unknown",
        "input": {}
    }"#;
    let unknown = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(unknown_request),
        Some(&token),
    );
    assert!(unknown.starts_with("HTTP/1.1 200 OK"), "unknown={unknown}");
    let unknown_json: serde_json::Value =
        serde_json::from_str(http_body(&unknown)).expect("unknown json");
    assert_eq!(unknown_json["requestId"], "invoke-test-2");
    assert_eq!(unknown_json["status"], "failed");
    assert_eq!(unknown_json["error"]["code"], "unknown_capability");

    let lowercase_bearer = http_request_with_raw_authorization(
        &host,
        port,
        "POST",
        "/v1/invoke",
        request,
        &format!("bearer {token}"),
    );
    assert!(
        lowercase_bearer.starts_with("HTTP/1.1 200 OK"),
        "lowercase_bearer={lowercase_bearer}"
    );

    for (case, authorization) in [
        ("wrong token", format!("Bearer {token}-wrong")),
        ("extra bearer segment", format!("Bearer {token} trailing")),
        ("wrong scheme", format!("Basic {token}")),
        ("missing bearer token", "Bearer".to_string()),
    ] {
        let rejected = http_request_with_raw_authorization(
            &host,
            port,
            "POST",
            "/v1/invoke",
            request,
            &authorization,
        );
        assert!(
            rejected.starts_with("HTTP/1.1 401 Unauthorized"),
            "{case} should be rejected, response={rejected}"
        );
    }

    stop_child(child);
}

#[test]
fn serve_command_accepts_root_contract_local_capability_invoke_fixture() {
    let temp_dir = unique_temp_dir("serve-contract-invoke-fixture");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = shared_local_capability_example("talk-invoke-request.json");

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(&request),
        Some(&token),
    );
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "talk-request-1");
    assert_eq!(json["status"], "succeeded");
    assert_eq!(json["output"]["text"], "hello from talk");
    assert_eq!(json["output"]["transcript"], "hello from talk");
    assert_eq!(json["output"]["caller"], "hook");
    let evidence_path = PathBuf::from(
        json["output"]["evidencePath"]
            .as_str()
            .expect("evidence path"),
    );
    assert!(evidence_path.exists(), "evidence_path={evidence_path:?}");
}

#[test]
fn serve_command_forwards_invoke_mode_and_context_to_http_provider() {
    let temp_dir = unique_temp_dir("serve-http-provider-context");
    let (endpoint, provider_handle) = spawn_http_provider();
    let config_path = write_http_provider_config(&temp_dir, &endpoint);
    let (child, manifest) = spawn_talk_server_with_config(&temp_dir, &config_path);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = json!({
        "requestId": "invoke-http-context-1",
        "caller": "hook",
        "capability": "voice.capture.once",
        "input": {
            "mode": "translate",
            "context": {
                "source": "hook-panel",
                "appName": "Hook",
                "windowTitle": "Neuro editor",
                "selectedText": "hello source text",
                "metadata": {
                    "traceId": "trace-123",
                    "nested": { "enabled": true }
                }
            }
        }
    })
    .to_string();

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(&request),
        Some(&token),
    );
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "invoke-http-context-1");
    assert_eq!(json["status"], "succeeded");
    assert_eq!(json["output"]["text"], "processed via http");
    assert_eq!(json["output"]["transcript"], "transcribed via http");

    let bodies = provider_handle.join().expect("provider thread joins");
    assert_eq!(bodies.len(), 2);
    let transcribe: serde_json::Value =
        serde_json::from_str(&bodies[0]).expect("transcribe request json");
    let process: serde_json::Value =
        serde_json::from_str(&bodies[1]).expect("process request json");

    assert_eq!(transcribe["context"]["source"], "hook-panel");
    assert_eq!(transcribe["context"]["app_name"], "Hook");
    assert_eq!(transcribe["context"]["window_title"], "Neuro editor");
    assert_eq!(transcribe["context"]["selected_text"], "hello source text");
    assert_eq!(transcribe["context"]["metadata"]["traceId"], "trace-123");
    assert_eq!(transcribe["context"]["metadata"]["nested"]["enabled"], true);
    assert_eq!(process["mode"], "translate");
    assert_eq!(process["context"], transcribe["context"]);
}

#[test]
fn serve_command_rejects_non_dictation_mode_for_voice_dictate_capability() {
    let temp_dir = unique_temp_dir("serve-voice-dictate-conflicting-mode");
    let (child, manifest) = spawn_talk_server(&temp_dir);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = json!({
        "requestId": "voice-dictate-mode-conflict-1",
        "caller": "hook",
        "capability": "voice.dictate",
        "input": {
            "mode": "translate"
        }
    })
    .to_string();

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(&request),
        Some(&token),
    );
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 400 Bad Request"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "voice-dictate-mode-conflict-1");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "invalid_request");
    assert_eq!(
        json["error"]["message"],
        "input.mode is not supported for capability voice.dictate"
    );
}

#[test]
fn serve_command_forces_dictation_mode_for_voice_dictate_capability() {
    let temp_dir = unique_temp_dir("serve-voice-dictate-forced-mode");
    let (endpoint, provider_handle) = spawn_http_provider();
    let config_path =
        write_http_provider_config_with_voice_mode(&temp_dir, &endpoint, Some("translate"));
    let (child, manifest) = spawn_talk_server_with_config(&temp_dir, &config_path);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = json!({
        "requestId": "voice-dictate-forced-mode-1",
        "caller": "hook",
        "capability": "voice.dictate",
        "input": {
            "context": { "source": "hook-panel" }
        }
    })
    .to_string();

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(&request),
        Some(&token),
    );
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "voice-dictate-forced-mode-1");
    assert_eq!(json["status"], "succeeded");

    let bodies = provider_handle.join().expect("provider thread joins");
    assert_eq!(bodies.len(), 2);
    let process: serde_json::Value =
        serde_json::from_str(&bodies[1]).expect("process request json");
    assert_eq!(process["mode"], "transcribe");
}

#[test]
fn serve_command_returns_failed_envelope_when_provider_fails() {
    let temp_dir = unique_temp_dir("serve-http-provider-fails");
    let (endpoint, provider_handle) = spawn_failing_http_transcriber();
    let config_path = write_http_provider_config(&temp_dir, &endpoint);
    let (child, manifest) = spawn_talk_server_with_config(&temp_dir, &config_path);
    let (host, port, token) = endpoint_from_manifest(&manifest);
    let request = json!({
        "requestId": "invoke-http-fail-1",
        "caller": "loom",
        "capability": "voice.capture.once",
        "input": {
            "mode": "dictation",
            "context": { "source": "loom-smoke" }
        }
    })
    .to_string();

    let response = http_request(
        &host,
        port,
        "POST",
        "/v1/invoke",
        Some(&request),
        Some(&token),
    );
    stop_child(child);

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "response={response}"
    );
    let json: serde_json::Value = serde_json::from_str(http_body(&response)).expect("invoke json");
    assert_eq!(json["requestId"], "invoke-http-fail-1");
    assert_eq!(json["status"], "failed");
    assert_eq!(json["error"]["code"], "voice_session_failed");
    assert!(json["error"]["message"]
        .as_str()
        .expect("error message")
        .contains("transcriber returned HTTP 500"));
    assert_eq!(json["output"]["caller"], "loom");
    let evidence_path = PathBuf::from(
        json["output"]["evidencePath"]
            .as_str()
            .expect("evidence path"),
    );
    assert!(evidence_path.exists(), "evidence_path={evidence_path:?}");

    let bodies = provider_handle.join().expect("provider thread joins");
    assert_eq!(bodies.len(), 1);
}

#[test]
fn once_command_accepts_mock_text_and_persists_artifacts() {
    let temp_dir = unique_temp_dir("once");
    let config_path = write_test_config(&temp_dir);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "hello cli",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("once ok"), "stdout={stdout}");
    assert!(stdout.contains("text=hello cli"), "stdout={stdout}");

    let log_dir = temp_dir.join("logs");
    let session_files = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    assert_eq!(session_files.len(), 1, "session_files={session_files:?}");

    let raw_json = fs::read_to_string(&session_files[0]).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");
    assert_eq!(json["status"], "completed");
    assert_eq!(json["transcript"], "hello cli");
    assert_eq!(json["output_text"], "hello cli");
    assert_eq!(json["insert_outcome"]["method"], "dry_run");

    let audio_dir = temp_dir.join("audio");
    let audio_files = fs::read_dir(&audio_dir)
        .expect("read audio artifact dir")
        .map(|entry| entry.expect("audio file entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("wav"))
        .collect::<Vec<_>>();
    assert_eq!(audio_files.len(), 1, "audio_files={audio_files:?}");

    let wav_info =
        read_wav_info(&AudioArtifact::new(audio_files[0].clone(), "audio/wav")).expect("read wav");
    assert_eq!(wav_info.sample_rate_hz, 16_000);
    assert_eq!(wav_info.channels, 1);
    assert_eq!(wav_info.bits_per_sample, 16);
    assert!(wav_info.duration_samples > 0);
}

#[test]
fn once_command_rejects_blank_mock_text_before_artifacts() {
    let temp_dir = unique_temp_dir("once-blank-mock-text");
    let config_path = write_test_config(&temp_dir);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "   ",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mock text override must not be blank"),
        "stderr={stderr}"
    );

    assert!(
        !temp_dir.join("audio").exists(),
        "audio artifacts should not be created for blank mock text"
    );
    assert!(
        !temp_dir.join("logs").exists(),
        "session logs should not be created for blank mock text"
    );
}

#[test]
fn once_command_rejects_mock_text_with_surrounding_whitespace_before_artifacts() {
    let temp_dir = unique_temp_dir("once-mock-text-whitespace");
    let config_path = write_test_config(&temp_dir);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            " hello ",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        !output.status.success(),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("mock text override must not have leading or trailing whitespace"),
        "stderr={stderr}"
    );

    assert!(
        !temp_dir.join("audio").exists(),
        "audio artifacts should not be created for malformed mock text"
    );
    assert!(
        !temp_dir.join("logs").exists(),
        "session logs should not be created for malformed mock text"
    );
}

#[test]
fn once_uses_existing_loom_managed_talk_config_when_claimed() {
    let temp_dir = unique_temp_dir("talk-loom-config");
    let config_path = write_test_config(&temp_dir);
    let loom = spawn_fake_loom_talk_config_server("managed transcript", false);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_LOOM_BASE_URL", loom.base_url())
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once with Loom config");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("managed transcript"),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn once_treats_blank_loom_auth_token_as_unset() {
    let temp_dir = unique_temp_dir("talk-blank-loom-auth-token");
    let config_path = write_test_config(&temp_dir);
    let loom = spawn_fake_loom_server_rejecting_authorization("managed transcript without auth");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_LOOM_BASE_URL", loom.base_url())
        .env("TALK_LOOM_AUTH_TOKEN", "   ")
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once with blank Loom auth token");

    assert!(
        output.status.success(),
        "blank TALK_LOOM_AUTH_TOKEN should be treated as unset; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("managed transcript without auth"),
        "stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn readiness_command_reports_json_native_backend_statuses() {
    let temp_dir = unique_temp_dir("native-readiness");
    let config_path = write_test_config_with_output_mode_trigger_clipboard_and_audio_backend(
        &temp_dir,
        "clipboard_paste",
        "toggle",
        Some("native_windows"),
        Some("native_windows"),
    );
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_DISABLE_NATIVE_AUDIO", "1")
        .env("TALK_DISABLE_NATIVE_CLIPBOARD", "1")
        .args([
            "readiness",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk readiness");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("readiness command json");
    assert_eq!(json["app"], "talk");
    assert_eq!(json["allReady"], false);
    assert_eq!(json["audio"]["configuredBackend"], "native_windows");
    assert_eq!(json["audio"]["nativeWindows"]["status"], "unavailable");
    assert_eq!(
        json["audio"]["nativeWindows"]["reason"],
        "native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO"
    );
    let audio_native = json["audio"]["nativeWindows"]
        .as_object()
        .expect("audio native readiness object");
    assert!(audio_native.contains_key("requestedDeviceName"));
    assert!(audio_native.contains_key("deviceName"));
    assert!(audio_native.contains_key("availableDeviceNames"));
    assert!(audio_native.contains_key("defaultSampleRateHz"));
    assert!(audio_native.contains_key("defaultChannels"));
    assert!(audio_native.contains_key("sampleFormat"));
    assert_eq!(json["clipboard"]["configuredBackend"], "native_windows");
    assert_eq!(json["clipboard"]["nativeWindows"]["status"], "unavailable");
    assert_eq!(
        json["clipboard"]["nativeWindows"]["reason"],
        "native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD"
    );
}

#[test]
fn once_command_native_windows_audio_backend_is_not_silent_fallback() {
    let temp_dir = unique_temp_dir("audio-native-windows");
    let config_path = write_test_config_with_output_mode_trigger_clipboard_and_audio_backend(
        &temp_dir,
        "dry_run",
        "toggle",
        None,
        Some("native_windows"),
    );
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_DISABLE_NATIVE_AUDIO", "1")
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "hello native audio",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        !output.status.success(),
        "native audio should require an explicit real backend instead of writing a silent WAV"
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one failed session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "failed");
    assert!(json["transcript"].is_null());
    assert!(json["output_text"].is_null());
    assert!(json["insert_outcome"].is_null());
    assert!(json["error"]
        .as_str()
        .expect("failure reason")
        .contains("native_windows"));

    let audio_dir = temp_dir.join("audio");
    let audio_files = fs::read_dir(&audio_dir)
        .map(|entries| {
            entries
                .map(|entry| entry.expect("audio file entry").path())
                .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("wav"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        audio_files.is_empty(),
        "native audio failure must not be masked by silent wav artifacts: {audio_files:?}"
    );
}

#[test]
fn once_command_uses_clipboard_fallback_when_configured() {
    let temp_dir = unique_temp_dir("clipboard-fallback");
    let config_path = write_test_config_with_output_mode(&temp_dir, "clipboard_paste");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_DISABLE_NATIVE_CLIPBOARD", "1")
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "hello clipboard",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["output_text"], "hello clipboard");
    assert_eq!(json["insert_outcome"]["method"], "clipboard_fallback");
    let fallback_reason = json["insert_outcome"]["reason"]
        .as_str()
        .expect("fallback reason");
    assert!(
        fallback_reason.contains("native clipboard paste is not enabled"),
        "fallback_reason={fallback_reason}"
    );
    assert!(
        !fallback_reason.contains("MVP")
            && !fallback_reason.contains("Hook")
            && !fallback_reason.contains("HookLess"),
        "fallback reason must use current Talk product wording, fallback_reason={fallback_reason}"
    );
}

#[test]
fn once_command_native_windows_clipboard_backend_is_not_silent_fallback() {
    let temp_dir = unique_temp_dir("clipboard-native-windows");
    let config_path = write_test_config_with_output_mode_trigger_and_clipboard_backend(
        &temp_dir,
        "clipboard_paste",
        "toggle",
        Some("native_windows"),
    );
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .env("TALK_DISABLE_NATIVE_CLIPBOARD", "1")
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "hello native clipboard",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        !output.status.success(),
        "native clipboard should require an explicit real backend instead of falling back"
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one failed session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "failed");
    assert_eq!(json["output_text"], "hello native clipboard");
    assert!(json["insert_outcome"].is_null());
    assert!(json["error"]
        .as_str()
        .expect("failure reason")
        .contains("native_windows"));
}

#[test]
fn once_command_records_hotkey_trigger_events() {
    let temp_dir = unique_temp_dir("hotkey-trigger-events");
    let config_path =
        write_test_config_with_output_mode_and_trigger(&temp_dir, "dry_run", "push_to_talk");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--mock-text",
            "hello hotkey",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["trigger_mode"], "push_to_talk");
    assert_eq!(
        json["trigger_events"],
        serde_json::json!(["trigger_start", "trigger_stop"])
    );
    assert_eq!(json["output_text"], "hello hotkey");
}

#[test]
fn check_command_rejects_http_provider_without_endpoint() {
    let temp_dir = unique_temp_dir("http-missing-endpoint");
    let config_path = write_http_provider_config_without_endpoint(&temp_dir);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "check",
            "--config",
            config_path.to_str().expect("config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk check");

    assert!(!output.status.success(), "check unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("provider.endpoint must be set for http provider"),
        "stderr={stderr}"
    );
}

#[test]
fn once_command_uses_http_provider_when_configured() {
    let temp_dir = unique_temp_dir("http-provider");
    let (endpoint, provider_handle) = spawn_http_provider();
    let config_path = write_http_provider_config(&temp_dir, &endpoint);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let bodies = provider_handle.join().expect("provider thread joins");
    assert_eq!(bodies.len(), 2);
    assert!(
        bodies[0].contains("\"audio_path\""),
        "transcribe body={}",
        bodies[0]
    );
    assert!(
        bodies[1].contains("\"transcript\":\"transcribed via http\""),
        "process body={}",
        bodies[1]
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["transcript"], "transcribed via http");
    assert_eq!(json["output_text"], "processed via http");
    assert_eq!(json["insert_outcome"]["method"], "dry_run");
}

#[test]
fn once_command_uses_openai_compatible_provider_when_configured() {
    let temp_dir = unique_temp_dir("openai-compatible-provider");
    let (transcription_endpoint, chat_endpoint, transcription_handle, chat_handle) =
        spawn_openai_compatible_provider();
    let config_path = write_openai_compatible_provider_config(
        &temp_dir,
        &transcription_endpoint,
        &chat_endpoint,
        "command",
    );
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let transcription_request = transcription_handle
        .join()
        .expect("transcription provider thread joins");
    let chat_request = chat_handle.join().expect("chat provider thread joins");

    assert!(
        transcription_request.contains("gpt-4o-mini-transcribe"),
        "transcription request={transcription_request}"
    );
    assert!(
        chat_request.contains("\"model\":\"gpt-4o-mini\""),
        "chat request={chat_request}"
    );
    assert!(
        chat_request.contains("\"role\":\"system\""),
        "chat request={chat_request}"
    );
    assert!(
        chat_request.contains("\"role\":\"user\""),
        "chat request={chat_request}"
    );
    assert!(
        chat_request.contains("transcribed via openai-compatible"),
        "chat request={chat_request}"
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["transcript"], "transcribed via openai-compatible");
    assert_eq!(
        json["output_text"],
        "assistant reply from openai-compatible"
    );
    assert_eq!(json["insert_outcome"]["method"], "dry_run");
}

#[test]
fn once_command_uses_openai_compatible_chat_audio_input_transcriber_when_configured() {
    let temp_dir = unique_temp_dir("openai-chat-audio-input-provider");
    let (endpoint, handle) = spawn_openai_audio_input_provider();
    let config_path =
        write_openai_compatible_chat_audio_input_provider_config(&temp_dir, &endpoint, "command");
    let explicit_audio_path = temp_dir.join("fixtures").join("spoken.wav");
    let explicit_audio = AudioArtifact::new(explicit_audio_path.clone(), "audio/wav");
    write_silent_wav(&explicit_audio, WavSettings::mono_16khz(), 320)
        .expect("write explicit audio file");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--audio-file",
            explicit_audio_path.to_str().expect("utf8 audio path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let requests = handle.join().expect("provider thread joins");
    assert_eq!(requests.len(), 2);
    assert!(
        requests[0].contains("\"model\":\"qwen3-asr-flash\""),
        "transcription request={}",
        requests[0]
    );
    assert!(
        requests[0].contains("\"type\":\"input_audio\""),
        "transcription request={}",
        requests[0]
    );
    assert!(
        requests[0].contains("data:audio/wav;base64,"),
        "transcription request={}",
        requests[0]
    );
    assert!(
        requests[1].contains("\"model\":\"qwen3.7-plus\""),
        "chat request={}",
        requests[1]
    );
    assert!(
        requests[1].contains("transcribed via audio input chat"),
        "chat request={}",
        requests[1]
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["transcript"], "transcribed via audio input chat");
    assert_eq!(json["output_text"], "assistant reply from audio input chat");
    assert_eq!(json["insert_outcome"]["method"], "dry_run");
}

#[test]
fn once_command_uses_explicit_audio_file_without_capturing_new_audio() {
    let temp_dir = unique_temp_dir("explicit-audio-file");
    let (transcription_endpoint, chat_endpoint, transcription_handle, chat_handle) =
        spawn_openai_compatible_provider();
    let config_path = write_openai_compatible_provider_config(
        &temp_dir,
        &transcription_endpoint,
        &chat_endpoint,
        "command",
    );
    let explicit_audio_path = temp_dir.join("fixtures").join("spoken.wav");
    let explicit_audio = AudioArtifact::new(explicit_audio_path.clone(), "audio/wav");
    write_silent_wav(&explicit_audio, WavSettings::mono_16khz(), 320)
        .expect("write explicit audio file");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--audio-file",
            explicit_audio_path.to_str().expect("utf8 audio path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once with explicit audio file");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let transcription_request = transcription_handle
        .join()
        .expect("transcription provider thread joins");
    let chat_request = chat_handle.join().expect("chat provider thread joins");

    assert!(
        transcription_request.contains("filename=\"spoken.wav\""),
        "transcription request={transcription_request}"
    );
    assert!(
        chat_request.contains("transcribed via openai-compatible"),
        "chat request={chat_request}"
    );
    assert!(
        !temp_dir.join("audio").exists(),
        "capture temp dir should not be created when using explicit audio file"
    );

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "completed");
    assert_eq!(json["transcript"], "transcribed via openai-compatible");
    assert_eq!(
        json["output_text"],
        "assistant reply from openai-compatible"
    );
}

#[test]
fn once_command_rejects_missing_audio_file_before_provider_calls() {
    let temp_dir = unique_temp_dir("missing-audio-file");
    let config_path = write_test_config(&temp_dir);
    let missing_audio_path = temp_dir.join("fixtures").join("missing.wav");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--audio-file",
            missing_audio_path
                .to_str()
                .expect("utf8 missing audio path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once with missing audio file");

    assert!(!output.status.success(), "once unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("audio file does not exist"),
        "stderr={stderr}"
    );
    assert!(
        !temp_dir.join("logs").exists(),
        "session log dir should not be created when explicit audio file is invalid"
    );
    assert!(
        !temp_dir.join("audio").exists(),
        "capture temp dir should not be created when explicit audio file is invalid"
    );
}

#[test]
fn play_wav_command_rejects_missing_audio_file_before_playback() {
    let temp_dir = unique_temp_dir("play-wav-missing-file");
    let missing_audio_path = temp_dir.join("fixtures").join("missing.wav");
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "play-wav",
            "--file",
            missing_audio_path.to_str().expect("utf8 audio path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk play-wav");

    assert!(
        !output.status.success(),
        "play-wav should fail for missing audio file, stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("audio file does not exist"),
        "stderr={stderr}"
    );
}

#[test]
fn probe_audio_command_reports_json_signal_metrics_for_silent_backend() {
    let temp_dir = unique_temp_dir("probe-audio-silent");
    let config_path = write_test_config(&temp_dir);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "probe-audio",
            "--config",
            config_path.to_str().expect("utf8 config path"),
            "--seconds",
            "2",
            "--json",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk probe-audio");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("probe-audio command json");
    assert_eq!(json["app"], "talk");
    assert_eq!(json["requestedDurationSeconds"], 2);
    assert_eq!(json["audio"]["configuredBackend"], "silent");
    assert_eq!(json["audio"]["signal"]["sampleRateHz"], 16_000);
    assert_eq!(json["audio"]["signal"]["channels"], 1);
    assert_eq!(json["audio"]["signal"]["durationSeconds"], 2.0);
    assert_eq!(json["audio"]["signal"]["peak"], 0.0);
    assert_eq!(json["audio"]["signal"]["rms"], 0.0);
    assert_eq!(json["audio"]["signal"]["silent"], true);
    assert!(json["audio"]["signal"]["artifactPath"]
        .as_str()
        .expect("artifactPath")
        .ends_with(".wav"));
}

#[test]
fn once_command_persists_failed_session_when_provider_fails() {
    let temp_dir = unique_temp_dir("http-provider-fails");
    let (endpoint, provider_handle) = spawn_failing_http_transcriber();
    let config_path = write_http_provider_config(&temp_dir, &endpoint);
    let exe = env!("CARGO_BIN_EXE_talk");

    let output = Command::new(exe)
        .args([
            "once",
            "--config",
            config_path.to_str().expect("utf8 config path"),
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("run talk once");

    assert!(!output.status.success(), "once unexpectedly succeeded");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("transcriber returned HTTP 500"),
        "stderr={stderr}"
    );

    let bodies = provider_handle.join().expect("provider thread joins");
    assert_eq!(bodies.len(), 1);

    let log_dir = temp_dir.join("logs");
    let session_file = fs::read_dir(&log_dir)
        .expect("read session log dir")
        .map(|entry| entry.expect("session file entry").path())
        .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .expect("one failed session json");
    let raw_json = fs::read_to_string(&session_file).expect("read session json");
    let json: serde_json::Value = serde_json::from_str(&raw_json).expect("valid session json");

    assert_eq!(json["status"], "failed");
    assert_eq!(json["trigger_mode"], "toggle");
    assert_eq!(
        json["trigger_events"],
        serde_json::json!(["trigger_start", "trigger_stop"])
    );
    assert!(json["transcript"].is_null());
    assert!(json["output_text"].is_null());
    assert!(json["insert_outcome"].is_null());
    assert!(json["error"]
        .as_str()
        .expect("failure reason")
        .contains("transcriber returned HTTP 500"));
}
