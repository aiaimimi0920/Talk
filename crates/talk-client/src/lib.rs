use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use talk_audio::{summarize_prepared_wav_signal, trim_wav_silence_bytes};
use talk_core::{validate_http_endpoint, OpenAiTranscriptionTransport, TalkError, VoiceMode};

mod correction;
mod streaming_asr;
pub use correction::parse_cloud_correction_patch;
pub use streaming_asr::{
    final_transcript_from_streaming_asr_events, local_streaming_server_message_to_asr_event,
    parse_local_streaming_asr_server_message, parse_streaming_asr_json_line,
    run_external_streaming_asr_command, serialize_local_streaming_asr_client_message,
    LocalStreamingAsrClientMessage, LocalStreamingAsrReady, LocalStreamingAsrServerMessage,
    LocalStreamingAsrServiceClient, MockStreamingAsrEngine, StreamingAsrEngine, StreamingAsrEvent,
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

#[derive(Debug, Clone)]
pub struct HttpTranscriber {
    endpoint: String,
    client: reqwest::Client,
}

impl HttpTranscriber {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            client: reqwest::Client::new(),
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
        let response = self
            .client
            .post(&self.endpoint)
            .json(&HttpTranscribeRequest {
                audio_path: audio_path.display().to_string(),
                context,
            })
            .send()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;

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
            client: reqwest::Client::new(),
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
        let response = self
            .client
            .post(&self.endpoint)
            .json(&HttpProcessRequest {
                transcript,
                mode,
                context,
            })
            .send()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;

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
            client: reqwest::Client::new(),
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
        let bytes = prepared_audio_upload_bytes(&audio_path, original_bytes)?;
        match self.transport {
            OpenAiTranscriptionTransport::AudioTranscriptions => {
                let file_name = audio_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .ok_or_else(|| {
                        TalkError::Io("failed to determine audio file name".to_string())
                    })?;
                let file_part = reqwest::multipart::Part::bytes(bytes)
                    .file_name(file_name.to_string())
                    .mime_str("audio/wav")
                    .map_err(|error| TalkError::Provider(error.to_string()))?;
                let form = reqwest::multipart::Form::new()
                    .text("model", self.model.clone())
                    .part("file", file_part);

                let request = self.client.post(&self.endpoint).multipart(form);
                let request = with_optional_bearer_auth(request, self.api_key.as_deref());
                let response = request
                    .send()
                    .await
                    .map_err(|error| TalkError::Provider(error.to_string()))?;

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
                    messages: vec![OpenAiChatMessageWithAudioInput {
                        role: "user",
                        content: vec![OpenAiAudioInputContentPart {
                            r#type: "input_audio",
                            input_audio: OpenAiAudioInputData {
                                data: audio_data_uri(&bytes, "audio/wav"),
                            },
                        }],
                    }],
                };
                let request_builder = self.client.post(&self.endpoint).json(&request);
                let request_builder =
                    with_optional_bearer_auth(request_builder, self.api_key.as_deref());
                let response = request_builder
                    .send()
                    .await
                    .map_err(|error| TalkError::Provider(error.to_string()))?;

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
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionsRequest {
    model: String,
    messages: Vec<OpenAiChatMessage>,
}

#[derive(Debug, Serialize)]
struct OpenAiChatCompletionsAudioInputRequest {
    model: String,
    messages: Vec<OpenAiChatMessageWithAudioInput>,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct OpenAiChatMessageWithAudioInput {
    role: &'static str,
    content: Vec<OpenAiAudioInputContentPart>,
}

#[derive(Debug, Serialize)]
struct OpenAiAudioInputContentPart {
    r#type: &'static str,
    input_audio: OpenAiAudioInputData,
}

#[derive(Debug, Serialize)]
struct OpenAiAudioInputData {
    data: String,
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
    content: String,
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
        };
        let request_builder = self.client.post(&self.endpoint).json(&request);
        let request_builder = with_optional_bearer_auth(request_builder, self.api_key.as_deref());
        let response = request_builder
            .send()
            .await
            .map_err(|error| TalkError::Provider(error.to_string()))?;

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
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime_type};base64,{encoded}")
}

fn prepared_audio_upload_bytes(
    audio_path: &Path,
    original_bytes: Vec<u8>,
) -> Result<Vec<u8>, TalkError> {
    if let Ok(summary) = summarize_prepared_wav_signal(audio_path) {
        if summary.duration_seconds >= 1.0 && summary.peak < 0.05 && summary.rms < 0.003 {
            return Err(TalkError::Provider(format!(
                "captured speech signal is too weak for provider transcription (prepared_duration_seconds={:.2}, prepared_peak={:.3}, prepared_rms={:.4})",
                summary.duration_seconds, summary.peak, summary.rms
            )));
        }
    }
    match trim_wav_silence_bytes(audio_path) {
        Ok(Some(trimmed_bytes)) => Ok(trimmed_bytes),
        Ok(None) | Err(_) => Ok(original_bytes),
    }
}

fn build_openai_processing_messages(
    transcript: String,
    mode: VoiceMode,
    context: FrontContext,
) -> Result<Vec<OpenAiChatMessage>, TalkError> {
    let mut user_sections = vec![format!("Transcript:\n{transcript}")];
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

fn system_prompt_for_mode(mode: VoiceMode) -> &'static str {
    match mode {
        VoiceMode::Transcribe | VoiceMode::Dictate => {
            "You clean up speech-to-text dictation. Return only the final text with punctuation and light corrections. Do not add commentary."
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

fn front_context_has_details(context: &FrontContext) -> bool {
    context.source.is_some()
        || context.app_name.is_some()
        || context.window_title.is_some()
        || context.selected_text.is_some()
        || !context.extra.is_empty()
}
