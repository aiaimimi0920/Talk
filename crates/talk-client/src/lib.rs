use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use talk_audio::summarize_and_trim_prepared_wav_bytes;
use talk_core::{validate_http_endpoint, OpenAiTranscriptionTransport, TalkError, VoiceMode};

mod correction;
mod streaming_asr;
pub use correction::parse_cloud_correction_patch;
pub use streaming_asr::{
    final_transcript_from_streaming_asr_events, local_streaming_server_message_to_asr_event,
    parse_local_streaming_asr_server_message, parse_streaming_asr_json_line,
    run_external_streaming_asr_command, run_external_streaming_asr_command_with_timeout,
    serialize_local_streaming_asr_client_message, LocalStreamingAsrClientMessage,
    LocalStreamingAsrReady, LocalStreamingAsrServerMessage, LocalStreamingAsrServiceClient,
    MockStreamingAsrEngine, StreamingAsrEngine, StreamingAsrEvent,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrontContext {
    pub source: Option<String>,
    #[serde(alias = "appName")]
    pub app_name: Option<String>,
    #[serde(alias = "windowTitle")]
    pub window_title: Option<String>,
    #[serde(alias = "selectedText")]
    pub selected_text: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[async_trait]
pub trait Transcriber: Send + Sync {
    async fn transcribe(
        &self,
        audio_path: PathBuf,
        context: FrontContext,
    ) -> Result<String, TalkError>;
}

#[async_trait]
pub trait TextProcessor: Send + Sync {
    async fn process(
        &self,
        transcript: String,
        mode: VoiceMode,
        context: FrontContext,
    ) -> Result<String, TalkError>;
}

#[derive(Debug, Clone)]
pub struct MockTranscriber {
    transcript: String,
}

impl MockTranscriber {
    pub fn new(transcript: impl Into<String>) -> Self {
        Self {
            transcript: transcript.into(),
        }
    }
}

#[async_trait]
impl Transcriber for MockTranscriber {
    async fn transcribe(
        &self,
        audio_path: PathBuf,
        _context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_empty_audio_path(&audio_path)?;
        if self.transcript.trim().is_empty() {
            return Err(TalkError::Provider(
                "mock transcriber returned blank text".to_string(),
            ));
        }
        if self.transcript.trim() != self.transcript {
            return Err(TalkError::Provider(
                "mock transcriber text must not have leading or trailing whitespace".to_string(),
            ));
        }
        Ok(self.transcript.clone())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopTextProcessor;

#[async_trait]
impl TextProcessor for NoopTextProcessor {
    async fn process(
        &self,
        transcript: String,
        _mode: VoiceMode,
        _context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_blank_transcript(&transcript)?;
        Ok(transcript)
    }
}

/// Connect timeout for cloud provider requests — fail fast if the host is
/// unreachable rather than hanging the dictation finalize path.
const HTTP_CONNECT_TIMEOUT_SECS: u64 = 10;
/// Overall request timeout. Generous enough for a large base64 audio upload plus
/// model inference, but bounded so a stalled/half-open connection cannot hang a
/// dictation forever (`reqwest::Client::new()` has no timeout at all).
const HTTP_REQUEST_TIMEOUT_SECS: u64 = 120;

fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(HTTP_CONNECT_TIMEOUT_SECS))
        .timeout(std::time::Duration::from_secs(HTTP_REQUEST_TIMEOUT_SECS))
        .build()
        .expect("static Talk HTTP client configuration must be valid")
}

fn shared_http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(build_http_client).clone()
}

/// One transient network hiccup (e.g. a refused connection while a local
/// gateway restarts) should not fail an entire dictation, so cloud requests
/// retry exactly once after a short backoff. Only connection-establishment
/// failures are retried: the request never reached the server, so a second
/// attempt is always safe.
const CLOUD_SEND_MAX_RETRIES: usize = 1;
const CLOUD_SEND_RETRY_BACKOFF_MS: u64 = 300;

fn reqwest_error_is_safely_retryable(error: &reqwest::Error) -> bool {
    error.is_connect()
}

async fn send_with_connect_retry<F>(build_request: F) -> Result<reqwest::Response, TalkError>
where
    F: Fn() -> Result<reqwest::RequestBuilder, TalkError>,
{
    let mut attempted_retries = 0usize;
    loop {
        match build_request()?.send().await {
            Ok(response) => return Ok(response),
            Err(error)
                if attempted_retries < CLOUD_SEND_MAX_RETRIES
                    && reqwest_error_is_safely_retryable(&error) =>
            {
                attempted_retries += 1;
                tokio::time::sleep(std::time::Duration::from_millis(
                    CLOUD_SEND_RETRY_BACKOFF_MS,
                ))
                .await;
            }
            Err(error) => return Err(TalkError::Provider(error.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpTranscriber {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpTranscriber {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: shared_http_client(),
        }
    }
}

#[derive(Debug, Serialize)]
struct HttpTranscribeRequest {
    audio_path: String,
    context: FrontContext,
}

#[derive(Debug, Deserialize)]
struct TextResponse {
    text: String,
}

#[async_trait]
impl Transcriber for HttpTranscriber {
    async fn transcribe(
        &self,
        audio_path: PathBuf,
        context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_empty_audio_path(&audio_path)?;
        reject_invalid_endpoint(&self.endpoint, "transcriber")?;
        let request_body = HttpTranscribeRequest {
            audio_path: audio_path.display().to_string(),
            context,
        };
        let response =
            send_with_connect_retry(|| Ok(self.client.post(&self.endpoint).json(&request_body)))
                .await?;

        if !response.status().is_success() {
            return Err(TalkError::Provider(format!(
                "transcriber returned HTTP {}",
                response.status()
            )));
        }

        let body = response
            .json::<TextResponse>()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;
        validate_response_text(body.text, "transcriber")
    }
}

#[derive(Debug, Clone)]
pub struct HttpTextProcessor {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpTextProcessor {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: shared_http_client(),
        }
    }
}

#[derive(Debug, Serialize)]
struct HttpProcessRequest {
    transcript: String,
    mode: VoiceMode,
    context: FrontContext,
}

#[async_trait]
impl TextProcessor for HttpTextProcessor {
    async fn process(
        &self,
        transcript: String,
        mode: VoiceMode,
        context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_blank_transcript(&transcript)?;
        reject_invalid_endpoint(&self.endpoint, "text processor")?;
        let request_body = HttpProcessRequest {
            transcript,
            mode,
            context,
        };
        let response =
            send_with_connect_retry(|| Ok(self.client.post(&self.endpoint).json(&request_body)))
                .await?;

        if !response.status().is_success() {
            return Err(TalkError::Provider(format!(
                "text processor returned HTTP {}",
                response.status()
            )));
        }

        let body = response
            .json::<TextResponse>()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;
        validate_response_text(body.text, "text processor")
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTranscriber {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    transport: OpenAiTranscriptionTransport,
    client: reqwest::Client,
}

impl OpenAiCompatibleTranscriber {
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self::new_with_transport(
            endpoint,
            model,
            api_key,
            OpenAiTranscriptionTransport::AudioTranscriptions,
        )
    }

    pub fn new_with_transport(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
        transport: OpenAiTranscriptionTransport,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
            transport,
            client: shared_http_client(),
        }
    }
}

#[async_trait]
impl Transcriber for OpenAiCompatibleTranscriber {
    async fn transcribe(
        &self,
        audio_path: PathBuf,
        _context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_empty_audio_path(&audio_path)?;
        reject_invalid_endpoint(&self.endpoint, "openai-compatible transcriber endpoint")?;
        reject_required_value(
            &self.model,
            "openai-compatible transcriber model",
            "openai-compatible transcriber model must not be blank",
        )?;

        let original_bytes = std::fs::read(&audio_path).map_err(|error| {
            TalkError::Io(format!(
                "failed to read audio artifact {}: {error}",
                audio_path.display()
            ))
        })?;
        let bytes = prepared_audio_upload_bytes(original_bytes)?;
        match self.transport {
            OpenAiTranscriptionTransport::AudioTranscriptions => {
                let file_name = audio_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        TalkError::Io("failed to determine audio file name".to_string())
                    })?;
                // Multipart bodies are not cloneable, so each attempt rebuilds
                // the form from the prepared audio bytes.
                let response = send_with_connect_retry(|| {
                    let file_part = reqwest::multipart::Part::bytes(bytes.clone())
                        .file_name(file_name.to_string())
                        .mime_str("audio/wav")
                        .map_err(|error| TalkError::Provider(error.to_string()))?;
                    let form = reqwest::multipart::Form::new()
                        .text("model", self.model.clone())
                        .part("file", file_part);
                    Ok(with_optional_bearer_auth(
                        self.client.post(&self.endpoint).multipart(form),
                        self.api_key.as_deref(),
                    ))
                })
                .await?;

                if !response.status().is_success() {
                    return Err(TalkError::Provider(format!(
                        "openai-compatible transcriber returned HTTP {}",
                        response.status()
                    )));
                }

                let body = response
                    .json::<TextResponse>()
                    .await
                    .map_err(|error| TalkError::Provider(error.to_string()))?;
                validate_response_text(body.text, "openai-compatible transcriber")
            }
            OpenAiTranscriptionTransport::ChatCompletionsAudioInput => {
                let request = OpenAiChatCompletionsAudioInputRequest {
                    model: self.model.clone(),
                    messages: build_openai_audio_input_transcription_messages(&bytes),
                };
                let response = send_with_connect_retry(|| {
                    Ok(with_optional_bearer_auth(
                        self.client.post(&self.endpoint).json(&request),
                        self.api_key.as_deref(),
                    ))
                })
                .await?;

                if !response.status().is_success() {
                    return Err(TalkError::Provider(format!(
                        "openai-compatible transcriber returned HTTP {}",
                        response.status()
                    )));
                }

                let body = response
                    .json::<OpenAiChatCompletionsResponse>()
                    .await
                    .map_err(|error| TalkError::Provider(error.to_string()))?;
                let text = body
                    .choices
                    .into_iter()
                    .next()
                    .map(|choice| choice.message.content)
                    .ok_or_else(|| {
                        TalkError::Provider(
                            "openai-compatible transcriber returned no choices".to_string(),
                        )
                    })?;
                validate_response_text(text, "openai-compatible transcriber")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTextProcessor {
    endpoint: String,
    model: String,
    api_key: Option<String>,
    client: reqwest::Client,
}

impl OpenAiCompatibleTextProcessor {
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            api_key,
            client: shared_http_client(),
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionsRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_thinking: Option<bool>,
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionsAudioInputRequest {
    model: String,
    messages: Vec<Value>,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionsResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatResponseMessage {
    #[serde(default, deserialize_with = "deserialize_chat_message_content")]
    content: String,
}

/// Extract assistant text from a chat-completions `content` field. OpenAI-
/// compatible providers may return it as a plain string, an array of content
/// parts (`{ "type": "text", "text": ... }`, common with vLLM / multimodal
/// gateways), or `null` (a reasoning/tool-only turn). Non-text shapes yield an
/// empty string, which the caller then rejects as a blank transcript instead of
/// failing to deserialize the whole response.
fn deserialize_chat_message_content<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(chat_message_content_text(&value))
}

fn chat_message_content_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<String>(),
        _ => String::new(),
    }
}

#[async_trait]
impl TextProcessor for OpenAiCompatibleTextProcessor {
    async fn process(
        &self,
        transcript: String,
        mode: VoiceMode,
        context: FrontContext,
    ) -> Result<String, TalkError> {
        reject_blank_transcript(&transcript)?;
        reject_invalid_endpoint(&self.endpoint, "openai-compatible text processor endpoint")?;
        reject_required_value(
            &self.model,
            "openai-compatible text processor model",
            "openai-compatible text processor model must not be blank",
        )?;

        let request = OpenAiChatCompletionsRequest {
            model: self.model.clone(),
            messages: build_openai_processing_messages(transcript, mode, context)?,
            enable_thinking: qwen3_thinking_override(&self.model),
        };
        let response = send_with_connect_retry(|| {
            Ok(with_optional_bearer_auth(
                self.client.post(&self.endpoint).json(&request),
                self.api_key.as_deref(),
            ))
        })
        .await?;

        if !response.status().is_success() {
            return Err(TalkError::Provider(format!(
                "openai-compatible text processor returned HTTP {}",
                response.status()
            )));
        }

        let body = response
            .json::<OpenAiChatCompletionsResponse>()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;
        let text = body
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .ok_or_else(|| {
                TalkError::Provider(
                    "openai-compatible text processor returned no choices".to_string(),
                )
            })?;
        validate_response_text(text, "openai-compatible text processor")
    }
}

fn qwen3_thinking_override(model: &str) -> Option<bool> {
    model
        .trim()
        .to_ascii_lowercase()
        .starts_with("qwen3")
        .then_some(false)
}

fn validate_response_text(text: String, component: &str) -> Result<String, TalkError> {
    if text.trim().is_empty() {
        return Err(TalkError::Provider(format!(
            "{component} returned blank text"
        )));
    }
    if text.trim() != text {
        return Err(TalkError::Provider(format!(
            "{component} text must not have leading or trailing whitespace"
        )));
    }
    Ok(text)
}

fn reject_blank_transcript(transcript: &str) -> Result<(), TalkError> {
    if transcript.trim().is_empty() {
        return Err(TalkError::Provider(
            "text processor received blank transcript".to_string(),
        ));
    }
    Ok(())
}

fn reject_empty_audio_path(audio_path: &Path) -> Result<(), TalkError> {
    if audio_path.as_os_str().is_empty()
        || audio_path.as_os_str().to_string_lossy().trim().is_empty()
    {
        return Err(TalkError::Provider(
            "transcriber received empty audio path".to_string(),
        ));
    }
    Ok(())
}

fn reject_invalid_endpoint(endpoint: &str, component: &str) -> Result<(), TalkError> {
    validate_http_endpoint(endpoint, &format!("{component} endpoint")).map_err(TalkError::Provider)
}

fn reject_required_value(
    value: &str,
    _subject: &str,
    blank_message: &str,
) -> Result<(), TalkError> {
    if value.trim().is_empty() {
        return Err(TalkError::Provider(blank_message.to_string()));
    }
    if value.trim() != value {
        return Err(TalkError::Provider(format!(
            "{} must not have leading or trailing whitespace",
            blank_message.trim_end_matches(" must not be blank")
        )));
    }
    Ok(())
}

fn with_optional_bearer_auth(
    request: reqwest::RequestBuilder,
    api_key: Option<&str>,
) -> reqwest::RequestBuilder {
    match api_key {
        Some(api_key) => request.bearer_auth(api_key),
        None => request,
    }
}

fn audio_data_uri(bytes: &[u8], mime_type: &str) -> String {
    let encoded_len = base64::encoded_len(bytes.len(), true).unwrap_or(0);
    let mut uri =
        String::with_capacity("data:".len() + mime_type.len() + ";base64,".len() + encoded_len);
    uri.push_str("data:");
    uri.push_str(mime_type);
    uri.push_str(";base64,");
    base64::engine::general_purpose::STANDARD.encode_string(bytes, &mut uri);
    uri
}

fn prepared_audio_upload_bytes(original_bytes: Vec<u8>) -> Result<Vec<u8>, TalkError> {
    // A single decode of the already-read bytes yields both the weak-signal
    // summary and the silence-trimmed upload payload. Decode failures fall
    // back to uploading the original bytes untouched, matching the previous
    // per-call error swallowing.
    let Ok(prepared) = summarize_and_trim_prepared_wav_bytes(&original_bytes) else {
        return Ok(original_bytes);
    };
    let summary = prepared.summary;
    if summary.duration_seconds >= 1.0 && summary.peak < 0.05 && summary.rms < 0.003 {
        return Err(TalkError::Provider(format!(
            "captured speech signal is too weak for provider transcription (prepared_duration_seconds={:.2}, prepared_peak={:.3}, prepared_rms={:.4})",
            summary.duration_seconds, summary.peak, summary.rms
        )));
    }
    Ok(prepared.trimmed_wav_bytes.unwrap_or(original_bytes))
}

fn build_openai_processing_messages(
    transcript: String,
    mode: VoiceMode,
    context: FrontContext,
) -> Result<Vec<OpenAiChatMessage>, TalkError> {
    let mut user_sections = vec![format!("Transcript:\n{transcript}")];
    if let Some(hints) = canonical_term_hints_for_mode(mode) {
        user_sections.push(build_canonical_term_hints_section(hints));
    }
    if let Some(variants) = canonical_term_variants_for_mode(mode) {
        user_sections.push(build_canonical_term_variants_section(variants));
    }
    if let Some(examples) = canonical_phrase_examples_for_mode(mode) {
        user_sections.push(build_canonical_phrase_examples_section(examples));
    }
    if front_context_has_details(&context) {
        let context_json = serde_json::to_string_pretty(&context)
            .map_err(|error| TalkError::Provider(error.to_string()))?;
        user_sections.push(format!("Front context JSON:\n{context_json}"));
    }
    Ok(vec![
        OpenAiChatMessage {
            role: "system",
            content: system_prompt_for_mode(mode).to_string(),
        },
        OpenAiChatMessage {
            role: "user",
            content: user_sections.join("\n\n"),
        },
    ])
}

fn build_openai_audio_input_transcription_messages(audio_bytes: &[u8]) -> Vec<Value> {
    vec![json!({
        "role": "user",
        "content": [
            {
                "type": "input_audio",
                "input_audio": {
                    "data": audio_data_uri(audio_bytes, "audio/wav")
                }
            }
        ]
    })]
}

fn system_prompt_for_mode(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Transcribe | VoiceMode::Dictate => {
            "You clean up speech-to-text dictation. Preserve the original language, mixed-language tokens, product names, paths, hotkeys, numbers, and ASCII terms. If a hinted domain term appears as an obvious phonetic or spacing variant, normalize it to the canonical spelling only when the surrounding words make the intent unambiguous. Do not expand plain Talk into Neuro Talk unless the transcript contains a Neuro-specific variant such as \"你 o talk\", \"neo tok\", or \"neotok\". If a hinted canonical term or path is obviously clipped at the edge of the utterance, complete only the missing trailing characters. You may also receive full-phrase examples from this project; only use one when the transcript is clearly trying to say that exact phrase or an obviously clipped tail of it. Only fix obvious speech-to-text mistakes and punctuation. Do not translate, summarize, paraphrase, rewrite, or add commentary. Return only the final text."
        }
        VoiceMode::Document | VoiceMode::Polish => {
            "You rewrite dictated text into polished formal or document-ready writing. Return only the final rewritten text."
        }
        VoiceMode::Translate => {
            "You translate the transcript. If the user did not specify a target language, translate it into natural English. Return only the translated text."
        }
        VoiceMode::Generate => {
            "Treat the transcript as the user's generation instruction. Return only the generated final content, not the instruction itself."
        }
        VoiceMode::Command => {
            "You are a concise voice assistant. Treat the transcript as the user's request and reply with only the answer text, without preamble."
        }
        VoiceMode::Smart => {
            "Infer whether the transcript is dictation, document polishing, a command, or a generation request. Return only the final user-facing result text."
        }
    }
}

const DICTATION_CANONICAL_TERM_HINTS: &[&str] = &[
    "Talk",
    "Neuro",
    "Neuro Talk",
    "local first ASR",
    "qwen3 asr flash",
    r"C:\Users\Public\Talk\logs",
];

const DICTATION_CANONICAL_TERM_VARIANTS: &[(&str, &str)] = &[
    ("rock foster a s r", "local first ASR"),
    ("rock for ster a s r", "local first ASR"),
    ("localhost asr", "local first ASR"),
    ("local host asr", "local first ASR"),
    ("你 o talk", "Neuro Talk"),
    ("neo tok", "Neuro Talk"),
    ("neotok", "Neuro Talk"),
    ("tok", "Talk"),
    ("套口", "Talk"),
    ("套可", "Talk"),
    ("透过", "Talk"),
    ("千问三 a s r flash", "qwen3 asr flash"),
    ("千问三 asr flash", "qwen3 asr flash"),
    ("text 测试页", "テスト 页面"),
    ("test 测试页", "テスト 页面"),
    ("我你好", "你好呀"),
    ("掀开项目例会", "先开项目例会"),
    ("继续进入", "继续记录"),
    ("c 盘的 us", r"C:\Users\Public\Talk\logs"),
];

const DICTATION_CANONICAL_PHRASE_EXAMPLES: &[&str] = &[
    "你好呀",
    "打开 Talk 的 local first ASR 测试",
    "请帮我打开 Talk 的 local first ASR テスト 页面。",
    "请把 Neuro Talk 的 qwen3 asr flash 结果保存到 C:\\Users\\Public\\Talk\\logs。",
    "今天下午三点半我们先开项目例会，确认 Talk 的默认识别模型，然后把多语言测试结果同步给 Neuro 团队。",
    "现在办公室里有一点空调和键盘声，请继续记录 Talk 的多语言识别测试结果。",
];

fn canonical_term_hints_for_mode(mode: VoiceMode) -> Option<&'static [&'static str]> {
    match mode {
        VoiceMode::Transcribe | VoiceMode::Dictate => Some(DICTATION_CANONICAL_TERM_HINTS),
        VoiceMode::Document
        | VoiceMode::Polish
        | VoiceMode::Translate
        | VoiceMode::Generate
        | VoiceMode::Command
        | VoiceMode::Smart => None,
    }
}

fn build_canonical_term_hints_section(hints: &[&str]) -> String {
    let bullets = hints
        .iter()
        .map(|hint| format!("- {hint}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Canonical term hints:\nUse these only when the transcript clearly intends the term; otherwise ignore them.\n{bullets}"
    )
}

fn canonical_term_variants_for_mode(
    mode: VoiceMode,
) -> Option<&'static [(&'static str, &'static str)]> {
    match mode {
        VoiceMode::Transcribe | VoiceMode::Dictate => Some(DICTATION_CANONICAL_TERM_VARIANTS),
        VoiceMode::Document
        | VoiceMode::Polish
        | VoiceMode::Translate
        | VoiceMode::Generate
        | VoiceMode::Command
        | VoiceMode::Smart => None,
    }
}

fn canonical_phrase_examples_for_mode(mode: VoiceMode) -> Option<&'static [&'static str]> {
    match mode {
        VoiceMode::Transcribe | VoiceMode::Dictate => Some(DICTATION_CANONICAL_PHRASE_EXAMPLES),
        VoiceMode::Document
        | VoiceMode::Polish
        | VoiceMode::Translate
        | VoiceMode::Generate
        | VoiceMode::Command
        | VoiceMode::Smart => None,
    }
}

fn build_canonical_term_variants_section(variants: &[(&str, &str)]) -> String {
    let bullets = variants
        .iter()
        .map(|(variant, canonical)| format!("- {variant} -> {canonical}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Common recognition variants in this project:\nUse these only when the surrounding words clearly indicate the canonical term.\nKeep plain Talk as Talk; only upgrade to Neuro Talk when the transcript contains a Neuro-specific variant such as 你 o talk, neo tok, or neotok.\n{bullets}"
    )
}

fn build_canonical_phrase_examples_section(examples: &[&str]) -> String {
    let bullets = examples
        .iter()
        .map(|example| format!("- {example}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Frequent full-phrase examples in this project:\nUse these only when the transcript is clearly trying to say the same full phrase or an obviously clipped tail of it.\n{bullets}"
    )
}

fn front_context_has_details(context: &FrontContext) -> bool {
    context.source.is_some()
        || context.app_name.is_some()
        || context.window_title.is_some()
        || context.selected_text.is_some()
        || !context.extra.is_empty()
}

#[cfg(test)]
mod tests {
    use super::OpenAiChatResponseMessage;

    fn parse_content(json: &str) -> String {
        serde_json::from_str::<OpenAiChatResponseMessage>(json)
            .expect("chat response message should parse")
            .content
    }

    #[test]
    fn chat_content_accepts_plain_string() {
        assert_eq!(parse_content(r#"{"content":"hello"}"#), "hello");
    }

    #[test]
    fn chat_content_concatenates_text_parts_array() {
        assert_eq!(
            parse_content(
                r#"{"content":[{"type":"text","text":"hel"},{"type":"text","text":"lo"}]}"#
            ),
            "hello"
        );
    }

    #[test]
    fn chat_content_treats_null_as_empty() {
        assert_eq!(parse_content(r#"{"content":null}"#), "");
    }

    #[test]
    fn chat_content_treats_missing_field_as_empty() {
        assert_eq!(parse_content(r#"{}"#), "");
    }
}
