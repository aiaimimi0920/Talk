use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{ErrorKind, Read, Write};
use std::net::{IpAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use talk_audio::{
    play_wav, probe_audio_signal, probe_native_windows_audio_readiness_for_device,
    AudioPlaybackRequest, AudioSignalProbeRequest,
};
use talk_client::FrontContext;
use talk_core::{AudioBackendMode, SessionStatus, TalkConfig, VoiceMode};
use talk_runtime::{load_effective_config, run_voice_session, run_voice_session_with_audio_file};
use uuid::Uuid;

const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const MAX_HTTP_BODY_BYTES: usize = 1024 * 1024;

#[derive(Debug, Parser)]
#[command(
    name = "talk",
    version,
    about = "Talk standalone Neuro voice input and speech interaction app"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, default_value = "examples/dev-config.toml")]
    config: PathBuf,
}

#[derive(Debug, Subcommand)]
enum Command {
    Check {
        #[arg(long)]
        config: PathBuf,
    },
    Readiness {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    Once {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        mock_text: Option<String>,
        #[arg(long)]
        audio_file: Option<PathBuf>,
    },
    PlayWav {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        output_device: Option<String>,
    },
    ProbeAudio {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        seconds: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Serve {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        manifest_dir: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Check { config }) => check_command(&config).await,
        Some(Command::Readiness { config, json }) => readiness_command(&config, json).await,
        Some(Command::Once {
            config,
            mock_text,
            audio_file,
        }) => once_command(&config, mock_text, audio_file.as_deref()).await,
        Some(Command::PlayWav {
            file,
            output_device,
        }) => play_wav_command(&file, output_device.as_deref()),
        Some(Command::ProbeAudio {
            config,
            seconds,
            json,
        }) => probe_audio_command(&config, seconds, json).await,
        Some(Command::Serve {
            config,
            host,
            port,
            manifest_dir,
        }) => serve_command(&config, &host, port, manifest_dir.as_deref()).await,
        None => once_command(&cli.config, None, None).await,
    }
}

async fn check_command(path: &Path) -> Result<()> {
    let config = load_effective_config(path).await?;
    println!(
        "config ok :: trigger={} provider={:?} output={:?}",
        config.trigger.toggle_shortcut, config.provider.kind, config.output.mode
    );
    Ok(())
}

async fn readiness_command(path: &Path, json_output: bool) -> Result<()> {
    let config = load_effective_config(path).await?;
    let report = build_native_readiness_report(path, &config);

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "native readiness :: audio={} clipboard={}",
            report.audio.native_windows.status.as_str(),
            report.clipboard.native_windows.status.as_str()
        );
        if let Some(reason) = report.audio.native_windows.reason.as_deref() {
            println!("audio reason :: {reason}");
        }
        if let Some(reason) = report.clipboard.native_windows.reason.as_deref() {
            println!("clipboard reason :: {reason}");
        }
        if let Some(device_name) = report.audio.native_windows.device_name.as_deref() {
            println!("audio device :: {device_name}");
        }
        if let Some(sample_rate_hz) = report.audio.native_windows.default_sample_rate_hz {
            println!("audio sample rate hz :: {sample_rate_hz}");
        }
        if let Some(channels) = report.audio.native_windows.default_channels {
            println!("audio channels :: {channels}");
        }
        if let Some(sample_format) = report.audio.native_windows.sample_format.as_deref() {
            println!("audio sample format :: {sample_format}");
        }
    }

    Ok(())
}

async fn once_command(
    path: &Path,
    mock_text: Option<String>,
    audio_file: Option<&Path>,
) -> Result<()> {
    let config = load_effective_config(path).await?;
    let report = match audio_file {
        Some(audio_file) => {
            run_voice_session_with_audio_file(
                &config,
                audio_file.to_path_buf(),
                mock_text,
                None,
                FrontContext::default(),
                |_| {},
            )
            .await?
        }
        None => {
            run_voice_session(&config, mock_text, None, FrontContext::default(), |_| {}).await?
        }
    };
    if report.session.status() == SessionStatus::Failed {
        anyhow::bail!(
            "{}",
            report
                .session
                .error()
                .unwrap_or("voice session failed without reason")
        );
    }

    let outcome = report
        .outcome
        .as_ref()
        .context("completed voice session did not record insertion outcome")?;

    println!(
        "once ok :: session={} state={:?} outcome={:?} text={}",
        report.session.id(),
        report.session.status(),
        outcome,
        report.session.output_text().unwrap_or_default()
    );
    Ok(())
}

