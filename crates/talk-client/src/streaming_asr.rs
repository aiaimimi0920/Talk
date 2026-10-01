use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use talk_core::TalkError;
use tokio::net::TcpStream;
use tokio::time::Instant;
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
    audio_base64_scratch: String,
    active_session_id: Option<String>,
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
        Ok(Self {
            socket,
            audio_base64_scratch: String::new(),
            active_session_id: None,
        })
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
        let session_id = session_id.into();
        self.active_session_id = None;
        self.send_client_message(LocalStreamingAsrClientMessage::start(
            session_id.clone(),
            sample_rate_hz,
            channels,
            language,
        )?)
        .await?;
        match self.next_server_message(ready_timeout).await? {
            LocalStreamingAsrServerMessage::Ready(ready) => {
                self.active_session_id = Some(session_id);
                Ok(ready)
            }
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
        session_id: impl AsRef<str>,
        sequence: u64,
        pcm_bytes: &[u8],
    ) -> Result<(), TalkError> {
        let json = serialize_local_streaming_asr_audio_message(
            session_id.as_ref(),
            sequence,
            pcm_bytes,
            &mut self.audio_base64_scratch,
        )?;
        self.send_client_json(json).await
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
        const MAX_MESSAGES_UNTIL_FINAL: usize = 4_096;

        let deadline = Instant::now() + final_timeout;
        let mut events = Vec::new();
        let mut messages_received = 0usize;
        let expected_session_id = self.active_session_id.clone();
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(TalkError::Provider(
                    "timed out waiting for local streaming ASR service message".to_string(),
                ));
            }
            let message = self
                .try_next_server_item(remaining, |raw| {
                    parse_local_streaming_asr_server_event_for_session(
                        raw,
                        expected_session_id.as_deref(),
                    )
                })
                .await?
                .ok_or_else(|| {
                    TalkError::Provider(
                        "timed out waiting for local streaming ASR service message".to_string(),
                    )
                })?;
            messages_received += 1;
            if let Some(event) = message {
                let is_final = event.is_final();
                push_coalesced_asr_event(&mut events, event);
                if is_final {
                    return Ok(events);
                }
            }
            if messages_received >= MAX_MESSAGES_UNTIL_FINAL {
                return Err(TalkError::Provider(format!(
                    "local streaming ASR service exceeded {MAX_MESSAGES_UNTIL_FINAL} messages without a final result"
                )));
            }
        }
    }

    /// Safety backstop bounding how many messages one non-blocking drain will
    /// consume. Normal ticks drain only a handful before hitting the idle gap;
    /// this cap ensures a daemon streaming partials faster than `idle_timeout`
    /// cannot spin this loop indefinitely (which, on the desktop, would hold the
    /// shared-state lock on the UI thread). Remaining messages drain next tick.
    const MAX_MESSAGES_PER_AVAILABLE_DRAIN: usize = 256;

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
        let mut messages_drained = 0usize;
        let expected_session_id = self.active_session_id.clone();
        loop {
            let Some(message) = self
                .try_next_server_item(idle_timeout, |raw| {
                    parse_local_streaming_asr_server_event_for_session(
                        raw,
                        expected_session_id.as_deref(),
                    )
                })
                .await?
            else {
                return Ok(events);
            };
            messages_drained += 1;
            if let Some(event) = message {
                let is_final = event.is_final();
                push_coalesced_asr_event(&mut events, event);
                if is_final {
                    return Ok(events);
                }
            }
            if messages_drained >= Self::MAX_MESSAGES_PER_AVAILABLE_DRAIN {
                return Ok(events);
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
        self.try_next_server_item(receive_timeout, parse_local_streaming_asr_server_message)
            .await
    }

    async fn try_next_server_item<T, F>(
        &mut self,
        receive_timeout: Duration,
        parser: F,
    ) -> Result<Option<T>, TalkError>
    where
        F: Fn(&str) -> Result<T, TalkError>,
    {
        if receive_timeout.is_zero() {
            return Err(TalkError::InvalidConfig(
                "local streaming ASR receive timeout must be greater than 0".to_string(),
            ));
        }
        let deadline = Instant::now() + receive_timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let next = match tokio::time::timeout(remaining, self.socket.next()).await {
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
                    return parser(&text).map(Some);
                }
                Message::Binary(bytes) => {
                    return parser(local_streaming_asr_binary_server_text(bytes.as_ref())?)
                        .map(Some);
                }
                Message::Ping(payload) => {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Ok(None);
                    }
                    let send_result =
                        tokio::time::timeout(remaining, self.socket.send(Message::Pong(payload)))
                            .await;
                    match send_result {
                        Ok(result) => result.map_err(|error| {
                            TalkError::Provider(format!(
                                "failed to answer local streaming ASR ping: {error}"
                            ))
                        })?,
                        Err(_) => return Ok(None),
                    }
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
        self.send_client_json(json).await
    }

    async fn send_client_json(&mut self, json: String) -> Result<(), TalkError> {
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

fn push_coalesced_asr_event(events: &mut Vec<StreamingAsrEvent>, event: StreamingAsrEvent) {
    if let (
        Some(StreamingAsrEvent::Partial {
            segment_id: previous_segment_id,
            ..
        }),
        StreamingAsrEvent::Partial { segment_id, .. },
    ) = (events.last_mut(), &event)
    {
        if previous_segment_id == segment_id {
            *events.last_mut().expect("last event exists") = event;
            return;
        }
    }
    events.push(event);
}

#[derive(Debug, Deserialize)]
struct LocalStreamingAsrServerJsonMessage<'a> {
    #[serde(rename = "type", borrow)]
    kind: LocalStreamingAsrJsonString<'a>,
    #[serde(default, borrow)]
    session_id: Option<LocalStreamingAsrJsonString<'a>>,
    #[serde(default, borrow)]
    segment_id: Option<LocalStreamingAsrJsonString<'a>>,
    #[serde(default, borrow)]
    text: Option<LocalStreamingAsrJsonString<'a>>,
    #[serde(default, borrow)]
    engine: Option<LocalStreamingAsrJsonString<'a>>,
    #[serde(default, borrow)]
    model: Option<LocalStreamingAsrJsonString<'a>>,
    #[serde(default)]
    sample_rate_hz: Option<u32>,
    #[serde(default)]
    channels: Option<u16>,
    #[serde(default, borrow)]
    message: Option<LocalStreamingAsrJsonString<'a>>,
}

#[derive(Debug)]
struct LocalStreamingAsrJsonString<'a>(Cow<'a, str>);

