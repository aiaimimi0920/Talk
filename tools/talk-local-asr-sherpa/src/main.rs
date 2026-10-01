use anyhow::{Context, Result};
use base64::Engine;
use clap::{builder::BoolishValueParser, ArgAction, Parser, ValueEnum};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig,
    OfflineTransducerModelConfig, OfflineWhisperModelConfig,
};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

const MAX_OFFLINE_WAV_DURATION_SECONDS: u64 = 10 * 60;
const MAX_ONLINE_TAIL_PADDING_MS: u32 = 2_000;
// Two seconds at 384 kHz. This keeps a malformed local sample-rate setting from
// turning the finalization scratch buffer into an unbounded allocation.
const MAX_ONLINE_TAIL_PADDING_SAMPLES: u64 = 768_000;

#[derive(Debug, Parser)]
#[command(
    name = "talk-local-asr-sherpa",
    version,
    about = "Talk local ASR worker for sherpa-onnx streaming and offline recognition"
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
    #[arg(long)]
    sense_voice_model: Option<PathBuf>,
    #[arg(long, default_value = "auto")]
    sense_voice_language: String,
    #[arg(
        long,
        action = ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = BoolishValueParser::new(),
        default_value_t = true
    )]
    sense_voice_use_itn: bool,
    #[arg(long)]
    whisper_language: Option<String>,
    #[arg(long, default_value = "transcribe")]
    whisper_task: String,
    #[arg(long, default_value_t = 0)]
    whisper_tail_paddings: i32,
    #[arg(long, default_value = "cpu")]
    provider: String,
    #[arg(long, default_value_t = 2)]
    num_threads: u32,
    #[arg(long, default_value_t = 16000)]
    sample_rate_hz: u32,
    #[arg(long, default_value = "greedy_search")]
    decoding_method: String,
    #[arg(
        long,
        action = ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = BoolishValueParser::new(),
        default_value_t = true
    )]
    enable_endpoint: bool,
    /// Finalize and reset the recognizer on each detected endpoint so
    /// multi-utterance dictation does not collapse into one segment. Off by
    /// default (behaviour-preserving) until validated on real speech.
    #[arg(
        long,
        action = ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true",
        value_parser = BoolishValueParser::new(),
        default_value_t = false
    )]
    endpoint_reset: bool,
    /// Endpoint rule 1: min trailing silence (seconds) to fire an endpoint even
    /// with no decoded text. sherpa-onnx standard default.
    #[arg(long, default_value_t = 2.4)]
    rule1_min_trailing_silence: f32,
    /// Endpoint rule 2: min trailing silence (seconds) after some decoded text.
    #[arg(long, default_value_t = 1.2)]
    rule2_min_trailing_silence: f32,
    /// Endpoint rule 3: min utterance length (seconds) to force an endpoint.
    #[arg(long, default_value_t = 20.0)]
    rule3_min_utterance_length: f32,
    /// Beam width for modified_beam_search (used only with that decoding method).
    /// Must be >= 1; the crate default of 0 would break beam search.
    #[arg(long, default_value_t = 4)]
    max_active_paths: i32,
    /// Boost applied to hotword phrases (used only when hotwords are configured).
    /// The crate default of 0.0 applies no biasing.
    #[arg(long, default_value_t = 1.5)]
    hotwords_score: f32,
    /// Penalty subtracted from the blank token during transducer decoding
    /// (higher reduces deletions). 0.0 is the sherpa default.
    #[arg(long, default_value_t = 0.0)]
    blank_penalty: f32,
    /// Silence appended before input_finished so the streaming encoder can emit
    /// tokens that still depend on right context at the physical end of speech.
    #[arg(long, default_value_t = 300)]
    online_tail_padding_ms: u32,
    /// Modeling unit for tokenizing raw-text hotwords: cjkchar | bpe |
    /// cjkchar+bpe. Required (with --bpe-vocab for bpe) for hotwords to match on
    /// SentencePiece/BPE models. Unset = sherpa cannot tokenize raw hotwords.
    #[arg(long)]
    modeling_unit: Option<String>,
    /// Text vocabulary used to tokenize raw hotwords when --modeling-unit
    /// includes bpe. This is sherpa's bpe.vocab input, not binary bpe.model.
    #[arg(long)]
    bpe_vocab: Option<PathBuf>,
    #[arg(long)]
    hotwords_file: Option<PathBuf>,
    #[arg(long)]
    rule_fsts: Option<PathBuf>,
    #[arg(long)]
    rule_fars: Option<PathBuf>,
    #[arg(long)]
    offline_wav: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum DaemonMode {
    DryRun,
    SherpaOnline,
    SherpaOffline,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum SherpaOnlineModelFamily {
    Transducer,
    Paraformer,
    SenseVoice,
    Whisper,
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
    sherpa_offline: Option<SherpaOfflineConfig>,
}

#[derive(Debug, Clone, PartialEq)]
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
    endpoint_reset: bool,
    rule1_min_trailing_silence: f32,
    rule2_min_trailing_silence: f32,
    rule3_min_utterance_length: f32,
    max_active_paths: i32,
    hotwords_score: f32,
    blank_penalty: f32,
    online_tail_padding_ms: u32,
    modeling_unit: Option<String>,
    bpe_vocab: Option<PathBuf>,
    hotwords_file: Option<PathBuf>,
    rule_fsts: Option<PathBuf>,
    rule_fars: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
struct SherpaOfflineConfig {
    audio_wav: PathBuf,
    tokens: PathBuf,
    model_config: SherpaOfflineModelConfig,
    provider: String,
    num_threads: u32,
    sample_rate_hz: u32,
    decoding_method: String,
    max_active_paths: i32,
    hotwords_score: f32,
    blank_penalty: f32,
    modeling_unit: Option<String>,
    bpe_vocab: Option<PathBuf>,
    hotwords_file: Option<PathBuf>,
    rule_fsts: Option<PathBuf>,
    rule_fars: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
enum SherpaOfflineModelConfig {
    Transducer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
    },
    SenseVoice {
        model: PathBuf,
        language: String,
        use_itn: bool,
    },
    Whisper {
        encoder: PathBuf,
        decoder: PathBuf,
        language: Option<String>,
        task: String,
        tail_paddings: i32,
    },
}

impl DaemonConfig {
    fn from_cli(cli: Cli) -> Result<Self> {
        if cli.mode != DaemonMode::SherpaOffline {
            validate_loopback_bind(cli.bind)?;
        }
        validate_nonblank("--engine", &cli.engine)?;
        validate_nonblank("--model", &cli.model)?;
        if cli.mode == DaemonMode::DryRun {
            validate_nonblank("--dry-run-text", &cli.dry_run_text)?;
            if let Some(partial_text) = cli.dry_run_partial_text.as_deref() {
                validate_nonblank("--dry-run-partial-text", partial_text)?;
            }
        }
        if cli.mode != DaemonMode::SherpaOffline && cli.offline_wav.is_some() {
            anyhow::bail!("--offline-wav requires --mode sherpa-offline");
        }

        let sherpa_online = match cli.mode {
            DaemonMode::DryRun | DaemonMode::SherpaOffline => None,
            DaemonMode::SherpaOnline => Some(SherpaOnlineConfig::from_cli(&cli)?),
        };
        let sherpa_offline = match cli.mode {
            DaemonMode::SherpaOffline => Some(SherpaOfflineConfig::from_cli(&cli)?),
            DaemonMode::DryRun | DaemonMode::SherpaOnline => None,
        };

        Ok(Self {
            dry_run_text: cli.dry_run_text,
            dry_run_partial_text: cli.dry_run_partial_text,
            engine: cli.engine,
            model: cli.model,
            mode: cli.mode,
            sherpa_online,
            sherpa_offline,
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
            DaemonMode::SherpaOffline => {
                anyhow::bail!(
                    "sherpa-offline is a one-shot mode and cannot create a streaming engine"
                )
            }
        }
    }
}

impl SherpaOfflineConfig {
    fn from_cli(cli: &Cli) -> Result<Self> {
        if cli.model_family == SherpaOnlineModelFamily::Paraformer {
            anyhow::bail!("sherpa-offline does not support --model-family paraformer");
        }
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
        if cli.max_active_paths < 1 {
            anyhow::bail!("--max-active-paths must be greater than 0");
        }
        if !cli.hotwords_score.is_finite() || cli.hotwords_score < 0.0 {
            anyhow::bail!("--hotwords-score must be a finite, non-negative number");
        }
        if !cli.blank_penalty.is_finite() {
            anyhow::bail!("--blank-penalty must be a finite number");
        }
        validate_modeling_unit_and_bpe_vocab(
            cli.modeling_unit.as_deref(),
            cli.bpe_vocab.as_deref(),
        )?;

        let audio_wav = required_existing_file("--offline-wav", cli.offline_wav.as_ref())?;
        let tokens = required_existing_file("--tokens", cli.tokens.as_ref())?;
        let model_config = match cli.model_family {
            SherpaOnlineModelFamily::Transducer => SherpaOfflineModelConfig::Transducer {
                encoder: required_existing_file("--encoder", cli.encoder.as_ref())?,
                decoder: required_existing_file("--decoder", cli.decoder.as_ref())?,
                joiner: required_existing_file("--joiner", cli.joiner.as_ref())?,
            },
            SherpaOnlineModelFamily::SenseVoice => {
                validate_nonblank("--sense-voice-language", &cli.sense_voice_language)?;
                match cli.sense_voice_language.as_str() {
                    "auto" | "zh" | "en" | "ja" | "ko" | "yue" => {}
                    other => anyhow::bail!(
                        "--sense-voice-language must be auto, zh, en, ja, ko, or yue, got {other}"
                    ),
                }
                SherpaOfflineModelConfig::SenseVoice {
                    model: required_existing_file(
                        "--sense-voice-model",
                        cli.sense_voice_model.as_ref(),
                    )?,
                    language: cli.sense_voice_language.clone(),
                    use_itn: cli.sense_voice_use_itn,
                }
            }
            SherpaOnlineModelFamily::Whisper => {
                if let Some(language) = cli.whisper_language.as_deref() {
                    validate_nonblank("--whisper-language", language)?;
                }
                match cli.whisper_task.as_str() {
                    "transcribe" | "translate" => {}
                    other => {
                        anyhow::bail!("--whisper-task must be transcribe or translate, got {other}")
                    }
                }
                if cli.whisper_tail_paddings < 0 {
                    anyhow::bail!("--whisper-tail-paddings must be non-negative");
                }
                SherpaOfflineModelConfig::Whisper {
                    encoder: required_existing_file("--encoder", cli.encoder.as_ref())?,
                    decoder: required_existing_file("--decoder", cli.decoder.as_ref())?,
                    language: cli.whisper_language.clone(),
                    task: cli.whisper_task.clone(),
                    tail_paddings: cli.whisper_tail_paddings,
                }
            }
            SherpaOnlineModelFamily::Paraformer => {
                anyhow::bail!("sherpa-offline does not support --model-family paraformer")
            }
        };
        Ok(Self {
            audio_wav,
            tokens,
            model_config,
            provider: cli.provider.clone(),
            num_threads: cli.num_threads,
            sample_rate_hz: cli.sample_rate_hz,
            decoding_method: cli.decoding_method.clone(),
            max_active_paths: cli.max_active_paths,
            hotwords_score: cli.hotwords_score,
            blank_penalty: cli.blank_penalty,
            modeling_unit: cli.modeling_unit.clone(),
            bpe_vocab: validate_optional_existing_file("--bpe-vocab", cli.bpe_vocab.as_ref())?,
            hotwords_file: validate_optional_existing_file(
                "--hotwords-file",
                cli.hotwords_file.as_ref(),
            )?,
            rule_fsts: validate_optional_existing_file("--rule-fsts", cli.rule_fsts.as_ref())?,
            rule_fars: validate_optional_existing_file("--rule-fars", cli.rule_fars.as_ref())?,
        })
    }
}

impl SherpaOnlineConfig {
    fn from_cli(cli: &Cli) -> Result<Self> {
        if matches!(
            cli.model_family,
            SherpaOnlineModelFamily::SenseVoice | SherpaOnlineModelFamily::Whisper
        ) {
            anyhow::bail!("--model-family sense-voice and whisper require --mode sherpa-offline");
        }
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
        validate_nonnegative_seconds(
            "--rule1-min-trailing-silence",
            cli.rule1_min_trailing_silence,
        )?;
        validate_nonnegative_seconds(
            "--rule2-min-trailing-silence",
            cli.rule2_min_trailing_silence,
        )?;
        validate_nonnegative_seconds(
            "--rule3-min-utterance-length",
            cli.rule3_min_utterance_length,
        )?;
        if cli.max_active_paths < 1 {
            anyhow::bail!("--max-active-paths must be greater than 0");
        }
        if !cli.hotwords_score.is_finite() || cli.hotwords_score < 0.0 {
            anyhow::bail!("--hotwords-score must be a finite, non-negative number");
        }
        if !cli.blank_penalty.is_finite() {
            anyhow::bail!("--blank-penalty must be a finite number");
        }
        tail_padding_sample_count(cli.sample_rate_hz, cli.online_tail_padding_ms)?;
        validate_modeling_unit_and_bpe_vocab(
            cli.modeling_unit.as_deref(),
            cli.bpe_vocab.as_deref(),
        )?;
        let bpe_vocab = validate_optional_existing_file("--bpe-vocab", cli.bpe_vocab.as_ref())?;

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
            SherpaOnlineModelFamily::SenseVoice => {
                anyhow::bail!("--model-family sense-voice requires --mode sherpa-offline")
            }
            SherpaOnlineModelFamily::Whisper => {
                anyhow::bail!("--model-family whisper requires --mode sherpa-offline")
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
            endpoint_reset: cli.endpoint_reset,
            rule1_min_trailing_silence: cli.rule1_min_trailing_silence,
            rule2_min_trailing_silence: cli.rule2_min_trailing_silence,
            rule3_min_utterance_length: cli.rule3_min_utterance_length,
            max_active_paths: cli.max_active_paths,
            hotwords_score: cli.hotwords_score,
            blank_penalty: cli.blank_penalty,
            online_tail_padding_ms: cli.online_tail_padding_ms,
            modeling_unit: cli.modeling_unit.clone(),
            bpe_vocab,
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
    if let Some(offline) = config.sherpa_offline.as_ref() {
        run_sherpa_offline(&config.engine, &config.model, offline)?;
        return Ok(());
    }
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

fn run_sherpa_offline(engine: &str, model: &str, config: &SherpaOfflineConfig) -> Result<()> {
    let (sample_rate_hz, samples) = read_offline_wav(&config.audio_wav, config.sample_rate_hz)?;
    let mut recognizer_config = OfflineRecognizerConfig::default();
    recognizer_config.feat_config.sample_rate = sample_rate_hz as i32;
    match &config.model_config {
        SherpaOfflineModelConfig::Transducer {
            encoder,
            decoder,
            joiner,
        } => {
            recognizer_config.model_config.transducer = OfflineTransducerModelConfig {
                encoder: Some(path_to_utf8("--encoder", encoder)?),
                decoder: Some(path_to_utf8("--decoder", decoder)?),
                joiner: Some(path_to_utf8("--joiner", joiner)?),
            };
        }
        SherpaOfflineModelConfig::SenseVoice {
            model,
            language,
            use_itn,
        } => {
            recognizer_config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
                model: Some(path_to_utf8("--sense-voice-model", model)?),
                language: Some(language.clone()),
                use_itn: *use_itn,
            };
        }
        SherpaOfflineModelConfig::Whisper {
            encoder,
            decoder,
            language,
            task,
            tail_paddings,
        } => {
            recognizer_config.model_config.whisper = OfflineWhisperModelConfig {
                encoder: Some(path_to_utf8("--encoder", encoder)?),
                decoder: Some(path_to_utf8("--decoder", decoder)?),
                language: language.clone(),
                task: Some(task.clone()),
                tail_paddings: *tail_paddings,
                enable_token_timestamps: false,
                enable_segment_timestamps: false,
            };
        }
    }
    recognizer_config.model_config.tokens = Some(path_to_utf8("--tokens", &config.tokens)?);
    recognizer_config.model_config.num_threads = config.num_threads as i32;
    recognizer_config.model_config.provider = Some(config.provider.clone());
    recognizer_config.model_config.modeling_unit = config.modeling_unit.clone();
    recognizer_config.model_config.bpe_vocab = config
        .bpe_vocab
        .as_deref()
        .map(|path| path_to_utf8("--bpe-vocab", path))
        .transpose()?;
    recognizer_config.decoding_method = Some(config.decoding_method.clone());
    recognizer_config.max_active_paths = config.max_active_paths;
    recognizer_config.hotwords_file = config
        .hotwords_file
        .as_deref()
        .map(|path| path_to_utf8("--hotwords-file", path))
        .transpose()?;
    recognizer_config.hotwords_score = config.hotwords_score;
    recognizer_config.rule_fsts = config
        .rule_fsts
        .as_deref()
        .map(|path| path_to_utf8("--rule-fsts", path))
        .transpose()?;
    recognizer_config.rule_fars = config
        .rule_fars
        .as_deref()
        .map(|path| path_to_utf8("--rule-fars", path))
        .transpose()?;
    recognizer_config.blank_penalty = config.blank_penalty;

    let recognizer = OfflineRecognizer::create(&recognizer_config)
        .context("failed to create sherpa-onnx offline recognizer")?;
    let stream = recognizer.create_stream();
    stream.accept_waveform(sample_rate_hz as i32, &samples);
    recognizer.decode(&stream);
    let result = stream
        .get_result()
        .context("sherpa-onnx offline recognizer produced no result")?;
    let text = result.text.trim();
    if text.is_empty() {
        anyhow::bail!("sherpa-onnx offline recognizer produced blank text");
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "engine": engine,
            "model": model,
            "text": text,
            "sample_rate_hz": sample_rate_hz,
            "channels": 1,
        }))?
    );
    Ok(())
}

fn read_offline_wav(path: &Path, expected_sample_rate_hz: u32) -> Result<(u32, Vec<f32>)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open offline WAV {}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        anyhow::bail!(
            "offline WAV must be mono PCM16, got channels={} bits={} format={:?}",
            spec.channels,
            spec.bits_per_sample,
            spec.sample_format
        );
    }
    if spec.sample_rate != expected_sample_rate_hz {
        anyhow::bail!(
            "offline WAV sample rate {} does not match --sample-rate-hz {}",
            spec.sample_rate,
            expected_sample_rate_hz
        );
    }
    let max_samples = u64::from(spec.sample_rate)
        .checked_mul(MAX_OFFLINE_WAV_DURATION_SECONDS)
        .context("offline WAV duration limit overflow")?;
    if u64::from(reader.duration()) > max_samples {
        anyhow::bail!(
            "offline WAV exceeds the {} second benchmark limit: {}",
            MAX_OFFLINE_WAV_DURATION_SECONDS,
            path.display()
        );
    }
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / 32768.0))
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read offline WAV samples from {}", path.display()))?;
    if samples.is_empty() {
        anyhow::bail!("offline WAV contains no samples: {}", path.display());
    }
    Ok((spec.sample_rate, samples))
}

