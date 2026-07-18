use anyhow::{Context, Result};
use base64::Engine;
use clap::{Parser, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

#[derive(Debug, Parser)]
#[command(
    name = "talk-local-asr-sherpa",
    version,
    about = "Talk local streaming ASR daemon skeleton for sherpa-onnx"
)]
struct Cli {
    #[arg(long, default_value = "127.0.0.1:53171")]
    bind: SocketAddr,
    #[arg(long, value_enum, default_value = "dry-run")]
    mode: DaemonMode,
    #[arg(long, default_value = "你好。")]
    dry_run_text: String,
    #[arg(long)]
    dry_run_partial_text: Option<String>,
    #[arg(long, default_value = "sherpa-onnx")]
    engine: String,
    #[arg(long, default_value = "dry-run-streaming-zipformer")]
    model: String,
    #[arg(long, value_enum, default_value = "transducer")]
    model_family: SherpaOnlineModelFamily,
    #[arg(long)]
    tokens: Option<PathBuf>,
    #[arg(long)]
    encoder: Option<PathBuf>,
    #[arg(long)]
    decoder: Option<PathBuf>,
    #[arg(long)]
    joiner: Option<PathBuf>,
    #[arg(long, default_value = "cpu")]
    provider: String,
    #[arg(long, default_value_t = 2)]
    num_threads: u32,
    #[arg(long, default_value_t = 16000)]
    sample_rate_hz: u32,
    #[arg(long, default_value = "greedy_search")]
    decoding_method: String,
    #[arg(long, default_value_t = true)]
    enable_endpoint: bool,
    #[arg(long)]
    hotwords_file: Option<PathBuf>,
    #[arg(long)]
    rule_fsts: Option<PathBuf>,
    #[arg(long)]
    rule_fars: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum DaemonMode {
    DryRun,
    SherpaOnline,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SherpaOnlineModelFamily {
    Transducer,
    Paraformer,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Start {
        session_id: String,
        sample_rate_hz: u32,
        channels: u16,
        #[serde(default)]
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

#[derive(Debug, Clone)]
struct DaemonConfig {
    dry_run_text: String,
    dry_run_partial_text: Option<String>,
    engine: String,
    model: String,
    mode: DaemonMode,
    sherpa_online: Option<SherpaOnlineConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SherpaOnlineConfig {
    model_family: SherpaOnlineModelFamily,
    tokens: PathBuf,
    encoder: PathBuf,
    decoder: PathBuf,
    joiner: Option<PathBuf>,
    provider: String,
    num_threads: u32,
    sample_rate_hz: u32,
    decoding_method: String,
    enable_endpoint: bool,
    hotwords_file: Option<PathBuf>,
    rule_fsts: Option<PathBuf>,
    rule_fars: Option<PathBuf>,
}

impl DaemonConfig {
    fn from_cli(cli: Cli) -> Result<Self> {
        validate_loopback_bind(cli.bind)?;
        validate_nonblank("--engine", &cli.engine)?;
        validate_nonblank("--model", &cli.model)?;
        if cli.mode == DaemonMode::DryRun {
            validate_nonblank("--dry-run-text", &cli.dry_run_text)?;
            if let Some(partial_text) = cli.dry_run_partial_text.as_deref() {
                validate_nonblank("--dry-run-partial-text", partial_text)?;
            }
        }

        let sherpa_online = match cli.mode {
            DaemonMode::DryRun => None,
            DaemonMode::SherpaOnline => Some(SherpaOnlineConfig::from_cli(&cli)?),
        };

        Ok(Self {
            dry_run_text: cli.dry_run_text,
            dry_run_partial_text: cli.dry_run_partial_text,
            engine: cli.engine,
            model: cli.model,
            mode: cli.mode,
            sherpa_online,
        })
    }

    fn create_engine(&self) -> Result<Arc<dyn LocalStreamingAsrEngine>> {
        match self.mode {
            DaemonMode::DryRun => Ok(Arc::new(DryRunEngine {
                engine: self.engine.clone(),
                model: self.model.clone(),
                final_text: self.dry_run_text.clone(),
                partial_text: self.dry_run_partial_text.clone(),
            })),
            DaemonMode::SherpaOnline => {
                let config = self
                    .sherpa_online
                    .clone()
                    .context("sherpa-online config missing")?;
                Ok(Arc::new(SherpaOnlineEngine::new(
                    self.engine.clone(),
                    self.model.clone(),
                    config,
                )?))
            }
        }
    }
}

impl SherpaOnlineConfig {
    fn from_cli(cli: &Cli) -> Result<Self> {
        validate_nonblank("--provider", &cli.provider)?;
        validate_nonblank("--decoding-method", &cli.decoding_method)?;
        if cli.num_threads == 0 {
            anyhow::bail!("--num-threads must be greater than 0");
        }
        if cli.sample_rate_hz == 0 {
            anyhow::bail!("--sample-rate-hz must be greater than 0");
        }
        match cli.decoding_method.as_str() {
            "greedy_search" | "modified_beam_search" => {}
            other => anyhow::bail!(
                "--decoding-method must be greedy_search or modified_beam_search, got {other}"
            ),
        }

        let tokens = required_existing_file("--tokens", cli.tokens.as_ref())?;
        let encoder = required_existing_file("--encoder", cli.encoder.as_ref())?;
        let decoder = required_existing_file("--decoder", cli.decoder.as_ref())?;
        let joiner = match cli.model_family {
            SherpaOnlineModelFamily::Transducer => {
                Some(required_existing_file("--joiner", cli.joiner.as_ref())?)
            }
            SherpaOnlineModelFamily::Paraformer => {
                validate_optional_existing_file("--joiner", cli.joiner.as_ref())?
            }
        };
        let hotwords_file =
            validate_optional_existing_file("--hotwords-file", cli.hotwords_file.as_ref())?;
        let rule_fsts = validate_optional_existing_file("--rule-fsts", cli.rule_fsts.as_ref())?;
        let rule_fars = validate_optional_existing_file("--rule-fars", cli.rule_fars.as_ref())?;

        Ok(Self {
            model_family: cli.model_family,
            tokens,
            encoder,
            decoder,
            joiner,
            provider: cli.provider.clone(),
            num_threads: cli.num_threads,
            sample_rate_hz: cli.sample_rate_hz,
            decoding_method: cli.decoding_method.clone(),
            enable_endpoint: cli.enable_endpoint,
            hotwords_file,
            rule_fsts,
            rule_fars,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let bind = cli.bind;
    let config = DaemonConfig::from_cli(cli)?;
    let engine = config.create_engine()?;

    let listener = TcpListener::bind(bind)
        .await
        .with_context(|| format!("failed to bind local ASR daemon at {bind}"))?;
    eprintln!(
        "talk-local-asr-sherpa listening on ws://{} with {} / {}",
        bind,
        engine.ready_engine(),
        engine.ready_model()
    );

    loop {
        let (stream, peer) = listener.accept().await?;
        let engine = engine.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, peer, engine).await {
                eprintln!("talk-local-asr-sherpa connection failed for {peer}: {error:#}");
            }
        });
    }
}

trait LocalStreamingAsrEngine: Send + Sync {
    fn ready_engine(&self) -> &str;
    fn ready_model(&self) -> &str;
    fn start_session(
        &self,
        sample_rate_hz: u32,
        channels: u16,
        language: Option<String>,
    ) -> Result<Box<dyn LocalStreamingAsrSession + Send>>;
}

trait LocalStreamingAsrSession {
    fn accept_pcm_i16_le(&mut self, pcm: &[u8]) -> Result<Option<LocalAsrText>>;
    fn finish(&mut self) -> Result<LocalAsrText>;
}

#[derive(Debug, Clone)]
struct LocalAsrText {
    segment_id: String,
    text: String,
}

struct DryRunEngine {
    engine: String,
    model: String,
    final_text: String,
    partial_text: Option<String>,
}

impl LocalStreamingAsrEngine for DryRunEngine {
    fn ready_engine(&self) -> &str {
        &self.engine
    }

    fn ready_model(&self) -> &str {
        &self.model
    }

    fn start_session(
        &self,
        _sample_rate_hz: u32,
        _channels: u16,
        _language: Option<String>,
    ) -> Result<Box<dyn LocalStreamingAsrSession + Send>> {
        Ok(Box::new(DryRunSession {
            segment_id: "dry-run-segment-1".to_string(),
            final_text: self.final_text.clone(),
            partial_text: self.partial_text.clone(),
            partial_emitted: false,
        }))
    }
}

struct DryRunSession {
    segment_id: String,
    final_text: String,
    partial_text: Option<String>,
    partial_emitted: bool,
}

impl LocalStreamingAsrSession for DryRunSession {
    fn accept_pcm_i16_le(&mut self, _pcm: &[u8]) -> Result<Option<LocalAsrText>> {
        if self.partial_emitted {
            return Ok(None);
        }
        self.partial_emitted = true;
        Ok(self.partial_text.as_ref().map(|text| LocalAsrText {
            segment_id: self.segment_id.clone(),
            text: text.clone(),
        }))
    }

    fn finish(&mut self) -> Result<LocalAsrText> {
        Ok(LocalAsrText {
            segment_id: self.segment_id.clone(),
            text: self.final_text.clone(),
        })
    }
}

struct SherpaOnlineEngine {
    engine: String,
    model: String,
    config: SherpaOnlineConfig,
    recognizer: Arc<sherpa_onnx::OnlineRecognizer>,
}

impl SherpaOnlineEngine {
    fn new(engine: String, model: String, config: SherpaOnlineConfig) -> Result<Self> {
        let recognizer_config = config.to_sherpa_recognizer_config()?;
        let recognizer = sherpa_onnx::OnlineRecognizer::create(&recognizer_config)
            .context("failed to create sherpa-onnx online recognizer from model config")?;
        Ok(Self {
            engine,
            model,
            config,
            recognizer: Arc::new(recognizer),
        })
    }
}

impl LocalStreamingAsrEngine for SherpaOnlineEngine {
    fn ready_engine(&self) -> &str {
        &self.engine
    }

    fn ready_model(&self) -> &str {
        &self.model
    }

    fn start_session(
        &self,
        sample_rate_hz: u32,
        channels: u16,
        _language: Option<String>,
    ) -> Result<Box<dyn LocalStreamingAsrSession + Send>> {
        if sample_rate_hz != self.config.sample_rate_hz {
            anyhow::bail!(
                "start.sample_rate_hz {sample_rate_hz} does not match sherpa model sample rate {}",
                self.config.sample_rate_hz
            );
        }
        if channels != 1 {
            anyhow::bail!("sherpa-online currently requires mono PCM, got {channels} channels");
        }
        Ok(Box::new(SherpaOnlineSession {
            recognizer: self.recognizer.clone(),
            stream: self.recognizer.create_stream(),
            sample_rate_hz,
            segment_id: "sherpa-segment-1".to_string(),
            last_text: String::new(),
        }))
    }
}

struct SherpaOnlineSession {
    recognizer: Arc<sherpa_onnx::OnlineRecognizer>,
    stream: sherpa_onnx::OnlineStream,
    sample_rate_hz: u32,
    segment_id: String,
    last_text: String,
}

impl SherpaOnlineSession {
    fn decode_ready(&mut self) {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
    }

    fn current_text(&self) -> Option<String> {
        self.recognizer
            .get_result(&self.stream)
            .map(|result| result.text)
            .filter(|text| !text.trim().is_empty())
    }
}

impl LocalStreamingAsrSession for SherpaOnlineSession {
    fn accept_pcm_i16_le(&mut self, pcm: &[u8]) -> Result<Option<LocalAsrText>> {
        let samples = pcm_i16_le_to_f32(pcm)?;
        if samples.is_empty() {
            return Ok(None);
        }
        self.stream
            .accept_waveform(self.sample_rate_hz as i32, &samples);
        self.decode_ready();

        let Some(text) = self.current_text() else {
            return Ok(None);
        };
        if text == self.last_text {
            return Ok(None);
        }
        self.last_text = text.clone();
        Ok(Some(LocalAsrText {
            segment_id: self.segment_id.clone(),
            text,
        }))
    }

    fn finish(&mut self) -> Result<LocalAsrText> {
        self.stream.input_finished();
        self.decode_ready();
        let text = self
            .current_text()
            .unwrap_or_else(|| self.last_text.clone());
        if text.trim().is_empty() {
            anyhow::bail!("sherpa-online produced no final transcript");
        }
        Ok(LocalAsrText {
            segment_id: self.segment_id.clone(),
            text,
        })
    }
}

impl SherpaOnlineConfig {
    fn to_sherpa_recognizer_config(&self) -> Result<sherpa_onnx::OnlineRecognizerConfig> {
        let mut config = sherpa_onnx::OnlineRecognizerConfig::default();
        config.feat_config.sample_rate = self.sample_rate_hz as i32;
        config.model_config.tokens = Some(path_to_sherpa_string("--tokens", &self.tokens)?);
        config.model_config.num_threads = self.num_threads as i32;
        config.model_config.provider = Some(self.provider.clone());
        config.decoding_method = Some(self.decoding_method.clone());
        config.enable_endpoint = self.enable_endpoint;
        config.hotwords_file =
            optional_path_to_sherpa_string("--hotwords-file", &self.hotwords_file)?;
        config.rule_fsts = optional_path_to_sherpa_string("--rule-fsts", &self.rule_fsts)?;
        config.rule_fars = optional_path_to_sherpa_string("--rule-fars", &self.rule_fars)?;

        match self.model_family {
            SherpaOnlineModelFamily::Transducer => {
                config.model_config.transducer.encoder =
                    Some(path_to_sherpa_string("--encoder", &self.encoder)?);
                config.model_config.transducer.decoder =
                    Some(path_to_sherpa_string("--decoder", &self.decoder)?);
                config.model_config.transducer.joiner = Some(path_to_sherpa_string(
                    "--joiner",
                    self.joiner
                        .as_ref()
                        .context("transducer sherpa config missing joiner")?,
                )?);
            }
            SherpaOnlineModelFamily::Paraformer => {
                config.model_config.paraformer.encoder =
                    Some(path_to_sherpa_string("--encoder", &self.encoder)?);
                config.model_config.paraformer.decoder =
                    Some(path_to_sherpa_string("--decoder", &self.decoder)?);
            }
        }

        Ok(config)
    }
}

async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    engine: Arc<dyn LocalStreamingAsrEngine>,
) -> Result<()> {
    if !peer.ip().is_loopback() {
        anyhow::bail!("refusing non-loopback peer {peer}");
    }
    let mut websocket = accept_async(stream).await?;
    let mut active_session = None::<StreamingSession>;

    while let Some(message) = websocket.next().await {
        let message = message?;
        let Some(client_message) = parse_client_message(message)? else {
            continue;
        };
        match client_message {
            ClientMessage::Start {
                session_id,
                sample_rate_hz,
                channels,
                language,
            } => {
                validate_session_id(&session_id)?;
                if sample_rate_hz == 0 {
                    anyhow::bail!("start.sample_rate_hz must be greater than 0");
                }
                if channels == 0 {
                    anyhow::bail!("start.channels must be greater than 0");
                }
                let asr_session =
                    engine.start_session(sample_rate_hz, channels, language.clone())?;
                active_session = Some(StreamingSession {
                    session_id: session_id.clone(),
                    sample_rate_hz,
                    channels,
                    audio_chunks: 0,
                    last_sequence: None,
                    language,
                    asr_session,
                });
                websocket
                    .send(Message::Text(
                        json!({
                            "type": "ready",
                            "engine": engine.ready_engine(),
                            "model": engine.ready_model(),
                            "sample_rate_hz": sample_rate_hz,
                            "channels": channels
                        })
                        .to_string()
                        .into(),
                    ))
                    .await?;
            }
            ClientMessage::Audio {
                session_id,
                sequence,
                pcm_base64,
            } => {
                let session = active_session
                    .as_mut()
                    .context("audio received before start")?;
                if session.session_id != session_id {
                    anyhow::bail!("audio session_id does not match active session");
                }
                if pcm_base64.trim().is_empty() {
                    anyhow::bail!("audio.pcm_base64 must not be blank");
                }
                let pcm = base64::engine::general_purpose::STANDARD
                    .decode(pcm_base64.as_bytes())
                    .context("audio.pcm_base64 must be valid base64 PCM")?;
                session.audio_chunks = session.audio_chunks.saturating_add(1);
                session.last_sequence = Some(sequence);
                let partial_to_send = session.asr_session.accept_pcm_i16_le(&pcm)?;
                if let Some(partial) = partial_to_send {
                    websocket
                        .send(Message::Text(
                            json!({
                                "type": "partial",
                                "session_id": &session.session_id,
                                "segment_id": partial.segment_id,
                                "text": partial.text
                            })
                            .to_string()
                            .into(),
                        ))
                        .await?;
                }
            }
            ClientMessage::Stop { session_id } => {
                let mut session = active_session
                    .take()
                    .context("stop received before start")?;
                if session.session_id != session_id {
                    anyhow::bail!("stop session_id does not match active session");
                }
                let final_text = session.asr_session.finish()?;
                websocket
                    .send(Message::Text(
                        json!({
                            "type": "final",
                            "session_id": session.session_id,
                            "segment_id": final_text.segment_id,
                            "text": final_text.text,
                            "sample_rate_hz": session.sample_rate_hz,
                            "channels": session.channels,
                            "audio_chunks": session.audio_chunks,
                            "last_sequence": session.last_sequence,
                            "language": session.language
                        })
                        .to_string()
                        .into(),
                    ))
                    .await?;
                return Ok(());
            }
            ClientMessage::Cancel { session_id } => {
                if let Some(session) = active_session.as_ref() {
                    if session.session_id != session_id {
                        anyhow::bail!("cancel session_id does not match active session");
                    }
                }
                return Ok(());
            }
        }
    }

    Ok(())
}

struct StreamingSession {
    session_id: String,
    sample_rate_hz: u32,
    channels: u16,
    audio_chunks: u64,
    last_sequence: Option<u64>,
    language: Option<String>,
    asr_session: Box<dyn LocalStreamingAsrSession + Send>,
}

fn parse_client_message(message: Message) -> Result<Option<ClientMessage>> {
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(&text)?)),
        Message::Binary(bytes) => {
            let text = String::from_utf8(bytes.to_vec())
                .context("binary client message must be UTF-8 JSON")?;
            Ok(Some(serde_json::from_str(&text)?))
        }
        Message::Ping(_) | Message::Pong(_) => Ok(None),
        Message::Close(_) => Ok(None),
        Message::Frame(_) => Ok(None),
    }
}

fn required_existing_file(name: &str, value: Option<&PathBuf>) -> Result<PathBuf> {
    let path = value.with_context(|| format!("{name} must be set for sherpa-online mode"))?;
    validate_existing_file(name, path)
}

fn validate_optional_existing_file(name: &str, value: Option<&PathBuf>) -> Result<Option<PathBuf>> {
    value
        .map(|path| validate_existing_file(name, path))
        .transpose()
}

fn validate_existing_file(name: &str, path: &Path) -> Result<PathBuf> {
    validate_path_is_not_blank(name, path)?;
    if !path.exists() {
        anyhow::bail!("{name} does not exist: {}", path.display());
    }
    if !path.is_file() {
        anyhow::bail!("{name} must be a file: {}", path.display());
    }
    Ok(path.to_path_buf())
}

fn validate_path_is_not_blank(name: &str, path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path.as_os_str().to_string_lossy().trim().is_empty() {
        anyhow::bail!("{name} must not be blank");
    }
    Ok(())
}

fn path_to_sherpa_string(name: &str, path: &Path) -> Result<String> {
    validate_path_is_not_blank(name, path)?;
    Ok(path.to_string_lossy().into_owned())
}

fn optional_path_to_sherpa_string(name: &str, path: &Option<PathBuf>) -> Result<Option<String>> {
    path.as_deref()
        .map(|path| path_to_sherpa_string(name, path))
        .transpose()
}

fn pcm_i16_le_to_f32(pcm: &[u8]) -> Result<Vec<f32>> {
    if pcm.len() % 2 != 0 {
        anyhow::bail!("PCM byte length must be even for signed 16-bit little-endian audio");
    }
    Ok(pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0)
        .collect())
}

