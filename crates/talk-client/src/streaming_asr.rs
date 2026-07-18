use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use talk_core::TalkError;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LocalStreamingAsrClientMessage {
    Start {
        session_id: String,
        sample_rate_hz: u32,
        channels: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        language: Option<String>,
    },
    Audio {
        session_id: String,
        sequence: u64,
        pcm_base64: String,
    },
    Stop {
        session_id: String,
    },
    Cancel {
        session_id: String,
    },
}

impl LocalStreamingAsrClientMessage {
    pub fn start(
        session_id: impl Into<String>,
        sample_rate_hz: u32,
        channels: u16,
        language: Option<&str>,
    ) -> Result<Self, TalkError> {
        let session_id = validate_local_streaming_session_id(session_id.into())?;
        if sample_rate_hz == 0 {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR sample_rate_hz must be greater than 0".to_string(),
            ));
        }
        if channels == 0 {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR channels must be greater than 0".to_string(),
            ));
        }
        let language = validate_optional_local_streaming_language(language)?;
        Ok(Self::Start {
            session_id,
            sample_rate_hz,
            channels,
            language,
        })
    }

    pub fn audio(
        session_id: impl Into<String>,
        sequence: u64,
        pcm_bytes: &[u8],
    ) -> Result<Self, TalkError> {
        let session_id = validate_local_streaming_session_id(session_id.into())?;
        if pcm_bytes.is_empty() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR PCM chunk must not be empty".to_string(),
            ));
        }
        Ok(Self::Audio {
            session_id,
            sequence,
            pcm_base64: base64::engine::general_purpose::STANDARD.encode(pcm_bytes),
        })
    }

    pub fn stop(session_id: impl Into<String>) -> Result<Self, TalkError> {
        Ok(Self::Stop {
            session_id: validate_local_streaming_session_id(session_id.into())?,
        })
    }

    pub fn cancel(session_id: impl Into<String>) -> Result<Self, TalkError> {
        Ok(Self::Cancel {
            session_id: validate_local_streaming_session_id(session_id.into())?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalStreamingAsrReady {
    pub engine: String,
    pub model: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalStreamingAsrServerMessage {
    Ready(LocalStreamingAsrReady),
    Partial {
        session_id: String,
        segment_id: String,
        text: String,
    },
    Final {
        session_id: String,
        segment_id: String,
        text: String,
    },
    Error {
        session_id: String,
        message: String,
    },
}

type LocalStreamingAsrSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub struct LocalStreamingAsrServiceClient {
    socket: LocalStreamingAsrSocket,
}

impl LocalStreamingAsrServiceClient {
    pub async fn connect(endpoint: &str, connect_timeout: Duration) -> Result<Self, TalkError> {
        validate_local_streaming_endpoint(endpoint)?;
        if connect_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR connect timeout must be greater than 0".to_string(),
            ));
        }
        let connect_result = tokio::time::timeout(connect_timeout, connect_async(endpoint))
            .await
            .map_err(|_| {
                TalkError::Provider(format!(
                    "timed out connecting to local streaming ASR service at {endpoint}"
                ))
            })?;
        let (socket, _) = connect_result.map_err(|error| {
            TalkError::Provider(format!(
                "failed to connect to local streaming ASR service at {endpoint}: {error}"
            ))
        })?;
        Ok(Self { socket })
    }

    pub async fn start(
        &mut self,
        session_id: impl Into<String>,
        sample_rate_hz: u32,
        channels: u16,
        language: Option<&str>,
        ready_timeout: Duration,
    ) -> Result<LocalStreamingAsrReady, TalkError> {
        if ready_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR ready timeout must be greater than 0".to_string(),
            ));
        }
        self.send_client_message(LocalStreamingAsrClientMessage::start(
            session_id,
            sample_rate_hz,
            channels,
            language,
        )?)
        .await?;
        match self.next_server_message(ready_timeout).await? {
            LocalStreamingAsrServerMessage::Ready(ready) => Ok(ready),
            LocalStreamingAsrServerMessage::Error {
                session_id,
                message,
            } => Err(TalkError::Provider(format!(
                "local streaming ASR service error for session {session_id}: {message}"
            ))),
            other => Err(TalkError::Provider(format!(
                "local streaming ASR service sent {other:?} before ready"
            ))),
        }
    }

    pub async fn send_audio(
        &mut self,
        session_id: impl Into<String>,
        sequence: u64,
        pcm_bytes: &[u8],
    ) -> Result<(), TalkError> {
        self.send_client_message(LocalStreamingAsrClientMessage::audio(
            session_id, sequence, pcm_bytes,
        )?)
        .await
    }

    pub async fn stop(&mut self, session_id: impl Into<String>) -> Result<(), TalkError> {
        self.send_client_message(LocalStreamingAsrClientMessage::stop(session_id)?)
            .await
    }

    pub async fn cancel(&mut self, session_id: impl Into<String>) -> Result<(), TalkError> {
        self.send_client_message(LocalStreamingAsrClientMessage::cancel(session_id)?)
            .await
    }

    pub async fn collect_asr_events_until_final(
        &mut self,
        final_timeout: Duration,
    ) -> Result<Vec<StreamingAsrEvent>, TalkError> {
        if final_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR final timeout must be greater than 0".to_string(),
            ));
        }
        let mut events = Vec::new();
        loop {
            let message = self.next_server_message(final_timeout).await?;
            if let Some(event) = local_streaming_server_message_to_asr_event(message)? {
                let is_final = event.is_final();
                events.push(event);
                if is_final {
                    return Ok(events);
                }
            }
        }
    }

    pub async fn collect_available_asr_events_until_idle(
        &mut self,
        idle_timeout: Duration,
    ) -> Result<Vec<StreamingAsrEvent>, TalkError> {
        if idle_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR idle timeout must be greater than 0".to_string(),
            ));
        }
        let mut events = Vec::new();
        loop {
            let Some(message) = self.try_next_server_message(idle_timeout).await? else {
                return Ok(events);
            };
            if let Some(event) = local_streaming_server_message_to_asr_event(message)? {
                let is_final = event.is_final();
                events.push(event);
                if is_final {
                    return Ok(events);
                }
            }
        }
    }

    pub async fn next_server_message(
        &mut self,
        receive_timeout: Duration,
    ) -> Result<LocalStreamingAsrServerMessage, TalkError> {
        match self.try_next_server_message(receive_timeout).await? {
            Some(message) => Ok(message),
            None => Err(TalkError::Provider(
                "timed out waiting for local streaming ASR service message".to_string(),
            )),
        }
    }

    pub async fn try_next_server_message(
        &mut self,
        receive_timeout: Duration,
    ) -> Result<Option<LocalStreamingAsrServerMessage>, TalkError> {
        if receive_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR receive timeout must be greater than 0".to_string(),
            ));
        }
        loop {
            let next = match tokio::time::timeout(receive_timeout, self.socket.next()).await {
                Ok(next) => next,
                Err(_) => return Ok(None),
            };
            let Some(message) = next else {
                return Err(TalkError::Provider(
                    "local streaming ASR service closed the connection".to_string(),
                ));
            };
            let message = message.map_err(|error| {
                TalkError::Provider(format!(
                    "failed to read local streaming ASR service message: {error}"
                ))
            })?;
            match message {
                Message::Text(text) => {
                    return parse_local_streaming_asr_server_message(&text).map(Some);
                }
                Message::Binary(bytes) => {
                    let text = String::from_utf8(bytes.to_vec()).map_err(|error| {
                        TalkError::Provider(format!(
                            "local streaming ASR binary message must be UTF-8 JSON: {error}"
                        ))
                    })?;
                    return parse_local_streaming_asr_server_message(&text).map(Some);
                }
                Message::Ping(payload) => {
                    self.socket
                        .send(Message::Pong(payload))
                        .await
                        .map_err(|error| {
                            TalkError::Provider(format!(
                                "failed to answer local streaming ASR ping: {error}"
                            ))
                        })?;
                }
                Message::Pong(_) => {}
                Message::Close(_) => {
                    return Err(TalkError::Provider(
                        "local streaming ASR service closed the connection".to_string(),
                    ));
                }
                Message::Frame(_) => {}
            }
        }
    }

    async fn send_client_message(
        &mut self,
        message: LocalStreamingAsrClientMessage,
    ) -> Result<(), TalkError> {
        let json = serialize_local_streaming_asr_client_message(&message)?;
        self.socket
            .send(Message::Text(json.into()))
            .await
            .map_err(|error| {
                TalkError::Provider(format!(
                    "failed to send local streaming ASR client message: {error}"
                ))
            })
    }
}