fn play_wav_command(file: &Path, output_device: Option<&str>) -> Result<()> {
    play_wav(&AudioPlaybackRequest {
        audio_path: file.to_path_buf(),
        output_device: output_device.map(str::to_string),
    })?;
    println!(
        "play-wav ok :: file={} output_device={}",
        file.display(),
        output_device.unwrap_or("default")
    );
    Ok(())
}

async fn probe_audio_command(
    path: &Path,
    requested_seconds: Option<u64>,
    json_output: bool,
) -> Result<()> {
    let config = load_effective_config(path).await?;
    let capture_seconds = requested_seconds.unwrap_or(config.audio.max_recording_seconds);
    let probe = probe_audio_signal(&AudioSignalProbeRequest {
        backend: config.audio.backend,
        temp_dir: config.audio.temp_dir.clone(),
        session_id: format!("audio-probe-{}", Uuid::new_v4()),
        input_device: config.audio.input_device.clone(),
        wav_settings: talk_audio::WavSettings {
            sample_rate_hz: config.audio.sample_rate_hz,
            channels: config.audio.channels,
        },
        capture_seconds,
    })?;
    let native_windows =
        matches!(config.audio.backend, AudioBackendMode::NativeWindows).then(|| {
            probe_native_windows_audio_readiness_for_device(config.audio.input_device.as_deref())
        });
    let report = AudioProbeReport {
        app: "talk",
        config_path: canonical_display_path(path),
        requested_duration_seconds: capture_seconds,
        audio: AudioProbeAudioReport {
            configured_backend: audio_backend_mode_name(config.audio.backend),
            native_windows,
            signal: AudioProbeSignalReport {
                artifact_path: probe.artifact.path.display().to_string(),
                mime_type: probe.artifact.mime_type,
                sample_rate_hz: probe.signal.sample_rate_hz,
                channels: probe.signal.channels,
                duration_seconds: probe.signal.duration_seconds,
                peak: probe.signal.peak,
                rms: probe.signal.rms,
                silent: probe.signal.silent,
            },
        },
    };

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "audio probe :: backend={} duration={:.3}s peak={:.6} rms={:.6} silent={} artifact={}",
            report.audio.configured_backend,
            report.audio.signal.duration_seconds,
            report.audio.signal.peak,
            report.audio.signal.rms,
            report.audio.signal.silent,
            report.audio.signal.artifact_path
        );
        if let Some(native_windows) = report.audio.native_windows.as_ref() {
            println!(
                "audio device :: requested={} selected={}",
                native_windows
                    .requested_device_name
                    .as_deref()
                    .unwrap_or("<default>"),
                native_windows.device_name.as_deref().unwrap_or("<unknown>")
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeReadinessReport {
    app: &'static str,
    config_path: String,
    all_ready: bool,
    audio: NativeAudioBackendReport,
    clipboard: NativeClipboardBackendReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeAudioBackendReport {
    configured_backend: &'static str,
    native_windows: talk_audio::NativeWindowsAudioReadiness,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeClipboardBackendReport {
    configured_backend: &'static str,
    native_windows: talk_insert::NativeWindowsClipboardReadiness,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioProbeReport {
    app: &'static str,
    config_path: String,
    requested_duration_seconds: u64,
    audio: AudioProbeAudioReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioProbeAudioReport {
    configured_backend: &'static str,
    native_windows: Option<talk_audio::NativeWindowsAudioReadiness>,
    signal: AudioProbeSignalReport,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AudioProbeSignalReport {
    artifact_path: String,
    mime_type: String,
    sample_rate_hz: u32,
    channels: u16,
    duration_seconds: f64,
    peak: f32,
    rms: f32,
    silent: bool,
}

fn build_native_readiness_report(path: &Path, config: &TalkConfig) -> NativeReadinessReport {
    let audio_readiness = talk_audio::probe_native_windows_audio_readiness_for_device(
        config.audio.input_device.as_deref(),
    );
    let clipboard_readiness = talk_insert::probe_native_windows_clipboard_readiness();

    NativeReadinessReport {
        app: "talk",
        config_path: canonical_display_path(path),
        all_ready: audio_readiness.status == talk_core::NativeReadinessStatus::Ready
            && clipboard_readiness.status == talk_core::NativeReadinessStatus::Ready,
        audio: NativeAudioBackendReport {
            configured_backend: audio_backend_mode_name(config.audio.backend),
            native_windows: audio_readiness,
        },
        clipboard: NativeClipboardBackendReport {
            configured_backend: clipboard_backend_mode_name(config.output.clipboard_backend),
            native_windows: clipboard_readiness,
        },
    }
}

fn canonical_display_path(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn audio_backend_mode_name(mode: talk_core::AudioBackendMode) -> &'static str {
    match mode {
        talk_core::AudioBackendMode::Silent => "silent",
        talk_core::AudioBackendMode::NativeWindows => "native_windows",
    }
}

fn clipboard_backend_mode_name(mode: talk_core::ClipboardBackendMode) -> &'static str {
    match mode {
        talk_core::ClipboardBackendMode::Fallback => "fallback",
        talk_core::ClipboardBackendMode::NativeWindows => "native_windows",
    }
}

async fn serve_command(
    config_path: &Path,
    host: &str,
    port: u16,
    manifest_dir: Option<&Path>,
) -> Result<()> {
    let Some(bind_host) = normalized_loopback_host(host) else {
        anyhow::bail!("Talk capability server host must be loopback, got {host}");
    };

    let config = load_effective_config(config_path).await?;
    let listener = TcpListener::bind((bind_host.as_str(), port))
        .with_context(|| format!("bind Talk capability server to {host}:{port}"))?;
    listener
        .set_nonblocking(false)
        .context("set Talk capability server blocking mode")?;
    let local_addr = listener
        .local_addr()
        .context("read Talk capability server address")?;
    let auth_token = Uuid::new_v4().to_string();
    let manifest_dir = manifest_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_talk_manifest_dir);
    let base_url = format!("http://{}:{}", url_host(local_addr.ip()), local_addr.port());
    write_talk_manifest(&manifest_dir, &base_url, &auth_token)?;
    println!(
        "talk serve ready :: {} manifest={}",
        base_url,
        manifest_dir.join("talk.json").display()
    );

    loop {
        let (mut stream, _) = listener.accept().context("accept Talk request")?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("set Talk request read timeout")?;
        match read_http_request(&mut stream)? {
            HttpReadOutcome::Empty => {}
            HttpReadOutcome::Rejected { status, body } => {
                write_http_json_response(&mut stream, status, &body)?;
            }
            HttpReadOutcome::Request(request) => {
                let parsed = ParsedHttpRequest::from_raw(&request);
                let (status, body) = route_talk_request(&config, &auth_token, &parsed).await?;
                write_http_json_response(&mut stream, status, &body)?;
            }
        }
    }
}

fn normalized_loopback_host(host: &str) -> Option<String> {
    if host.eq_ignore_ascii_case("localhost") {
        return Some(host.to_string());
    }
    parse_host_ip(host)
        .filter(|ip| ip.is_loopback())
        .map(|ip| ip.to_string())
}

fn parse_host_ip(host: &str) -> Option<IpAddr> {
    host.parse::<IpAddr>().ok().or_else(|| {
        host.strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .and_then(|value| value.parse::<IpAddr>().ok())
    })
}

fn url_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

fn default_talk_manifest_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().to_string_lossy().trim().is_empty())
        .map(|path| path.join("Neuro").join("capabilities"))
        .unwrap_or_else(|| PathBuf::from(".runtime").join("neuro").join("capabilities"))
}

fn write_talk_manifest(manifest_dir: &Path, base_url: &str, auth_token: &str) -> Result<()> {
    std::fs::create_dir_all(manifest_dir)
        .with_context(|| format!("create manifest dir {}", manifest_dir.display()))?;
    let manifest = json!({
        "schemaVersion": 1,
        "appId": "talk",
        "displayName": "Talk",
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "transport": {
            "type": "http",
            "baseUrl": base_url,
            "auth": "bearer",
            "authToken": auth_token
        },
        "capabilities": [
            "voice.capture.once",
            "voice.dictate"
        ],
        "startedAt": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or_default()
    });
    let path = manifest_dir.join("talk.json");
    std::fs::write(&path, serde_json::to_string_pretty(&manifest)?)
        .with_context(|| format!("write Talk manifest {}", path.display()))
}

async fn route_talk_request(
    config: &TalkConfig,
    auth_token: &str,
    request: &ParsedHttpRequest,
) -> Result<(u16, String)> {
    if !request.valid_request_line {
        return Ok(invalid_http_request("malformed HTTP request line"));
    }
    if request.method.eq_ignore_ascii_case("GET") && !request.body.is_empty() {
        return Ok(invalid_http_request("GET requests must not include a body"));
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/health") => Ok((
            200,
            json!({ "status": "ready", "appId": "talk" }).to_string(),
        )),
        ("GET", "/v1/capabilities") => Ok((200, talk_capabilities_json().to_string())),
        ("POST", "/v1/invoke") => {
            if request.has_ambiguous_authorization_header() {
                return Ok(invalid_http_request(
                    "Talk invoke requires a single Authorization header",
                ));
            }
            if !request.has_bearer(auth_token) {
                return Ok((
                    401,
                    json!({
                        "status": "failed",
                        "error": {
                            "code": "unauthorized",
                            "message": "missing or invalid Talk bearer token"
                        }
                    })
                    .to_string(),
                ));
            }
            if !request.has_single_content_length_header() {
                return Ok(invalid_http_request(
                    "Talk invoke requires a single Content-Length header",
                ));
            }
            if !request.has_json_content_type() {
                return Ok(invalid_http_request(
                    "Talk invoke requires Content-Type: application/json",
                ));
            }
            invoke_talk_capability(config, &request.body).await
        }
        _ => Ok((
            404,
            json!({
                "status": "failed",
                "error": {
                    "code": "not_found",
                    "message": "Talk endpoint was not found"
                }
            })
            .to_string(),
        )),
    }
}

fn talk_capabilities_json() -> Value {
    json!({
        "schemaVersion": 1,
        "appId": "talk",
        "displayName": "Talk",
        "capabilities": [
            {
                "id": "voice.capture.once",
                "description": "Capture one Talk voice session and return transcript, output text, and evidence."
            },
            {
                "id": "voice.dictate",
                "description": "Run dictation through Talk's configured voice pipeline."
            }
        ]
    })
}

#[derive(Debug)]
struct TalkInvokeRequest {
    request_id: String,
    caller: String,
    capability: String,
    input: Option<Value>,
}

async fn invoke_talk_capability(config: &TalkConfig, body: &str) -> Result<(u16, String)> {
    let request = match parse_talk_invoke_request(body) {
        Ok(request) => request,
        Err(error) => {
            return invalid_talk_invoke_request(error.request_id.as_deref(), &error.message);
        }
    };
    if request.request_id.trim().is_empty() {
        return invalid_talk_invoke_request(None, "requestId is required");
    }
    if request.request_id.trim() != request.request_id {
        return invalid_talk_invoke_request(
            None,
            "requestId must not have leading or trailing whitespace",
        );
    }
    if request.request_id.chars().any(char::is_whitespace) {
        return invalid_talk_invoke_request(None, "requestId must not contain whitespace");
    }
    if request.caller.trim().is_empty() {
        return invalid_talk_invoke_request(Some(&request.request_id), "caller is required");
    }
    if request.caller.trim() != request.caller {
        return invalid_talk_invoke_request(
            Some(&request.request_id),
            "caller must not have leading or trailing whitespace",
        );
    }
    if !is_local_capability_app_id(&request.caller) {
        return invalid_talk_invoke_request(Some(&request.request_id), "caller is invalid");
    }
    if request.capability.trim().is_empty() {
        return invalid_talk_invoke_request(Some(&request.request_id), "capability is required");
    }
    if request.capability.trim() != request.capability {
        return invalid_talk_invoke_request(
            Some(&request.request_id),
            "capability must not have leading or trailing whitespace",
        );
    }
    if !is_capability_id_shape(&request.capability) {
        return invalid_talk_invoke_request(
            Some(&request.request_id),
            "capability must be a dot-separated id",
        );
    }
    let Some(input) = request.input else {
        return invalid_talk_invoke_request(Some(&request.request_id), "input is required");
    };
    if !input.is_object() {
        return invalid_talk_invoke_request(Some(&request.request_id), "input must be an object");
    }

    match request.capability.as_str() {
        "voice.capture.once" | "voice.dictate" => {
            let mock_text = match optional_input_string_field(&input, "mockText") {
                Ok(mock_text) => mock_text,
                Err(message) => {
                    return invalid_talk_invoke_request(Some(&request.request_id), &message)
                }
            };
            let mode_override = match voice_mode_from_invoke_input(&input) {
                Ok(mode) => mode,
                Err(message) => {
                    return invalid_talk_invoke_request(Some(&request.request_id), &message)
                }
            };
            let mode_override = match request.capability.as_str() {
                "voice.dictate" => {
                    if mode_override.is_some_and(|mode| {
                        !matches!(mode, VoiceMode::Dictate | VoiceMode::Transcribe)
                    }) {
                        return invalid_talk_invoke_request(
                            Some(&request.request_id),
                            "input.mode is not supported for capability voice.dictate",
                        );
                    }
                    Some(VoiceMode::Transcribe)
                }
                _ => mode_override,
            };
            let context = match front_context_from_invoke_input(&input) {
                Ok(context) => context,
                Err(message) => {
                    return invalid_talk_invoke_request(Some(&request.request_id), &message)
                }
            };
            let report =
                run_voice_session(config, mock_text, mode_override, context, |_| {}).await?;
            if report.session.status() == SessionStatus::Completed {
                Ok((
                    200,
                    json!({
                        "requestId": request.request_id,
                        "status": "succeeded",
                        "output": {
                            "text": report.session.output_text().unwrap_or_default(),
                            "transcript": report.session.transcript().unwrap_or_default(),
                            "sessionId": report.session.id(),
                            "evidencePath": report.log_path,
                            "triggerEvents": report.trigger_events,
                            "caller": request.caller
                        }
                    })
                    .to_string(),
                ))
            } else {
                Ok((
                    200,
                    json!({
                        "requestId": request.request_id,
                        "status": "failed",
                        "error": {
                            "code": "voice_session_failed",
                            "message": report.session.error().unwrap_or("voice session failed")
                        },
                        "output": {
                            "sessionId": report.session.id(),
                            "evidencePath": report.log_path,
                            "triggerEvents": report.trigger_events,
                            "caller": request.caller
                        }
                    })
                    .to_string(),
                ))
            }
        }
        _ => Ok((
            200,
            json!({
                "requestId": request.request_id,
                "status": "failed",
                "error": {
                    "code": "unknown_capability",
                    "message": format!("Talk capability '{}' is not supported", request.capability)
                }
            })
            .to_string(),
        )),
    }
}

#[derive(Debug)]
struct TalkInvokeParseError {
    request_id: Option<String>,
    message: String,
}

impl TalkInvokeParseError {
    fn new(request_id: Option<String>, message: impl Into<String>) -> Self {
        Self {
            request_id,
            message: message.into(),
        }
    }
}

fn parse_talk_invoke_request(
    body: &str,
) -> std::result::Result<TalkInvokeRequest, TalkInvokeParseError> {
    let value = serde_json::from_str::<Value>(body)
        .map_err(|_| TalkInvokeParseError::new(None, "invalid Talk invoke request"))?;
    let object = value
        .as_object()
        .ok_or_else(|| TalkInvokeParseError::new(None, "invalid Talk invoke request"))?;
    let request_id = required_talk_invoke_string_field(object, "requestId")
        .map_err(|message| TalkInvokeParseError::new(None, message))?;
    let caller = required_talk_invoke_string_field(object, "caller")
        .map_err(|message| TalkInvokeParseError::new(Some(request_id.clone()), message))?;
    let capability = required_talk_invoke_string_field(object, "capability")
        .map_err(|message| TalkInvokeParseError::new(Some(request_id.clone()), message))?;
    let input = object.get("input").cloned();
    Ok(TalkInvokeRequest {
        request_id,
        caller,
        capability,
        input,
    })
}

fn required_talk_invoke_string_field(
    object: &serde_json::Map<String, Value>,
    field_name: &str,
) -> std::result::Result<String, String> {
    let Some(value) = object.get(field_name) else {
        return Err(format!("{field_name} is required"));
    };
    let Some(value) = value.as_str() else {
        return Err(format!("{field_name} must be a string"));
    };
    Ok(value.to_string())
}

fn voice_mode_from_invoke_input(input: &Value) -> std::result::Result<Option<VoiceMode>, String> {
    let Some(mode) = input.get("mode") else {
        return Ok(None);
    };
    let Some(mode) = mode.as_str() else {
        return Err("input.mode must be a string".to_string());
    };
    if mode.trim().is_empty() {
        return Err("input.mode must not be blank".to_string());
    }
    if mode.trim() != mode {
        return Err("input.mode must not have leading or trailing whitespace".to_string());
    }
    let mode = match mode {
        "transcribe" | "dictate" | "dictation" => VoiceMode::Transcribe,
        "document" => VoiceMode::Document,
        "polish" => VoiceMode::Polish,
        "generate" => VoiceMode::Generate,
        "smart" => VoiceMode::Smart,
        "translate" => VoiceMode::Translate,
        "command" => VoiceMode::Command,
        _ => return Err(format!("input.mode is not supported: {mode}")),
    };
    Ok(Some(mode))
}

fn optional_input_string_field(
    input: &Value,
    field_name: &str,
) -> std::result::Result<Option<String>, String> {
    let Some(value) = input.get(field_name) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(format!("input.{field_name} must be a string"));
    };
    if value.trim().is_empty() {
        return Err(format!("input.{field_name} must not be blank"));
    }
    if value.trim() != value {
        return Err(format!(
            "input.{field_name} must not have leading or trailing whitespace"
        ));
    }
    Ok(Some(value.to_string()))
}