fn validate_loopback_bind(bind: SocketAddr) -> Result<()> {
    if !bind.ip().is_loopback() {
        anyhow::bail!("--bind must use a loopback address");
    }
    if bind.port() == 0 {
        anyhow::bail!("--bind port must be between 1 and 65535");
    }
    Ok(())
}

fn validate_nonblank(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be blank");
    }
    if value.trim() != value {
        anyhow::bail!("{name} must not have leading or trailing whitespace");
    }
    Ok(())
}

fn validate_session_id(session_id: &str) -> Result<()> {
    validate_nonblank("session_id", session_id)
}

#[cfg(test)]
mod tests {
    use super::{
        handle_connection, validate_loopback_bind, Cli, DaemonConfig, DaemonMode,
        SherpaOnlineModelFamily,
    };
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::fs;
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn bind_must_be_loopback() {
        let public: SocketAddr = "0.0.0.0:53171".parse().unwrap();
        assert!(validate_loopback_bind(public).is_err());

        let loopback: SocketAddr = "127.0.0.1:53171".parse().unwrap();
        assert!(validate_loopback_bind(loopback).is_ok());
    }

    #[test]
    fn dry_run_mode_does_not_require_model_files() {
        let config = DaemonConfig::from_cli(test_cli()).expect("dry-run config should validate");

        assert_eq!(config.mode, DaemonMode::DryRun);
        assert!(config.sherpa_online.is_none());
        assert_eq!(config.model, "dry-run-streaming-zipformer");
    }