fn path_to_utf8(name: &str, path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_string)
        .with_context(|| format!("{name} path is not valid UTF-8: {}", path.display()))
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
        let tail_padding_samples = tail_padding_sample_count(
            self.config.sample_rate_hz,
            self.config.online_tail_padding_ms,
        )?;
        Ok(Box::new(SherpaOnlineSession {
            recognizer: self.recognizer.clone(),
            stream: self.recognizer.create_stream(),
            sample_rate_hz,
            sample_scratch: Vec::new(),
            segment_id: "sherpa-segment-1".to_string(),
            last_text: String::new(),
            endpoint_reset: self.config.endpoint_reset,
            committed_prefix: String::new(),
            tail_padding_samples,
        }))
    }
}

struct SherpaOnlineSession {
    recognizer: Arc<sherpa_onnx::OnlineRecognizer>,
    stream: sherpa_onnx::OnlineStream,
    sample_rate_hz: u32,
    sample_scratch: Vec<f32>,
    segment_id: String,
    last_text: String,
    /// When true, finalize + reset the recognizer on each detected endpoint.
    endpoint_reset: bool,
    /// Text of already-finalized (endpoint-committed) utterances, prepended to
    /// the in-progress segment so the emitted transcript stays monotonic.
    committed_prefix: String,
    tail_padding_samples: usize,
}