impl LocalStreamingAsrJsonString<'_> {
    fn as_str(&self) -> &str {
        self.0.as_ref()
    }

    fn into_owned(self) -> String {
        self.0.into_owned()
    }
}

impl<'de: 'a, 'a> Deserialize<'de> for LocalStreamingAsrJsonString<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct JsonStringVisitor<'a>(std::marker::PhantomData<&'a str>);

        impl<'de: 'a, 'a> serde::de::Visitor<'de> for JsonStringVisitor<'a> {
            type Value = LocalStreamingAsrJsonString<'a>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON string")
            }

            fn visit_borrowed_str<E>(self, value: &'de str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(LocalStreamingAsrJsonString(Cow::Borrowed(value)))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(LocalStreamingAsrJsonString(Cow::Owned(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(LocalStreamingAsrJsonString(Cow::Owned(value)))
            }
        }

        deserializer.deserialize_str(JsonStringVisitor(std::marker::PhantomData))
    }
}

pub fn serialize_local_streaming_asr_client_message(
    message: &LocalStreamingAsrClientMessage,
) -> Result<String, TalkError> {
    serialize_local_streaming_asr_json(message)
}

#[derive(Serialize)]
struct LocalStreamingAsrAudioJsonMessage<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    session_id: &'a str,
    sequence: u64,
    pcm_base64: &'a str,
}

fn serialize_local_streaming_asr_audio_message(
    session_id: &str,
    sequence: u64,
    pcm_bytes: &[u8],
    pcm_base64_scratch: &mut String,
) -> Result<String, TalkError> {
    validate_local_streaming_session_id_ref(session_id)?;
    if pcm_bytes.is_empty() {
        return Err(TalkError::InvalidConfig(
            "local streaming ASR PCM chunk must not be empty".to_string(),
        ));
    }

    pcm_base64_scratch.clear();
    base64::engine::general_purpose::STANDARD.encode_string(pcm_bytes, pcm_base64_scratch);
    serialize_local_streaming_asr_json(&LocalStreamingAsrAudioJsonMessage {
        kind: "audio",
        session_id,
        sequence,
        pcm_base64: pcm_base64_scratch,
    })
}