    #[test]
    fn sherpa_transducer_mode_requires_existing_encoder_decoder_joiner_and_tokens() {
        let temp_dir = unique_temp_dir("talk-sherpa-transducer-model");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.onnx");
        let joiner = write_marker_file(&temp_dir, "joiner.onnx");

        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOnline;
        cli.model_family = SherpaOnlineModelFamily::Transducer;
        cli.tokens = Some(tokens.clone());
        cli.encoder = Some(encoder.clone());
        cli.decoder = Some(decoder.clone());
        cli.joiner = Some(joiner.clone());

        let config = DaemonConfig::from_cli(cli).expect("transducer config should validate");
        let sherpa = config
            .sherpa_online
            .expect("real sherpa config should be present");
        assert_eq!(sherpa.model_family, SherpaOnlineModelFamily::Transducer);
        assert_eq!(sherpa.tokens, tokens);
        assert_eq!(sherpa.encoder, encoder);
        assert_eq!(sherpa.decoder, decoder);
        assert_eq!(sherpa.joiner.as_deref(), Some(joiner.as_path()));
    }

    #[test]
    fn sherpa_paraformer_mode_requires_existing_encoder_decoder_and_tokens_without_joiner() {
        let temp_dir = unique_temp_dir("talk-sherpa-paraformer-model");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.onnx");

        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOnline;
        cli.model_family = SherpaOnlineModelFamily::Paraformer;
        cli.tokens = Some(tokens.clone());
        cli.encoder = Some(encoder.clone());
        cli.decoder = Some(decoder.clone());

        let config = DaemonConfig::from_cli(cli).expect("paraformer config should validate");
        let sherpa = config
            .sherpa_online
            .expect("real sherpa config should be present");
        assert_eq!(sherpa.model_family, SherpaOnlineModelFamily::Paraformer);
        assert_eq!(sherpa.tokens, tokens);
        assert_eq!(sherpa.encoder, encoder);
        assert_eq!(sherpa.decoder, decoder);
        assert!(sherpa.joiner.is_none());
    }

