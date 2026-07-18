use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use talk_client::{
    final_transcript_from_streaming_asr_events, FrontContext, LocalStreamingAsrServiceClient,
    OpenAiCompatibleTranscriber, StreamingAsrEvent, Transcriber,
};
use talk_core::OpenAiTranscriptionTransport;

#[derive(Debug, Parser)]
#[command(
    name = "asr-bench",
    version,
    about = "Talk local ASR benchmark schema runner"
)]
struct Cli {
    #[arg(long)]
    engine: Option<String>,
    #[arg(long)]
    audio_wav: Option<PathBuf>,
    #[arg(long)]
    streaming_endpoint: Option<String>,
    #[arg(long)]
    cloud_openai_compatible_endpoint: Option<String>,
    #[arg(long)]
    cloud_openai_compatible_model: Option<String>,
    #[arg(long, default_value = "audio_transcriptions")]
    cloud_openai_compatible_transport: String,
    #[arg(long, default_value = "TALK_PROVIDER_API_KEY")]
    cloud_openai_compatible_api_key_env: String,
    #[arg(long, default_value_t = 80)]
    chunk_ms: u64,
    #[arg(long, default_value_t = 1000)]
    connect_timeout_ms: u64,
    #[arg(long, default_value_t = 1000)]
    ready_timeout_ms: u64,
    #[arg(long, default_value_t = 10)]
    partial_idle_timeout_ms: u64,
    #[arg(long, default_value_t = 7000)]
    final_timeout_ms: u64,
    #[arg(long)]
    dry_run_text: Option<String>,
    #[arg(long)]
    reference_text: Option<String>,
    #[arg(long)]
    output_json: PathBuf,
    #[arg(long = "compare-report")]
    compare_reports: Vec<PathBuf>,
    #[arg(long)]
    model_size_mb: Option<u64>,
    #[arg(long)]
    sample_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct AsrBenchReport {
    engine: String,
    audio_duration_ms: u64,
    cold_start_ms: u64,
    first_partial_ms: u64,
    final_latency_ms: u64,
    rtf: f64,
    peak_rss_mb: u64,
    text: String,
    cer: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model_size_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sample_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct AsrBenchComparisonSummary {
    selected_engine: String,
    candidates: Vec<AsrBenchComparisonCandidate>,
}

#[derive(Debug, Serialize)]
struct AsrBenchComparisonCandidate {
    engine: String,
    sources: Vec<PathBuf>,
    sample_count: usize,
    sample_ids: Vec<String>,
    score: f64,
    cer: f64,
    first_partial_ms: u64,
    final_latency_ms: u64,
    rtf: f64,
    peak_rss_mb: u64,
    model_size_mb: Option<u64>,
    text: String,
}

#[derive(Debug)]
struct StreamingServiceBenchConfig {
    endpoint: String,
    audio_wav: PathBuf,
    reference_text: Option<String>,
    output_json: PathBuf,
    chunk_ms: u64,
    connect_timeout: Duration,
    ready_timeout: Duration,
    partial_idle_timeout: Duration,
    final_timeout: Duration,
    model_size_mb: Option<u64>,
    sample_id: Option<String>,
}

#[derive(Debug)]
struct CloudOpenAiCompatibleBenchConfig {
    endpoint: String,
    model: String,
    transport: OpenAiTranscriptionTransport,
    api_key: Option<String>,
    audio_wav: PathBuf,
    reference_text: Option<String>,
    output_json: PathBuf,
    model_size_mb: Option<u64>,
    sample_id: Option<String>,
}

struct PreparedStreamingWav {
    sample_rate_hz: u32,
    channels: u16,
    duration_ms: u64,
    chunks: Vec<Vec<u8>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if !cli.compare_reports.is_empty() {
        let summary = compare_asr_bench_reports(&cli.compare_reports)?;
        write_comparison_summary(&cli.output_json, &summary)?;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }

    if let Some(endpoint) = cli.streaming_endpoint.clone() {
        let audio_wav = cli
            .audio_wav
            .clone()
            .context("--audio-wav is required when --streaming-endpoint is set")?;
        run_streaming_service_benchmark(StreamingServiceBenchConfig {
            endpoint,
            audio_wav,
            reference_text: cli.reference_text.clone(),
            output_json: cli.output_json.clone(),
            chunk_ms: cli.chunk_ms,
            connect_timeout: Duration::from_millis(cli.connect_timeout_ms),
            ready_timeout: Duration::from_millis(cli.ready_timeout_ms),
            partial_idle_timeout: Duration::from_millis(cli.partial_idle_timeout_ms),
            final_timeout: Duration::from_millis(cli.final_timeout_ms),
            model_size_mb: cli.model_size_mb,
            sample_id: cli.sample_id.clone(),
        })
        .await?;
        return Ok(());
    }

    if let Some(endpoint) = cli.cloud_openai_compatible_endpoint.clone() {
        let audio_wav = cli
            .audio_wav
            .clone()
            .context("--audio-wav is required when --cloud-openai-compatible-endpoint is set")?;
        let model = cli.cloud_openai_compatible_model.clone().context(
            "--cloud-openai-compatible-model is required when --cloud-openai-compatible-endpoint is set",
        )?;
        let transport =
            parse_openai_transcription_transport(&cli.cloud_openai_compatible_transport)?;
        let api_key = read_optional_api_key_env(&cli.cloud_openai_compatible_api_key_env)?;
        run_cloud_openai_compatible_benchmark(CloudOpenAiCompatibleBenchConfig {
            endpoint,
            model,
            transport,
            api_key,
            audio_wav,
            reference_text: cli.reference_text.clone(),
            output_json: cli.output_json.clone(),
            model_size_mb: cli.model_size_mb,
            sample_id: cli.sample_id.clone(),
        })
        .await?;
        return Ok(());
    }

    run_dry_run_benchmark(cli)
}

fn run_dry_run_benchmark(cli: Cli) -> Result<()> {
    let started_at = Instant::now();
    let engine = cli
        .engine
        .clone()
        .context("--engine is required unless --compare-report is set")?;
    validate_engine_name(&engine)?;

    let text = cli.dry_run_text.clone().context(
        "asr-bench currently requires --dry-run-text until a concrete ASR engine adapter is wired",
    )?;
    let sample_id = validate_optional_sample_id(cli.sample_id.as_deref())?;
    let audio_duration_ms = match cli.audio_wav.as_deref() {
        Some(path) => wav_duration_ms(path)?,
        None => 0,
    };
    let final_latency_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let rtf = if audio_duration_ms == 0 {
        0.0
    } else {
        final_latency_ms as f64 / audio_duration_ms as f64
    };
    let cer = cli
        .reference_text
        .as_deref()
        .map(|reference| character_error_rate(reference, &text))
        .unwrap_or(0.0);

    let report = AsrBenchReport {
        engine,
        audio_duration_ms,
        cold_start_ms: 0,
        first_partial_ms: 0,
        final_latency_ms,
        rtf,
        peak_rss_mb: 0,
        text,
        cer,
        model_size_mb: cli.model_size_mb,
        sample_id,
    };

    write_report(&cli.output_json, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_streaming_service_benchmark(
    config: StreamingServiceBenchConfig,
) -> Result<AsrBenchReport> {
    validate_engine_name("streaming_service")?;
    validate_duration("connect_timeout", config.connect_timeout)?;
    validate_duration("ready_timeout", config.ready_timeout)?;
    validate_duration("partial_idle_timeout", config.partial_idle_timeout)?;
    validate_duration("final_timeout", config.final_timeout)?;
    if config.chunk_ms == 0 {
        anyhow::bail!("--chunk-ms must be greater than 0");
    }

    let prepared = read_streaming_wav_chunks(&config.audio_wav, config.chunk_ms)?;
    let started_at = Instant::now();
    let mut client =
        LocalStreamingAsrServiceClient::connect(&config.endpoint, config.connect_timeout).await?;
    let ready = client
        .start(
            "asr-bench",
            prepared.sample_rate_hz,
            prepared.channels,
            None,
            config.ready_timeout,
        )
        .await?;
    let cold_start_ms = elapsed_ms(started_at);

    let mut events = Vec::<StreamingAsrEvent>::new();
    let mut first_partial_ms = None::<u64>;
    for (sequence, chunk) in prepared.chunks.iter().enumerate() {
        client
            .send_audio("asr-bench", sequence as u64, chunk)
            .await?;
        let available = client
            .collect_available_asr_events_until_idle(config.partial_idle_timeout)
            .await?;
        for event in available {
            if first_partial_ms.is_none() && !event.is_final() {
                first_partial_ms = Some(elapsed_ms(started_at));
            }
            events.push(event);
        }
    }

    client.stop("asr-bench").await?;
    let final_events = client
        .collect_asr_events_until_final(config.final_timeout)
        .await?;
    for event in final_events {
        if first_partial_ms.is_none() && !event.is_final() {
            first_partial_ms = Some(elapsed_ms(started_at));
        }
        events.push(event);
    }

    let final_latency_ms = elapsed_ms(started_at);
    let text = final_transcript_from_streaming_asr_events(&events)?;
    let cer = config
        .reference_text
        .as_deref()
        .map(|reference| character_error_rate(reference, &text))
        .unwrap_or(0.0);
    let rtf = if prepared.duration_ms == 0 {
        0.0
    } else {
        final_latency_ms as f64 / prepared.duration_ms as f64
    };
    let report = AsrBenchReport {
        engine: format!("streaming_service:{}:{}", ready.engine, ready.model),
        audio_duration_ms: prepared.duration_ms,
        cold_start_ms,
        first_partial_ms: first_partial_ms.unwrap_or(0),
        final_latency_ms,
        rtf,
        peak_rss_mb: 0,
        text,
        cer,
        model_size_mb: config.model_size_mb,
        sample_id: validate_optional_sample_id(config.sample_id.as_deref())?,
    };

    write_report(&config.output_json, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report)
}

async fn run_cloud_openai_compatible_benchmark(
    config: CloudOpenAiCompatibleBenchConfig,
) -> Result<AsrBenchReport> {
    let endpoint = validate_nonblank_cli_value(
        "--cloud-openai-compatible-endpoint",
        config.endpoint.as_str(),
    )?;
    let model =
        validate_nonblank_cli_value("--cloud-openai-compatible-model", config.model.as_str())?;
    let audio_duration_ms = wav_duration_ms(&config.audio_wav)?;
    let started_at = Instant::now();
    let transcriber = OpenAiCompatibleTranscriber::new_with_transport(
        endpoint,
        model,
        config.api_key,
        config.transport,
    );
    let text = transcriber
        .transcribe(config.audio_wav.clone(), FrontContext::default())
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    let final_latency_ms = elapsed_ms(started_at);
    let cer = config
        .reference_text
        .as_deref()
        .map(|reference| character_error_rate(reference, &text))
        .unwrap_or(0.0);
    let rtf = if audio_duration_ms == 0 {
        0.0
    } else {
        final_latency_ms as f64 / audio_duration_ms as f64
    };
    let report = AsrBenchReport {
        engine: format!(
            "cloud_openai_compatible:{}:{}",
            openai_transcription_transport_label(config.transport),
            model
        ),
        audio_duration_ms,
        cold_start_ms: 0,
        first_partial_ms: final_latency_ms,
        final_latency_ms,
        rtf,
        peak_rss_mb: 0,
        text,
        cer,
        model_size_mb: config.model_size_mb,
        sample_id: validate_optional_sample_id(config.sample_id.as_deref())?,
    };

    write_report(&config.output_json, &report)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(report)
}

fn parse_openai_transcription_transport(value: &str) -> Result<OpenAiTranscriptionTransport> {
    match value {
        "audio_transcriptions" => Ok(OpenAiTranscriptionTransport::AudioTranscriptions),
        "chat_completions_audio_input" => {
            Ok(OpenAiTranscriptionTransport::ChatCompletionsAudioInput)
        }
        _ => anyhow::bail!(
            "--cloud-openai-compatible-transport must be audio_transcriptions or chat_completions_audio_input"
        ),
    }
}

fn openai_transcription_transport_label(transport: OpenAiTranscriptionTransport) -> &'static str {
    match transport {
        OpenAiTranscriptionTransport::AudioTranscriptions => "audio_transcriptions",
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput => "chat_completions_audio_input",
    }
}

fn read_optional_api_key_env(env_name: &str) -> Result<Option<String>> {
    let env_name = validate_nonblank_cli_value("--cloud-openai-compatible-api-key-env", env_name)?;
    match std::env::var(env_name) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error)
            .with_context(|| format!("failed to read API key environment variable {env_name}")),
    }
}

fn validate_nonblank_cli_value<'a>(name: &str, value: &'a str) -> Result<&'a str> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} must not be blank");
    }
    if value.trim() != value {
        anyhow::bail!("{name} must not have leading or trailing whitespace");
    }
    Ok(value)
}