fn serialize_local_streaming_asr_json(message: &impl Serialize) -> Result<String, TalkError> {
    serde_json::to_string(message).map_err(|error| {
        TalkError::Provider(format!(
            "failed to serialize local streaming ASR client message: {error}"
        ))
    })
}

pub fn parse_local_streaming_asr_server_message(
    raw: &str,
) -> Result<LocalStreamingAsrServerMessage, TalkError> {
    let item = deserialize_local_streaming_asr_server_json(raw)?;
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

#[cfg(test)]
fn parse_local_streaming_asr_server_event(
    raw: &str,
) -> Result<Option<StreamingAsrEvent>, TalkError> {
    parse_local_streaming_asr_server_event_for_session(raw, None)
}

fn parse_local_streaming_asr_server_event_for_session(
    raw: &str,
    expected_session_id: Option<&str>,
) -> Result<Option<StreamingAsrEvent>, TalkError> {
    let item = deserialize_local_streaming_asr_server_json(raw)?;
    match item.kind.as_str() {
        "ready" => {
            required_local_streaming_wire_string(item.engine, "engine", "ready")?;
            required_local_streaming_wire_string(item.model, "model", "ready")?;
            required_local_streaming_positive_u32(item.sample_rate_hz, "sample_rate_hz", "ready")?;
            required_local_streaming_positive_u16(item.channels, "channels", "ready")?;
            Ok(None)
        }
        "partial" => {
            let session_id = required_local_streaming_session_id_wire(item.session_id, "partial")?;
            validate_local_streaming_event_session_id(
                expected_session_id,
                session_id.as_str(),
                "partial",
            )?;
            StreamingAsrEvent::try_partial(
                required_local_streaming_wire_string(item.segment_id, "segment_id", "partial")?
                    .into_owned(),
                required_local_streaming_wire_string(item.text, "text", "partial")?.into_owned(),
            )
            .map(Some)
        }
        "final" => {
            let session_id = required_local_streaming_session_id_wire(item.session_id, "final")?;
            validate_local_streaming_event_session_id(
                expected_session_id,
                session_id.as_str(),
                "final",
            )?;
            StreamingAsrEvent::try_final(
                required_local_streaming_wire_string(item.segment_id, "segment_id", "final")?
                    .into_owned(),
                required_local_streaming_wire_string(item.text, "text", "final")?.into_owned(),
            )
            .map(Some)
        }
        "error" => {
            let session_id = required_local_streaming_session_id_wire(item.session_id, "error")?;
            validate_local_streaming_event_session_id(
                expected_session_id,
                session_id.as_str(),
                "error",
            )?;
            let message = required_local_streaming_wire_string(item.message, "message", "error")?;
            Err(TalkError::Provider(format!(
                "local streaming ASR service error for session {}: {}",
                session_id.as_str(),
                message.as_str()
            )))
        }
        other => Err(TalkError::Provider(format!(
            "unknown local streaming ASR server message type: {other}"
        ))),
    }
}

fn validate_local_streaming_event_session_id(
    expected_session_id: Option<&str>,
    actual_session_id: &str,
    message_type: &str,
) -> Result<(), TalkError> {
    if let Some(expected_session_id) = expected_session_id {
        if actual_session_id != expected_session_id {
            return Err(TalkError::Provider(format!(
                "local streaming ASR {message_type} session_id {actual_session_id} does not match active session {expected_session_id}"
            )));
        }
    }
    Ok(())
}

fn deserialize_local_streaming_asr_server_json(
    raw: &str,
) -> Result<LocalStreamingAsrServerJsonMessage<'_>, TalkError> {
    serde_json::from_str(raw).map_err(|error| {
        TalkError::Provider(format!(
            "invalid local streaming ASR server json message: {error}"
        ))
    })
}

#[cfg(test)]
fn parse_local_streaming_asr_binary_server_message(
    bytes: &[u8],
) -> Result<LocalStreamingAsrServerMessage, TalkError> {
    let text = local_streaming_asr_binary_server_text(bytes)?;
    parse_local_streaming_asr_server_message(text)
}