#[derive(Debug, Deserialize)]
struct LocalStreamingAsrServerJsonMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    segment_id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    engine: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    sample_rate_hz: Option<u32>,
    #[serde(default)]
    channels: Option<u16>,
    #[serde(default)]
    message: Option<String>,
}

pub fn serialize_local_streaming_asr_client_message(
    message: &LocalStreamingAsrClientMessage,
) -> Result<String, TalkError> {
    serde_json::to_string(message).map_err(|error| {
        TalkError::Provider(format!(
            "failed to serialize local streaming ASR client message: {error}"
        ))
    })
}

pub fn parse_local_streaming_asr_server_message(
    raw: &str,
) -> Result<LocalStreamingAsrServerMessage, TalkError> {
    let item: LocalStreamingAsrServerJsonMessage = serde_json::from_str(raw).map_err(|error| {
        TalkError::Provider(format!(
            "invalid local streaming ASR server json message: {error}"
        ))
    })?;
    match item.kind.as_str() {
        "ready" => Ok(LocalStreamingAsrServerMessage::Ready(
            LocalStreamingAsrReady {
                engine: required_local_streaming_string(item.engine, "engine", "ready")?,
                model: required_local_streaming_string(item.model, "model", "ready")?,
                sample_rate_hz: required_local_streaming_positive_u32(
                    item.sample_rate_hz,
                    "sample_rate_hz",
                    "ready",
                )?,
                channels: required_local_streaming_positive_u16(
                    item.channels,
                    "channels",
                    "ready",
                )?,
            },
        )),
        "partial" => Ok(LocalStreamingAsrServerMessage::Partial {
            session_id: required_local_streaming_session_id(item.session_id, "partial")?,
            segment_id: required_local_streaming_string(item.segment_id, "segment_id", "partial")?,
            text: required_local_streaming_string(item.text, "text", "partial")?,
        }),
        "final" => Ok(LocalStreamingAsrServerMessage::Final {
            session_id: required_local_streaming_session_id(item.session_id, "final")?,
            segment_id: required_local_streaming_string(item.segment_id, "segment_id", "final")?,
            text: required_local_streaming_string(item.text, "text", "final")?,
        }),
        "error" => Ok(LocalStreamingAsrServerMessage::Error {
            session_id: required_local_streaming_session_id(item.session_id, "error")?,
            message: required_local_streaming_string(item.message, "message", "error")?,
        }),
        other => Err(TalkError::Provider(format!(
            "unknown local streaming ASR server message type: {other}"
        ))),
    }
}