fn validate_engine_name(engine: &str) -> Result<()> {
    if engine.trim().is_empty() {
        anyhow::bail!("--engine must not be blank");
    }
    if engine.trim() != engine {
        anyhow::bail!("--engine must not have leading or trailing whitespace");
    }
    Ok(())
}

fn validate_duration(name: &str, duration: Duration) -> Result<()> {
    if duration.is_zero() {
        anyhow::bail!("{name} must be greater than 0");
    }
    Ok(())
}

fn validate_optional_sample_id(sample_id: Option<&str>) -> Result<Option<String>> {
    match sample_id {
        Some(value) => {
            if value.trim().is_empty() {
                anyhow::bail!("--sample-id must not be blank");
            }
            if value.trim() != value {
                anyhow::bail!("--sample-id must not have leading or trailing whitespace");
            }
            Ok(Some(value.to_string()))
        }
        None => Ok(None),
    }
}

fn wav_duration_ms(path: &Path) -> Result<u64> {
    let reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open wav file {}", path.display()))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        anyhow::bail!("wav file {} has zero channels", path.display());
    }
    if spec.sample_rate == 0 {
        anyhow::bail!("wav file {} has zero sample rate", path.display());
    }
    let frames = u64::from(reader.duration()) / u64::from(spec.channels);
    Ok(duration_ms_from_frames(frames, spec.sample_rate))
}

