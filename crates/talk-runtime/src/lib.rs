use anyhow::{Context, Result};
mod credentials;
mod loom_config;
mod segmenter;
mod speculative;
mod voice_processing;
use credentials::resolve_provider_credential;
pub use segmenter::{
    evaluate_segment_readiness, SegmentReadiness, SegmenterConfig, SegmenterInput,
};
use serde::Serialize;
pub use speculative::{
    run_mock_speculative_session, SpeculativeCorrectionRequest, SpeculativeRuntimeEvent,
    SpeculativeRuntimeState,
};
use std::path::{Path, PathBuf};
use std::time::Duration;
use talk_audio::{
    capture_audio, AudioCaptureRequest, RecordingPcmChunk, RecordingPcmCursor, RecordingSession,
    WavSettings,
};
use talk_client::{
    final_transcript_from_streaming_asr_events, run_external_streaming_asr_command, FrontContext,
    HttpTextProcessor, HttpTranscriber, LocalStreamingAsrServiceClient, MockTranscriber,
    NoopTextProcessor, OpenAiCompatibleTextProcessor, OpenAiCompatibleTranscriber,
    StreamingAsrEvent, TextProcessor, Transcriber,
};
use talk_core::{
    ClipboardBackendMode, OutputMode, ProviderKind, SessionStatus,
    SpeculativeStreamingServiceConfig, TalkConfig, TriggerMode, VoiceEvent, VoiceEventKind,
    VoiceMode, VoiceSession,
};
use talk_hotkey::{HotkeyAction, HotkeyStateMachine};
use talk_insert::{
    ClipboardFallbackInserter, ClipboardPasteInserter, ClipboardRestorePolicy, DryRunInserter,
    InsertMethod, InsertOutcome, TextInserter, WindowsClipboardBackend, WindowsPasteShortcut,
};
use uuid::Uuid;
pub use voice_processing::{
    analyze_smart_voice_mode, count_non_whitespace, count_sentence_boundaries,
    infer_smart_voice_mode, smart_transcribe_fallback_is_stable, validate_faithful_output,
    voice_mode_requires_faithful_output, FaithfulOutputFallbackReason, FaithfulOutputValidation,
    SmartLeadingIntent, SmartRouteEvidence, SmartRouteReason, SmartVoiceRouteAnalysis,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimePhase {
    TriggerArmed,
    Recording,
    Transcribing,
    Processing,
    Inserting,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug)]