/// Concatenate the endpoint-committed prefix and the in-progress segment text.
/// Boundary whitespace from the model is removed before the transcript is sent
/// over the strict streaming protocol; whitespace inside a segment is preserved.
fn combine_committed_and_segment(committed_prefix: &str, segment_text: &str) -> String {
    let committed_prefix = committed_prefix.trim();
    let segment_text = segment_text.trim();
    let needs_word_boundary = committed_prefix
        .chars()
        .next_back()
        .zip(segment_text.chars().next())
        .is_some_and(|(left, right)| left.is_ascii_alphanumeric() && right.is_ascii_alphanumeric());
    let mut combined = String::with_capacity(
        committed_prefix.len() + segment_text.len() + usize::from(needs_word_boundary),
    );
    combined.push_str(committed_prefix);
    if needs_word_boundary {
        combined.push(' ');
    }
    combined.push_str(segment_text);
    combined
}

impl SherpaOnlineSession {
    fn decode_ready(&mut self) {
        while self.recognizer.is_ready(&self.stream) {
            self.recognizer.decode(&self.stream);
        }
    }

    fn current_segment_text(&self) -> String {
        self.recognizer
            .get_result(&self.stream)
            .map(|result| result.text)
            .unwrap_or_default()
    }

    /// Full transcript so far: endpoint-committed prefix plus the in-progress
    /// segment, or `None` when still empty.
    fn accumulated_text(&self) -> Option<String> {
        let text =
            combine_committed_and_segment(&self.committed_prefix, &self.current_segment_text());
        (!text.trim().is_empty()).then_some(text)
    }
}