pub fn local_streaming_server_message_to_asr_event(
    message: LocalStreamingAsrServerMessage,
) -> Result<Option<StreamingAsrEvent>, TalkError> {
    match message {
        LocalStreamingAsrServerMessage::Ready(_) => Ok(None),
        LocalStreamingAsrServerMessage::Partial {
            segment_id, text, ..
        } => StreamingAsrEvent::try_partial(segment_id, text).map(Some),
        LocalStreamingAsrServerMessage::Final {
            segment_id, text, ..
        } => StreamingAsrEvent::try_final(segment_id, text).map(Some),
        LocalStreamingAsrServerMessage::Error {
            session_id,
            message,
        } => Err(TalkError::Provider(format!(
            "local streaming ASR service error for session {session_id}: {message}"
        ))),
    }
}

fn validate_local_streaming_session_id(session_id: String) -> Result<String, TalkError> {
    if session_id.trim().is_empty() {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR session_id must not be blank".to_string(),
        ));
    }
    if session_id.trim() != session_id {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR session_id must not have leading or trailing whitespace"
                .to_string(),
        ));
    }
    Ok(session_id)
}

fn validate_local_streaming_endpoint(endpoint: &str) -> Result<(), TalkError> {
    if endpoint.trim().is_empty() {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR endpoint must not be blank".to_string(),
        ));
    }
    if endpoint.trim() != endpoint {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR endpoint must not have leading or trailing whitespace".to_string(),
        ));
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR endpoint must not contain whitespace".to_string(),
        ));
    }
    if !endpoint
        .split_once("://")
        .is_some_and(|(scheme, rest)| scheme.eq_ignore_ascii_case("ws") && !rest.is_empty())
    {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR endpoint must use ws scheme".to_string(),
        ));
    }
    let host = local_streaming_endpoint_host(endpoint).ok_or_else(|| {
        TalkError::InvalidConfig("local streaming ASR endpoint must include a host".to_string())
    })?;
    if !local_streaming_endpoint_host_is_loopback(host) {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR endpoint host must be loopback".to_string(),
        ));
    }
    Ok(())
}

