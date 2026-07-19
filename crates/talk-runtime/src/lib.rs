use anyhow::{Context, Result};
mod credentials;
mod loom_config;
mod segmenter;
mod speculative;
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
use talk_audio::{capture_audio, AudioCaptureRequest, RecordingPcmCursor, WavSettings};
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

pub fn infer_smart_voice_mode(transcript: &str) -> VoiceMode {
    let trimmed = transcript.trim();
    if trimmed.is_empty() {
        return VoiceMode::Transcribe;
    }

    let lower = trimmed.to_lowercase();
    let english_words = lower
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let has_word = |word: &str| english_words.iter().any(|candidate| *candidate == word);
    let has_any_word = |words: &[&str]| words.iter().any(|word| has_word(word));
    let has_any_phrase = |phrases: &[&str]| phrases.iter().any(|phrase| lower.contains(phrase));

    if has_any_phrase(&[
        "打开", "关闭", "启动", "运行", "执行", "删除", "复制", "移动",
    ]) || has_any_word(&["open", "launch", "run", "close", "delete", "copy", "move"])
    {
        return VoiceMode::Command;
    }

    if has_any_phrase(&["公文", "正式", "润色", "改写"]) || has_any_word(&["polish", "rewrite"])
    {
        return VoiceMode::Document;
    }

    if has_any_phrase(&["生成", "写一篇", "写一段", "创作", "帮我写"])
        || has_any_word(&["draft", "write", "compose", "generate"])
    {
        return VoiceMode::Generate;
    }

    VoiceMode::Transcribe
}