impl LocalStreamingAsrSession for SherpaOnlineSession {
    fn accept_pcm_i16_le(&mut self, pcm: &[u8]) -> Result<Option<LocalAsrText>> {
        pcm_i16_le_to_f32_into(pcm, &mut self.sample_scratch)?;
        if self.sample_scratch.is_empty() {
            return Ok(None);
        }
        self.stream
            .accept_waveform(self.sample_rate_hz as i32, &self.sample_scratch);
        self.decode_ready();

        // On a detected endpoint, roll the finished utterance into the committed
        // prefix and reset the recognizer so the next utterance starts fresh.
        // The emitted transcript stays monotonic (prefix never shrinks).
        if self.endpoint_reset && self.recognizer.is_endpoint(&self.stream) {
            self.committed_prefix =
                combine_committed_and_segment(&self.committed_prefix, &self.current_segment_text());
            self.recognizer.reset(&self.stream);
        }

        let Some(text) = self.accumulated_text() else {
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
        self.sample_scratch.clear();
        self.sample_scratch.resize(self.tail_padding_samples, 0.0);
        if !self.sample_scratch.is_empty() {
            self.stream
                .accept_waveform(self.sample_rate_hz as i32, &self.sample_scratch);
        }
        self.stream.input_finished();
        self.decode_ready();
        let text = self
            .accumulated_text()
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
        config.model_config.modeling_unit = self.modeling_unit.clone();
        config.model_config.bpe_vocab =
            optional_path_to_sherpa_string("--bpe-vocab", &self.bpe_vocab)?;
        config.decoding_method = Some(self.decoding_method.clone());
        config.enable_endpoint = self.enable_endpoint;
        config.rule1_min_trailing_silence = self.rule1_min_trailing_silence;
        config.rule2_min_trailing_silence = self.rule2_min_trailing_silence;
        config.rule3_min_utterance_length = self.rule3_min_utterance_length;
        config.max_active_paths = self.max_active_paths;
        config.hotwords_score = self.hotwords_score;
        config.blank_penalty = self.blank_penalty;
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
            SherpaOnlineModelFamily::SenseVoice => {
                anyhow::bail!("sense-voice is not an online recognizer model")
            }
            SherpaOnlineModelFamily::Whisper => {
                anyhow::bail!("whisper is not an online recognizer model")
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
                    pcm_scratch: Vec::new(),
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
                if !should_accept_audio_sequence(session.last_sequence, sequence) {
                    continue;
                }
                if pcm_base64.trim().is_empty() {
                    anyhow::bail!("audio.pcm_base64 must not be blank");
                }
                decode_pcm_base64_into(&pcm_base64, &mut session.pcm_scratch)?;
                let partial_to_send = session
                    .asr_session
                    .accept_pcm_i16_le(&session.pcm_scratch)?;
                session.audio_chunks = session.audio_chunks.saturating_add(1);
                session.last_sequence = Some(sequence);
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
                let final_text = match session.asr_session.finish() {
                    Ok(final_text) => final_text,
                    Err(error) => {
                        // Surface a structured error frame (e.g. no speech was
                        // detected) instead of silently dropping the socket, so
                        // the client can report a meaningful failure rather than
                        // a generic "connection closed".
                        let _ = websocket
                            .send(Message::Text(
                                streaming_asr_error_frame(&session.session_id, &error.to_string())
                                    .into(),
                            ))
                            .await;
                        return Err(error);
                    }
                };
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
    pcm_scratch: Vec<u8>,
    asr_session: Box<dyn LocalStreamingAsrSession + Send>,
}

fn should_accept_audio_sequence(last_sequence: Option<u64>, sequence: u64) -> bool {
    match last_sequence {
        Some(last_sequence) => sequence > last_sequence,
        None => true,
    }
}

/// Build a structured `error` server frame the client understands, so a session
/// failure (e.g. no speech detected) is reported rather than dropped silently.
fn streaming_asr_error_frame(session_id: &str, message: &str) -> String {
    json!({
        "type": "error",
        "session_id": session_id,
        "message": message,
    })
    .to_string()
}

fn parse_client_message(message: Message) -> Result<Option<ClientMessage>> {
    match message {
        Message::Text(text) => Ok(Some(serde_json::from_str(&text)?)),
        Message::Binary(bytes) => {
            let text = std::str::from_utf8(bytes.as_ref())
                .context("binary client message must be UTF-8 JSON")?;
            Ok(Some(serde_json::from_str(text)?))
        }
        Message::Ping(_) | Message::Pong(_) => Ok(None),
        Message::Close(_) => Ok(None),
        Message::Frame(_) => Ok(None),
    }
}

fn required_existing_file(name: &str, value: Option<&PathBuf>) -> Result<PathBuf> {
    let path = value.with_context(|| format!("{name} must be set for the selected sherpa mode"))?;
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

fn decode_pcm_base64_into(pcm_base64: &str, pcm_scratch: &mut Vec<u8>) -> Result<()> {
    pcm_scratch.clear();
    if let Err(error) =
        base64::engine::general_purpose::STANDARD.decode_vec(pcm_base64.as_bytes(), pcm_scratch)
    {
        pcm_scratch.clear();
        return Err(error).context("audio.pcm_base64 must be valid base64 PCM");
    }
    Ok(())
}

fn pcm_i16_le_to_f32_into(pcm: &[u8], sample_scratch: &mut Vec<f32>) -> Result<()> {
    sample_scratch.clear();
    if !pcm.len().is_multiple_of(2) {
        anyhow::bail!("PCM byte length must be even for signed 16-bit little-endian audio");
    }
    sample_scratch.extend(
        pcm.chunks_exact(2)
            .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]) as f32 / 32768.0),
    );
    Ok(())
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

fn validate_nonnegative_seconds(name: &str, value: f32) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        anyhow::bail!("{name} must be a finite, non-negative number of seconds");
    }
    Ok(())
}