fn local_streaming_asr_binary_server_text(bytes: &[u8]) -> Result<&str, TalkError> {
    std::str::from_utf8(bytes).map_err(|error| {
        TalkError::Provider(format!(
            "local streaming ASR binary message must be UTF-8 JSON: {error}"
        ))
    })
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
    validate_local_streaming_session_id_ref(&session_id)?;
    Ok(session_id)
}

fn validate_local_streaming_session_id_ref(session_id: &str) -> Result<(), TalkError> {
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
    Ok(())
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
    value: Option<LocalStreamingAsrJsonString<'_>>,
    message_type: &str,
) -> Result<String, TalkError> {
    required_local_streaming_session_id_wire(value, message_type)
        .map(LocalStreamingAsrJsonString::into_owned)
}

fn required_local_streaming_session_id_wire<'a>(
    value: Option<LocalStreamingAsrJsonString<'a>>,
    message_type: &str,
) -> Result<LocalStreamingAsrJsonString<'a>, TalkError> {
    let value = required_local_streaming_wire_string(value, "session_id", message_type)?;
    validate_local_streaming_session_id_ref(value.as_str()).map_err(|error| {
        TalkError::Provider(format!(
            "invalid local streaming ASR {message_type} session_id: {error}"
        ))
    })?;
    Ok(value)
}

fn required_local_streaming_string(
    value: Option<LocalStreamingAsrJsonString<'_>>,
    field: &str,
    message_type: &str,
) -> Result<String, TalkError> {
    required_local_streaming_wire_string(value, field, message_type)
        .map(LocalStreamingAsrJsonString::into_owned)
}

fn required_local_streaming_wire_string<'a>(
    value: Option<LocalStreamingAsrJsonString<'a>>,
    field: &str,
    message_type: &str,
) -> Result<LocalStreamingAsrJsonString<'a>, TalkError> {
    let Some(value) = value else {
        return Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message missing {field}"
        )));
    };
    if value.as_str().trim().is_empty() {
        return Err(TalkError::Provider(format!(
            "local streaming ASR {message_type} message {field} must not be blank"
        )));
    }
    if value.as_str().trim() != value.as_str() {
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
    let mut final_segments = Vec::<(&str, &str)>::new();
    let mut final_segment_indices = HashMap::<&str, usize>::new();
    for event in events.iter().filter(|event| event.is_final()) {
        if let Some(index) = final_segment_indices.get(event.segment_id()).copied() {
            final_segments[index].1 = event.text();
        } else {
            final_segment_indices.insert(event.segment_id(), final_segments.len());
            final_segments.push((event.segment_id(), event.text()));
        }
    }

    if !final_segments.is_empty() {
        let mut transcript = String::new();
        for (_, text) in final_segments {
            append_streaming_asr_transcript_segment(&mut transcript, text);
        }
        return Ok(transcript);
    }

    // A timeout can leave several open segments without a final event. Keep
    // the latest revision of every segment in stream order rather than losing
    // all but the last segment.
    let mut partial_segments = Vec::<(&str, &str)>::new();
    let mut partial_segment_indices = HashMap::<&str, usize>::new();
    for event in events {
        if let Some(index) = partial_segment_indices.get(event.segment_id()).copied() {
            partial_segments[index].1 = event.text();
        } else {
            partial_segment_indices.insert(event.segment_id(), partial_segments.len());
            partial_segments.push((event.segment_id(), event.text()));
        }
    }

    if partial_segments.is_empty() {
        return Err(TalkError::Provider(
            "external streaming ASR command produced no events".to_string(),
        ));
    }
    let mut transcript = String::new();
    for (_, text) in partial_segments {
        append_streaming_asr_transcript_segment(&mut transcript, text);
    }
    if transcript.is_empty() {
        return Err(TalkError::Provider(
            "external streaming ASR command produced only blank events".to_string(),
        ));
    }
    Ok(transcript)
}

fn append_streaming_asr_transcript_segment(transcript: &mut String, segment: &str) {
    let segment = segment.trim();
    if segment.is_empty() {
        return;
    }
    let needs_space = transcript
        .chars()
        .next_back()
        .zip(segment.chars().next())
        .is_some_and(|(left, right)| {
            left.is_ascii() && !left.is_ascii_whitespace() && right.is_ascii_alphanumeric()
        });
    if needs_space {
        transcript.push(' ');
    }
    transcript.push_str(segment);
}

