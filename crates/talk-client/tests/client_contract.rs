use base64::Engine;
use serde_json::json;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use talk_audio::{
    read_wav_info, write_captured_wav, AudioArtifact, CapturedAudioBuffer, WavSettings,
};
use talk_client::{
    FrontContext, HttpTextProcessor, HttpTranscriber, MockTranscriber, NoopTextProcessor,
    OpenAiCompatibleTextProcessor, OpenAiCompatibleTranscriber, TextProcessor, Transcriber,
};
use talk_core::{OpenAiTranscriptionTransport, VoiceMode};

#[tokio::test]
async fn mock_transcriber_and_noop_processor_return_expected_text() {
    let transcriber = MockTranscriber::new("hello neuro");
    let context = FrontContext::default();
    let transcript = transcriber
        .transcribe(PathBuf::from("sample.wav"), context.clone())
        .await
        .expect("mock transcript");

    assert_eq!(transcript, "hello neuro");

    let processor = NoopTextProcessor;
    let processed = processor
        .process(transcript, VoiceMode::Dictate, context)
        .await
        .expect("noop process");

    assert_eq!(processed, "hello neuro");
}

#[tokio::test]
async fn mock_transcriber_rejects_blank_transcript() {
    let transcriber = MockTranscriber::new(" \t ");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("blank mock transcript should fail");

    assert!(
        error
            .to_string()
            .contains("mock transcriber returned blank text"),
        "error={error}"
    );
}