fn validate_modeling_unit_and_bpe_vocab(
    modeling_unit: Option<&str>,
    bpe_vocab: Option<&Path>,
) -> Result<()> {
    match modeling_unit {
        None | Some("cjkchar") => Ok(()),
        Some("bpe" | "cjkchar+bpe") if bpe_vocab.is_some() => Ok(()),
        Some("bpe" | "cjkchar+bpe") => {
            anyhow::bail!("--bpe-vocab is required when --modeling-unit includes bpe")
        }
        Some(other) => {
            anyhow::bail!("--modeling-unit must be cjkchar, bpe, or cjkchar+bpe, got {other}")
        }
    }
}

fn tail_padding_sample_count(sample_rate_hz: u32, padding_ms: u32) -> Result<usize> {
    if sample_rate_hz == 0 {
        anyhow::bail!("--sample-rate-hz must be greater than 0");
    }
    if padding_ms > MAX_ONLINE_TAIL_PADDING_MS {
        anyhow::bail!(
            "--online-tail-padding-ms must be between 0 and {MAX_ONLINE_TAIL_PADDING_MS}"
        );
    }
    let samples = u64::from(sample_rate_hz)
        .checked_mul(u64::from(padding_ms))
        .context("online tail-padding sample count overflow")?
        / 1_000;
    if samples > MAX_ONLINE_TAIL_PADDING_SAMPLES {
        anyhow::bail!(
            "--online-tail-padding-ms and --sample-rate-hz require {samples} padding samples, exceeding the safe limit of {MAX_ONLINE_TAIL_PADDING_SAMPLES}"
        );
    }
    usize::try_from(samples).context("online tail-padding sample count exceeds usize")
}

fn validate_session_id(session_id: &str) -> Result<()> {
    validate_nonblank("session_id", session_id)
}

#[cfg(test)]
mod tests {
    use super::{
        combine_committed_and_segment, decode_pcm_base64_into, handle_connection,
        parse_client_message, pcm_i16_le_to_f32_into, should_accept_audio_sequence,
        streaming_asr_error_frame, tail_padding_sample_count, validate_loopback_bind,
        validate_modeling_unit_and_bpe_vocab, validate_nonnegative_seconds, Cli, DaemonConfig,
        DaemonMode, LocalAsrText, LocalStreamingAsrEngine, LocalStreamingAsrSession,
        SherpaOfflineModelConfig, SherpaOnlineModelFamily,
    };
    use anyhow::Result;
    use base64::Engine;
    use clap::Parser;
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::fs;
    use std::net::SocketAddr;
    use std::path::{Path, PathBuf};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn validate_nonnegative_seconds_rejects_negative_and_nonfinite() {
        assert!(validate_nonnegative_seconds("--rule1", 2.4).is_ok());
        assert!(validate_nonnegative_seconds("--rule1", 0.0).is_ok());
        assert!(validate_nonnegative_seconds("--rule1", -0.1).is_err());
        assert!(validate_nonnegative_seconds("--rule1", f32::NAN).is_err());
        assert!(validate_nonnegative_seconds("--rule1", f32::INFINITY).is_err());
    }

    #[test]
    fn online_tail_padding_converts_milliseconds_to_samples_and_bounds_allocation() {
        assert_eq!(tail_padding_sample_count(16_000, 300).unwrap(), 4_800);
        assert_eq!(tail_padding_sample_count(48_000, 0).unwrap(), 0);
        assert_eq!(tail_padding_sample_count(384_000, 2_000).unwrap(), 768_000);
        assert!(tail_padding_sample_count(0, 300).is_err());
        assert!(tail_padding_sample_count(16_000, 2_001).is_err());
        assert!(tail_padding_sample_count(u32::MAX, 2_000).is_err());
    }