fn front_context_from_invoke_input(input: &Value) -> std::result::Result<FrontContext, String> {
    let Some(context) = input.get("context") else {
        return Ok(FrontContext::default());
    };
    let context = serde_json::from_value::<FrontContext>(context.clone())
        .map_err(|_| "input.context is invalid".to_string())?;
    validate_optional_context_label(context.source.as_deref(), "source")?;
    validate_optional_context_label(context.app_name.as_deref(), "appName")?;
    validate_optional_context_label(context.window_title.as_deref(), "windowTitle")?;
    Ok(context)
}

fn validate_optional_context_label(
    value: Option<&str>,
    field_name: &str,
) -> std::result::Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Err(format!("input.context.{field_name} must not be blank"));
    }
    if value.trim() != value {
        return Err(format!(
            "input.context.{field_name} must not have leading or trailing whitespace"
        ));
    }
    Ok(())
}

fn is_local_capability_app_id(caller: &str) -> bool {
    matches!(
        caller,
        "hook" | "talk" | "loom" | "tea" | "gateway" | "platform"
    )
}

fn is_capability_id_shape(capability: &str) -> bool {
    capability.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    }) && capability.contains('.')
}

fn invalid_talk_invoke_request(request_id: Option<&str>, message: &str) -> Result<(u16, String)> {
    Ok((
        400,
        json!({
            "requestId": request_id,
            "status": "failed",
            "error": {
                "code": "invalid_request",
                "message": message
            }
        })
        .to_string(),
    ))
}