fn local_streaming_endpoint_host(endpoint: &str) -> Option<&str> {
    let (_, rest) = endpoint.split_once("://")?;
    let authority_end = rest
        .find(|ch| ['/', '?', '#'].contains(&ch))
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.contains('@') {
        return None;
    }
    if let Some(bracketed) = authority.strip_prefix('[') {
        let closing = bracketed.find(']')?;
        return Some(&bracketed[..closing]);
    }
    Some(
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host),
    )
}

fn local_streaming_endpoint_host_is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

fn validate_optional_local_streaming_language(
    language: Option<&str>,
) -> Result<Option<String>, TalkError> {
    match language {
        Some(language) if language.trim().is_empty() => Err(TalkError::InvalidConfig(
            "local streaming ASR language must not be blank".to_string(),
        )),
        Some(language) if language.trim() != language => Err(TalkError::InvalidConfig(
            "local streaming ASR language must not have leading or trailing whitespace".to_string(),
        )),
        Some(language) => Ok(Some(language.to_string())),
        None => Ok(None),
    }
}

fn required_local_streaming_session_id(
    value: Option<String>,
    message_type: &str,
) -> Result<String, TalkError> {
    let value = required_local_streaming_string(value, "session_id", message_type)?;
    validate_local_streaming_session_id(value).map_err(|error| {
        TalkError::Provider(format!(
            "invalid local streaming ASR {message_type} session_id: {error}"
        ))
    })
}

fn required_local_streaming_string(
    value: Option<String>,
    field: &str,
    message_type: &str,
) -> Result<String, TalkError> {
    let Some(value) = value else {
        return Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message missing {field}"
        )));
    };
    if value.trim().is_empty() {
        return Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message {field} must not be blank"
        )));
    }
    if value.trim() != value {
        return Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message {field} must not have leading or trailing whitespace"
        )));
    }
    Ok(value)
}

fn required_local_streaming_positive_u32(
    value: Option<u32>,
    field: &str,
    message_type: &str,
) -> Result<u32, TalkError> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message {field} must be greater than 0"
        ))),
        None => Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message missing {field}"
        ))),
    }
}

fn required_local_streaming_positive_u16(
    value: Option<u16>,
    field: &str,
    message_type: &str,
) -> Result<u16, TalkError> {
    match value {
        Some(value) if value > 0 => Ok(value),
        Some(_) => Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message {field} must be greater than 0"
        ))),
        None => Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message missing {field}"
        ))),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingAsrEvent {
    Partial { segment_id: String, text: String },
    Final { segment_id: String, text: String },
}

impl StreamingAsrEvent {
    pub fn partial(segment_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::try_partial(segment_id, text).expect("valid static partial ASR event")
    }

    pub fn final_segment(segment_id: impl Into<String>, text: impl Into<String>) -> Self {
        Self::try_final(segment_id, text).expect("valid static final ASR event")
    }

    pub fn try_partial(
        segment_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Self, TalkError> {
        Self::new(segment_id, text, false)
    }

    pub fn try_final(
        segment_id: impl Into<String>,
        text: impl Into<String>,
    ) -> Result<Self, TalkError> {
        Self::new(segment_id, text, true)
    }

    fn new(
        segment_id: impl Into<String>,
        text: impl Into<String>,
        final_segment: bool,
    ) -> Result<Self, TalkError> {
        let segment_id = segment_id.into();
        let text = text.into();
        if segment_id.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "streaming ASR segment id must not be blank".to_string(),
            ));
        }
        if text.trim().is_empty() {
            return Err(TalkError::InvalidConfig(
                "streaming ASR text must not be blank".to_string(),
            ));
        }
        Ok(if final_segment {
            Self::Final { segment_id, text }
        } else {
            Self::Partial { segment_id, text }
        })
    }

    pub fn segment_id(&self) -> &str {
        match self {
            Self::Partial { segment_id, .. } | Self::Final { segment_id, .. } => segment_id,
        }
    }

    pub fn text(&self) -> &str {
        match self {
            Self::Partial { text, .. } | Self::Final { text, .. } => text,
        }
    }

    pub fn is_final(&self) -> bool {
        matches!(self, Self::Final { .. })
    }
}