    #[test]
    fn bpe_modeling_units_require_text_vocabulary() {
        assert!(validate_modeling_unit_and_bpe_vocab(None, None).is_ok());
        assert!(validate_modeling_unit_and_bpe_vocab(Some("cjkchar"), None).is_ok());
        assert!(validate_modeling_unit_and_bpe_vocab(Some("bpe"), None).is_err());
        assert!(validate_modeling_unit_and_bpe_vocab(
            Some("cjkchar+bpe"),
            Some(Path::new("bpe.vocab")),
        )
        .is_ok());
        assert!(validate_modeling_unit_and_bpe_vocab(Some("sentencepiece"), None).is_err());
    }

    #[test]
    fn binary_client_message_parsing_borrows_payload_bytes() {
        let source = include_str!("main.rs");
        let start = source
            .find("        Message::Binary(bytes) => {")
            .expect("binary client message branch");
        let end = source[start..]
            .find("        Message::Ping(_) | Message::Pong(_) =>")
            .map(|offset| start + offset)
            .expect("message branch following binary client message");
        let binary_branch = &source[start..end];

        assert!(binary_branch.contains("std::str::from_utf8(bytes.as_ref())"));
        assert!(!binary_branch.contains("bytes.to_vec()"));
        assert!(!binary_branch.contains("String::from_utf8"));

        let parsed = parse_client_message(Message::Binary(
            br#"{"type":"stop","session_id":"session-1"}"#.to_vec().into(),
        ))
        .expect("valid binary JSON should parse")
        .expect("binary JSON should produce a client message");
        assert!(
            matches!(parsed, super::ClientMessage::Stop { session_id } if session_id == "session-1")
        );

        let error = parse_client_message(Message::Binary(vec![0xff, 0xfe].into()))
            .expect_err("invalid binary UTF-8 should fail")
            .to_string();
        assert!(error.contains("binary client message must be UTF-8 JSON"));
    }

    #[test]
    fn combine_committed_and_segment_is_passthrough_when_prefix_empty() {
        assert_eq!(combine_committed_and_segment("", "你好呀"), "你好呀");
        assert_eq!(combine_committed_and_segment("", ""), "");
    }

    #[test]
    fn combine_committed_and_segment_trims_model_boundary_whitespace() {
        assert_eq!(
            combine_committed_and_segment("  first  ", "  second  "),
            "first second"
        );
        assert_eq!(
            combine_committed_and_segment("  你好  ", "  呀  "),
            "你好呀"
        );
        assert_eq!(combine_committed_and_segment("  ", "  text  "), "text");
        assert_eq!(combine_committed_and_segment("  text  ", "  "), "text");
    }

    #[test]
    fn combine_committed_and_segment_prepends_committed_prefix() {
        assert_eq!(
            combine_committed_and_segment("第一句。", "第二句"),
            "第一句。第二句"
        );
    }

    #[test]
    fn combine_committed_and_segment_preserves_ascii_word_boundary() {
        assert_eq!(
            combine_committed_and_segment("hello", "world"),
            "hello world"
        );
        assert_eq!(combine_committed_and_segment("hello", "."), "hello.");
    }

    #[test]
    fn error_frame_has_client_parseable_shape() {
        let frame =
            streaming_asr_error_frame("sess-1", "sherpa-online produced no final transcript");
        let value: Value = serde_json::from_str(&frame).expect("error frame must be valid json");

        assert_eq!(value["type"], "error");
        assert_eq!(value["session_id"], "sess-1");
        assert_eq!(
            value["message"],
            "sherpa-online produced no final transcript"
        );
    }

    #[test]
    fn bind_must_be_loopback() {
        let public: SocketAddr = "0.0.0.0:53171".parse().unwrap();
        assert!(validate_loopback_bind(public).is_err());

        let loopback: SocketAddr = "127.0.0.1:53171".parse().unwrap();
        assert!(validate_loopback_bind(loopback).is_ok());
    }

    #[test]
    fn audio_sequence_must_strictly_increase() {
        assert!(!should_accept_audio_sequence(Some(5), 5));
        assert!(!should_accept_audio_sequence(Some(5), 4));
        assert!(should_accept_audio_sequence(Some(5), 6));
    }