fn invalid_http_request(message: &str) -> (u16, String) {
    (
        400,
        json!({
            "requestId": null,
            "status": "failed",
            "error": {
                "code": "invalid_request",
                "message": message
            }
        })
        .to_string(),
    )
}

enum HttpReadOutcome {
    Empty,
    Request(String),
    Rejected { status: u16, body: String },
}

fn read_http_request(stream: &mut impl Read) -> Result<HttpReadOutcome> {
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) if request.is_empty() => return Ok(HttpReadOutcome::Empty),
            Ok(0) => break,
            Ok(bytes) => {
                request.extend_from_slice(&buffer[..bytes]);
                if request_exceeds_size_limit(&request) {
                    return Ok(payload_too_large_response());
                }
                if request_has_full_body(&request) {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) && request.is_empty() =>
            {
                return Ok(HttpReadOutcome::Empty);
            }
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                ) =>
            {
                break;
            }
            Err(error) => return Err(error).context("read Talk request"),
        }
    }
    if content_length_is_invalid(&request) {
        let (status, body) = invalid_http_request("invalid Content-Length header");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if request_uses_transfer_encoding(&request) {
        let (status, body) = invalid_http_request("transfer-encoding is not supported");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if request_headers_are_incomplete(&request) {
        let (status, body) = invalid_http_request("incomplete HTTP headers");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if request_has_malformed_header_line(&request) {
        let (status, body) = invalid_http_request("malformed HTTP header line");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if http_1_1_host_header_is_invalid(&request) {
        let (status, body) = invalid_http_request("HTTP/1.1 requests require a single Host header");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if request_body_is_truncated(&request) {
        let (status, body) = invalid_http_request("incomplete HTTP request body");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    if request_body_exceeds_declared_length(&request) {
        let (status, body) = invalid_http_request("HTTP request body exceeds declared length");
        return Ok(HttpReadOutcome::Rejected { status, body });
    }
    match String::from_utf8(request) {
        Ok(request) => Ok(HttpReadOutcome::Request(request)),
        Err(_) => {
            let (status, body) = invalid_http_request("HTTP request must be UTF-8");
            Ok(HttpReadOutcome::Rejected { status, body })
        }
    }
}

fn payload_too_large_response() -> HttpReadOutcome {
    HttpReadOutcome::Rejected {
        status: 413,
        body: json!({
            "requestId": null,
            "status": "failed",
            "error": {
                "code": "payload_too_large",
                "message": "request body is too large"
            }
        })
        .to_string(),
    }
}

fn request_exceeds_size_limit(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return request.len() > MAX_HTTP_HEADER_BYTES;
    };
    if header_end > MAX_HTTP_HEADER_BYTES {
        return true;
    }

    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    content_length > MAX_HTTP_BODY_BYTES
        || request.len().saturating_sub(body_start) > MAX_HTTP_BODY_BYTES
}

fn request_has_full_body(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    request.len().saturating_sub(body_start) >= content_length
}

fn request_headers_are_incomplete(request: &[u8]) -> bool {
    !request.is_empty() && !request.windows(4).any(|window| window == b"\r\n\r\n")
}

fn request_has_malformed_header_line(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    header_text.lines().skip(1).any(|line| {
        if line.trim().is_empty() {
            return false;
        }
        let Some((name, _)) = line.split_once(':') else {
            return true;
        };
        name.trim().is_empty() || name.trim() != name || !is_http_token(name)
    })
}

fn is_http_token(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn http_1_1_host_header_is_invalid(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines.next().unwrap_or("");
    let request_line_parts = request_line.split_whitespace().collect::<Vec<_>>();
    if !request_line_parts
        .get(2)
        .is_some_and(|version| version.eq_ignore_ascii_case("HTTP/1.1"))
    {
        return false;
    }

    let host_values = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("host").then_some(value.trim())
        })
        .collect::<Vec<_>>();

    if host_values.len() != 1 || host_values[0].is_empty() {
        return true;
    }

    !host_header_has_single_value(host_values[0])
}

fn host_header_has_single_value(value: &str) -> bool {
    let mut segments = value.split(',').map(str::trim);
    let Some(first) = segments.next() else {
        return false;
    };
    !first.is_empty() && !first.chars().any(char::is_whitespace) && segments.next().is_none()
}

fn request_body_is_truncated(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    request.len().saturating_sub(body_start) < content_length
}

fn request_body_exceeds_declared_length(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let content_length = content_length(&header_text);
    let body_start = header_end + 4;
    request.len().saturating_sub(body_start) > content_length
}

fn content_length(headers: &str) -> usize {
    headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0)
}

fn content_length_is_invalid(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    let mut seen = false;
    for line in header_text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        let value = value.trim();
        if seen
            || value.is_empty()
            || !value.bytes().all(|byte| byte.is_ascii_digit())
            || value.parse::<usize>().is_err()
        {
            return true;
        }
        seen = true;
    }
    false
}

fn request_uses_transfer_encoding(request: &[u8]) -> bool {
    let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_text = String::from_utf8_lossy(&request[..header_end]);
    header_text.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding") && !value.trim().is_empty()
    })
}