pub trait StreamingAsrEngine {
    fn next_event(&mut self) -> Option<StreamingAsrEvent>;
}

pub struct MockStreamingAsrEngine {
    events: VecDeque<StreamingAsrEvent>,
}

impl MockStreamingAsrEngine {
    pub fn new(events: Vec<StreamingAsrEvent>) -> Self {
        Self {
            events: events.into(),
        }
    }
}

impl StreamingAsrEngine for MockStreamingAsrEngine {
    fn next_event(&mut self) -> Option<StreamingAsrEvent> {
        self.events.pop_front()
    }
}

#[derive(Debug, serde::Deserialize)]
struct ExternalAsrJsonLine {
    #[serde(rename = "type")]
    kind: String,
    segment_id: String,
    text: String,
}

pub fn parse_streaming_asr_json_line(line: &str) -> Result<StreamingAsrEvent, TalkError> {
    let item: ExternalAsrJsonLine = serde_json::from_str(line).map_err(|error| {
        TalkError::Provider(format!("invalid streaming ASR json line: {error}"))
    })?;
    match item.kind.as_str() {
        "partial" => StreamingAsrEvent::try_partial(item.segment_id, item.text),
        "final" => StreamingAsrEvent::try_final(item.segment_id, item.text),
        other => Err(TalkError::Provider(format!(
            "unknown streaming ASR event type: {other}"
        ))),
    }
}

pub fn final_transcript_from_streaming_asr_events(
    events: &[StreamingAsrEvent],
) -> Result<String, TalkError> {
    events
        .iter()
        .rev()
        .find(|event| event.is_final())
        .or_else(|| events.last())
        .map(|event| event.text().to_string())
        .ok_or_else(|| {
            TalkError::Provider("external streaming ASR command produced no events".to_string())
        })
}

pub fn run_external_streaming_asr_command(
    command_line: &str,
    audio_path: &Path,
) -> Result<Vec<StreamingAsrEvent>, TalkError> {
    if command_line.trim().is_empty() {
        return Err(TalkError::InvalidConfig(
            "external streaming ASR command must not be blank".to_string(),
        ));
    }
    if audio_path.as_os_str().is_empty()
        || audio_path.as_os_str().to_string_lossy().trim().is_empty()
    {
        return Err(TalkError::InvalidConfig(
            "external streaming ASR audio path must not be blank".to_string(),
        ));
    }

    let rendered_command = render_external_asr_command(command_line, audio_path);
    let mut command = shell_command(&rendered_command);
    command
        .env("TALK_LOCAL_ASR_AUDIO_FILE", audio_path)
        .env("TALK_LOCAL_ASR_OUTPUT", "jsonl");
    let output = command.output().map_err(|error| {
        TalkError::Provider(format!(
            "failed to run external streaming ASR command: {error}"
        ))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TalkError::Provider(format!(
            "external streaming ASR command exited with {}: {}",
            output.status,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8(output.stdout).map_err(|error| {
        TalkError::Provider(format!(
            "external streaming ASR stdout must be UTF-8 JSON lines: {error}"
        ))
    })?;
    let events = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_streaming_asr_json_line)
        .collect::<Result<Vec<_>, TalkError>>()?;
    if events.is_empty() {
        return Err(TalkError::Provider(
            "external streaming ASR command produced no events".to_string(),
        ));
    }
    Ok(events)
}

fn render_external_asr_command(command_line: &str, audio_path: &Path) -> String {
    if command_line.contains("{audio_path}") {
        command_line.replace("{audio_path}", &quote_shell_argument(audio_path))
    } else {
        command_line.to_string()
    }
}

fn quote_shell_argument(path: &Path) -> String {
    let value = path.display().to_string();
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(windows)]
fn shell_command(command_line: &str) -> Command {
    let mut command = Command::new("cmd");
    command.arg("/C").arg(command_line);
    command
}

#[cfg(not(windows))]
fn shell_command(command_line: &str) -> Command {
    let mut command = Command::new("sh");
    command.arg("-c").arg(command_line);
    command
}