    #[test]
    fn pcm_base64_decode_reuses_scratch_capacity_and_clears_failed_output() {
        let mut scratch = Vec::new();
        decode_pcm_base64_into("AAECAwQFBgc=", &mut scratch).expect("decode first PCM chunk");
        assert_eq!(scratch, [0, 1, 2, 3, 4, 5, 6, 7]);
        let first_pointer = scratch.as_ptr();
        let first_capacity = scratch.capacity();

        let equal_size_pcm =
            base64::engine::general_purpose::STANDARD.encode([7, 6, 5, 4, 3, 2, 1, 0]);
        decode_pcm_base64_into(&equal_size_pcm, &mut scratch).expect("decode equal-size PCM chunk");
        assert_eq!(scratch, [7, 6, 5, 4, 3, 2, 1, 0]);
        assert_eq!(scratch.as_ptr(), first_pointer);
        assert_eq!(scratch.capacity(), first_capacity);

        decode_pcm_base64_into("AQI=", &mut scratch).expect("decode smaller PCM chunk");
        assert_eq!(scratch, [1, 2]);
        assert_eq!(scratch.as_ptr(), first_pointer);
        assert_eq!(scratch.capacity(), first_capacity);

        let error = decode_pcm_base64_into("%%%", &mut scratch)
            .expect_err("invalid base64 PCM must fail")
            .to_string();
        assert!(error.contains("audio.pcm_base64 must be valid base64 PCM"));
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), first_capacity);
    }

    #[test]
    fn pcm_i16_conversion_reuses_scratch_capacity_and_preserves_values() {
        let first_pcm = [i16::MIN, -16_384, -1, 0, 1, 8_192, 16_384, i16::MAX]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let mut scratch = Vec::new();
        pcm_i16_le_to_f32_into(&first_pcm, &mut scratch).expect("convert first PCM chunk");
        assert_eq!(
            scratch,
            [
                -1.0,
                -0.5,
                -1.0 / 32768.0,
                0.0,
                1.0 / 32768.0,
                0.25,
                0.5,
                i16::MAX as f32 / 32768.0,
            ]
        );
        let first_pointer = scratch.as_ptr();
        let first_capacity = scratch.capacity();

        pcm_i16_le_to_f32_into(&first_pcm, &mut scratch).expect("convert equal-size PCM chunk");
        assert_eq!(scratch.as_ptr(), first_pointer);
        assert_eq!(scratch.capacity(), first_capacity);

        pcm_i16_le_to_f32_into(&[0, 0, 255, 127], &mut scratch).expect("convert smaller PCM chunk");
        assert_eq!(scratch, [0.0, i16::MAX as f32 / 32768.0]);
        assert_eq!(scratch.as_ptr(), first_pointer);
        assert_eq!(scratch.capacity(), first_capacity);

        let error = pcm_i16_le_to_f32_into(&[0], &mut scratch)
            .expect_err("odd PCM byte length must fail")
            .to_string();
        assert!(error.contains("PCM byte length must be even"));
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), first_capacity);
    }

    #[test]
    fn pcm_base64_decode_and_i16_conversion_preserve_endian_and_scale() {
        let pcm = [i16::MIN, -1, 0, 1, i16::MAX]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let encoded = base64::engine::general_purpose::STANDARD.encode(&pcm);
        let mut byte_scratch = Vec::new();
        let mut sample_scratch = Vec::new();

        decode_pcm_base64_into(&encoded, &mut byte_scratch).expect("decode PCM payload");
        pcm_i16_le_to_f32_into(&byte_scratch, &mut sample_scratch)
            .expect("convert decoded PCM payload");

        assert_eq!(byte_scratch, pcm);
        assert_eq!(
            sample_scratch,
            [
                -1.0,
                -1.0 / 32768.0,
                0.0,
                1.0 / 32768.0,
                i16::MAX as f32 / 32768.0,
            ]
        );
    }

    #[test]
    fn dry_run_mode_does_not_require_model_files() {
        let config = DaemonConfig::from_cli(test_cli()).expect("dry-run config should validate");

        assert_eq!(config.mode, DaemonMode::DryRun);
        assert!(config.sherpa_online.is_none());
        assert!(config.sherpa_offline.is_none());
        assert_eq!(config.model, "dry-run-streaming-zipformer");
    }

    #[test]
    fn sherpa_offline_mode_requires_transducer_model_and_pcm16_wav() {
        let temp_dir = unique_temp_dir("talk-sherpa-offline-model");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.int8.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.onnx");
        let joiner = write_marker_file(&temp_dir, "joiner.int8.onnx");
        let wav = temp_dir.join("sample.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav, spec).expect("create wav");
        writer.write_sample::<i16>(1).expect("write wav sample");
        writer.finalize().expect("finalize wav");

        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOffline;
        cli.tokens = Some(tokens.clone());
        cli.encoder = Some(encoder.clone());
        cli.decoder = Some(decoder.clone());
        cli.joiner = Some(joiner.clone());
        cli.offline_wav = Some(wav.clone());

        let config = DaemonConfig::from_cli(cli).expect("offline config should validate");
        let offline = config.sherpa_offline.expect("offline config");
        assert_eq!(offline.audio_wav, wav);
        assert_eq!(offline.tokens, tokens);
        assert_eq!(
            offline.model_config,
            SherpaOfflineModelConfig::Transducer {
                encoder,
                decoder,
                joiner,
            }
        );
    }

    #[test]
    fn sherpa_offline_mode_rejects_paraformer_family() {
        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOffline;
        cli.model_family = SherpaOnlineModelFamily::Paraformer;

        let error = DaemonConfig::from_cli(cli)
            .expect_err("offline paraformer should be rejected")
            .to_string();
        assert!(error.contains("does not support --model-family paraformer"));
    }

    #[test]
    fn sherpa_offline_mode_accepts_sense_voice_model() {
        let temp_dir = unique_temp_dir("talk-sherpa-offline-sense-voice");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let model = write_marker_file(&temp_dir, "model.int8.onnx");
        let wav = temp_dir.join("sample.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav, spec).expect("create wav");
        writer.write_sample::<i16>(1).expect("write wav sample");
        writer.finalize().expect("finalize wav");

        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOffline;
        cli.model_family = SherpaOnlineModelFamily::SenseVoice;
        cli.tokens = Some(tokens.clone());
        cli.sense_voice_model = Some(model.clone());
        cli.sense_voice_language = "auto".to_string();
        cli.sense_voice_use_itn = true;
        cli.offline_wav = Some(wav.clone());

        let config = DaemonConfig::from_cli(cli).expect("SenseVoice config should validate");
        let offline = config.sherpa_offline.expect("offline config");
        assert_eq!(offline.audio_wav, wav);
        assert_eq!(offline.tokens, tokens);
        assert_eq!(
            offline.model_config,
            SherpaOfflineModelConfig::SenseVoice {
                model,
                language: "auto".to_string(),
                use_itn: true,
            }
        );
    }

    #[test]
    fn sherpa_online_mode_rejects_sense_voice_model() {
        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOnline;
        cli.model_family = SherpaOnlineModelFamily::SenseVoice;

        let error = DaemonConfig::from_cli(cli)
            .expect_err("online SenseVoice should be rejected")
            .to_string();
        assert!(error.contains("sense-voice and whisper require --mode sherpa-offline"));
    }

    #[test]
    fn sherpa_offline_mode_accepts_whisper_model() {
        let temp_dir = unique_temp_dir("talk-sherpa-offline-whisper");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "base-encoder.int8.onnx");
        let decoder = write_marker_file(&temp_dir, "base-decoder.int8.onnx");
        let wav = temp_dir.join("sample.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav, spec).expect("create wav");
        writer.write_sample::<i16>(1).expect("write wav sample");
        writer.finalize().expect("finalize wav");

        let mut cli = test_cli();
        cli.mode = DaemonMode::SherpaOffline;
        cli.model_family = SherpaOnlineModelFamily::Whisper;
        cli.tokens = Some(tokens.clone());
        cli.encoder = Some(encoder.clone());
        cli.decoder = Some(decoder.clone());
        cli.whisper_language = None;
        cli.whisper_task = "transcribe".to_string();
        cli.whisper_tail_paddings = 0;
        cli.offline_wav = Some(wav.clone());

        let config = DaemonConfig::from_cli(cli).expect("Whisper config should validate");
        let offline = config.sherpa_offline.expect("offline config");
        assert_eq!(offline.audio_wav, wav);
        assert_eq!(offline.tokens, tokens);
        assert_eq!(
            offline.model_config,
            SherpaOfflineModelConfig::Whisper {
                encoder,
                decoder,
                language: None,
                task: "transcribe".to_string(),
                tail_paddings: 0,
            }
        );
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

    #[test]
    fn cli_accepts_explicit_boolean_values_for_endpoint_flags() {
        let temp_dir = unique_temp_dir("talk-sherpa-cli-bool-flags");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.onnx");

        let cli = Cli::try_parse_from([
            "talk-local-asr-sherpa",
            "--bind",
            "127.0.0.1:53171",
            "--mode",
            "sherpa-online",
            "--model-family",
            "paraformer",
            "--tokens",
            tokens.to_str().expect("tokens path"),
            "--encoder",
            encoder.to_str().expect("encoder path"),
            "--decoder",
            decoder.to_str().expect("decoder path"),
            "--provider",
            "cpu",
            "--num-threads",
            "2",
            "--sample-rate-hz",
            "16000",
            "--decoding-method",
            "greedy_search",
            "--enable-endpoint",
            "true",
            "--endpoint-reset",
            "false",
        ])
        .expect("cli should accept explicit boolean values");

        assert!(cli.enable_endpoint);
        assert!(!cli.endpoint_reset);
    }

    #[test]
    fn cli_accepts_desktop_paraformer_launch_shape_with_explicit_endpoint_values() {
        let temp_dir = unique_temp_dir("talk-sherpa-cli-desktop-paraformer-shape");
        let tokens = write_marker_file(&temp_dir, "tokens.txt");
        let encoder = write_marker_file(&temp_dir, "encoder.int8.onnx");
        let decoder = write_marker_file(&temp_dir, "decoder.int8.onnx");

        let cli = Cli::try_parse_from([
            "talk-local-asr-sherpa",
            "--bind",
            "127.0.0.1:53171",
            "--mode",
            "sherpa-online",
            "--model",
            "paraformer-bilingual-zh-en",
            "--model-family",
            "paraformer",
            "--tokens",
            tokens.to_str().expect("tokens path"),
            "--encoder",
            encoder.to_str().expect("encoder path"),
            "--decoder",
            decoder.to_str().expect("decoder path"),
            "--provider",
            "cpu",
            "--num-threads",
            "2",
            "--sample-rate-hz",
            "16000",
            "--decoding-method",
            "greedy_search",
            "--enable-endpoint",
            "true",
            "--endpoint-reset",
            "true",
        ])
        .expect("desktop paraformer launch shape should parse");

        assert_eq!(cli.bind, "127.0.0.1:53171".parse().expect("bind"));
        assert_eq!(cli.mode, DaemonMode::SherpaOnline);
        assert_eq!(cli.model, "paraformer-bilingual-zh-en");
        assert_eq!(cli.model_family, SherpaOnlineModelFamily::Paraformer);
        assert_eq!(cli.tokens.as_deref(), Some(tokens.as_path()));
        assert_eq!(cli.encoder.as_deref(), Some(encoder.as_path()));
        assert_eq!(cli.decoder.as_deref(), Some(decoder.as_path()));
        assert!(cli.enable_endpoint);
        assert!(cli.endpoint_reset);
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
            sense_voice_model: None,
            sense_voice_language: "auto".to_string(),
            sense_voice_use_itn: true,
            whisper_language: None,
            whisper_task: "transcribe".to_string(),
            whisper_tail_paddings: 0,
            provider: "cpu".to_string(),
            num_threads: 2,
            sample_rate_hz: 16000,
            decoding_method: "greedy_search".to_string(),
            enable_endpoint: true,
            endpoint_reset: false,
            rule1_min_trailing_silence: 2.4,
            rule2_min_trailing_silence: 1.2,
            rule3_min_utterance_length: 20.0,
            max_active_paths: 4,
            hotwords_score: 1.5,
            blank_penalty: 0.0,
            online_tail_padding_ms: 300,
            modeling_unit: None,
            bpe_vocab: None,
            hotwords_file: None,
            rule_fsts: None,
            rule_fars: None,
            offline_wav: None,
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

    struct CountingEngine {
        accepted_audio_chunks: Arc<AtomicUsize>,
        accepted_pcm: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl LocalStreamingAsrEngine for CountingEngine {
        fn ready_engine(&self) -> &str {
            "counting"
        }

        fn ready_model(&self) -> &str {
            "counting"
        }

        fn start_session(
            &self,
            _sample_rate_hz: u32,
            _channels: u16,
            _language: Option<String>,
        ) -> Result<Box<dyn LocalStreamingAsrSession + Send>> {
            Ok(Box::new(CountingSession {
                accepted_audio_chunks: self.accepted_audio_chunks.clone(),
                accepted_pcm: self.accepted_pcm.clone(),
            }))
        }
    }

    struct CountingSession {
        accepted_audio_chunks: Arc<AtomicUsize>,
        accepted_pcm: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl LocalStreamingAsrSession for CountingSession {
        fn accept_pcm_i16_le(&mut self, pcm: &[u8]) -> Result<Option<LocalAsrText>> {
            self.accepted_audio_chunks.fetch_add(1, Ordering::SeqCst);
            self.accepted_pcm.lock().unwrap().push(pcm.to_vec());
            Ok(None)
        }

        fn finish(&mut self) -> Result<LocalAsrText> {
            Ok(LocalAsrText {
                segment_id: "counting-segment-1".to_string(),
                text: "done".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn daemon_ignores_duplicate_and_stale_audio_sequences() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let accepted_audio_chunks = Arc::new(AtomicUsize::new(0));
        let accepted_pcm = Arc::new(Mutex::new(Vec::new()));
        let engine = Arc::new(CountingEngine {
            accepted_audio_chunks: accepted_audio_chunks.clone(),
            accepted_pcm: accepted_pcm.clone(),
        });
        let server = tokio::spawn(async move {
            let (stream, peer) = listener.accept().await.unwrap();
            handle_connection(stream, peer, engine).await.unwrap();
        });

        let (mut websocket, _) = connect_async(endpoint).await.unwrap();
        websocket
            .send(Message::Text(
                r#"{"type":"start","session_id":"sequence-session","sample_rate_hz":16000,"channels":1}"#
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

        for sequence in [5, 5, 4, 6] {
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "type": "audio",
                        "session_id": "sequence-session",
                        "sequence": sequence,
                        "pcm_base64": "AAAA"
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
        websocket
            .send(Message::Text(
                r#"{"type":"stop","session_id":"sequence-session"}"#.into(),
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
        assert_eq!(final_message["audio_chunks"], 2);
        assert_eq!(final_message["last_sequence"], 6);
        assert_eq!(accepted_audio_chunks.load(Ordering::SeqCst), 2);
        assert_eq!(*accepted_pcm.lock().unwrap(), [[0, 0, 0], [0, 0, 0]]);

        server.await.unwrap();
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