    #[test]
    fn sherpa_mode_rejects_missing_model_files() {
        let temp_dir = unique_temp_dir("talk-sherpa-missing-model");
        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOnline;
        cli.tokens = Some(temp_dir.join("missing-tokens.txt"));
        cli.encoder = Some(temp_dir.join("missing-encoder.onnx"));
        cli.decoder = Some(temp_dir.join("missing-decoder.onnx"));
        cli.joiner = Some(temp_dir.join("missing-joiner.onnx"));

        let error = DaemonConfig::from_cli(cli)
            .expect_err("missing real model files should fail validation")
            .to_string();

        assert!(error.contains("--tokens does not exist"));
    }

    #[test]
    fn sherpa_mode_rejects_zero_threads_and_blank_provider() {
        let temp_dir = unique_temp_dir("talk-sherpa-invalid-runtime");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.onnx");
        let joiner = write_marker_file(&temp_dir, "joiner.onnx");

        let mut zero_threads = test_cli();
        zero_threads.mode = DaemonMode::SherpaOnline;
        zero_threads.tokens = Some(tokens.clone());
        zero_threads.encoder = Some(encoder.clone());
        zero_threads.decoder = Some(decoder.clone());
        zero_threads.joiner = Some(joiner.clone());
        zero_threads.num_threads = 0;
        let error = DaemonConfig::from_cli(zero_threads)
            .expect_err("zero threads should fail validation")
            .to_string();
        assert!(error.contains("--num-threads must be greater than 0"));

        let mut blank_provider = test_cli();
        blank_provider.mode = DaemonMode::SherpaOnline;
        blank_provider.tokens = Some(tokens);
        blank_provider.encoder = Some(encoder);
        blank_provider.decoder = Some(decoder);
        blank_provider.joiner = Some(joiner);
        blank_provider.provider = " ".to_string();
        let error = DaemonConfig::from_cli(blank_provider)
            .expect_err("blank provider should fail validation")
            .to_string();
        assert!(error.contains("--provider must not be blank"));
    }