pub struct VoiceRunReport {
    pub session: VoiceSession,
    pub outcome: Option<InsertOutcome>,
    pub trigger_events: Vec<&'static str>,
    pub log_path: PathBuf,
    pub requested_mode: VoiceMode,
    pub smart_routed_mode: Option<VoiceMode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeVoiceTextResult {
    pub transcript: Option<String>,
    pub processed_output: Option<String>,
    pub smart_routed_mode: Option<VoiceMode>,
}

pub fn runtime_voice_text_result(report: &VoiceRunReport) -> RuntimeVoiceTextResult {
    RuntimeVoiceTextResult {
        transcript: report.session.transcript().map(str::to_string),
        processed_output: report.session.output_text().map(str::to_string),
        smart_routed_mode: report.smart_routed_mode,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInsertContext {
    pub requested_mode: VoiceMode,
    pub smart_routed_mode: Option<VoiceMode>,
    pub transcript: String,
    pub output_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeInsertDirective {
    UseConfiguredOutput,
    DryRunOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeVoiceModeResolution {
    requested_mode: VoiceMode,
    smart_routed_mode: Option<VoiceMode>,
    processing_mode: VoiceMode,
    smart_route_analysis: Option<SmartVoiceRouteAnalysis>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeProcessedOutput {
    pub text: String,
    pub faithful_validation: Option<FaithfulOutputValidation>,
}

#[derive(Debug, Clone, Copy)]
struct RuntimeProcessingDiagnostics {
    resolution: RuntimeVoiceModeResolution,
    faithful_validation: Option<FaithfulOutputValidation>,
}

fn load_config(path: &Path) -> Result<TalkConfig> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read config {}", path.display()))?;
    TalkConfig::from_toml_str(&raw)
        .with_context(|| format!("failed to parse config {}", path.display()))
}

pub async fn load_effective_config(path: &Path) -> Result<TalkConfig> {
    let local = load_config(path)?;
    let Some(base_url) = std::env::var("TALK_LOOM_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(local);
    };
    let auth_token = std::env::var("TALK_LOOM_AUTH_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    match loom_config::is_talk_managed(&base_url, auth_token.as_deref()).await {
        Ok(false) => Ok(local),
        Ok(true) => {
            match loom_config::read_talk_config(&base_url, auth_token.as_deref()).await {
                Ok(response) if response.created => {
                    match loom_config::write_talk_config(
                        &base_url,
                        auth_token.as_deref(),
                        response.document.revision,
                        &local,
                    )
                    .await
                    {
                        Ok(seeded) => Ok(seeded.config),
                        Err(error) => {
                            eprintln!(
                            "Talk Loom seed write failed; using local read-only fallback: {error}"
                        );
                            Ok(local)
                        }
                    }
                }
                Ok(response) => Ok(response.config),
                Err(error) => {
                    eprintln!("Talk Loom-managed config read failed; using local read-only fallback: {error}");
                    Ok(local)
                }
            }
        }
        Err(error) => {
            eprintln!("Talk Loom claim probe failed; using local config: {error}");
            Ok(local)
        }
    }
}

fn validate_mock_text_override(mock_text: Option<String>) -> Result<Option<String>> {
    match mock_text {
        Some(value) if value.trim().is_empty() => {
            Err(anyhow::anyhow!("mock text override must not be blank"))
        }
        Some(value) if value.trim() != value => Err(anyhow::anyhow!(
            "mock text override must not have leading or trailing whitespace"
        )),
        other => Ok(other),
    }
}

fn resolve_runtime_voice_mode(
    config: &TalkConfig,
    mode_override: Option<VoiceMode>,
    transcript: Option<&str>,
) -> RuntimeVoiceModeResolution {
    resolve_runtime_voice_mode_with_evidence(
        config,
        mode_override,
        transcript,
        SmartRouteEvidence::default(),
    )
}

fn resolve_runtime_voice_mode_with_evidence(
    config: &TalkConfig,
    mode_override: Option<VoiceMode>,
    transcript: Option<&str>,
    route_evidence: SmartRouteEvidence,
) -> RuntimeVoiceModeResolution {
    let requested_mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
    let smart_route_analysis = if requested_mode == VoiceMode::Smart {
        transcript.map(|transcript| analyze_smart_voice_mode(transcript, route_evidence))
    } else {
        None
    };
    let smart_routed_mode = smart_route_analysis.map(|analysis| analysis.resolved_mode);
    let processing_mode = smart_routed_mode.unwrap_or(requested_mode);

    RuntimeVoiceModeResolution {
        requested_mode,
        smart_routed_mode,
        processing_mode,
        smart_route_analysis,
    }
}

fn runtime_insert_context(
    resolution: RuntimeVoiceModeResolution,
    transcript: &str,
    output_text: &str,
) -> RuntimeInsertContext {
    RuntimeInsertContext {
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
        transcript: transcript.to_string(),
        output_text: output_text.to_string(),
    }
}

pub async fn run_voice_session<F>(
    config: &TalkConfig,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    let mock_text = validate_mock_text_override(mock_text)?;
    let mut session = VoiceSession::new(Uuid::new_v4().to_string());
    phase_callback(RuntimePhase::TriggerArmed);
    let trigger_events = apply_configured_trigger_sequence(config, &mut session, |phase| {
        phase_callback(phase);
    })?;
    let audio_artifact = match capture_audio_artifact(config, session.id()) {
        Ok(audio_artifact) => audio_artifact,
        Err(error) => {
            return persist_failed_session_with_mode_override(
                config,
                session,
                &trigger_events,
                error,
                false,
                mode_override,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };

    run_voice_session_from_audio_artifact(
        config,
        session,
        trigger_events,
        audio_artifact.path,
        mock_text,
        mode_override,
        context,
        phase_callback,
    )
    .await
}

pub async fn run_voice_session_with_audio_file<F>(
    config: &TalkConfig,
    audio_path: PathBuf,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    validate_explicit_audio_file(&audio_path)?;

    let mock_text = validate_mock_text_override(mock_text)?;
    let mut session = VoiceSession::new(Uuid::new_v4().to_string());
    phase_callback(RuntimePhase::TriggerArmed);
    let trigger_events = apply_configured_trigger_sequence(config, &mut session, |phase| {
        phase_callback(phase);
    })?;

    run_voice_session_from_audio_artifact(
        config,
        session,
        trigger_events,
        audio_path,
        mock_text,
        mode_override,
        context,
        phase_callback,
    )
    .await
}

pub async fn run_voice_session_from_audio_artifact<F>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    audio_path: PathBuf,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    run_voice_session_from_audio_artifact_with_insert_hook(
        config,
        session,
        trigger_events,
        audio_path,
        mock_text,
        mode_override,
        context,
        |_| RuntimeInsertDirective::UseConfiguredOutput,
        phase_callback,
    )
    .await
}

pub async fn run_voice_session_from_audio_artifact_with_insert_hooks<F, G, H>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    audio_path: PathBuf,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    before_insert: G,
    after_insert: H,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    let transcript = match transcribe_output(config, mock_text, audio_path, context.clone()).await {
        Ok(transcript) => transcript,
        Err(error) => {
            return persist_failed_session_with_mode_override(
                config,
                session,
                &trigger_events,
                error,
                false,
                mode_override,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::TranscriptReady {
        text: transcript.clone(),
    })?;
    phase_callback(RuntimePhase::Processing);

    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
    let processed_output = match process_output_with_diagnostics(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                false,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    let RuntimeProcessedOutput {
        text: output,
        faithful_validation,
    } = processed_output;
    let processing_diagnostics = RuntimeProcessingDiagnostics {
        resolution,
        faithful_validation,
    };
    session.apply(VoiceEvent::ProcessedTextReady {
        text: output.clone(),
    })?;
    phase_callback(RuntimePhase::Inserting);

    let insert_context = runtime_insert_context(resolution, &transcript, &output);
    let outcome = match insert_output_with_hooks(
        config,
        &output,
        &insert_context,
        &before_insert,
        &after_insert,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                true,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(
        config,
        &session,
        Some(&outcome),
        &trigger_events,
        Some(processing_diagnostics),
    )?;

    Ok(VoiceRunReport {
        session,
        outcome: Some(outcome),
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

pub async fn run_voice_session_from_transcript_with_insert_hooks<F, G, H>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    before_insert: G,
    after_insert: H,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    run_voice_session_from_transcript_with_route_evidence_and_insert_hooks(
        config,
        session,
        trigger_events,
        transcript,
        mode_override,
        SmartRouteEvidence::default(),
        context,
        before_insert,
        after_insert,
        phase_callback,
    )
    .await
}

pub async fn run_voice_session_from_transcript_with_route_evidence_and_insert_hooks<F, G, H>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
    mode_override: Option<VoiceMode>,
    route_evidence: SmartRouteEvidence,
    context: FrontContext,
    before_insert: G,
    after_insert: H,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    if transcript.trim().is_empty() {
        return persist_failed_session_with_mode_override(
            config,
            session,
            &trigger_events,
            anyhow::anyhow!("local ASR transcript must not be blank"),
            false,
            mode_override,
            |phase| {
                phase_callback(phase);
            },
        );
    }

    session.apply(VoiceEvent::TranscriptReady {
        text: transcript.clone(),
    })?;
    phase_callback(RuntimePhase::Processing);

    let resolution = resolve_runtime_voice_mode_with_evidence(
        config,
        mode_override,
        Some(&transcript),
        route_evidence,
    );
    let processed_output = match process_output_with_diagnostics(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                false,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    let RuntimeProcessedOutput {
        text: output,
        faithful_validation,
    } = processed_output;
    let processing_diagnostics = RuntimeProcessingDiagnostics {
        resolution,
        faithful_validation,
    };
    session.apply(VoiceEvent::ProcessedTextReady {
        text: output.clone(),
    })?;
    phase_callback(RuntimePhase::Inserting);

    let insert_context = runtime_insert_context(resolution, &transcript, &output);
    let outcome = match insert_output_with_hooks(
        config,
        &output,
        &insert_context,
        &before_insert,
        &after_insert,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                true,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(
        config,
        &session,
        Some(&outcome),
        &trigger_events,
        Some(processing_diagnostics),
    )?;

    Ok(VoiceRunReport {
        session,
        outcome: Some(outcome),
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

pub fn run_voice_session_from_local_transcript_with_insert_hooks<F, G, H>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
    mode_override: Option<VoiceMode>,
    before_insert: G,
    after_insert: H,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    run_voice_session_from_local_transcript_with_route_evidence_and_insert_hooks(
        config,
        session,
        trigger_events,
        transcript,
        mode_override,
        SmartRouteEvidence::default(),
        before_insert,
        after_insert,
        phase_callback,
    )
}

pub fn run_voice_session_from_local_transcript_with_route_evidence_and_insert_hooks<F, G, H>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
    mode_override: Option<VoiceMode>,
    route_evidence: SmartRouteEvidence,
    before_insert: G,
    after_insert: H,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    if transcript.trim().is_empty() {
        return persist_failed_session_with_mode_override(
            config,
            session,
            &trigger_events,
            anyhow::anyhow!("local ASR transcript must not be blank"),
            false,
            mode_override,
            |phase| {
                phase_callback(phase);
            },
        );
    }

    session.apply(VoiceEvent::TranscriptReady {
        text: transcript.clone(),
    })?;
    session.apply(VoiceEvent::ProcessedTextReady {
        text: transcript.clone(),
    })?;
    phase_callback(RuntimePhase::Inserting);

    let resolution = resolve_runtime_voice_mode_with_evidence(
        config,
        mode_override,
        Some(&transcript),
        route_evidence,
    );
    let insert_context = runtime_insert_context(resolution, &transcript, &transcript);
    let outcome = match insert_output_with_hooks(
        config,
        &transcript,
        &insert_context,
        &before_insert,
        &after_insert,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                true,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(
        config,
        &session,
        Some(&outcome),
        &trigger_events,
        Some(RuntimeProcessingDiagnostics {
            resolution,
            faithful_validation: None,
        }),
    )?;

    Ok(VoiceRunReport {
        session,
        outcome: Some(outcome),
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

trait StreamingPcmSource {
    fn drain_pcm_chunk(&self, cursor: &mut RecordingPcmCursor)
        -> Result<Option<RecordingPcmChunk>>;

    fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<()>;
}

impl StreamingPcmSource for RecordingSession {
    fn drain_pcm_chunk(
        &self,
        cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>> {
        RecordingSession::drain_pcm_chunk(self, cursor).map_err(Into::into)
    }

    fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<()> {
        RecordingSession::discard_consumed_pcm(self, cursor).map_err(Into::into)
    }
}

trait StreamingPcmSender {
    async fn send_pcm_chunk(&mut self, chunk: &RecordingPcmChunk) -> Result<()>;
}

#[derive(Debug)]
struct StreamingPcmSendTimeout {
    timeout_ms: u128,
}

impl std::fmt::Display for StreamingPcmSendTimeout {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "local streaming ASR audio send timed out after {} ms",
            self.timeout_ms
        )
    }
}

impl std::error::Error for StreamingPcmSendTimeout {}

struct LocalStreamingClientPcmSender<'a> {
    client: &'a mut LocalStreamingAsrServiceClient,
    session_id: &'a str,
}

impl StreamingPcmSender for LocalStreamingClientPcmSender<'_> {
    async fn send_pcm_chunk(&mut self, chunk: &RecordingPcmChunk) -> Result<()> {
        self.client
            .send_audio(self.session_id, chunk.sequence, &chunk.bytes)
            .await
            .map_err(Into::into)
    }
}

async fn send_available_recording_pcm<S, T>(
    source: &S,
    cursor: &mut RecordingPcmCursor,
    expected_sample_rate_hz: u32,
    expected_channels: u16,
    send_timeout: Duration,
    sender: &mut T,
) -> Result<usize>
where
    S: StreamingPcmSource,
    T: StreamingPcmSender,
{
    send_available_recording_pcm_with_limit(
        source,
        cursor,
        expected_sample_rate_hz,
        expected_channels,
        send_timeout,
        None,
        sender,
    )
    .await
}

async fn send_available_recording_pcm_with_limit<S, T>(
    source: &S,
    cursor: &mut RecordingPcmCursor,
    expected_sample_rate_hz: u32,
    expected_channels: u16,
    send_timeout: Duration,
    max_chunks: Option<usize>,
    sender: &mut T,
) -> Result<usize>
where
    S: StreamingPcmSource,
    T: StreamingPcmSender,
{
    let mut sent_chunks = 0usize;
    loop {
        if max_chunks.is_some_and(|limit| sent_chunks >= limit) {
            break;
        }
        let cursor_before_drain = cursor.clone();
        let Some(chunk) = source.drain_pcm_chunk(cursor)? else {
            break;
        };
        if chunk.sample_rate_hz != expected_sample_rate_hz || chunk.channels != expected_channels {
            *cursor = cursor_before_drain;
            anyhow::bail!(
                "recording PCM chunk format {} Hz / {} channels does not match streaming_service {} Hz / {} channels",
                chunk.sample_rate_hz,
                chunk.channels,
                expected_sample_rate_hz,
                expected_channels
            );
        }
        match tokio::time::timeout(send_timeout, sender.send_pcm_chunk(&chunk)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                *cursor = cursor_before_drain;
                return Err(error);
            }
            Err(_) => {
                *cursor = cursor_before_drain;
                return Err(StreamingPcmSendTimeout {
                    timeout_ms: send_timeout.as_millis(),
                }
                .into());
            }
        }
        source.discard_consumed_pcm(cursor)?;
        sent_chunks = sent_chunks.saturating_add(1);
    }
    Ok(sent_chunks)
}

const MAX_RETAINED_STREAMING_ASR_EVENTS: usize = 128;

fn retain_latest_streaming_asr_events<I>(
    retained: &mut Vec<StreamingAsrEvent>,
    incoming: I,
    max_events: usize,
) where
    I: IntoIterator<Item = StreamingAsrEvent>,
{
    if max_events == 0 {
        retained.clear();
        return;
    }

    retained.extend(incoming);
    let excess = retained.len().saturating_sub(max_events);
    if excess > 0 {
        retained.drain(..excess);
    }
}

pub async fn run_local_streaming_asr_service_from_recording(
    config: &TalkConfig,
    session_id: &str,
    recording: &talk_audio::RecordingSession,
    language: Option<&str>,
) -> Result<Vec<StreamingAsrEvent>> {
    let service = local_streaming_service_config(config)?;

    let mut client = LocalStreamingAsrServiceClient::connect(
        &service.endpoint,
        Duration::from_millis(service.connect_timeout_ms),
    )
    .await?;
    client
        .start(
            session_id,
            service.sample_rate_hz,
            service.channels,
            language,
            Duration::from_millis(service.idle_timeout_ms),
        )
        .await?;

    let mut cursor = RecordingPcmCursor::default();
    let mut sender = LocalStreamingClientPcmSender {
        client: &mut client,
        session_id,
    };
    let sent_chunks = send_available_recording_pcm(
        recording,
        &mut cursor,
        service.sample_rate_hz,
        service.channels,
        Duration::from_millis(service.idle_timeout_ms),
        &mut sender,
    )
    .await?;
    if sent_chunks == 0 {
        anyhow::bail!("recording produced no PCM chunks for streaming_service local ASR");
    }

    client.stop(session_id).await?;
    client
        .collect_asr_events_until_final(Duration::from_millis(service.final_timeout_ms))
        .await
        .map_err(Into::into)
}

pub struct LocalStreamingAsrLiveSession {
    client: LocalStreamingAsrServiceClient,
    cursor: RecordingPcmCursor,
    events: Vec<StreamingAsrEvent>,
    session_id: String,
    sample_rate_hz: u32,
    channels: u16,
    send_timeout: Duration,
    final_timeout: Duration,
}

impl LocalStreamingAsrLiveSession {
    pub async fn start(
        config: &TalkConfig,
        session_id: &str,
        language: Option<&str>,
    ) -> Result<Self> {
        let service = local_streaming_service_config(config)?;
        let mut client = LocalStreamingAsrServiceClient::connect(
            &service.endpoint,
            Duration::from_millis(service.connect_timeout_ms),
        )
        .await?;
        client
            .start(
                session_id,
                service.sample_rate_hz,
                service.channels,
                language,
                Duration::from_millis(service.idle_timeout_ms),
            )
            .await?;

        Ok(Self {
            client,
            cursor: RecordingPcmCursor::default(),
            events: Vec::new(),
            session_id: session_id.to_string(),
            sample_rate_hz: service.sample_rate_hz,
            channels: service.channels,
            send_timeout: Duration::from_millis(service.idle_timeout_ms),
            final_timeout: Duration::from_millis(service.final_timeout_ms),
        })
    }

    pub async fn pump_available_audio(
        &mut self,
        recording: &talk_audio::RecordingSession,
        event_idle_timeout: Duration,
    ) -> Result<Vec<StreamingAsrEvent>> {
        let live_send_timeout = self.send_timeout.min(Duration::from_millis(25));
        if let Err(error) = self
            .send_available_audio_with_limit(recording, Some(1), live_send_timeout)
            .await
        {
            if error.downcast_ref::<StreamingPcmSendTimeout>().is_some() {
                return Ok(Vec::new());
            }
            return Err(error);
        }
        let events = self
            .client
            .collect_available_asr_events_until_idle(event_idle_timeout)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        retain_latest_streaming_asr_events(
            &mut self.events,
            events.iter().cloned(),
            MAX_RETAINED_STREAMING_ASR_EVENTS,
        );
        Ok(events)
    }

    pub async fn stop(
        mut self,
        recording: talk_audio::RecordingSession,
    ) -> Result<Vec<StreamingAsrEvent>> {
        let events_result = async {
            self.send_available_audio(&recording).await?;
            self.client.stop(&self.session_id).await?;
            let final_events = self
                .client
                .collect_asr_events_until_final(self.final_timeout)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            retain_latest_streaming_asr_events(
                &mut self.events,
                final_events,
                MAX_RETAINED_STREAMING_ASR_EVENTS,
            );
            Ok(self.events)
        }
        .await;
        let cancel_result = recording.cancel();
        match (events_result, cancel_result) {
            (Ok(events), Ok(())) => Ok(events),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(anyhow::anyhow!(error.to_string())),
        }
    }

    pub async fn cancel(mut self) -> Result<()> {
        self.client
            .cancel(&self.session_id)
            .await
            .map_err(Into::into)
    }

    async fn send_available_audio(
        &mut self,
        recording: &talk_audio::RecordingSession,
    ) -> Result<usize> {
        self.send_available_audio_with_limit(recording, None, self.send_timeout)
            .await
    }

    async fn send_available_audio_with_limit(
        &mut self,
        recording: &talk_audio::RecordingSession,
        max_chunks: Option<usize>,
        send_timeout: Duration,
    ) -> Result<usize> {
        let mut sender = LocalStreamingClientPcmSender {
            client: &mut self.client,
            session_id: &self.session_id,
        };
        send_available_recording_pcm_with_limit(
            recording,
            &mut self.cursor,
            self.sample_rate_hz,
            self.channels,
            send_timeout,
            max_chunks,
            &mut sender,
        )
        .await
    }
}

fn local_streaming_service_config(
    config: &TalkConfig,
) -> Result<&SpeculativeStreamingServiceConfig> {
    if !config.speculative.enabled {
        anyhow::bail!("speculative dictation must be enabled for streaming_service local ASR");
    }
    if !config
        .speculative
        .local_asr
        .trim()
        .eq_ignore_ascii_case("streaming_service")
    {
        anyhow::bail!(
            "speculative.local_asr must be streaming_service for local streaming ASR runtime"
        );
    }
    config
        .speculative
        .streaming_service
        .as_ref()
        .context("speculative.streaming_service must be set")
}

pub async fn process_voice_transcript_text(
    config: &TalkConfig,
    transcript: String,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
) -> Result<String> {
    Ok(
        process_voice_transcript_text_with_diagnostics(config, transcript, mode_override, context)
            .await?
            .text,
    )
}

pub async fn process_voice_transcript_text_with_diagnostics(
    config: &TalkConfig,
    transcript: String,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
) -> Result<RuntimeProcessedOutput> {
    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
    process_output_with_diagnostics(
        config,
        transcript,
        Some(resolution.processing_mode),
        context,
    )
    .await
}

pub async fn run_voice_session_from_external_asr_command_with_insert_hooks<F, G, H>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    audio_path: PathBuf,
    command_line: String,
    mode_override: Option<VoiceMode>,
    _context: FrontContext,
    before_insert: G,
    after_insert: H,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    H: Fn(),
{
    let events = match run_external_streaming_asr_command(&command_line, &audio_path) {
        Ok(events) => events,
        Err(error) => {
            return persist_failed_session_with_mode_override(
                config,
                session,
                &trigger_events,
                anyhow::anyhow!(error.to_string()),
                false,
                mode_override,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    let transcript = match final_transcript_from_streaming_asr_events(&events) {
        Ok(transcript) => transcript,
        Err(error) => {
            return persist_failed_session_with_mode_override(
                config,
                session,
                &trigger_events,
                anyhow::anyhow!(error.to_string()),
                false,
                mode_override,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };

    run_voice_session_from_local_transcript_with_insert_hooks(
        config,
        session,
        trigger_events,
        transcript,
        mode_override,
        before_insert,
        after_insert,
        phase_callback,
    )
}

pub async fn run_voice_session_from_audio_artifact_with_insert_hook<F, G>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    audio_path: PathBuf,
    mock_text: Option<String>,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
    before_insert: G,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
    G: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
{
    let mut session = session;
    let mut phase_callback = phase_callback;

    let transcript = match transcribe_output(config, mock_text, audio_path, context.clone()).await {
        Ok(transcript) => transcript,
        Err(error) => {
            return persist_failed_session_with_mode_override(
                config,
                session,
                &trigger_events,
                error,
                false,
                mode_override,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::TranscriptReady {
        text: transcript.clone(),
    })?;
    phase_callback(RuntimePhase::Processing);

    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
    let processed_output = match process_output_with_diagnostics(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                false,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    let RuntimeProcessedOutput {
        text: output,
        faithful_validation,
    } = processed_output;
    let processing_diagnostics = RuntimeProcessingDiagnostics {
        resolution,
        faithful_validation,
    };
    session.apply(VoiceEvent::ProcessedTextReady {
        text: output.clone(),
    })?;
    phase_callback(RuntimePhase::Inserting);
    let insert_context = runtime_insert_context(resolution, &transcript, &output);
    let insert_directive = before_insert(&insert_context);

    let outcome = match insert_output_with_single_hook(config, &output, insert_directive) {
        Ok(outcome) => outcome,
        Err(error) => {
            return persist_failed_session_with_resolution(
                config,
                session,
                &trigger_events,
                error,
                true,
                resolution,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(
        config,
        &session,
        Some(&outcome),
        &trigger_events,
        Some(processing_diagnostics),
    )?;

    Ok(VoiceRunReport {
        session,
        outcome: Some(outcome),
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

pub fn complete_failed_session<F>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    error: anyhow::Error,
    insert_failure: bool,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    complete_failed_session_with_mode_override(
        config,
        session,
        trigger_events,
        None,
        error,
        insert_failure,
        phase_callback,
    )
}

pub fn complete_failed_session_with_mode_override<F>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: Vec<&'static str>,
    mode_override: Option<VoiceMode>,
    error: anyhow::Error,
    insert_failure: bool,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    persist_failed_session_with_mode_override(
        config,
        session,
        &trigger_events,
        error,
        insert_failure,
        mode_override,
        phase_callback,
    )
}

pub fn complete_cancelled_session<F>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    session.apply(VoiceEvent::TriggerCancel)?;
    phase_callback(RuntimePhase::Cancelled);
    let resolution = resolve_runtime_voice_mode(config, None, session.transcript());
    let log_path = persist_session_log(
        config,
        &session,
        None,
        &trigger_events,
        Some(RuntimeProcessingDiagnostics {
            resolution,
            faithful_validation: None,
        }),
    )?;
    Ok(VoiceRunReport {
        session,
        outcome: None,
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

fn capture_audio_artifact(
    config: &TalkConfig,
    session_id: &str,
) -> Result<talk_audio::AudioArtifact> {
    let request = AudioCaptureRequest {
        backend: config.audio.backend,
        temp_dir: config.audio.temp_dir.clone(),
        session_id: session_id.to_string(),
        input_device: config.audio.input_device.clone(),
        wav_settings: WavSettings {
            sample_rate_hz: config.audio.sample_rate_hz,
            channels: config.audio.channels,
        },
        max_recording_seconds: config.audio.max_recording_seconds,
        silent_samples: 320,
    };
    capture_audio(&request).map_err(Into::into)
}

fn persist_failed_session_with_mode_override<F>(
    config: &TalkConfig,
    session: VoiceSession,
    trigger_events: &[&'static str],
    error: anyhow::Error,
    insert_failure: bool,
    mode_override: Option<VoiceMode>,
    phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    let resolution = resolve_runtime_voice_mode(config, mode_override, session.transcript());
    persist_failed_session_with_resolution(
        config,
        session,
        trigger_events,
        error,
        insert_failure,
        resolution,
        phase_callback,
    )
}

fn persist_failed_session_with_resolution<F>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: &[&'static str],
    error: anyhow::Error,
    insert_failure: bool,
    resolution: RuntimeVoiceModeResolution,
    mut phase_callback: F,
) -> Result<VoiceRunReport>
where
    F: FnMut(RuntimePhase),
{
    let reason = error.to_string();
    let event = if insert_failure {
        VoiceEvent::InsertFailed { reason }
    } else {
        VoiceEvent::Error { reason }
    };
    session.apply(event)?;
    phase_callback(RuntimePhase::Failed);
    let log_path = persist_session_log(
        config,
        &session,
        None,
        trigger_events,
        Some(RuntimeProcessingDiagnostics {
            resolution,
            faithful_validation: None,
        }),
    )?;
    Ok(VoiceRunReport {
        session,
        outcome: None,
        trigger_events: trigger_events.to_vec(),
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
}

fn apply_configured_trigger_sequence<F>(
    config: &TalkConfig,
    session: &mut VoiceSession,
    mut phase_callback: F,
) -> Result<Vec<&'static str>>
where
    F: FnMut(RuntimePhase),
{
    let mut hotkeys = HotkeyStateMachine::new_toggle(config.trigger.toggle_shortcut.clone());
    let actions = match config.trigger.mode {
        TriggerMode::Toggle => [HotkeyAction::TogglePressed, HotkeyAction::TogglePressed],
        TriggerMode::PushToTalk => [
            HotkeyAction::PushToTalkPressed,
            HotkeyAction::PushToTalkReleased,
        ],
    };
    let mut trigger_events = Vec::new();
    for action in actions {
        if let Some(event) = hotkeys.handle_action(action) {
            session.apply(event.clone())?;
            trigger_events.push(voice_event_kind_name(event.kind()));
            match event.kind() {
                VoiceEventKind::TriggerStart => phase_callback(RuntimePhase::Recording),
                VoiceEventKind::TriggerStop => phase_callback(RuntimePhase::Transcribing),
                VoiceEventKind::TriggerCancel => phase_callback(RuntimePhase::Cancelled),
                _ => {}
            }
        }
    }
    Ok(trigger_events)
}

async fn transcribe_output(
    config: &TalkConfig,
    mock_text: Option<String>,
    audio_path: PathBuf,
    context: FrontContext,
) -> Result<String> {
    match config.provider.kind {
        ProviderKind::Mock => {
            let transcript = mock_text.or_else(|| config.provider.mock_transcript.clone());
            let Some(transcript) = transcript else {
                return Err(anyhow::anyhow!(
                    "provider.mock_transcript must be set for mock provider"
                ));
            };
            MockTranscriber::new(transcript)
                .transcribe(audio_path, context)
                .await
                .map_err(Into::into)
        }
        ProviderKind::Http => {
            let endpoint = config
                .provider
                .endpoint
                .as_deref()
                .context("provider.endpoint must be set for http provider")?;
            HttpTranscriber::new(endpoint)
                .transcribe(audio_path, context)
                .await
                .map_err(Into::into)
        }
        ProviderKind::OpenAiCompatible => {
            let endpoint = config
                .provider
                .audio_transcriptions_endpoint
                .as_deref()
                .context(
                "provider.audio_transcriptions_endpoint must be set for openai_compatible provider",
            )?;
            let model = config.provider.transcription_model.as_deref().context(
                "provider.transcription_model must be set for openai_compatible provider",
            )?;
            OpenAiCompatibleTranscriber::new_with_transport(
                endpoint,
                model,
                resolve_provider_api_key(config)?,
                config.provider.transcription_transport,
            )
            .transcribe(audio_path, context)
            .await
            .map_err(Into::into)
        }
    }
}

async fn process_output_with_diagnostics(
    config: &TalkConfig,
    transcript: String,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
) -> Result<RuntimeProcessedOutput> {
    let mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
    let faithful_baseline = voice_mode_requires_faithful_output(mode).then(|| transcript.clone());
    let output = match config.provider.kind {
        ProviderKind::Mock => NoopTextProcessor.process(transcript, mode, context).await,
        ProviderKind::Http => {
            let endpoint = config
                .provider
                .endpoint
                .as_deref()
                .context("provider.endpoint must be set for http provider")?;
            HttpTextProcessor::new(endpoint)
                .process(transcript, mode, context)
                .await
        }
        ProviderKind::OpenAiCompatible => {
            let endpoint = config
                .provider
                .chat_completions_endpoint
                .as_deref()
                .context(
                    "provider.chat_completions_endpoint must be set for openai_compatible provider",
                )?;
            let model = config
                .provider
                .chat_model
                .as_deref()
                .context("provider.chat_model must be set for openai_compatible provider")?;
            OpenAiCompatibleTextProcessor::new(endpoint, model, resolve_provider_api_key(config)?)
                .process(transcript, mode, context)
                .await
        }
    }
    .map_err(anyhow::Error::from)?;

    let Some(faithful_baseline) = faithful_baseline else {
        return Ok(RuntimeProcessedOutput {
            text: output,
            faithful_validation: None,
        });
    };
    let validation = validate_faithful_output(&faithful_baseline, &output);
    if validation.accepted {
        return Ok(RuntimeProcessedOutput {
            text: output,
            faithful_validation: Some(validation),
        });
    }

    let reason = validation
        .fallback_reason
        .map(FaithfulOutputFallbackReason::as_str)
        .unwrap_or("unknown");
    eprintln!(
        "Talk faithful output fallback mode={mode:?} reason={reason} input_chars={} output_chars={} retention_ratio={:.3} normalized_change_ratio={:.3}",
        validation.input_char_count,
        validation.output_char_count,
        validation.retention_ratio,
        validation.normalized_change_ratio,
    );
    Ok(RuntimeProcessedOutput {
        text: faithful_baseline,
        faithful_validation: Some(validation),
    })
}

pub fn provider_text_processing_credentials_available(config: &TalkConfig) -> bool {
    match config.provider.kind {
        ProviderKind::Mock | ProviderKind::Http => true,
        ProviderKind::OpenAiCompatible => resolve_provider_credential(config).is_available(),
    }
}

fn resolve_provider_api_key(config: &TalkConfig) -> Result<Option<String>> {
    let credential = resolve_provider_credential(config);
    if credential.is_available() {
        return Ok(credential.into_api_key());
    }

    if let Some(env_name) = config.provider.api_key_env.as_deref() {
        anyhow::bail!(
            "provider credential is unavailable from provider.api_key_env {env_name} or the standard DashScope credential file"
        );
    }
    anyhow::bail!(
        "provider credential is unavailable; set provider.api_key, provider.api_key_env, or the standard DashScope credential file"
    )
}

fn insert_output_with_hooks<F, G>(
    config: &TalkConfig,
    output: &str,
    insert_context: &RuntimeInsertContext,
    before_insert: &F,
    after_insert: &G,
) -> Result<InsertOutcome>
where
    F: Fn(&RuntimeInsertContext) -> RuntimeInsertDirective,
    G: Fn(),
{
    let insert_directive = before_insert(insert_context);
    let result = insert_output_with_single_hook(config, output, insert_directive);
    after_insert();
    result
}

fn insert_output_with_single_hook(
    config: &TalkConfig,
    output: &str,
    insert_directive: RuntimeInsertDirective,
) -> Result<InsertOutcome> {
    if insert_directive == RuntimeInsertDirective::DryRunOnly {
        return DryRunInserter::default()
            .insert_text(output)
            .map_err(Into::into);
    }

    match config.output.mode {
        OutputMode::DryRun => DryRunInserter::default()
            .insert_text(output)
            .map_err(Into::into),
        OutputMode::ClipboardPaste => match config.output.clipboard_backend {
            ClipboardBackendMode::Fallback => ClipboardFallbackInserter
                .insert_text(output)
                .map_err(Into::into),
            ClipboardBackendMode::NativeWindows => {
                if std::env::var_os("TALK_DISABLE_NATIVE_CLIPBOARD").is_some() {
                    anyhow::bail!(
                        "native_windows clipboard backend disabled by TALK_DISABLE_NATIVE_CLIPBOARD"
                    );
                }
                let restore_policy = if config.output.restore_clipboard {
                    ClipboardRestorePolicy::RestoreOriginal
                } else {
                    ClipboardRestorePolicy::LeaveInsertedText
                };
                ClipboardPasteInserter::new(
                    WindowsClipboardBackend,
                    WindowsPasteShortcut,
                    restore_policy,
                )
                .insert_text(output)
                .map_err(Into::into)
            }
        },
    }
}

#[derive(Debug, Serialize)]
struct SessionLog<'a> {
    id: &'a str,
    status: &'static str,
    transcript: Option<&'a str>,
    output_text: Option<&'a str>,
    error: Option<&'a str>,
    trigger_mode: &'static str,
    trigger_events: &'a [&'static str],
    #[serde(skip_serializing_if = "Option::is_none")]
    processing: Option<SessionProcessingLog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    insert_outcome: Option<InsertOutcomeLog<'a>>,
}

#[derive(Debug, Serialize)]
struct SessionProcessingLog {
    requested_mode: VoiceMode,
    resolved_mode: VoiceMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route_input_char_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sentence_boundary_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    streaming_segment_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    faithful_input_char_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    faithful_output_char_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    retention_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    normalized_change_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    preservation_fallback_reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct InsertOutcomeLog<'a> {
    method: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

fn persist_session_log(
    config: &TalkConfig,
    session: &VoiceSession,
    outcome: Option<&InsertOutcome>,
    trigger_events: &[&'static str],
    processing: Option<RuntimeProcessingDiagnostics>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(&config.logging.dir).with_context(|| {
        format!(
            "failed to create session log dir {}",
            config.logging.dir.display()
        )
    })?;
    let log = SessionLog {
        id: session.id(),
        status: status_name(session.status()),
        transcript: session.transcript(),
        output_text: session.output_text(),
        error: session.error(),
        trigger_mode: trigger_mode_name(config.trigger.mode),
        trigger_events,
        processing: processing.map(session_processing_log),
        insert_outcome: outcome.map(insert_outcome_log),
    };
    let path = config.logging.dir.join(format!("{}.json", session.id()));
    let json = serde_json::to_string_pretty(&log).context("failed to serialize session log")?;
    std::fs::write(&path, json)
        .with_context(|| format!("failed to write session log {}", path.display()))?;
    Ok(path)
}

pub fn update_session_log_after_text_processing(
    path: &Path,
    output_text: &str,
    faithful_validation: Option<FaithfulOutputValidation>,
) -> Result<()> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read session log {}", path.display()))?;
    let mut log: serde_json::Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse session log {}", path.display()))?;
    let root = log
        .as_object_mut()
        .context("session log root must be a JSON object")?;
    root.insert(
        "output_text".to_string(),
        serde_json::Value::String(output_text.to_string()),
    );
    let processing = root
        .get_mut("processing")
        .and_then(serde_json::Value::as_object_mut)
        .context("session log processing diagnostics must be a JSON object")?;

    const FAITHFUL_KEYS: [&str; 5] = [
        "faithful_input_char_count",
        "faithful_output_char_count",
        "retention_ratio",
        "normalized_change_ratio",
        "preservation_fallback_reason",
    ];
    for key in FAITHFUL_KEYS {
        processing.remove(key);
    }
    if let Some(validation) = faithful_validation {
        processing.insert(
            "faithful_input_char_count".to_string(),
            serde_json::Value::from(validation.input_char_count),
        );
        processing.insert(
            "faithful_output_char_count".to_string(),
            serde_json::Value::from(validation.output_char_count),
        );
        processing.insert(
            "retention_ratio".to_string(),
            serde_json::Value::from(validation.retention_ratio),
        );
        processing.insert(
            "normalized_change_ratio".to_string(),
            serde_json::Value::from(validation.normalized_change_ratio),
        );
        if let Some(reason) = validation.fallback_reason {
            processing.insert(
                "preservation_fallback_reason".to_string(),
                serde_json::Value::String(reason.as_str().to_string()),
            );
        }
    }

    let updated = serde_json::to_string_pretty(&log)
        .context("failed to serialize updated session processing log")?;
    std::fs::write(path, updated)
        .with_context(|| format!("failed to update session log {}", path.display()))?;
    Ok(())
}

fn session_processing_log(diagnostics: RuntimeProcessingDiagnostics) -> SessionProcessingLog {
    let route = diagnostics.resolution.smart_route_analysis;
    let faithful = diagnostics.faithful_validation;
    SessionProcessingLog {
        requested_mode: diagnostics.resolution.requested_mode,
        resolved_mode: diagnostics.resolution.processing_mode,
        route_reason: route.map(|analysis| analysis.reason.as_str()),
        route_input_char_count: route.map(|analysis| analysis.non_whitespace_char_count),
        sentence_boundary_count: route.map(|analysis| analysis.sentence_boundary_count),
        streaming_segment_count: route.map(|analysis| analysis.committed_streaming_segment_count),
        faithful_input_char_count: faithful.map(|validation| validation.input_char_count),
        faithful_output_char_count: faithful.map(|validation| validation.output_char_count),
        retention_ratio: faithful.map(|validation| validation.retention_ratio),
        normalized_change_ratio: faithful.map(|validation| validation.normalized_change_ratio),
        preservation_fallback_reason: faithful
            .and_then(|validation| validation.fallback_reason)
            .map(FaithfulOutputFallbackReason::as_str),
    }
}

fn validate_explicit_audio_file(audio_path: &Path) -> Result<()> {
    if audio_path.as_os_str().is_empty()
        || audio_path.as_os_str().to_string_lossy().trim().is_empty()
    {
        anyhow::bail!("audio file path must not be empty");
    }
    if !audio_path.exists() {
        anyhow::bail!("audio file does not exist: {}", audio_path.display());
    }
    if !audio_path.is_file() {
        anyhow::bail!("audio file is not a file: {}", audio_path.display());
    }
    Ok(())
}

fn trigger_mode_name(mode: TriggerMode) -> &'static str {
    match mode {
        TriggerMode::Toggle => "toggle",
        TriggerMode::PushToTalk => "push_to_talk",
    }
}

fn voice_event_kind_name(kind: VoiceEventKind) -> &'static str {
    match kind {
        VoiceEventKind::TriggerStart => "trigger_start",
        VoiceEventKind::TriggerStop => "trigger_stop",
        VoiceEventKind::TriggerCancel => "trigger_cancel",
        VoiceEventKind::TranscriptReady => "transcript_ready",
        VoiceEventKind::ProcessedTextReady => "processed_text_ready",
        VoiceEventKind::InsertSucceeded => "insert_succeeded",
        VoiceEventKind::InsertFailed => "insert_failed",
        VoiceEventKind::Error => "error",
    }
}

fn status_name(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Idle => "idle",
        SessionStatus::Recording => "recording",
        SessionStatus::Transcribing => "transcribing",
        SessionStatus::Processing => "processing",
        SessionStatus::Inserting => "inserting",
        SessionStatus::Completed => "completed",
        SessionStatus::Failed => "failed",
        SessionStatus::Cancelled => "cancelled",
    }
}

fn insert_outcome_log(outcome: &InsertOutcome) -> InsertOutcomeLog<'_> {
    match outcome {
        InsertOutcome::Inserted { method } => InsertOutcomeLog {
            method: insert_method_name(*method),
            reason: None,
        },
        InsertOutcome::FallbackClipboard { reason } => InsertOutcomeLog {
            method: "clipboard_fallback",
            reason: Some(reason),
        },
    }
}

fn insert_method_name(method: InsertMethod) -> &'static str {
    match method {
        InsertMethod::DryRun => "dry_run",
        InsertMethod::ClipboardPaste => "clipboard_paste",
        InsertMethod::ClipboardFallback => "clipboard_fallback",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        retain_latest_streaming_asr_events, send_available_recording_pcm,
        send_available_recording_pcm_with_limit, RecordingPcmChunk, RecordingPcmCursor,
        StreamingPcmSender, StreamingPcmSource,
    };
    use anyhow::Result;
    use std::cell::Cell;
    use std::time::Duration;
    use talk_audio::{start_recording, AudioCaptureRequest, RecordingSession, WavSettings};
    use talk_core::AudioBackendMode;

    struct ContinuouslyAvailablePcmSource {
        emitted_chunks: Cell<usize>,
        reclaimed_chunks: Cell<usize>,
        total_chunks: usize,
    }

    impl ContinuouslyAvailablePcmSource {
        fn new(total_chunks: usize) -> Self {
            Self {
                emitted_chunks: Cell::new(0),
                reclaimed_chunks: Cell::new(0),
                total_chunks,
            }
        }
    }

    impl StreamingPcmSource for ContinuouslyAvailablePcmSource {
        fn drain_pcm_chunk(
            &self,
            _cursor: &mut RecordingPcmCursor,
        ) -> Result<Option<RecordingPcmChunk>> {
            if self.reclaimed_chunks.get() >= self.total_chunks {
                return Ok(None);
            }

            let emitted = self.emitted_chunks.get();
            if emitted >= self.total_chunks {
                anyhow::bail!("PCM consumer drained again before reclaiming sent chunks");
            }
            self.emitted_chunks.set(emitted + 1);
            Ok(Some(RecordingPcmChunk {
                sequence: emitted as u64,
                sample_rate_hz: 16_000,
                channels: 1,
                bytes: vec![0, 0],
            }))
        }

        fn discard_consumed_pcm(&self, _cursor: &mut RecordingPcmCursor) -> Result<()> {
            self.reclaimed_chunks
                .set(self.reclaimed_chunks.get().saturating_add(1));
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingPcmSender {
        sent_sequences: Vec<u64>,
    }

    impl StreamingPcmSender for RecordingPcmSender {
        async fn send_pcm_chunk(&mut self, chunk: &RecordingPcmChunk) -> Result<()> {
            self.sent_sequences.push(chunk.sequence);
            Ok(())
        }
    }

    struct ObservableRecordingPcmSource {
        recording: RecordingSession,
        reclaimed_chunks: Cell<usize>,
    }

    impl StreamingPcmSource for ObservableRecordingPcmSource {
        fn drain_pcm_chunk(
            &self,
            cursor: &mut RecordingPcmCursor,
        ) -> Result<Option<RecordingPcmChunk>> {
            self.recording.drain_pcm_chunk(cursor).map_err(Into::into)
        }

        fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<()> {
            self.recording.discard_consumed_pcm(cursor)?;
            self.reclaimed_chunks
                .set(self.reclaimed_chunks.get().saturating_add(1));
            Ok(())
        }
    }

    #[derive(Default)]
    struct FailFirstPcmSender {
        attempts: Vec<u64>,
        failed_once: bool,
    }

    impl StreamingPcmSender for FailFirstPcmSender {
        async fn send_pcm_chunk(&mut self, chunk: &RecordingPcmChunk) -> Result<()> {
            self.attempts.push(chunk.sequence);
            if !self.failed_once {
                self.failed_once = true;
                anyhow::bail!("simulated streaming send failure");
            }
            Ok(())
        }
    }

    struct PendingPcmSender;

    impl StreamingPcmSender for PendingPcmSender {
        async fn send_pcm_chunk(&mut self, _chunk: &RecordingPcmChunk) -> Result<()> {
            std::future::pending::<Result<()>>().await
        }
    }

    #[tokio::test]
    async fn streaming_pcm_reclaims_each_sent_chunk_before_draining_more() {
        let source = ContinuouslyAvailablePcmSource::new(3);
        let mut cursor = RecordingPcmCursor::default();
        let mut sender = RecordingPcmSender::default();

        let sent_chunks = send_available_recording_pcm(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            &mut sender,
        )
        .await
        .unwrap();

        assert_eq!(sent_chunks, 3);
        assert_eq!(sender.sent_sequences, vec![0, 1, 2]);
        assert_eq!(source.reclaimed_chunks.get(), 3);
    }

    #[tokio::test]
    async fn streaming_pcm_respects_per_pump_chunk_budget() {
        let source = ContinuouslyAvailablePcmSource::new(3);
        let mut cursor = RecordingPcmCursor::default();
        let mut sender = RecordingPcmSender::default();

        let sent_chunks = send_available_recording_pcm_with_limit(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            Some(1),
            &mut sender,
        )
        .await
        .unwrap();

        assert_eq!(sent_chunks, 1);
        assert_eq!(sender.sent_sequences, vec![0]);
        assert_eq!(source.reclaimed_chunks.get(), 1);

        let drained_chunks = send_available_recording_pcm(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            &mut sender,
        )
        .await
        .unwrap();

        assert_eq!(drained_chunks, 2);
        assert_eq!(sender.sent_sequences, vec![0, 1, 2]);
        assert_eq!(source.reclaimed_chunks.get(), 3);
    }

    #[test]
    fn streaming_asr_history_retains_only_latest_events() {
        let mut retained = vec![
            talk_client::StreamingAsrEvent::partial("seg-1", "one"),
            talk_client::StreamingAsrEvent::partial("seg-1", "two"),
        ];

        retain_latest_streaming_asr_events(
            &mut retained,
            vec![
                talk_client::StreamingAsrEvent::partial("seg-1", "three"),
                talk_client::StreamingAsrEvent::final_segment("seg-1", "four"),
            ],
            3,
        );

        assert_eq!(retained.len(), 3);
        assert_eq!(retained[0].text(), "two");
        assert_eq!(retained[1].text(), "three");
        assert_eq!(retained[2].text(), "four");
    }

    #[tokio::test]
    async fn streaming_pcm_retries_unconfirmed_chunk_after_send_failure() {
        let root =
            std::env::temp_dir().join(format!("talk-runtime-pcm-retry-{}", std::process::id()));
        let source = ObservableRecordingPcmSource {
            recording: start_recording(&AudioCaptureRequest {
                backend: AudioBackendMode::Silent,
                temp_dir: root,
                session_id: "pcm-retry".to_string(),
                input_device: None,
                wav_settings: WavSettings::mono_16khz(),
                max_recording_seconds: 1,
                silent_samples: 160,
            })
            .unwrap(),
            reclaimed_chunks: Cell::new(0),
        };
        let mut cursor = RecordingPcmCursor::default();
        let mut sender = FailFirstPcmSender::default();

        let first_error = send_available_recording_pcm(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            &mut sender,
        )
        .await
        .unwrap_err();
        assert!(first_error
            .to_string()
            .contains("simulated streaming send failure"));
        assert_eq!(source.reclaimed_chunks.get(), 0);

        let sent_chunks = send_available_recording_pcm(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            &mut sender,
        )
        .await
        .unwrap();

        assert_eq!(sent_chunks, 1);
        assert_eq!(sender.attempts, vec![0, 0]);
        assert_eq!(source.reclaimed_chunks.get(), 1);
    }

    #[tokio::test]
    async fn streaming_pcm_times_out_stalled_send_and_keeps_chunk_retryable() {
        let root =
            std::env::temp_dir().join(format!("talk-runtime-pcm-timeout-{}", std::process::id()));
        let source = ObservableRecordingPcmSource {
            recording: start_recording(&AudioCaptureRequest {
                backend: AudioBackendMode::Silent,
                temp_dir: root,
                session_id: "pcm-timeout".to_string(),
                input_device: None,
                wav_settings: WavSettings::mono_16khz(),
                max_recording_seconds: 1,
                silent_samples: 160,
            })
            .unwrap(),
            reclaimed_chunks: Cell::new(0),
        };
        let mut cursor = RecordingPcmCursor::default();
        let mut stalled_sender = PendingPcmSender;

        let send_result = tokio::time::timeout(
            Duration::from_millis(100),
            send_available_recording_pcm(
                &source,
                &mut cursor,
                16_000,
                1,
                Duration::from_millis(10),
                &mut stalled_sender,
            ),
        )
        .await
        .expect("streaming PCM helper must enforce its own send timeout");
        let error = send_result.unwrap_err();
        assert!(error
            .to_string()
            .contains("local streaming ASR audio send timed out"));
        assert_eq!(source.reclaimed_chunks.get(), 0);

        let mut retry_sender = RecordingPcmSender::default();
        let sent_chunks = send_available_recording_pcm(
            &source,
            &mut cursor,
            16_000,
            1,
            Duration::from_secs(1),
            &mut retry_sender,
        )
        .await
        .unwrap();

        assert_eq!(sent_chunks, 1);
        assert_eq!(retry_sender.sent_sequences, vec![0]);
        assert_eq!(source.reclaimed_chunks.get(), 1);
    }
}