fn is_http_origin_form_request_target(target: &str) -> bool {
    target.starts_with('/') && !target.contains('#')
}

#[derive(Debug)]
struct ParsedHttpRequest {
    valid_request_line: bool,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl ParsedHttpRequest {
    fn from_raw(raw: &str) -> Self {
        let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((raw, ""));
        let mut lines = head.lines();
        let request_line = lines.next().unwrap_or("");
        let request_line_parts = request_line.split_whitespace().collect::<Vec<_>>();
        let valid_request_line = request_line_parts.len() == 3
            && is_http_token(request_line_parts[0])
            && is_http_origin_form_request_target(request_line_parts[1])
            && matches!(
                request_line_parts[2],
                version if version.eq_ignore_ascii_case("HTTP/1.0")
                    || version.eq_ignore_ascii_case("HTTP/1.1")
            );
        let headers = lines
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.trim().to_string(), value.trim().to_string()))
            })
            .collect();
        Self {
            valid_request_line,
            method: request_line_parts
                .first()
                .copied()
                .unwrap_or("")
                .to_string(),
            path: request_line_parts.get(1).copied().unwrap_or("").to_string(),
            headers,
            body: body.to_string(),
        }
    }

    fn has_bearer(&self, token: &str) -> bool {
        self.headers.iter().any(|(name, value)| {
            let mut parts = value.split_whitespace();
            name.eq_ignore_ascii_case("authorization")
                && parts
                    .next()
                    .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
                && parts.next() == Some(token)
                && parts.next().is_none()
        })
    }

    fn has_ambiguous_authorization_header(&self) -> bool {
        self.headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .any(|(_, value)| value.contains(','))
            || self
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                .nth(1)
                .is_some()
    }

    fn has_json_content_type(&self) -> bool {
        let mut content_types = self
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            .map(|(_, value)| value);

        let Some(first) = content_types.next() else {
            return false;
        };
        if content_types.next().is_some() {
            return false;
        }
        if first.contains(',') {
            return false;
        }

        let mime = first.split(';').next().unwrap_or("").trim();
        mime.eq_ignore_ascii_case("application/json")
    }

    fn has_single_content_length_header(&self) -> bool {
        let mut content_lengths = self
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"));
        content_lengths.next().is_some() && content_lengths.next().is_none()
    }
}