pub const DEFAULT_EXTERNAL_STREAMING_ASR_TIMEOUT: Duration = Duration::from_secs(60);

pub fn run_external_streaming_asr_command(
    command_line: &str,
    audio_path: &Path,
) -> Result<Vec<StreamingAsrEvent>, TalkError> {
    run_external_streaming_asr_command_with_timeout(
        command_line,
        audio_path,
        DEFAULT_EXTERNAL_STREAMING_ASR_TIMEOUT,
    )
}

pub fn run_external_streaming_asr_command_with_timeout(
    command_line: &str,
    audio_path: &Path,
    timeout: Duration,
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
        .env("TALK_LOCAL_ASR_OUTPUT", "jsonl")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        TalkError::Provider(format!(
            "failed to run external streaming ASR command: {error}"
        ))
    })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_thread = stdout.map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    });
    let stderr_thread = stderr.map(|mut pipe| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            bytes
        })
    });

    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(TalkError::Provider(format!(
                        "external streaming ASR command timed out after {}ms",
                        timeout.as_millis()
                    )));
                }
                thread::sleep(Duration::from_millis(20));
            }
            Err(error) => {
                return Err(TalkError::Provider(format!(
                    "failed to wait for external streaming ASR command: {error}"
                )));
            }
        }
    };

    let stdout = stdout_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    let stderr = stderr_thread
        .and_then(|thread| thread.join().ok())
        .unwrap_or_default();
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        return Err(TalkError::Provider(format!(
            "external streaming ASR command exited with {}: {}",
            status,
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8(stdout).map_err(|error| {
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

#[cfg(test)]
mod tests {
    use super::{
        local_streaming_server_message_to_asr_event,
        parse_local_streaming_asr_binary_server_message, parse_local_streaming_asr_server_event,
        parse_local_streaming_asr_server_message, serialize_local_streaming_asr_audio_message,
        serialize_local_streaming_asr_client_message, LocalStreamingAsrClientMessage,
        LocalStreamingAsrJsonString, LocalStreamingAsrServerJsonMessage,
        LocalStreamingAsrServerMessage, StreamingAsrEvent,
    };
    use std::borrow::Cow;

    #[test]
    fn streaming_audio_serializer_reuses_base64_capacity_and_matches_owned_message_json() {
        let pcm_bytes = vec![0x5a; 2_560];
        let mut base64_scratch = String::new();

        let first = serialize_local_streaming_asr_audio_message(
            "session-1",
            7,
            &pcm_bytes,
            &mut base64_scratch,
        )
        .expect("serialize first borrowed audio message");
        let expected_first = serialize_local_streaming_asr_client_message(
            &LocalStreamingAsrClientMessage::audio("session-1", 7, &pcm_bytes)
                .expect("build first owned audio message"),
        )
        .expect("serialize first owned audio message");
        let scratch_pointer = base64_scratch.as_ptr();
        let scratch_capacity = base64_scratch.capacity();

        assert_eq!(first, expected_first);

        let second = serialize_local_streaming_asr_audio_message(
            "session-1",
            8,
            &pcm_bytes,
            &mut base64_scratch,
        )
        .expect("serialize second borrowed audio message");
        let expected_second = serialize_local_streaming_asr_client_message(
            &LocalStreamingAsrClientMessage::audio("session-1", 8, &pcm_bytes)
                .expect("build second owned audio message"),
        )
        .expect("serialize second owned audio message");

        assert_eq!(base64_scratch.as_ptr(), scratch_pointer);
        assert_eq!(base64_scratch.capacity(), scratch_capacity);
        assert_eq!(second, expected_second);
    }

    #[test]
    fn streaming_audio_serializer_preserves_session_and_pcm_validation() {
        let mut base64_scratch = String::new();

        let session_error =
            serialize_local_streaming_asr_audio_message(" ", 0, &[0, 1], &mut base64_scratch)
                .expect_err("blank session id must fail");
        let pcm_error =
            serialize_local_streaming_asr_audio_message("session-1", 0, &[], &mut base64_scratch)
                .expect_err("empty PCM must fail");

        assert!(
            session_error
                .to_string()
                .contains("session_id must not be blank"),
            "error={session_error}"
        );
        assert!(
            pcm_error
                .to_string()
                .contains("PCM chunk must not be empty"),
            "error={pcm_error}"
        );
    }

    #[test]
    fn streaming_server_json_borrows_unescaped_wire_fields() {
        let item: LocalStreamingAsrServerJsonMessage<'_> = serde_json::from_str(
            r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":"你好"}"#,
        )
        .expect("deserialize borrowable server message");

        assert_borrowed_json_string(&item.kind, "partial");
        assert_borrowed_json_string(
            item.session_id.as_ref().expect("borrowed session id"),
            "session-1",
        );
        assert_borrowed_json_string(
            item.segment_id.as_ref().expect("borrowed segment id"),
            "seg-1",
        );
        assert_borrowed_json_string(item.text.as_ref().expect("borrowed text"), "你好");

        let ready: LocalStreamingAsrServerJsonMessage<'_> = serde_json::from_str(
            r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer","sample_rate_hz":16000,"channels":1}"#,
        )
        .expect("deserialize borrowable ready message");
        assert_borrowed_json_string(
            ready.engine.as_ref().expect("borrowed engine"),
            "sherpa-onnx",
        );
        assert_borrowed_json_string(ready.model.as_ref().expect("borrowed model"), "zipformer");

        let error: LocalStreamingAsrServerJsonMessage<'_> = serde_json::from_str(
            r#"{"type":"error","session_id":"session-1","message":"model is not loaded"}"#,
        )
        .expect("deserialize borrowable error message");
        assert_borrowed_json_string(
            error
                .session_id
                .as_ref()
                .expect("borrowed error session id"),
            "session-1",
        );
        assert_borrowed_json_string(
            error.message.as_ref().expect("borrowed error message"),
            "model is not loaded",
        );
    }

    #[test]
    fn streaming_server_json_preserves_escaped_string_compatibility() {
        let raw = r#"{"type":"par\u0074ial","session_id":"session-\u0031","segment_id":"seg-\u0031","text":"line\nquoted \"text\""}"#;
        let item: LocalStreamingAsrServerJsonMessage<'_> =
            serde_json::from_str(raw).expect("deserialize escaped server message");
        assert!(matches!(
            &item.kind.0,
            Cow::Owned(value) if value == "partial"
        ));

        assert_eq!(
            parse_local_streaming_asr_server_message(raw).expect("parse escaped server message"),
            LocalStreamingAsrServerMessage::Partial {
                session_id: "session-1".to_string(),
                segment_id: "seg-1".to_string(),
                text: "line\nquoted \"text\"".to_string(),
            }
        );
        assert_eq!(
            parse_local_streaming_asr_server_event(raw)
                .expect("parse escaped server event message"),
            Some(StreamingAsrEvent::partial("seg-1", "line\nquoted \"text\""))
        );
    }

    #[test]
    fn streaming_borrowed_event_parser_matches_owned_message_conversion() {
        for raw in [
            r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer","sample_rate_hz":16000,"channels":1}"#,
            r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":"你好"}"#,
            r#"{"type":"final","session_id":"session-1","segment_id":"seg-1","text":"你好。"}"#,
        ] {
            let expected = local_streaming_server_message_to_asr_event(
                parse_local_streaming_asr_server_message(raw).expect("parse owned server message"),
            )
            .expect("convert owned server message");

            assert_eq!(
                parse_local_streaming_asr_server_event(raw)
                    .expect("parse borrowed server event message"),
                expected
            );
        }

        let raw = r#"{"type":"error","session_id":"session-1","message":"model is not loaded"}"#;
        let owned_error = local_streaming_server_message_to_asr_event(
            parse_local_streaming_asr_server_message(raw).expect("parse owned error message"),
        )
        .expect_err("owned error message must fail");
        let borrowed_error = parse_local_streaming_asr_server_event(raw)
            .expect_err("borrowed error message must fail");
        assert_eq!(borrowed_error.to_string(), owned_error.to_string());
    }

    #[test]
    fn streaming_borrowed_event_parser_preserves_owned_validation_errors() {
        for raw in [
            r#"{"type":"ready","model":"zipformer","sample_rate_hz":16000,"channels":1}"#,
            r#"{"type":"ready","engine":" ","model":"zipformer","sample_rate_hz":16000,"channels":1}"#,
            r#"{"type":"ready","engine":"sherpa-onnx","model":" zipformer","sample_rate_hz":16000,"channels":1}"#,
            r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer","sample_rate_hz":0,"channels":1}"#,
            r#"{"type":"ready","engine":"sherpa-onnx","model":"zipformer","sample_rate_hz":16000,"channels":0}"#,
            r#"{"type":"partial","segment_id":"seg-1","text":"你好"}"#,
            r#"{"type":"partial","session_id":" ","segment_id":"seg-1","text":"你好"}"#,
            r#"{"type":"partial","session_id":"session-1","segment_id":" seg-1","text":"你好"}"#,
            r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":" "}"#,
            r#"{"type":"final","session_id":"session-1","text":"你好。"}"#,
            r#"{"type":"final","session_id":"session-1","segment_id":"seg-1","text":"你好。 "}"#,
            r#"{"type":"error","message":"model is not loaded"}"#,
            r#"{"type":"error","session_id":"session-1"}"#,
            r#"{"type":"error","session_id":"session-1","message":" model is not loaded"}"#,
        ] {
            let owned_error = parse_local_streaming_asr_server_message(raw)
                .and_then(local_streaming_server_message_to_asr_event)
                .expect_err("owned invalid server event must fail");
            let borrowed_error = parse_local_streaming_asr_server_event(raw)
                .expect_err("borrowed invalid server event must fail");

            assert_eq!(
                borrowed_error.to_string(),
                owned_error.to_string(),
                "raw={raw}"
            );
        }
    }

    #[test]
    fn streaming_server_json_preserves_unknown_message_type_error() {
        let error = parse_local_streaming_asr_server_message(
            r#"{"type":"mystery","session_id":"session-1"}"#,
        )
        .expect_err("unknown server message type must fail");

        assert_eq!(
            error.to_string(),
            "provider error: unknown local streaming ASR server message type: mystery"
        );
    }

    #[test]
    fn streaming_binary_server_json_matches_text_for_partial_and_final_messages() {
        for raw in [
            r#"{"type":"partial","session_id":"session-1","segment_id":"seg-1","text":"你好"}"#,
            r#"{"type":"final","session_id":"session-1","segment_id":"seg-1","text":"你好，世界。"}"#,
        ] {
            assert_eq!(
                parse_local_streaming_asr_binary_server_message(raw.as_bytes())
                    .expect("parse binary server JSON"),
                parse_local_streaming_asr_server_message(raw).expect("parse text server JSON")
            );
        }
    }

    #[test]
    fn streaming_binary_server_json_preserves_invalid_utf8_error_contract() {
        let error = parse_local_streaming_asr_binary_server_message(&[0xff, 0xfe])
            .expect_err("invalid UTF-8 binary server message must fail");

        assert!(
            error
                .to_string()
                .contains("local streaming ASR binary message must be UTF-8 JSON:"),
            "error={error}"
        );
    }

    #[test]
    fn streaming_binary_server_json_preserves_text_json_error_contract() {
        let binary_error = parse_local_streaming_asr_binary_server_message(b"{")
            .expect_err("invalid binary JSON must fail");
        let text_error =
            parse_local_streaming_asr_server_message("{").expect_err("invalid text JSON must fail");

        assert_eq!(binary_error.to_string(), text_error.to_string());
        assert!(
            binary_error
                .to_string()
                .contains("invalid local streaming ASR server json message:"),
            "error={binary_error}"
        );
    }

    #[test]
    fn streaming_binary_receive_path_borrows_payload_bytes() {
        let source = include_str!("streaming_asr.rs");
        let start = source
            .find("                Message::Binary(bytes) =>")
            .expect("binary receive branch");
        let end = source[start..]
            .find("                Message::Ping(payload) =>")
            .map(|offset| start + offset)
            .expect("message branch following binary receive");
        let binary_branch = &source[start..end];

        assert!(binary_branch.contains("local_streaming_asr_binary_server_text(bytes.as_ref())?"));
        assert!(binary_branch.contains("return parser("));
        assert!(!binary_branch.contains(".to_vec()"));
        assert!(!binary_branch.contains("String::from_utf8("));
    }

    fn assert_borrowed_json_string(value: &LocalStreamingAsrJsonString<'_>, expected: &str) {
        assert!(matches!(
            &value.0,
            Cow::Borrowed(actual) if *actual == expected
        ));
    }
}