fn resolve_runtime_voice_mode(
    config: &TalkConfig,
    mode_override: Option<VoiceMode>,
    transcript: Option<&str>,
) -> RuntimeVoiceModeResolution {
    let requested_mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
    let smart_routed_mode = if requested_mode == VoiceMode::Smart {
        transcript.map(infer_smart_voice_mode)
    } else {
        None
    };
    let processing_mode = smart_routed_mode.unwrap_or(requested_mode);

    RuntimeVoiceModeResolution {
        requested_mode,
        smart_routed_mode,
        processing_mode,
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
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
    let output = match process_output(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                true,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(config, &session, Some(&outcome), &trigger_events)?;

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
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
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
    if transcript.trim().is_empty() {
        return persist_failed_session(
            config,
            session,
            &trigger_events,
            anyhow::anyhow!("local ASR transcript must not be blank"),
            false,
            |phase| {
                phase_callback(phase);
            },
        );
    }

    session.apply(VoiceEvent::TranscriptReady {
        text: transcript.clone(),
    })?;
    phase_callback(RuntimePhase::Processing);

    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
    let output = match process_output(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                true,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(config, &session, Some(&outcome), &trigger_events)?;

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
    mut session: VoiceSession,
    trigger_events: Vec<&'static str>,
    transcript: String,
    mode_override: Option<VoiceMode>,
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
        return persist_failed_session(
            config,
            session,
            &trigger_events,
            anyhow::anyhow!("local ASR transcript must not be blank"),
            false,
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

    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                true,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(config, &session, Some(&outcome), &trigger_events)?;

    Ok(VoiceRunReport {
        session,
        outcome: Some(outcome),
        trigger_events,
        log_path,
        requested_mode: resolution.requested_mode,
        smart_routed_mode: resolution.smart_routed_mode,
    })
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
    let mut sent_chunks = 0usize;
    while let Some(chunk) = recording.drain_pcm_chunk(&mut cursor)? {
        if chunk.sample_rate_hz != service.sample_rate_hz || chunk.channels != service.channels {
            anyhow::bail!(
                "recording PCM chunk format {} Hz / {} channels does not match streaming_service {} Hz / {} channels",
                chunk.sample_rate_hz,
                chunk.channels,
                service.sample_rate_hz,
                service.channels
            );
        }
        client
            .send_audio(session_id, chunk.sequence, &chunk.bytes)
            .await?;
        sent_chunks = sent_chunks.saturating_add(1);
    }
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
            final_timeout: Duration::from_millis(service.final_timeout_ms),
        })
    }

    pub async fn pump_available_audio(
        &mut self,
        recording: &talk_audio::RecordingSession,
        event_idle_timeout: Duration,
    ) -> Result<Vec<StreamingAsrEvent>> {
        self.send_available_audio(recording).await?;
        let events = self
            .client
            .collect_available_asr_events_until_idle(event_idle_timeout)
            .await
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        self.events.extend(events.iter().cloned());
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
            self.events.extend(final_events);
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
        let mut sent_chunks = 0usize;
        while let Some(chunk) = recording.drain_pcm_chunk(&mut self.cursor)? {
            if chunk.sample_rate_hz != self.sample_rate_hz || chunk.channels != self.channels {
                anyhow::bail!(
                    "recording PCM chunk format {} Hz / {} channels does not match streaming_service {} Hz / {} channels",
                    chunk.sample_rate_hz,
                    chunk.channels,
                    self.sample_rate_hz,
                    self.channels
                );
            }
            self.client
                .send_audio(&self.session_id, chunk.sequence, &chunk.bytes)
                .await?;
            sent_chunks = sent_chunks.saturating_add(1);
        }
        Ok(sent_chunks)
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
    let resolution = resolve_runtime_voice_mode(config, mode_override, Some(&transcript));
    process_output(
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                anyhow::anyhow!(error.to_string()),
                false,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    let transcript = match final_transcript_from_streaming_asr_events(&events) {
        Ok(transcript) => transcript,
        Err(error) => {
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                anyhow::anyhow!(error.to_string()),
                false,
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
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
    let output = match process_output(
        config,
        transcript.clone(),
        Some(resolution.processing_mode),
        context,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                false,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
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
            return persist_failed_session(
                config,
                session,
                &trigger_events,
                error,
                true,
                |phase| {
                    phase_callback(phase);
                },
            );
        }
    };
    session.apply(VoiceEvent::InsertSucceeded)?;
    phase_callback(RuntimePhase::Completed);
    let log_path = persist_session_log(config, &session, Some(&outcome), &trigger_events)?;

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
    persist_failed_session(
        config,
        session,
        &trigger_events,
        error,
        insert_failure,
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
    let log_path = persist_session_log(config, &session, None, &trigger_events)?;
    let resolution = resolve_runtime_voice_mode(config, None, session.transcript());
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

fn persist_failed_session<F>(
    config: &TalkConfig,
    mut session: VoiceSession,
    trigger_events: &[&'static str],
    error: anyhow::Error,
    insert_failure: bool,
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
    let log_path = persist_session_log(config, &session, None, trigger_events)?;
    let resolution = resolve_runtime_voice_mode(config, None, session.transcript());
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

async fn process_output(
    config: &TalkConfig,
    transcript: String,
    mode_override: Option<VoiceMode>,
    context: FrontContext,
) -> Result<String> {
    let mode = mode_override.unwrap_or_else(|| config.default_voice_mode());
    match config.provider.kind {
        ProviderKind::Mock => NoopTextProcessor
            .process(transcript, mode, context)
            .await
            .map_err(Into::into),
        ProviderKind::Http => {
            let endpoint = config
                .provider
                .endpoint
                .as_deref()
                .context("provider.endpoint must be set for http provider")?;
            HttpTextProcessor::new(endpoint)
                .process(transcript, mode, context)
                .await
                .map_err(Into::into)
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
                .map_err(Into::into)
        }
    }
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
    insert_outcome: Option<InsertOutcomeLog<'a>>,
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
        insert_outcome: outcome.map(insert_outcome_log),
    };
    let path = config.logging.dir.join(format!("{}.json", session.id()));
    let json = serde_json::to_string_pretty(&log).context("failed to serialize session log")?;
    std::fs::write(&path, json)
        .with_context(|| format!("failed to write session log {}", path.display()))?;
    Ok(path)
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