fn write_http_json_response(stream: &mut impl Write, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        413 => "Payload Too Large",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .context("write Talk response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use talk_client::FrontContext;
    use talk_core::{
        AudioBackendMode, AudioConfig, ClipboardBackendMode, DesktopConfig, LoggingConfig,
        OpenAiTranscriptionTransport, OutputConfig, OutputMode, ProviderConfig, ProviderKind,
        TalkConfig, TriggerConfig, TriggerMode, VoiceMode,
    };

    fn mock_config_without_transcript() -> TalkConfig {
        TalkConfig {
            trigger: TriggerConfig {
                mode: TriggerMode::Toggle,
                toggle_shortcut: "Ctrl+Alt+Space".to_string(),
            },
            desktop: DesktopConfig::default(),
            audio: AudioConfig {
                backend: AudioBackendMode::Silent,
                input_device: None,
                max_recording_seconds: 60,
                sample_rate_hz: 16_000,
                channels: 1,
                temp_dir: PathBuf::from(".runtime/talk/audio"),
            },
            provider: ProviderConfig {
                kind: ProviderKind::Mock,
                mock_transcript: None,
                endpoint: None,
                audio_transcriptions_endpoint: None,
                chat_completions_endpoint: None,
                transcription_transport: OpenAiTranscriptionTransport::AudioTranscriptions,
                transcription_model: None,
                chat_model: None,
                api_key: None,
                api_key_env: None,
            },
            output: OutputConfig {
                mode: OutputMode::DryRun,
                restore_clipboard: true,
                clipboard_backend: ClipboardBackendMode::Fallback,
            },
            logging: LoggingConfig {
                dir: PathBuf::from(".runtime/talk/logs"),
            },
            speculative: Default::default(),
            voice_mode: VoiceMode::Dictate,
        }
    }

    #[tokio::test]
    async fn mock_transcriber_requires_explicit_transcript_instead_of_hidden_fallback() {
        let config = mock_config_without_transcript();

        let report = run_voice_session(&config, None, None, FrontContext::default(), |_| {})
            .await
            .expect("runtime should return a failed session report");

        assert!(
            report
                .session
                .error()
                .unwrap_or_default()
                .to_string()
                .contains("provider.mock_transcript must be set for mock provider"),
            "report={:?}",
            report.session
        );
    }
}