    fn test_cli() -> Cli {
        Cli {
            bind: "127.0.0.1:53171".parse().unwrap(),
            dry_run_text: "你好。".to_string(),
            dry_run_partial_text: Some("你好".to_string()),
            engine: "sherpa-onnx".to_string(),
            model: "dry-run-streaming-zipformer".to_string(),
            mode: DaemonMode::DryRun,
            model_family: SherpaOnlineModelFamily::Transducer,
            tokens: None,
            encoder: None,
            decoder: None,
            joiner: None,
            provider: "cpu".to_string(),
            num_threads: 2,
            sample_rate_hz: 16000,
            decoding_method: "greedy_search".to_string(),
            enable_endpoint: true,
            hotwords_file: None,
            rule_fsts: None,
            rule_fars: None,
        }
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn write_marker_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"marker").expect("write marker file");
        path
    }

    #[tokio::test]
    async fn dry_run_daemon_emits_partial_after_first_audio_chunk() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let config = DaemonConfig::from_cli(test_cli()).unwrap();
        let engine = config.create_engine().unwrap();
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            handle_connection(stream, peer, engine).await.unwrap();
        });

        let (mut websocket, _) = connect_async(endpoint).await.unwrap();
        websocket
            .send(Message::Text(
                r#"{"type":"start","session_id":"daemon-partial-session","sample_rate_hz":16000,"channels":1,"language":"zh"}"#
                    .into(),
            ))
            .await
            .unwrap();
        let ready = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&ready).unwrap()["type"],
            "ready"
        );

        websocket
            .send(Message::Text(
                r#"{"type":"audio","session_id":"daemon-partial-session","sequence":0,"pcm_base64":"AAAA"}"#
                    .into(),
            ))
            .await
            .unwrap();
        let partial = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        let partial = serde_json::from_str::<Value>(&partial).unwrap();
        assert_eq!(partial["type"], "partial");
        assert_eq!(partial["session_id"], "daemon-partial-session");
        assert_eq!(partial["segment_id"], "dry-run-segment-1");
        assert_eq!(partial["text"], "你好");

        websocket
            .send(Message::Text(
                r#"{"type":"stop","session_id":"daemon-partial-session"}"#.into(),
            ))
            .await
            .unwrap();
        let final_message = websocket
            .next()
            .await
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        let final_message = serde_json::from_str::<Value>(&final_message).unwrap();
        assert_eq!(final_message["type"], "final");
        assert_eq!(final_message["segment_id"], partial["segment_id"]);
        assert_eq!(final_message["text"], "你好。");

        server.await.unwrap();
    }
}