#[tokio::test]
async fn mock_transcriber_rejects_transcript_with_surrounding_whitespace() {
    let transcriber = MockTranscriber::new(" hello neuro ");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("mock transcript with surrounding whitespace should fail");

    assert!(
        error
            .to_string()
            .contains("mock transcriber text must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn mock_transcriber_rejects_empty_audio_path() {
    let transcriber = MockTranscriber::new("hello neuro");

    let error = transcriber
        .transcribe(PathBuf::new(), FrontContext::default())
        .await
        .expect_err("empty audio path should fail");

    assert!(
        error
            .to_string()
            .contains("transcriber received empty audio path"),
        "error={error}"
    );
}

#[tokio::test]
async fn mock_transcriber_rejects_whitespace_only_audio_path() {
    let transcriber = MockTranscriber::new("hello neuro");

    let error = transcriber
        .transcribe(PathBuf::from(" \t "), FrontContext::default())
        .await
        .expect_err("whitespace-only audio path should fail");

    assert!(
        error
            .to_string()
            .contains("transcriber received empty audio path"),
        "error={error}"
    );
}

#[tokio::test]
async fn noop_text_processor_rejects_blank_transcript() {
    let processor = NoopTextProcessor;

    let error = processor
        .process(
            " \n\t ".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("blank transcript should fail before no-op processing");

    assert!(
        error
            .to_string()
            .contains("text processor received blank transcript"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_blank_transcript_before_provider_request() {
    let processor = HttpTextProcessor::new("http://127.0.0.1:1/provider");

    let error = processor
        .process(" ".to_string(), VoiceMode::Dictate, FrontContext::default())
        .await
        .expect_err("blank transcript should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor received blank transcript"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_empty_audio_path_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:1/provider");

    let error = transcriber
        .transcribe(PathBuf::new(), FrontContext::default())
        .await
        .expect_err("empty audio path should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber received empty audio path"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_whitespace_only_audio_path_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:1/provider");

    let error = transcriber
        .transcribe(PathBuf::from(" \t "), FrontContext::default())
        .await
        .expect_err("whitespace-only audio path should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber received empty audio path"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_blank_endpoint_before_provider_request() {
    let transcriber = HttpTranscriber::new(" \t ");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("blank endpoint should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must not be blank"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_blank_endpoint_before_provider_request() {
    let processor = HttpTextProcessor::new(" \n ");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("blank endpoint should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must not be blank"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_surrounding_whitespace_before_provider_request() {
    let transcriber = HttpTranscriber::new(" http://127.0.0.1:1/provider ");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint whitespace should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_surrounding_whitespace_before_provider_request()
{
    let processor = HttpTextProcessor::new(" http://127.0.0.1:1/provider ");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint whitespace should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_embedded_whitespace_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:1/pro vider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint embedded whitespace should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must not contain whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_embedded_whitespace_before_provider_request() {
    let processor = HttpTextProcessor::new("http://127.0.0.1:1/pro vider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint embedded whitespace should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must not contain whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_non_http_endpoint_scheme_before_provider_request() {
    let transcriber = HttpTranscriber::new("ftp://127.0.0.1/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("non-http endpoint scheme should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must use http or https scheme"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_non_http_endpoint_scheme_before_provider_request() {
    let processor = HttpTextProcessor::new("ftp://127.0.0.1/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("non-http endpoint scheme should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must use http or https scheme"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_without_host_before_provider_request() {
    let transcriber = HttpTranscriber::new("http:///provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint without host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must include a host"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_without_host_before_provider_request() {
    let processor = HttpTextProcessor::new("http:///provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint without host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must include a host"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_user_info_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://user:pass@127.0.0.1:1/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint user info should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must not include user info"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_user_info_before_provider_request() {
    let processor = HttpTextProcessor::new("http://user:pass@127.0.0.1:1/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint user info should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must not include user info"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_fragment_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:1/provider#debug");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint fragment should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint must not include a URL fragment"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_fragment_before_provider_request() {
    let processor = HttpTextProcessor::new("http://127.0.0.1:1/provider#debug");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint fragment should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint must not include a URL fragment"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_non_numeric_port_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:nope/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint non-numeric port should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint port must be numeric"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_non_numeric_port_before_provider_request() {
    let processor = HttpTextProcessor::new("http://127.0.0.1:nope/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint non-numeric port should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint port must be numeric"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_out_of_range_port_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://127.0.0.1:70000/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint out-of-range port should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint port must be between 1 and 65535"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_out_of_range_port_before_provider_request() {
    let processor = HttpTextProcessor::new("http://127.0.0.1:70000/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint out-of-range port should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint port must be between 1 and 65535"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_unbracketed_ipv6_host_before_provider_request() {
    let transcriber = HttpTranscriber::new("http://::1/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint unbracketed IPv6 host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint IPv6 hosts must use [brackets]"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_unbracketed_ipv6_host_before_provider_request() {
    let processor = HttpTextProcessor::new("http://::1/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint unbracketed IPv6 host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint IPv6 hosts must use [brackets]"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_endpoint_with_invalid_bracketed_ipv6_host_before_provider_request(
) {
    let transcriber = HttpTranscriber::new("http://[not-ip]/provider");

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("endpoint invalid bracketed IPv6 host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("transcriber endpoint bracketed host must be a valid IPv6 address"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_endpoint_with_invalid_bracketed_ipv6_host_before_provider_request(
) {
    let processor = HttpTextProcessor::new("http://[not-ip]/provider");

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("endpoint invalid bracketed IPv6 host should fail before provider request");

    assert!(
        error
            .to_string()
            .contains("text processor endpoint bracketed host must be a valid IPv6 address"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_blank_text_response() {
    let (endpoint, handle) = spawn_text_provider_response("   ");
    let transcriber = HttpTranscriber::new(endpoint);

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("blank transcriber response should fail");

    handle.join().expect("provider thread joins");
    assert!(
        error
            .to_string()
            .contains("transcriber returned blank text"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_transcriber_rejects_text_response_with_surrounding_whitespace() {
    let (endpoint, handle) = spawn_text_provider_response(" hello neuro ");
    let transcriber = HttpTranscriber::new(endpoint);

    let error = transcriber
        .transcribe(PathBuf::from("sample.wav"), FrontContext::default())
        .await
        .expect_err("transcriber response with surrounding whitespace should fail");

    handle.join().expect("provider thread joins");
    assert!(
        error
            .to_string()
            .contains("transcriber text must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_blank_text_response() {
    let (endpoint, handle) = spawn_text_provider_response("\t \n");
    let processor = HttpTextProcessor::new(endpoint);

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("blank text processor response should fail");

    handle.join().expect("provider thread joins");
    assert!(
        error
            .to_string()
            .contains("text processor returned blank text"),
        "error={error}"
    );
}

#[tokio::test]
async fn http_text_processor_rejects_text_response_with_surrounding_whitespace() {
    let (endpoint, handle) = spawn_text_provider_response("\thello neuro\n");
    let processor = HttpTextProcessor::new(endpoint);

    let error = processor
        .process(
            "hello neuro".to_string(),
            VoiceMode::Dictate,
            FrontContext::default(),
        )
        .await
        .expect_err("text processor response with surrounding whitespace should fail");

    handle.join().expect("provider thread joins");
    assert!(
        error
            .to_string()
            .contains("text processor text must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[tokio::test]
async fn openai_compatible_transcriber_uploads_audio_multipart_with_model_and_bearer_auth() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-openai-compatible-transcriber-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let audio_path = temp_dir.join("sample.wav");
    std::fs::write(&audio_path, b"fake wav bytes").expect("write fake audio");

    let (endpoint, handle) = spawn_openai_transcription_response(r#"{"text":"hello from audio"}"#);
    let transcriber = OpenAiCompatibleTranscriber::new(
        endpoint,
        "gpt-4o-mini-transcribe",
        Some("talk-test-key".to_string()),
    );

    let transcript = transcriber
        .transcribe(audio_path.clone(), FrontContext::default())
        .await
        .expect("openai-compatible transcript should succeed");

    let request = handle.join().expect("provider thread joins");

    assert_eq!(transcript, "hello from audio");
    assert!(request
        .headers
        .contains("POST /v1/audio/transcriptions HTTP/1.1"));
    assert!(request
        .headers
        .contains("authorization: Bearer talk-test-key"));
    assert!(request.body.contains("name=\"model\""));
    assert!(request.body.contains("gpt-4o-mini-transcribe"));
    assert!(request.body.contains("name=\"file\""));
    assert!(request.body.contains("filename=\"sample.wav\""));
    assert!(request.body.contains("fake wav bytes"));
}

#[tokio::test]
async fn openai_compatible_transcriber_can_use_chat_completions_audio_input_transport() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-openai-compatible-chat-audio-input-transcriber-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let audio_path = temp_dir.join("sample.wav");
    std::fs::write(&audio_path, b"fake wav bytes").expect("write fake audio");

    let (endpoint, handle) =
        spawn_openai_chat_response(r#"{"choices":[{"message":{"content":"hello from audio"}}]}"#);
    let transcriber = OpenAiCompatibleTranscriber::new_with_transport(
        endpoint,
        "qwen3-asr-flash",
        Some("talk-test-key".to_string()),
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput,
    );

    let transcript = transcriber
        .transcribe(audio_path.clone(), FrontContext::default())
        .await
        .expect("chat-completions audio-input transcript should succeed");

    let request = handle.join().expect("provider thread joins");
    let request_json: serde_json::Value =
        serde_json::from_str(&request.body).expect("audio input body json");
    let messages = request_json["messages"].as_array().expect("messages array");
    let content = messages[0]["content"].as_array().expect("content array");
    let audio_part = &content[0]["input_audio"]["data"];

    assert_eq!(transcript, "hello from audio");
    assert!(request
        .headers
        .contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request
        .headers
        .contains("authorization: Bearer talk-test-key"));
    assert_eq!(request_json["model"], "qwen3-asr-flash");
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(content[0]["type"], "input_audio");
    assert!(
        audio_part
            .as_str()
            .expect("audio data string")
            .starts_with("data:audio/wav;base64,"),
        "audio part={audio_part}"
    );
}

#[tokio::test]
async fn openai_compatible_chat_audio_input_transport_trims_trailing_silence_from_wav_payload() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-openai-compatible-chat-audio-input-trim-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let audio_path = temp_dir.join("sample.wav");
    let artifact = AudioArtifact::new(audio_path.clone(), "audio/wav");
    let source = CapturedAudioBuffer {
        sample_rate_hz: 16_000,
        channels: 1,
        samples: {
            let mut samples = vec![0.0_f32; 16_000 * 5];
            for (index, sample) in samples.iter_mut().take(16_000).enumerate() {
                *sample = if index % 2 == 0 { 0.6 } else { -0.6 };
            }
            samples
        },
    };
    write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
        .expect("write source wav with trailing silence");
    let original_info = read_wav_info(&artifact).expect("read original wav info");

    let (endpoint, handle) =
        spawn_openai_chat_response(r#"{"choices":[{"message":{"content":"hello from audio"}}]}"#);
    let transcriber = OpenAiCompatibleTranscriber::new_with_transport(
        endpoint,
        "qwen3-asr-flash",
        Some("talk-test-key".to_string()),
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput,
    );

    let transcript = transcriber
        .transcribe(audio_path.clone(), FrontContext::default())
        .await
        .expect("chat-completions audio-input transcript should succeed");

    let request = handle.join().expect("provider thread joins");
    let request_json: serde_json::Value =
        serde_json::from_str(&request.body).expect("audio input body json");
    let audio_data = request_json["messages"][0]["content"][0]["input_audio"]["data"]
        .as_str()
        .expect("audio data string");
    let uploaded_path = temp_dir.join("uploaded.wav");
    std::fs::write(&uploaded_path, decode_audio_data_uri(audio_data)).expect("write uploaded wav");
    let uploaded_info = read_wav_info(&AudioArtifact::new(uploaded_path, "audio/wav"))
        .expect("read uploaded wav info");

    assert_eq!(transcript, "hello from audio");
    assert_eq!(original_info.duration_samples, 80_000);
    assert!(
        uploaded_info.duration_samples < 40_000,
        "uploaded duration_samples={} original_duration_samples={}",
        uploaded_info.duration_samples,
        original_info.duration_samples
    );
}

#[tokio::test]
async fn openai_compatible_chat_audio_input_transport_rejects_extremely_weak_trimmed_audio_before_provider_request(
) {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-openai-compatible-chat-audio-input-weak-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let audio_path = temp_dir.join("weak.wav");
    let artifact = AudioArtifact::new(audio_path.clone(), "audio/wav");
    let source = CapturedAudioBuffer {
        sample_rate_hz: 16_000,
        channels: 1,
        samples: {
            let mut samples = vec![0.0_f32; 16_000 * 10];
            for index in 0..(16_000 * 2) {
                if index % 120 == 0 {
                    samples[16_000 * 5 + index] = if index % 240 == 0 { 0.03 } else { -0.03 };
                }
            }
            samples
        },
    };
    write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
        .expect("write weak sparse wav");

    let transcriber = OpenAiCompatibleTranscriber::new_with_transport(
        "http://127.0.0.1:1/v1/chat/completions",
        "qwen3-asr-flash",
        Some("talk-test-key".to_string()),
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput,
    );

    let error = transcriber
        .transcribe(audio_path, FrontContext::default())
        .await
        .expect_err("extremely weak trimmed audio should fail locally");

    assert!(
        error
            .to_string()
            .contains("captured speech signal is too weak for provider transcription"),
        "error={error}"
    );
}

#[tokio::test]
async fn openai_compatible_chat_audio_input_transport_allows_quiet_continuous_audio() {
    let temp_dir = std::env::temp_dir().join(format!(
        "talk-openai-compatible-chat-audio-input-quiet-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&temp_dir).expect("create temp dir");
    let audio_path = temp_dir.join("quiet.wav");
    let artifact = AudioArtifact::new(audio_path.clone(), "audio/wav");
    let source = CapturedAudioBuffer {
        sample_rate_hz: 16_000,
        channels: 1,
        samples: {
            let mut samples = vec![0.0_f32; 16_000 * 6];
            for (index, sample) in samples.iter_mut().enumerate() {
                if index >= 16_000 / 2 && index < 16_000 * 5 {
                    *sample = if index % 2 == 0 { 0.04 } else { -0.04 };
                }
            }
            samples
        },
    };
    write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
        .expect("write quiet continuous wav");

    let (endpoint, handle) =
        spawn_openai_chat_response(r#"{"choices":[{"message":{"content":"hello from audio"}}]}"#);
    let transcriber = OpenAiCompatibleTranscriber::new_with_transport(
        endpoint,
        "qwen3-asr-flash",
        Some("talk-test-key".to_string()),
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput,
    );

    let transcript = transcriber
        .transcribe(audio_path, FrontContext::default())
        .await
        .expect("quiet continuous audio should still be uploaded");

    let request = handle.join().expect("provider thread joins");
    assert_eq!(transcript, "hello from audio");
    assert!(
        request
            .headers
            .contains("POST /v1/chat/completions HTTP/1.1"),
        "headers={}",
        request.headers
    );
}

#[tokio::test]
async fn openai_compatible_text_processor_posts_chat_completions_and_extracts_first_message_content(
) {
    let (endpoint, handle) =
        spawn_openai_chat_response(r#"{"choices":[{"message":{"content":"assistant reply"}}]}"#);
    let processor = OpenAiCompatibleTextProcessor::new(
        endpoint,
        "gpt-4o-mini",
        Some("talk-test-key".to_string()),
    );

    let context = FrontContext {
        source: Some("hook-panel".to_string()),
        app_name: Some("Hook".to_string()),
        window_title: Some("Neuro editor".to_string()),
        selected_text: Some("selected seed".to_string()),
        ..FrontContext::default()
    };
    let processed = processor
        .process(
            "turn this into an answer".to_string(),
            VoiceMode::Command,
            context,
        )
        .await
        .expect("openai-compatible processor should succeed");

    let request = handle.join().expect("provider thread joins");
    let request_json: serde_json::Value =
        serde_json::from_str(&request.body).expect("chat completions body json");
    let messages = request_json["messages"].as_array().expect("messages array");

    assert_eq!(processed, "assistant reply");
    assert!(request
        .headers
        .contains("POST /v1/chat/completions HTTP/1.1"));
    assert!(request
        .headers
        .contains("authorization: Bearer talk-test-key"));
    assert_eq!(request_json["model"], "gpt-4o-mini");
    assert!(messages.iter().any(|message| message["role"] == "system"));
    assert!(messages.iter().any(|message| {
        message["role"] == "user"
            && message["content"]
                .as_str()
                .expect("user content")
                .contains("turn this into an answer")
    }));
}

fn spawn_text_provider_response(text: &str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind text provider");
    let endpoint = format!(
        "http://{}/provider",
        listener.local_addr().expect("provider addr")
    );
    let response_body = json!({ "text": text }).to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("provider stream read timeout");
        let request_body = read_http_body(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write provider response");
        request_body
    });
    (endpoint, handle)
}

fn read_http_body(stream: &mut TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];
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

    String::from_utf8(buffer[header_end..header_end + content_length].to_vec())
        .expect("utf8 request body")
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[derive(Debug)]
struct CapturedRequest {
    headers: String,
    body: String,
}

fn spawn_openai_transcription_response(
    response_body: &str,
) -> (String, thread::JoinHandle<CapturedRequest>) {
    spawn_captured_request_server("/v1/audio/transcriptions", response_body)
}

fn spawn_openai_chat_response(
    response_body: &str,
) -> (String, thread::JoinHandle<CapturedRequest>) {
    spawn_captured_request_server("/v1/chat/completions", response_body)
}

fn spawn_captured_request_server(
    path: &str,
    response_body: &str,
) -> (String, thread::JoinHandle<CapturedRequest>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind openai-compatible provider");
    let endpoint = format!(
        "http://{}{}",
        listener.local_addr().expect("provider addr"),
        path
    );
    let response_body = response_body.to_string();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept provider request");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("provider stream read timeout");
        let request = read_http_request(&mut stream);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        )
        .expect("write provider response");
        request
    });
    (endpoint, handle)
}

fn decode_audio_data_uri(uri: &str) -> Vec<u8> {
    let prefix = "data:audio/wav;base64,";
    let encoded = uri
        .strip_prefix(prefix)
        .expect("audio data uri prefix should be present");
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .expect("decode base64 audio data")
}

fn read_http_request(stream: &mut TcpStream) -> CapturedRequest {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 1024];
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

    let headers = String::from_utf8_lossy(&buffer[..header_end]).to_string();
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

    let body =
        String::from_utf8_lossy(&buffer[header_end..header_end + content_length]).to_string();

    CapturedRequest { headers, body }
}