fn read_streaming_wav_chunks(path: &Path, chunk_ms: u64) -> Result<PreparedStreamingWav> {
    if chunk_ms == 0 {
        anyhow::bail!("chunk_ms must be greater than 0");
    }
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open wav file {}", path.display()))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        anyhow::bail!("wav file {} has zero channels", path.display());
    }
    if spec.sample_rate == 0 {
        anyhow::bail!("wav file {} has zero sample rate", path.display());
    }
    if spec.sample_format != hound::SampleFormat::Int || spec.bits_per_sample != 16 {
        anyhow::bail!(
            "streaming_service benchmark requires 16-bit PCM WAV, got {:?}/{} bits",
            spec.sample_format,
            spec.bits_per_sample
        );
    }

    let samples = reader
        .samples::<i16>()
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to read PCM samples from {}", path.display()))?;
    if samples.is_empty() {
        anyhow::bail!("wav file {} contains no samples", path.display());
    }
    let frames = samples.len() as u64 / u64::from(spec.channels);
    if frames == 0 {
        anyhow::bail!(
            "wav file {} contains no complete audio frames",
            path.display()
        );
    }
    let frames_per_chunk = ((u128::from(spec.sample_rate) * u128::from(chunk_ms))
        .div_ceil(1000)
        .max(1)) as usize;
    let samples_per_chunk = frames_per_chunk
        .checked_mul(usize::from(spec.channels))
        .context("streaming_service benchmark chunk size overflow")?;
    let chunks = samples
        .chunks(samples_per_chunk)
        .map(i16_samples_to_le_bytes)
        .collect::<Vec<_>>();

    Ok(PreparedStreamingWav {
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        duration_ms: duration_ms_from_frames(frames, spec.sample_rate),
        chunks,
    })
}

fn duration_ms_from_frames(frames: u64, sample_rate_hz: u32) -> u64 {
    if frames == 0 {
        return 0;
    }
    ((u128::from(frames) * 1000).div_ceil(u128::from(sample_rate_hz))) as u64
}

fn i16_samples_to_le_bytes(samples: &[i16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn compare_asr_bench_reports(paths: &[PathBuf]) -> Result<AsrBenchComparisonSummary> {
    if paths.is_empty() {
        anyhow::bail!("at least one --compare-report path is required");
    }

    let loaded_reports = paths
        .iter()
        .map(|path| read_benchmark_report(path))
        .collect::<Result<Vec<_>>>()?;
    let mut candidates = aggregate_comparison_candidates(loaded_reports)?;
    candidates.sort_by(|left, right| {
        left.score
            .total_cmp(&right.score)
            .then_with(|| left.cer.total_cmp(&right.cer))
            .then_with(|| left.first_partial_ms.cmp(&right.first_partial_ms))
            .then_with(|| left.final_latency_ms.cmp(&right.final_latency_ms))
            .then_with(|| left.engine.cmp(&right.engine))
    });
    let selected_engine = candidates
        .first()
        .map(|candidate| candidate.engine.clone())
        .context("no ASR benchmark candidates were loaded")?;

    Ok(AsrBenchComparisonSummary {
        selected_engine,
        candidates,
    })
}

struct LoadedBenchReport {
    source: PathBuf,
    report: AsrBenchReport,
}

fn read_benchmark_report(path: &Path) -> Result<LoadedBenchReport> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read benchmark report {}", path.display()))?;
    let report: AsrBenchReport = serde_json::from_str(&json)
        .with_context(|| format!("failed to parse benchmark report {}", path.display()))?;
    validate_report_metrics(path, &report)?;
    Ok(LoadedBenchReport {
        source: path.to_path_buf(),
        report,
    })
}

fn aggregate_comparison_candidates(
    reports: Vec<LoadedBenchReport>,
) -> Result<Vec<AsrBenchComparisonCandidate>> {
    let mut groups = BTreeMap::<String, Vec<LoadedBenchReport>>::new();
    for loaded in reports {
        groups
            .entry(loaded.report.engine.clone())
            .or_default()
            .push(loaded);
    }

    validate_comparable_corpus(&groups)?;

    groups
        .into_values()
        .map(aggregate_comparison_candidate)
        .collect()
}

fn aggregate_comparison_candidate(
    reports: Vec<LoadedBenchReport>,
) -> Result<AsrBenchComparisonCandidate> {
    let sample_count = reports.len();
    if sample_count == 0 {
        anyhow::bail!("cannot aggregate an empty benchmark report group");
    }
    let engine = reports[0].report.engine.clone();
    let sources = reports
        .iter()
        .map(|loaded| loaded.source.clone())
        .collect::<Vec<_>>();
    let sample_ids = reports
        .iter()
        .filter_map(|loaded| loaded.report.sample_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let cer = mean_f64(
        reports
            .iter()
            .map(|loaded| loaded.report.cer)
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    let first_partial_ms = mean_u64_rounded(
        reports
            .iter()
            .map(|loaded| loaded.report.first_partial_ms)
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    let final_latency_ms = mean_u64_rounded(
        reports
            .iter()
            .map(|loaded| loaded.report.final_latency_ms)
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    let rtf = mean_f64(
        reports
            .iter()
            .map(|loaded| loaded.report.rtf)
            .collect::<Vec<_>>()
            .as_slice(),
    )?;
    let peak_rss_mb = reports
        .iter()
        .map(|loaded| loaded.report.peak_rss_mb)
        .max()
        .unwrap_or(0);
    let model_size_mb = reports
        .iter()
        .filter_map(|loaded| loaded.report.model_size_mb)
        .max();
    let text = reports
        .last()
        .map(|loaded| loaded.report.text.clone())
        .unwrap_or_default();
    let score = asr_bench_score_from_metrics(
        cer,
        first_partial_ms,
        final_latency_ms,
        rtf,
        peak_rss_mb,
        model_size_mb,
    );

    Ok(AsrBenchComparisonCandidate {
        engine,
        sources,
        sample_count,
        sample_ids,
        score,
        cer,
        first_partial_ms,
        final_latency_ms,
        rtf,
        peak_rss_mb,
        model_size_mb,
        text,
    })
}

fn validate_report_metrics(path: &Path, report: &AsrBenchReport) -> Result<()> {
    validate_engine_name(&report.engine)
        .with_context(|| format!("invalid engine in {}", path.display()))?;
    if !report.cer.is_finite() || report.cer < 0.0 {
        anyhow::bail!(
            "benchmark report {} has invalid cer {}",
            path.display(),
            report.cer
        );
    }
    if !report.rtf.is_finite() || report.rtf < 0.0 {
        anyhow::bail!(
            "benchmark report {} has invalid rtf {}",
            path.display(),
            report.rtf
        );
    }
    if let Some(sample_id) = report.sample_id.as_deref() {
        validate_optional_sample_id(Some(sample_id))
            .with_context(|| format!("invalid sample_id in {}", path.display()))?;
    }
    Ok(())
}

fn asr_bench_score_from_metrics(
    cer: f64,
    first_partial_ms: u64,
    final_latency_ms: u64,
    rtf: f64,
    peak_rss_mb: u64,
    model_size_mb: Option<u64>,
) -> f64 {
    cer * 1_000_000.0
        + first_partial_ms as f64
        + final_latency_ms as f64 * 0.1
        + rtf * 100.0
        + peak_rss_mb as f64 * 0.01
        + model_size_mb.unwrap_or(0) as f64 * 0.001
}

fn validate_comparable_corpus(groups: &BTreeMap<String, Vec<LoadedBenchReport>>) -> Result<()> {
    if groups.len() <= 1 {
        return Ok(());
    }

    let any_sample_id = groups
        .values()
        .flatten()
        .any(|loaded| loaded.report.sample_id.is_some());
    if any_sample_id {
        let mut expected = None::<BTreeSet<String>>;
        for (engine, reports) in groups {
            let mut ids = BTreeSet::new();
            for loaded in reports {
                let sample_id = loaded.report.sample_id.clone().with_context(|| {
                    format!(
                        "benchmark report {} for engine {engine} is missing sample_id",
                        loaded.source.display()
                    )
                })?;
                if !ids.insert(sample_id.clone()) {
                    anyhow::bail!("engine {engine} has duplicate sample_id {sample_id}");
                }
            }
            match &expected {
                Some(expected) if expected != &ids => {
                    anyhow::bail!(
                        "all compared engines must use the same sample_id set; engine {engine} has {:?}, expected {:?}",
                        ids,
                        expected
                    );
                }
                None => expected = Some(ids),
                _ => {}
            }
        }
        return Ok(());
    }

    let mut expected_count = None::<usize>;
    for (engine, reports) in groups {
        match expected_count {
            Some(count) if count != reports.len() => {
                anyhow::bail!(
                    "all compared engines must have the same sample count when sample_id is absent; engine {engine} has {}, expected {count}",
                    reports.len()
                );
            }
            None => expected_count = Some(reports.len()),
            _ => {}
        }
    }
    Ok(())
}

fn mean_f64(values: &[f64]) -> Result<f64> {
    if values.is_empty() {
        anyhow::bail!("cannot calculate mean of empty values");
    }
    Ok(values.iter().sum::<f64>() / values.len() as f64)
}

fn mean_u64_rounded(values: &[u64]) -> Result<u64> {
    if values.is_empty() {
        anyhow::bail!("cannot calculate mean of empty values");
    }
    let sum = values.iter().map(|value| u128::from(*value)).sum::<u128>();
    Ok(
        ((sum + (values.len() as u128 / 2)) / values.len() as u128).min(u128::from(u64::MAX))
            as u64,
    )
}

fn character_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let reference_chars = reference.chars().collect::<Vec<_>>();
    let hypothesis_chars = hypothesis.chars().collect::<Vec<_>>();
    if reference_chars.is_empty() {
        return if hypothesis_chars.is_empty() {
            0.0
        } else {
            1.0
        };
    }
    levenshtein_distance(&reference_chars, &hypothesis_chars) as f64 / reference_chars.len() as f64
}

fn levenshtein_distance(reference: &[char], hypothesis: &[char]) -> usize {
    let mut previous = (0..=hypothesis.len()).collect::<Vec<_>>();
    let mut current = vec![0; hypothesis.len() + 1];

    for (row, reference_char) in reference.iter().enumerate() {
        current[0] = row + 1;
        for (column, hypothesis_char) in hypothesis.iter().enumerate() {
            let substitution_cost = usize::from(reference_char != hypothesis_char);
            current[column + 1] = (previous[column + 1] + 1)
                .min(current[column] + 1)
                .min(previous[column] + substitution_cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[hypothesis.len()]
}

fn write_report(path: &Path, report: &AsrBenchReport) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write benchmark report {}", path.display()))
}

fn write_comparison_summary(path: &Path, summary: &AsrBenchComparisonSummary) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(summary)?;
    std::fs::write(path, json)
        .with_context(|| format!("failed to write ASR comparison summary {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{
        character_error_rate, compare_asr_bench_reports, run_cloud_openai_compatible_benchmark,
        run_dry_run_benchmark, run_streaming_service_benchmark, AsrBenchReport, Cli,
        CloudOpenAiCompatibleBenchConfig, StreamingServiceBenchConfig,
    };
    use futures_util::{SinkExt, StreamExt};
    use serde_json::Value;
    use std::fs;
    use std::time::Duration;
    use talk_core::OpenAiTranscriptionTransport;
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;
    use tokio_tungstenite::tungstenite::Message;

    #[test]
    fn cer_is_zero_for_exact_match() {
        assert_eq!(character_error_rate("你好", "你好"), 0.0);
    }

    #[test]
    fn cer_counts_unicode_character_edits() {
        assert!((character_error_rate("你好呀", "你好") - (1.0 / 3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn compare_reports_selects_lower_cer_then_lower_latency() {
        let temp_dir = unique_temp_dir("talk-asr-bench-compare");
        let fast_wrong = temp_dir.join("fast-wrong.json");
        let slower_accurate = temp_dir.join("slower-accurate.json");
        write_test_report(
            &fast_wrong,
            AsrBenchReport {
                engine: "zipformer-fast-wrong".to_string(),
                audio_duration_ms: 1_500,
                cold_start_ms: 10,
                first_partial_ms: 120,
                final_latency_ms: 220,
                rtf: 0.15,
                peak_rss_mb: 128,
                text: "你好".to_string(),
                cer: 0.333,
                model_size_mb: Some(128),
                sample_id: Some("huihui-nihaoya".to_string()),
            },
        );
        write_test_report(
            &slower_accurate,
            AsrBenchReport {
                engine: "paraformer-slower-accurate".to_string(),
                audio_duration_ms: 1_500,
                cold_start_ms: 30,
                first_partial_ms: 240,
                final_latency_ms: 360,
                rtf: 0.24,
                peak_rss_mb: 512,
                text: "你好呀".to_string(),
                cer: 0.0,
                model_size_mb: Some(999),
                sample_id: Some("huihui-nihaoya".to_string()),
            },
        );

        let summary = compare_asr_bench_reports(&[fast_wrong, slower_accurate]).unwrap();

        assert_eq!(summary.selected_engine, "paraformer-slower-accurate");
        assert_eq!(summary.candidates.len(), 2);
        assert!(summary.candidates[0].score < summary.candidates[1].score);
    }

    #[test]
    fn compare_reports_aggregates_same_engine_across_corpus_samples() {
        let temp_dir = unique_temp_dir("talk-asr-bench-corpus-compare");
        let zipformer_one = temp_dir.join("zipformer-one.json");
        let zipformer_two = temp_dir.join("zipformer-two.json");
        let paraformer_one = temp_dir.join("paraformer-one.json");
        let paraformer_two = temp_dir.join("paraformer-two.json");
        write_test_report(
            &zipformer_one,
            test_report("zipformer", "short-search", 0.0, 120, 240, 0.18),
        );
        write_test_report(
            &zipformer_two,
            test_report("zipformer", "mixed-english", 0.1, 160, 300, 0.21),
        );
        write_test_report(
            &paraformer_one,
            test_report("paraformer", "short-search", 0.08, 90, 250, 0.19),
        );
        write_test_report(
            &paraformer_two,
            test_report("paraformer", "mixed-english", 0.08, 100, 260, 0.20),
        );

        let summary = compare_asr_bench_reports(&[
            zipformer_one,
            zipformer_two,
            paraformer_one,
            paraformer_two,
        ])
        .unwrap();

        assert_eq!(summary.selected_engine, "zipformer");
        assert_eq!(summary.candidates.len(), 2);
        assert_eq!(summary.candidates[0].engine, "zipformer");
        assert_eq!(summary.candidates[0].sample_count, 2);
        assert_eq!(
            summary.candidates[0].sample_ids,
            vec!["mixed-english".to_string(), "short-search".to_string()]
        );
        assert!((summary.candidates[0].cer - 0.05).abs() < f64::EPSILON);
        assert_eq!(summary.candidates[0].first_partial_ms, 140);
        assert_eq!(summary.candidates[0].final_latency_ms, 270);
    }

    #[test]
    fn compare_reports_rejects_mismatched_corpus_sample_ids() {
        let temp_dir = unique_temp_dir("talk-asr-bench-corpus-mismatch");
        let zipformer = temp_dir.join("zipformer.json");
        let paraformer = temp_dir.join("paraformer.json");
        write_test_report(
            &zipformer,
            test_report("zipformer", "short-search", 0.0, 120, 240, 0.18),
        );
        write_test_report(
            &paraformer,
            test_report("paraformer", "different-sample", 0.0, 120, 240, 0.18),
        );

        let error = compare_asr_bench_reports(&[zipformer, paraformer]).unwrap_err();

        assert!(
            error.to_string().contains("same sample_id set"),
            "{error:?}"
        );
    }

    #[test]
    fn dry_run_benchmark_writes_optional_model_size() {
        let temp_dir = unique_temp_dir("talk-asr-bench-model-size");
        let output_json = temp_dir.join("report.json");

        run_dry_run_benchmark(Cli {
            engine: Some("sherpa-onnx-zipformer".to_string()),
            audio_wav: None,
            streaming_endpoint: None,
            cloud_openai_compatible_endpoint: None,
            cloud_openai_compatible_model: None,
            cloud_openai_compatible_transport: "audio_transcriptions".to_string(),
            cloud_openai_compatible_api_key_env: "TALK_PROVIDER_API_KEY".to_string(),
            chunk_ms: 80,
            connect_timeout_ms: 1000,
            ready_timeout_ms: 1000,
            partial_idle_timeout_ms: 10,
            final_timeout_ms: 7000,
            dry_run_text: Some("你好".to_string()),
            reference_text: Some("你好".to_string()),
            output_json: output_json.clone(),
            compare_reports: Vec::new(),
            model_size_mb: Some(162),
            sample_id: Some("short-search".to_string()),
        })
        .unwrap();

        let report_json = fs::read_to_string(output_json).unwrap();
        let report_json = serde_json::from_str::<Value>(&report_json).unwrap();
        assert_eq!(report_json["model_size_mb"], 162);
        assert_eq!(report_json["sample_id"], "short-search");

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn cloud_openai_compatible_benchmark_posts_wav_and_records_final_latency() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request = read_http_request(&mut stream).await;
            assert!(request.starts_with("POST / HTTP/1.1"), "{request}");
            assert!(
                request.contains("content-type: application/json"),
                "{request}"
            );
            assert!(request.contains("input_audio"), "{request}");
            assert!(request.contains("data:audio/wav;base64,"), "{request}");
            let body = r#"{"choices":[{"message":{"content":"你好呀"}}]}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            use tokio::io::AsyncWriteExt;
            stream.write_all(response.as_bytes()).await.unwrap();
        });

        let temp_dir = unique_temp_dir("talk-asr-bench-cloud-openai-compatible");
        let wav_path = temp_dir.join("input.wav");
        write_test_wav_i16(&wav_path, &[0, 1000, -1000, 0]);
        let output_json = temp_dir.join("report.json");

        let report = run_cloud_openai_compatible_benchmark(CloudOpenAiCompatibleBenchConfig {
            endpoint,
            model: "qwen-audio-test".to_string(),
            transport: OpenAiTranscriptionTransport::ChatCompletionsAudioInput,
            api_key: None,
            audio_wav: wav_path,
            reference_text: Some("你好呀".to_string()),
            output_json: output_json.clone(),
            model_size_mb: None,
            sample_id: Some("short-search-001".to_string()),
        })
        .await
        .unwrap();

        assert_eq!(
            report.engine,
            "cloud_openai_compatible:chat_completions_audio_input:qwen-audio-test"
        );
        assert_eq!(report.text, "你好呀");
        assert_eq!(report.cer, 0.0);
        assert_eq!(report.first_partial_ms, report.final_latency_ms);
        assert_eq!(report.sample_id.as_deref(), Some("short-search-001"));
        let report_json = fs::read_to_string(output_json).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&report_json).unwrap()["engine"],
            "cloud_openai_compatible:chat_completions_audio_input:qwen-audio-test"
        );

        server.await.unwrap();
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[tokio::test]
    async fn streaming_service_benchmark_sends_wav_and_records_partial_and_final() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();

            let start = websocket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            let start: Value = serde_json::from_str(&start).unwrap();
            assert_eq!(start["type"], "start");
            assert_eq!(start["sample_rate_hz"], 16000);
            assert_eq!(start["channels"], 1);
            websocket
                .send(Message::Text(
                    r#"{"type":"ready","engine":"fake","model":"streaming-test","sample_rate_hz":16000,"channels":1}"#
                        .into(),
                ))
                .await
                .unwrap();

            let audio = websocket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            let audio: Value = serde_json::from_str(&audio).unwrap();
            assert_eq!(audio["type"], "audio");
            assert!(!audio["pcm_base64"].as_str().unwrap().is_empty());
            websocket
                .send(Message::Text(
                    r#"{"type":"partial","session_id":"asr-bench","segment_id":"p1","text":"你好"}"#
                        .into(),
                ))
                .await
                .unwrap();

            let stop = websocket
                .next()
                .await
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            let stop: Value = serde_json::from_str(&stop).unwrap();
            assert_eq!(stop["type"], "stop");
            websocket
                .send(Message::Text(
                    r#"{"type":"final","session_id":"asr-bench","segment_id":"f1","text":"你好呀"}"#
                        .into(),
                ))
                .await
                .unwrap();
        });

        let temp_dir = unique_temp_dir("talk-asr-bench-streaming");
        let wav_path = temp_dir.join("input.wav");
        write_test_wav_i16(&wav_path, &[0, 1000, -1000, 0]);
        let output_json = temp_dir.join("report.json");

        let report = run_streaming_service_benchmark(StreamingServiceBenchConfig {
            endpoint,
            audio_wav: wav_path,
            reference_text: Some("你好呀".to_string()),
            output_json: output_json.clone(),
            chunk_ms: 20,
            connect_timeout: Duration::from_secs(1),
            ready_timeout: Duration::from_secs(1),
            partial_idle_timeout: Duration::from_millis(20),
            final_timeout: Duration::from_secs(1),
            model_size_mb: None,
            sample_id: None,
        })
        .await
        .unwrap();

        assert_eq!(report.engine, "streaming_service:fake:streaming-test");
        assert_eq!(report.text, "你好呀");
        assert_eq!(report.cer, 0.0);
        assert!(report.audio_duration_ms > 0);
        assert!(report.first_partial_ms <= report.final_latency_ms);
        let report_json = fs::read_to_string(output_json).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&report_json).unwrap()["text"],
            "你好呀"
        );

        server.await.unwrap();
        fs::remove_dir_all(temp_dir).unwrap();
    }

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_test_wav_i16(path: &std::path::Path, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for sample in samples {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn write_test_report(path: &std::path::Path, report: AsrBenchReport) {
        let json = serde_json::to_string_pretty(&report).unwrap();
        fs::write(path, json).unwrap();
    }

    async fn read_http_request(stream: &mut tokio::net::TcpStream) -> String {
        use tokio::io::AsyncReadExt;
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            let read = stream.read(&mut buffer).await.unwrap();
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            let request = String::from_utf8_lossy(&bytes);
            if let Some(header_end) = request.find("\r\n\r\n") {
                let headers = &request[..header_end];
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        String::from_utf8(bytes).unwrap()
    }

    fn test_report(
        engine: &str,
        sample_id: &str,
        cer: f64,
        first_partial_ms: u64,
        final_latency_ms: u64,
        rtf: f64,
    ) -> AsrBenchReport {
        AsrBenchReport {
            engine: engine.to_string(),
            audio_duration_ms: 1_500,
            cold_start_ms: 5,
            first_partial_ms,
            final_latency_ms,
            rtf,
            peak_rss_mb: 256,
            text: sample_id.to_string(),
            cer,
            model_size_mb: Some(512),
            sample_id: Some(sample_id.to_string()),
        }
    }
}
