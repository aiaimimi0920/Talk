#[cfg(windows)]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(windows)]
use cpal::Sample;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub use talk_core::NativeReadinessStatus;
use talk_core::{AudioBackendMode, TalkError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioArtifact {
    pub path: PathBuf,
    pub mime_type: String,
}

impl AudioArtifact {
    pub fn new(path: PathBuf, mime_type: impl Into<String>) -> Self {
        Self {
            path,
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPlan {
    temp_dir: PathBuf,
    session_id: String,
}

impl AudioPlan {
    pub fn new(temp_dir: PathBuf, session_id: impl Into<String>) -> Self {
        Self {
            temp_dir,
            session_id: session_id.into(),
        }
    }

    pub fn artifact(&self) -> AudioArtifact {
        AudioArtifact::new(
            self.temp_dir.join(format!("{}.wav", self.session_id)),
            "audio/wav",
        )
    }

    pub fn ensure_parent_dir(&self) -> Result<(), TalkError> {
        std::fs::create_dir_all(&self.temp_dir).map_err(|error| TalkError::Audio(error.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSettings {
    pub sample_rate_hz: u32,
    pub channels: u16,
}

impl WavSettings {
    pub fn mono_16khz() -> Self {
        Self {
            sample_rate_hz: 16_000,
            channels: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub duration_samples: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeWindowsAudioReadiness {
    pub status: NativeReadinessStatus,
    pub reason: Option<String>,
    pub requested_device_name: Option<String>,
    pub device_name: Option<String>,
    pub available_device_names: Vec<String>,
    pub default_sample_rate_hz: Option<u32>,
    pub default_channels: Option<u16>,
    pub sample_format: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CapturedAudioBuffer {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioCaptureRequest {
    pub backend: AudioBackendMode,
    pub temp_dir: PathBuf,
    pub session_id: String,
    pub input_device: Option<String>,
    pub wav_settings: WavSettings,
    pub max_recording_seconds: u64,
    pub silent_samples: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPlaybackRequest {
    pub audio_path: PathBuf,
    pub output_device: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioSignalProbeRequest {
    pub backend: AudioBackendMode,
    pub temp_dir: PathBuf,
    pub session_id: String,
    pub input_device: Option<String>,
    pub wav_settings: WavSettings,
    pub capture_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSignalSummary {
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub duration_seconds: f64,
    pub peak: f32,
    pub rms: f32,
    pub silent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreparedWavSignalSummary {
    pub duration_seconds: f64,
    pub peak: f32,
    pub rms: f32,
    pub trimmed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSignalProbe {
    pub artifact: AudioArtifact,
    pub signal: AudioSignalSummary,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioInputLevel {
    pub peak: f32,
    pub rms: f32,
}

pub struct RecordingSession {
    backend: RecordingBackend,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordingPcmCursor {
    source_sample_offset: usize,
    next_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingPcmChunk {
    pub sequence: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bytes: Vec<u8>,
}

impl RecordingPcmCursor {
    fn next_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }
}

enum RecordingBackend {
    Silent {
        artifact: AudioArtifact,
        settings: WavSettings,
        samples: usize,
    },
    NativeWindows(NativeWindowsRecording),
}

#[cfg(windows)]
struct NativeWindowsRecording {
    artifact: AudioArtifact,
    wav_settings: WavSettings,
    sample_rate_hz: u32,
    channels: u16,
    samples: Arc<Mutex<Vec<f32>>>,
    stream_errors: Arc<Mutex<Vec<String>>>,
    stream: Option<cpal::Stream>,
}

#[cfg(not(windows))]
struct NativeWindowsRecording;

impl std::fmt::Debug for RecordingSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RecordingSession(..)")
    }
}

pub fn probe_native_windows_audio_readiness() -> NativeWindowsAudioReadiness {
    probe_native_windows_audio_readiness_for_device(None)
}

pub fn probe_native_windows_audio_readiness_for_device(
    requested_device_name: Option<&str>,
) -> NativeWindowsAudioReadiness {
    if std::env::var_os("TALK_DISABLE_NATIVE_AUDIO").is_some() {
        return NativeWindowsAudioReadiness::unavailable(
            "native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO",
            requested_device_name.map(str::to_string),
            Vec::new(),
        );
    }

    probe_native_windows_audio_readiness_impl(requested_device_name)
}

pub fn start_recording(request: &AudioCaptureRequest) -> Result<RecordingSession, TalkError> {
    let artifact = AudioPlan::new(request.temp_dir.clone(), request.session_id.clone()).artifact();
    let backend = match request.backend {
        AudioBackendMode::Silent => RecordingBackend::Silent {
            artifact,
            settings: request.wav_settings,
            samples: request.silent_samples,
        },
        AudioBackendMode::NativeWindows => {
            if std::env::var_os("TALK_DISABLE_NATIVE_AUDIO").is_some() {
                return Err(TalkError::Audio(
                    "native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO"
                        .to_string(),
                ));
            }
            RecordingBackend::NativeWindows(start_native_windows_recording(request, artifact)?)
        }
    };

    Ok(RecordingSession { backend })
}

pub fn capture_audio(request: &AudioCaptureRequest) -> Result<AudioArtifact, TalkError> {
    start_recording(request)?.finish()
}

pub fn play_wav(request: &AudioPlaybackRequest) -> Result<(), TalkError> {
    play_wav_impl(request)
}

pub fn probe_audio_signal(
    request: &AudioSignalProbeRequest,
) -> Result<AudioSignalProbe, TalkError> {
    validate_probe_capture_seconds(request.capture_seconds)?;

    let capture_request = AudioCaptureRequest {
        backend: request.backend,
        temp_dir: request.temp_dir.clone(),
        session_id: request.session_id.clone(),
        input_device: request.input_device.clone(),
        wav_settings: request.wav_settings,
        max_recording_seconds: request.capture_seconds,
        silent_samples: probe_silent_sample_count(request.wav_settings, request.capture_seconds)?,
    };
    let recording = start_recording(&capture_request)?;
    if matches!(request.backend, AudioBackendMode::NativeWindows) {
        std::thread::sleep(Duration::from_secs(request.capture_seconds));
    }
    recording.finish_probe()
}

impl NativeWindowsAudioReadiness {
    fn ready(
        requested_device_name: Option<String>,
        device_name: Option<String>,
        available_device_names: Vec<String>,
        default_sample_rate_hz: u32,
        default_channels: u16,
        sample_format: impl Into<String>,
    ) -> Self {
        Self {
            status: NativeReadinessStatus::Ready,
            reason: None,
            requested_device_name,
            device_name,
            available_device_names,
            default_sample_rate_hz: Some(default_sample_rate_hz),
            default_channels: Some(default_channels),
            sample_format: Some(sample_format.into()),
        }
    }

    fn unavailable(
        reason: impl Into<String>,
        requested_device_name: Option<String>,
        available_device_names: Vec<String>,
    ) -> Self {
        Self {
            status: NativeReadinessStatus::Unavailable,
            reason: Some(reason.into()),
            requested_device_name,
            device_name: None,
            available_device_names,
            default_sample_rate_hz: None,
            default_channels: None,
            sample_format: None,
        }
    }
}

fn select_native_windows_input_device_name(
    available_device_names: &[String],
    requested_device_name: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(requested_device_name) = requested_device_name else {
        return Ok(None);
    };
    if requested_device_name.trim().is_empty() {
        return Err("requested input device name must not be blank".to_string());
    }
    if requested_device_name.trim() != requested_device_name {
        return Err(
            "requested input device name must not have leading or trailing whitespace".to_string(),
        );
    }
    if available_device_names.is_empty() {
        return Err("no input devices are available".to_string());
    }

    let requested_folded = requested_device_name.to_lowercase();
    let exact_matches = available_device_names
        .iter()
        .filter(|available| available.to_lowercase() == requested_folded)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(first_exact_match) = exact_matches.first() {
        return Ok(Some(first_exact_match.clone()));
    }

    let substring_matches = available_device_names
        .iter()
        .filter(|available| available.to_lowercase().contains(&requested_folded))
        .cloned()
        .collect::<Vec<_>>();

    match substring_matches.as_slice() {
        [] => Err(format!(
            "requested input device '{requested_device_name}' did not match any input device; available: {}",
            available_device_names.join(", ")
        )),
        [matched] => Ok(Some(matched.clone())),
        matches => Err(format!(
            "requested input device '{requested_device_name}' matched multiple input devices: {}",
            matches.join(", ")
        )),
    }
}

fn select_native_windows_output_device_name(
    available_device_names: &[String],
    requested_device_name: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(requested_device_name) = requested_device_name else {
        return Ok(None);
    };
    if requested_device_name.trim().is_empty() {
        return Err("requested output device name must not be blank".to_string());
    }
    if requested_device_name.trim() != requested_device_name {
        return Err(
            "requested output device name must not have leading or trailing whitespace".to_string(),
        );
    }
    if available_device_names.is_empty() {
        return Err("no output devices are available".to_string());
    }

    let requested_folded = requested_device_name.to_lowercase();
    let exact_matches = available_device_names
        .iter()
        .filter(|available| available.to_lowercase() == requested_folded)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(first_exact_match) = exact_matches.first() {
        return Ok(Some(first_exact_match.clone()));
    }

    let substring_matches = available_device_names
        .iter()
        .filter(|available| available.to_lowercase().contains(&requested_folded))
        .cloned()
        .collect::<Vec<_>>();

    match substring_matches.as_slice() {
        [] => Err(format!(
            "requested output device '{requested_device_name}' did not match any output device; available: {}",
            available_device_names.join(", ")
        )),
        [matched] => Ok(Some(matched.clone())),
        matches => Err(format!(
            "requested output device '{requested_device_name}' matched multiple output devices: {}",
            matches.join(", ")
        )),
    }
}

impl RecordingSession {
    pub fn drain_pcm_chunk(
        &self,
        cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        match &self.backend {
            RecordingBackend::Silent {
                settings, samples, ..
            } => drain_silent_pcm_chunk(cursor, *settings, *samples),
            RecordingBackend::NativeWindows(recording) => recording.drain_pcm_chunk(cursor),
        }
    }

    pub fn current_level(&self) -> Result<AudioInputLevel, TalkError> {
        match &self.backend {
            RecordingBackend::Silent { .. } => Ok(AudioInputLevel {
                peak: 0.0,
                rms: 0.0,
            }),
            RecordingBackend::NativeWindows(recording) => recording.current_level(),
        }
    }

    pub fn current_waveform(&self, bucket_count: usize) -> Result<Vec<f32>, TalkError> {
        match &self.backend {
            RecordingBackend::Silent { .. } => Ok(vec![0.0; bucket_count]),
            RecordingBackend::NativeWindows(recording) => recording.current_waveform(bucket_count),
        }
    }

    pub fn finish(mut self) -> Result<AudioArtifact, TalkError> {
        match &mut self.backend {
            RecordingBackend::Silent {
                artifact,
                settings,
                samples,
            } => {
                write_silent_wav(artifact, *settings, *samples)?;
                Ok(artifact.clone())
            }
            RecordingBackend::NativeWindows(recording) => recording.finish(),
        }
    }

    pub fn cancel(mut self) -> Result<(), TalkError> {
        match &mut self.backend {
            RecordingBackend::Silent { .. } => Ok(()),
            RecordingBackend::NativeWindows(recording) => recording.cancel(),
        }
    }

    pub fn finish_probe(mut self) -> Result<AudioSignalProbe, TalkError> {
        match &mut self.backend {
            RecordingBackend::Silent {
                artifact,
                settings,
                samples,
            } => {
                write_silent_wav(artifact, *settings, *samples)?;
                Ok(AudioSignalProbe {
                    artifact: artifact.clone(),
                    signal: silent_audio_signal_summary(*settings, *samples)?,
                })
            }
            RecordingBackend::NativeWindows(recording) => recording.finish_probe(),
        }
    }
}

pub fn summarize_recent_audio_level(
    source: &CapturedAudioBuffer,
    trailing_frames: usize,
) -> Result<AudioInputLevel, TalkError> {
    summarize_recent_interleaved_audio_level(&source.samples, source.channels, trailing_frames)
}

pub fn summarize_recent_audio_waveform(
    source: &CapturedAudioBuffer,
    trailing_frames: usize,
    bucket_count: usize,
) -> Result<Vec<f32>, TalkError> {
    summarize_recent_interleaved_audio_waveform(
        &source.samples,
        source.channels,
        trailing_frames,
        bucket_count,
    )
}

pub fn write_silent_wav(
    artifact: &AudioArtifact,
    settings: WavSettings,
    samples: usize,
) -> Result<(), TalkError> {
    validate_wav_settings(settings)?;
    ensure_artifact_parent_dir(artifact)?;

    let spec = hound::WavSpec {
        channels: settings.channels,
        sample_rate: settings.sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&artifact.path, spec)
        .map_err(|error| TalkError::Audio(error.to_string()))?;
    for _ in 0..samples {
        writer
            .write_sample::<i16>(0)
            .map_err(|error| TalkError::Audio(error.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|error| TalkError::Audio(error.to_string()))
}

pub fn write_captured_wav(
    artifact: &AudioArtifact,
    source: &CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<(), TalkError> {
    let pcm_bytes = encode_captured_pcm_bytes(source, settings)?;

    ensure_artifact_parent_dir(artifact)?;

    let spec = hound::WavSpec {
        channels: settings.channels,
        sample_rate: settings.sample_rate_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&artifact.path, spec)
        .map_err(|error| TalkError::Audio(error.to_string()))?;

    for sample in pcm_bytes.chunks_exact(2) {
        writer
            .write_sample::<i16>(i16::from_le_bytes([sample[0], sample[1]]))
            .map_err(|error| TalkError::Audio(error.to_string()))?;
    }

    writer
        .finalize()
        .map_err(|error| TalkError::Audio(error.to_string()))
}

pub fn read_wav_info(artifact: &AudioArtifact) -> Result<WavInfo, TalkError> {
    let reader = hound::WavReader::open(&artifact.path)
        .map_err(|error| TalkError::Audio(error.to_string()))?;
    let spec = reader.spec();
    Ok(WavInfo {
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        bits_per_sample: spec.bits_per_sample,
        duration_samples: reader.duration(),
    })
}

pub fn summarize_prepared_wav_signal(
    path: &std::path::Path,
) -> Result<PreparedWavSignalSummary, TalkError> {
    let source = read_playback_wav_buffer(path)?;
    let Some((start_frame, end_frame, trimmed)) = prepared_frame_range(&source) else {
        return Ok(PreparedWavSignalSummary {
            duration_seconds: audio_signal_duration_seconds(
                source.sample_rate_hz,
                source.channels,
                source.samples.len(),
            )?,
            peak: 0.0,
            rms: 0.0,
            trimmed: false,
        });
    };
    let channels = usize::from(source.channels);
    let prepared_samples = &source.samples[start_frame * channels..(end_frame + 1) * channels];
    Ok(PreparedWavSignalSummary {
        duration_seconds: audio_signal_duration_seconds(
            source.sample_rate_hz,
            source.channels,
            prepared_samples.len(),
        )?,
        peak: captured_audio_peak_abs(prepared_samples),
        rms: captured_audio_rms(prepared_samples),
        trimmed,
    })
}

pub fn trim_wav_silence_bytes(path: &std::path::Path) -> Result<Option<Vec<u8>>, TalkError> {
    let source = read_playback_wav_buffer(path)?;
    let channels = usize::from(source.channels);
    if channels == 0 || source.samples.is_empty() {
        return Ok(None);
    }
    if source.samples.len() % channels != 0 {
        return Err(TalkError::Audio(
            "playback wav samples must be frame-aligned with channels".to_string(),
        ));
    }

    let Some((start_frame, end_frame, trimmed)) = prepared_frame_range(&source) else {
        return Ok(None);
    };
    if !trimmed {
        return Ok(None);
    }

    let trimmed = CapturedAudioBuffer {
        sample_rate_hz: source.sample_rate_hz,
        channels: source.channels,
        samples: source.samples[start_frame * channels..(end_frame + 1) * channels].to_vec(),
    };

    encode_pcm_wav_bytes(
        &trimmed,
        WavSettings {
            sample_rate_hz: source.sample_rate_hz,
            channels: source.channels,
        },
    )
    .map(Some)
}

fn read_playback_wav_buffer(path: &std::path::Path) -> Result<CapturedAudioBuffer, TalkError> {
    let mut reader =
        hound::WavReader::open(path).map_err(|error| TalkError::Audio(error.to_string()))?;
    let spec = reader.spec();
    if spec.channels == 0 {
        return Err(TalkError::Audio(
            "playback wav channels must be greater than 0".to_string(),
        ));
    }
    if spec.sample_rate == 0 {
        return Err(TalkError::Audio(
            "playback wav sample_rate_hz must be greater than 0".to_string(),
        ));
    }

    let samples = match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Int, 1..=16) => reader
            .samples::<i16>()
            .map(|sample| {
                sample
                    .map(|sample| f32::from(sample) / f32::from(i16::MAX))
                    .map_err(|error| TalkError::Audio(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?,
        (hound::SampleFormat::Int, 17..=32) => {
            let scale = ((1_i64 << (spec.bits_per_sample - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| {
                    sample
                        .map(|sample| sample as f32 / scale)
                        .map_err(|error| TalkError::Audio(error.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        (hound::SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .map(|sample| sample.map_err(|error| TalkError::Audio(error.to_string())))
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(TalkError::Audio(format!(
                "unsupported playback wav format: {:?} {}-bit",
                spec.sample_format, spec.bits_per_sample
            )))
        }
    };

    Ok(CapturedAudioBuffer {
        sample_rate_hz: spec.sample_rate,
        channels: spec.channels,
        samples,
    })
}

fn ensure_artifact_parent_dir(artifact: &AudioArtifact) -> Result<(), TalkError> {
    if let Some(parent) = artifact.path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| TalkError::Audio(error.to_string()))?;
    }
    Ok(())
}

fn validate_wav_settings(settings: WavSettings) -> Result<(), TalkError> {
    if settings.sample_rate_hz == 0 {
        return Err(TalkError::Audio(
            "wav sample_rate_hz must be greater than 0".to_string(),
        ));
    }
    if settings.channels == 0 {
        return Err(TalkError::Audio(
            "wav channels must be greater than 0".to_string(),
        ));
    }
    Ok(())
}

fn resampled_frame_count(
    source_frames: usize,
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
) -> Result<usize, TalkError> {
    if source_frames == 0 {
        return Ok(0);
    }

    let target_frames = (source_frames as u128 * u128::from(target_sample_rate_hz))
        / u128::from(source_sample_rate_hz);
    usize::try_from(target_frames.max(1)).map_err(|_| {
        TalkError::Audio("captured audio is too large to resample on this platform".to_string())
    })
}

fn source_frame_index_for_target(
    target_frame_index: usize,
    source_frames: usize,
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
) -> Result<usize, TalkError> {
    let source_frame_index = (target_frame_index as u128 * u128::from(source_sample_rate_hz))
        / u128::from(target_sample_rate_hz);
    let source_frame_index = usize::try_from(source_frame_index).map_err(|_| {
        TalkError::Audio("captured audio is too large to resample on this platform".to_string())
    })?;
    Ok(source_frame_index.min(source_frames.saturating_sub(1)))
}

fn downmix_source_frame_to_mono(source: &CapturedAudioBuffer, source_frame_index: usize) -> f32 {
    let source_channels = usize::from(source.channels);
    let frame_start = source_frame_index * source_channels;
    let frame_end = frame_start + source_channels;
    let sum = source.samples[frame_start..frame_end]
        .iter()
        .copied()
        .sum::<f32>();
    sum / f32::from(source.channels)
}

fn float_sample_to_i16(sample: f32) -> i16 {
    (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16
}

fn frame_peak_abs(samples: &[f32], channels: usize, frame_index: usize) -> f32 {
    let frame_start = frame_index * channels;
    let frame_end = frame_start + channels;
    samples[frame_start..frame_end]
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max)
}

fn prepared_frame_range(source: &CapturedAudioBuffer) -> Option<(usize, usize, bool)> {
    const SILENCE_THRESHOLD: f32 = 0.01;
    const PADDING_MILLISECONDS: u32 = 200;

    let channels = usize::from(source.channels);
    if channels == 0 || source.samples.is_empty() || source.samples.len() % channels != 0 {
        return None;
    }
    let frame_count = source.samples.len() / channels;
    let first_active = (0..frame_count).find(|frame_index| {
        frame_peak_abs(&source.samples, channels, *frame_index) >= SILENCE_THRESHOLD
    })?;
    let last_active = (0..frame_count).rev().find(|frame_index| {
        frame_peak_abs(&source.samples, channels, *frame_index) >= SILENCE_THRESHOLD
    })?;
    let padding_frames =
        ((u64::from(source.sample_rate_hz) * u64::from(PADDING_MILLISECONDS)) / 1000) as usize;
    let start_frame = first_active.saturating_sub(padding_frames);
    let end_frame = last_active
        .saturating_add(padding_frames)
        .min(frame_count.saturating_sub(1));
    let trimmed = start_frame != 0 || end_frame != frame_count.saturating_sub(1);
    Some((start_frame, end_frame, trimmed))
}

fn encode_pcm_wav_bytes(
    source: &CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<Vec<u8>, TalkError> {
    validate_wav_settings(settings)?;
    if source.channels == 0 {
        return Err(TalkError::Audio(
            "captured audio channels must be greater than 0".to_string(),
        ));
    }
    let source_channels = usize::from(source.channels);
    if source.samples.len() % source_channels != 0 {
        return Err(TalkError::Audio(
            "captured audio samples must be frame-aligned with channels".to_string(),
        ));
    }
    let data_size = u32::try_from(source.samples.len() * std::mem::size_of::<i16>())
        .map_err(|_| TalkError::Audio("captured audio is too large to encode".to_string()))?;
    let block_align = settings.channels.saturating_mul(2);
    let byte_rate = settings
        .sample_rate_hz
        .checked_mul(u32::from(block_align))
        .ok_or_else(|| TalkError::Audio("wav byte_rate overflow".to_string()))?;
    let riff_size = 36_u32
        .checked_add(data_size)
        .ok_or_else(|| TalkError::Audio("wav riff size overflow".to_string()))?;

    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&settings.channels.to_le_bytes());
    bytes.extend_from_slice(&settings.sample_rate_hz.to_le_bytes());
    bytes.extend_from_slice(&byte_rate.to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());
    for sample in &source.samples {
        bytes.extend_from_slice(&float_sample_to_i16(*sample).to_le_bytes());
    }
    Ok(bytes)
}

fn encode_captured_pcm_bytes(
    source: &CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<Vec<u8>, TalkError> {
    validate_wav_settings(settings)?;
    if source.sample_rate_hz == 0 {
        return Err(TalkError::Audio(
            "captured audio sample_rate_hz must be greater than 0".to_string(),
        ));
    }
    if source.channels == 0 {
        return Err(TalkError::Audio(
            "captured audio channels must be greater than 0".to_string(),
        ));
    }
    let source_channels = usize::from(source.channels);
    if source.samples.len() % source_channels != 0 {
        return Err(TalkError::Audio(
            "captured audio samples must be frame-aligned with channels".to_string(),
        ));
    }

    let source_frames = source.samples.len() / source_channels;
    let target_frames = resampled_frame_count(
        source_frames,
        source.sample_rate_hz,
        settings.sample_rate_hz,
    )?;
    let mut bytes = Vec::with_capacity(
        target_frames * usize::from(settings.channels) * std::mem::size_of::<i16>(),
    );

    for target_frame_index in 0..target_frames {
        let source_frame_index = source_frame_index_for_target(
            target_frame_index,
            source_frames,
            source.sample_rate_hz,
            settings.sample_rate_hz,
        )?;
        let mono_sample = downmix_source_frame_to_mono(source, source_frame_index);

        for _ in 0..settings.channels {
            bytes.extend_from_slice(&float_sample_to_i16(mono_sample).to_le_bytes());
        }
    }

    Ok(bytes)
}

fn drain_silent_pcm_chunk(
    cursor: &mut RecordingPcmCursor,
    settings: WavSettings,
    samples: usize,
) -> Result<Option<RecordingPcmChunk>, TalkError> {
    validate_wav_settings(settings)?;
    if cursor.source_sample_offset >= samples {
        return Ok(None);
    }
    let remaining_samples = samples - cursor.source_sample_offset;
    cursor.source_sample_offset = samples;
    let sequence = cursor.next_sequence();
    Ok(Some(RecordingPcmChunk {
        sequence,
        sample_rate_hz: settings.sample_rate_hz,
        channels: settings.channels,
        bytes: vec![0; remaining_samples * std::mem::size_of::<i16>()],
    }))
}

fn validate_probe_capture_seconds(capture_seconds: u64) -> Result<(), TalkError> {
    if capture_seconds == 0 {
        return Err(TalkError::Audio(
            "audio probe capture_seconds must be greater than 0".to_string(),
        ));
    }
    Ok(())
}

fn probe_silent_sample_count(
    settings: WavSettings,
    capture_seconds: u64,
) -> Result<usize, TalkError> {
    validate_probe_capture_seconds(capture_seconds)?;
    validate_wav_settings(settings)?;
    let samples = u128::from(capture_seconds)
        * u128::from(settings.sample_rate_hz)
        * u128::from(settings.channels);
    usize::try_from(samples)
        .map_err(|_| TalkError::Audio("audio probe silent sample buffer is too large".to_string()))
}

fn captured_audio_peak_abs(samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .map(f32::abs)
        .fold(0.0_f32, f32::max)
}

fn captured_audio_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares = samples.iter().map(|sample| sample * sample).sum::<f32>();
    (sum_squares / samples.len() as f32).sqrt()
}

fn summarize_recent_interleaved_audio_level(
    samples: &[f32],
    channels: u16,
    trailing_frames: usize,
) -> Result<AudioInputLevel, TalkError> {
    if channels == 0 {
        return Err(TalkError::Audio(
            "live audio level channels must be greater than 0".to_string(),
        ));
    }

    let channel_count = usize::from(channels);
    if samples.is_empty() || trailing_frames == 0 {
        return Ok(AudioInputLevel {
            peak: 0.0,
            rms: 0.0,
        });
    }
    if samples.len() % channel_count != 0 {
        return Err(TalkError::Audio(
            "live audio level samples must be frame-aligned with channels".to_string(),
        ));
    }

    let frame_count = samples.len() / channel_count;
    let recent_frame_count = trailing_frames.min(frame_count);
    let recent_start = (frame_count - recent_frame_count) * channel_count;
    let recent_samples = &samples[recent_start..];

    Ok(AudioInputLevel {
        peak: captured_audio_peak_abs(recent_samples),
        rms: captured_audio_rms(recent_samples),
    })
}

fn summarize_recent_interleaved_audio_waveform(
    samples: &[f32],
    channels: u16,
    trailing_frames: usize,
    bucket_count: usize,
) -> Result<Vec<f32>, TalkError> {
    if channels == 0 {
        return Err(TalkError::Audio(
            "live audio waveform channels must be greater than 0".to_string(),
        ));
    }
    if bucket_count == 0 {
        return Ok(Vec::new());
    }

    let channel_count = usize::from(channels);
    if samples.is_empty() || trailing_frames == 0 {
        return Ok(vec![0.0; bucket_count]);
    }
    if samples.len() % channel_count != 0 {
        return Err(TalkError::Audio(
            "live audio waveform samples must be frame-aligned with channels".to_string(),
        ));
    }

    let frame_count = samples.len() / channel_count;
    let recent_frame_count = trailing_frames.min(frame_count);
    let recent_start_frame = frame_count - recent_frame_count;
    let frames_per_bucket = recent_frame_count as f32 / bucket_count as f32;
    let mut waveform = Vec::with_capacity(bucket_count);

    for bucket_index in 0..bucket_count {
        let start_offset = (bucket_index as f32 * frames_per_bucket).floor() as usize;
        let mut end_offset = ((bucket_index + 1) as f32 * frames_per_bucket).floor() as usize;
        if end_offset <= start_offset {
            end_offset = (start_offset + 1).min(recent_frame_count);
        }
        let start_frame = (recent_start_frame + start_offset).min(frame_count.saturating_sub(1));
        let end_frame = (recent_start_frame + end_offset).min(frame_count);
        let peak = (start_frame..end_frame)
            .map(|frame_index| frame_peak_abs(samples, channel_count, frame_index))
            .fold(0.0_f32, f32::max);
        waveform.push(peak.clamp(0.0, 1.0));
    }

    Ok(waveform)
}

fn live_level_trailing_frames(sample_rate_hz: u32) -> usize {
    ((u64::from(sample_rate_hz) * 120) / 1000).max(1) as usize
}

fn live_waveform_trailing_frames(sample_rate_hz: u32) -> usize {
    ((u64::from(sample_rate_hz) * 180) / 1000).max(1) as usize
}

fn audio_signal_duration_seconds(
    sample_rate_hz: u32,
    channels: u16,
    sample_count: usize,
) -> Result<f64, TalkError> {
    if sample_rate_hz == 0 {
        return Err(TalkError::Audio(
            "audio signal sample_rate_hz must be greater than 0".to_string(),
        ));
    }
    if channels == 0 {
        return Err(TalkError::Audio(
            "audio signal channels must be greater than 0".to_string(),
        ));
    }
    if sample_count % usize::from(channels) != 0 {
        return Err(TalkError::Audio(
            "audio signal samples must be frame-aligned with channels".to_string(),
        ));
    }

    Ok((sample_count as f64 / f64::from(channels)) / f64::from(sample_rate_hz))
}

fn summarize_captured_audio(source: &CapturedAudioBuffer) -> Result<AudioSignalSummary, TalkError> {
    let duration_seconds = audio_signal_duration_seconds(
        source.sample_rate_hz,
        source.channels,
        source.samples.len(),
    )?;
    let peak = captured_audio_peak_abs(&source.samples);
    let rms = captured_audio_rms(&source.samples);
    Ok(AudioSignalSummary {
        sample_rate_hz: source.sample_rate_hz,
        channels: source.channels,
        duration_seconds,
        peak,
        rms,
        silent: peak <= f32::EPSILON,
    })
}

fn silent_audio_signal_summary(
    settings: WavSettings,
    sample_count: usize,
) -> Result<AudioSignalSummary, TalkError> {
    let duration_seconds =
        audio_signal_duration_seconds(settings.sample_rate_hz, settings.channels, sample_count)?;
    Ok(AudioSignalSummary {
        sample_rate_hz: settings.sample_rate_hz,
        channels: settings.channels,
        duration_seconds,
        peak: 0.0,
        rms: 0.0,
        silent: true,
    })
}

#[cfg(windows)]
impl NativeWindowsRecording {
    fn drain_pcm_chunk(
        &self,
        cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        let samples = self
            .samples
            .lock()
            .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
        let channel_count = usize::from(self.channels);
        if channel_count == 0 {
            return Err(native_windows_audio_error(
                "captured audio channels must be greater than 0",
            ));
        }
        let aligned_available = samples.len() - (samples.len() % channel_count);
        if cursor.source_sample_offset >= aligned_available {
            return Ok(None);
        }
        let start = cursor.source_sample_offset - (cursor.source_sample_offset % channel_count);
        let chunk_samples = samples[start..aligned_available].to_vec();
        cursor.source_sample_offset = aligned_available;
        drop(samples);

        let source = CapturedAudioBuffer {
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            samples: chunk_samples,
        };
        let bytes = encode_captured_pcm_bytes(&source, self.wav_settings).map_err(|error| {
            native_windows_audio_error(format!("failed to encode captured PCM chunk: {error}"))
        })?;
        if bytes.is_empty() {
            return Ok(None);
        }
        let sequence = cursor.next_sequence();
        Ok(Some(RecordingPcmChunk {
            sequence,
            sample_rate_hz: self.wav_settings.sample_rate_hz,
            channels: self.wav_settings.channels,
            bytes,
        }))
    }

    fn current_level(&self) -> Result<AudioInputLevel, TalkError> {
        let samples = self
            .samples
            .lock()
            .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
        summarize_recent_interleaved_audio_level(
            &samples,
            self.channels,
            live_level_trailing_frames(self.sample_rate_hz),
        )
    }

    fn current_waveform(&self, bucket_count: usize) -> Result<Vec<f32>, TalkError> {
        let samples = self
            .samples
            .lock()
            .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
        summarize_recent_interleaved_audio_waveform(
            &samples,
            self.channels,
            live_waveform_trailing_frames(self.sample_rate_hz),
            bucket_count,
        )
    }

    fn finish(&mut self) -> Result<AudioArtifact, TalkError> {
        let captured = self.finish_captured_audio(true)?;
        write_captured_wav(&self.artifact, &captured, self.wav_settings).map_err(|error| {
            native_windows_audio_error(format!("failed to write captured WAV: {error}"))
        })?;
        Ok(self.artifact.clone())
    }

    fn cancel(&mut self) -> Result<(), TalkError> {
        self.stream.take();
        Ok(())
    }

    fn finish_probe(&mut self) -> Result<AudioSignalProbe, TalkError> {
        let captured = self.finish_captured_audio(false)?;
        let signal = summarize_captured_audio(&captured).map_err(|error| {
            native_windows_audio_error(format!("failed to summarize captured audio: {error}"))
        })?;
        write_captured_wav(&self.artifact, &captured, self.wav_settings).map_err(|error| {
            native_windows_audio_error(format!("failed to write captured WAV: {error}"))
        })?;
        Ok(AudioSignalProbe {
            artifact: self.artifact.clone(),
            signal,
        })
    }

    fn finish_captured_audio(
        &mut self,
        reject_silence: bool,
    ) -> Result<CapturedAudioBuffer, TalkError> {
        self.stream.take();

        let stream_errors = self
            .stream_errors
            .lock()
            .map_err(|_| native_windows_audio_error("input stream error lock was poisoned"))?;
        if !stream_errors.is_empty() {
            return Err(native_windows_audio_error(format!(
                "input stream reported errors: {}",
                stream_errors.join("; ")
            )));
        }
        drop(stream_errors);

        let samples = self
            .samples
            .lock()
            .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?
            .clone();
        if samples.is_empty() {
            return Err(native_windows_audio_error(
                "input stream produced no samples; microphone capture is unavailable",
            ));
        }
        if reject_silence && captured_audio_peak_abs(&samples) <= f32::EPSILON {
            return Err(native_windows_audio_error(
                "input stream produced only silence; microphone capture is unavailable or muted",
            ));
        }

        Ok(CapturedAudioBuffer {
            sample_rate_hz: self.sample_rate_hz,
            channels: self.channels,
            samples,
        })
    }
}

#[cfg(not(windows))]
impl NativeWindowsRecording {
    fn drain_pcm_chunk(
        &self,
        _cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }

    fn current_level(&self) -> Result<AudioInputLevel, TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }

    fn current_waveform(&self, _bucket_count: usize) -> Result<Vec<f32>, TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }

    fn finish(&mut self) -> Result<AudioArtifact, TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }

    fn cancel(&mut self) -> Result<(), TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }
}

#[cfg(windows)]
fn probe_native_windows_audio_readiness_impl(
    requested_device_name: Option<&str>,
) -> NativeWindowsAudioReadiness {
    let host = cpal::default_host();
    let available_device_names =
        available_native_windows_input_device_names(&host).unwrap_or_else(|_| Vec::new());
    let (device, device_name) =
        match resolve_native_windows_input_device(&host, requested_device_name) {
            Ok(selection) => selection,
            Err(error) => {
                return NativeWindowsAudioReadiness::unavailable(
                    native_windows_audio_reason(error),
                    requested_device_name.map(str::to_string),
                    available_device_names,
                );
            }
        };
    let supported_config = match device.default_input_config() {
        Ok(config) => config,
        Err(error) => {
            let subject = native_windows_input_device_subject(device_name.as_deref());
            return NativeWindowsAudioReadiness::unavailable(
                native_windows_audio_reason(format!(
                    "failed to get input config for {subject}: {error}"
                )),
                requested_device_name.map(str::to_string),
                available_device_names,
            );
        }
    };
    let sample_format = supported_config.sample_format();
    if !native_windows_input_sample_format_supported(sample_format) {
        return NativeWindowsAudioReadiness::unavailable(
            native_windows_audio_reason(format!("unsupported input sample format {sample_format}")),
            requested_device_name.map(str::to_string),
            available_device_names,
        );
    }

    NativeWindowsAudioReadiness::ready(
        requested_device_name.map(str::to_string),
        device_name,
        available_device_names,
        supported_config.sample_rate(),
        supported_config.channels(),
        sample_format.to_string(),
    )
}

#[cfg(not(windows))]
fn probe_native_windows_audio_readiness_impl(
    requested_device_name: Option<&str>,
) -> NativeWindowsAudioReadiness {
    NativeWindowsAudioReadiness::unavailable(
        "native_windows audio backend is only available on Windows",
        requested_device_name.map(str::to_string),
        Vec::new(),
    )
}

#[cfg(windows)]
fn play_wav_impl(request: &AudioPlaybackRequest) -> Result<(), TalkError> {
    validate_playback_audio_path(&request.audio_path)?;
    let source = read_playback_wav_buffer(&request.audio_path)?;
    let host = cpal::default_host();
    let (device, device_name) =
        resolve_native_windows_output_device(&host, request.output_device.as_deref())
            .map_err(native_windows_audio_error)?;
    let supported_config = device.default_output_config().map_err(|error| {
        let subject = native_windows_output_device_subject(device_name.as_deref());
        native_windows_audio_error(format!(
            "failed to get output config for {subject}: {error}"
        ))
    })?;
    let sample_format = supported_config.sample_format();
    let config: cpal::StreamConfig = supported_config.into();
    let playback_samples =
        render_output_playback_samples(&source, config.sample_rate.into(), config.channels)?;

    let cursor = Arc::new(Mutex::new(0usize));
    let stream_errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let stream = build_native_output_stream(
        &device,
        &config,
        sample_format,
        playback_samples,
        Arc::clone(&cursor),
        Arc::clone(&stream_errors),
    )?;

    stream.play().map_err(|error| {
        native_windows_audio_error(format!("failed to start output stream: {error}"))
    })?;

    let playback_target = max_playback_cursor_target(&source, &config)?;
    while *cursor
        .lock()
        .map_err(|_| native_windows_audio_error("playback cursor lock was poisoned"))?
        < playback_target
    {
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(100));
    drop(stream);

    let stream_errors = stream_errors
        .lock()
        .map_err(|_| native_windows_audio_error("output stream error lock was poisoned"))?;
    if !stream_errors.is_empty() {
        return Err(native_windows_audio_error(format!(
            "output stream reported errors: {}",
            stream_errors.join("; ")
        )));
    }

    Ok(())
}

#[cfg(not(windows))]
fn play_wav_impl(_request: &AudioPlaybackRequest) -> Result<(), TalkError> {
    Err(native_windows_audio_error(
        "native_windows audio playback is only available on Windows",
    ))
}

#[cfg(windows)]
fn native_windows_recording_duration(request: &AudioCaptureRequest) -> Result<Duration, TalkError> {
    let requested_seconds = match std::env::var_os("TALK_NATIVE_AUDIO_SECONDS") {
        Some(raw) => {
            let raw = raw.to_string_lossy();
            let seconds = raw.trim().parse::<u64>().map_err(|error| {
                native_windows_audio_error(format!(
                    "TALK_NATIVE_AUDIO_SECONDS must be a positive integer: {error}"
                ))
            })?;
            if seconds == 0 {
                return Err(native_windows_audio_error(
                    "TALK_NATIVE_AUDIO_SECONDS must be greater than 0",
                ));
            }
            seconds.min(request.max_recording_seconds)
        }
        None => request.max_recording_seconds,
    };
    if requested_seconds == 0 {
        return Err(native_windows_audio_error(
            "max_recording_seconds must be greater than 0",
        ));
    }
    Ok(Duration::from_secs(requested_seconds))
}

#[cfg(windows)]
fn max_native_capture_samples(
    config: &cpal::StreamConfig,
    recording_duration: Duration,
) -> Result<usize, TalkError> {
    let frames =
        u128::from(u32::from(config.sample_rate)) * u128::from(recording_duration.as_secs());
    let samples = frames * u128::from(config.channels);
    usize::try_from(samples)
        .map_err(|_| native_windows_audio_error("requested native recording duration is too large"))
}

#[cfg(windows)]
fn start_native_windows_recording(
    request: &AudioCaptureRequest,
    artifact: AudioArtifact,
) -> Result<NativeWindowsRecording, TalkError> {
    let recording_duration = native_windows_recording_duration(request)?;
    let host = cpal::default_host();
    let (device, device_name) =
        resolve_native_windows_input_device(&host, request.input_device.as_deref())
            .map_err(native_windows_audio_error)?;
    let supported_config = device.default_input_config().map_err(|error| {
        let subject = native_windows_input_device_subject(device_name.as_deref());
        native_windows_audio_error(format!("failed to get input config for {subject}: {error}"))
    })?;
    let sample_format = supported_config.sample_format();
    let config: cpal::StreamConfig = supported_config.into();
    let max_samples = max_native_capture_samples(&config, recording_duration)?;

    let samples = Arc::new(Mutex::new(Vec::<f32>::with_capacity(
        max_samples.min(1_000_000),
    )));
    let stream_errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let stream = build_native_input_stream(
        &device,
        &config,
        sample_format,
        Arc::clone(&samples),
        max_samples,
        Arc::clone(&stream_errors),
    )?;

    stream.play().map_err(|error| {
        native_windows_audio_error(format!("failed to start input stream: {error}"))
    })?;

    Ok(NativeWindowsRecording {
        artifact,
        wav_settings: request.wav_settings,
        sample_rate_hz: config.sample_rate.into(),
        channels: config.channels,
        samples,
        stream_errors,
        stream: Some(stream),
    })
}

#[cfg(not(windows))]
fn start_native_windows_recording(
    _request: &AudioCaptureRequest,
    _artifact: AudioArtifact,
) -> Result<NativeWindowsRecording, TalkError> {
    Err(native_windows_audio_error(
        "native_windows audio backend is only available on Windows",
    ))
}

fn validate_playback_audio_path(path: &std::path::Path) -> Result<(), TalkError> {
    if path.as_os_str().is_empty() || path.as_os_str().to_string_lossy().trim().is_empty() {
        return Err(TalkError::Audio(
            "audio file path must not be empty".to_string(),
        ));
    }
    if !path.exists() {
        return Err(TalkError::Audio(format!(
            "audio file does not exist: {}",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(TalkError::Audio(format!(
            "audio file is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn render_output_playback_samples(
    source: &CapturedAudioBuffer,
    target_sample_rate_hz: u32,
    target_channels: u16,
) -> Result<Arc<Vec<f32>>, TalkError> {
    validate_wav_settings(WavSettings {
        sample_rate_hz: target_sample_rate_hz,
        channels: target_channels,
    })?;
    if source.sample_rate_hz == 0 {
        return Err(TalkError::Audio(
            "playback audio sample_rate_hz must be greater than 0".to_string(),
        ));
    }
    if source.channels == 0 {
        return Err(TalkError::Audio(
            "playback audio channels must be greater than 0".to_string(),
        ));
    }
    let source_channels = usize::from(source.channels);
    if source.samples.len() % source_channels != 0 {
        return Err(TalkError::Audio(
            "playback audio samples must be frame-aligned with channels".to_string(),
        ));
    }

    let source_frames = source.samples.len() / source_channels;
    let target_frames =
        resampled_frame_count(source_frames, source.sample_rate_hz, target_sample_rate_hz)?;
    let mut rendered = Vec::with_capacity(target_frames * usize::from(target_channels));
    for target_frame_index in 0..target_frames {
        let mono_sample = interpolated_source_frame_to_mono(
            source,
            target_frame_index,
            source_frames,
            source.sample_rate_hz,
            target_sample_rate_hz,
        )?;
        for _ in 0..target_channels {
            rendered.push(mono_sample);
        }
    }

    Ok(Arc::new(rendered))
}

fn interpolated_source_frame_to_mono(
    source: &CapturedAudioBuffer,
    target_frame_index: usize,
    source_frames: usize,
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
) -> Result<f32, TalkError> {
    if source_frames == 0 {
        return Ok(0.0);
    }

    let source_position = (target_frame_index as f64 * f64::from(source_sample_rate_hz))
        / f64::from(target_sample_rate_hz);
    let left_index = source_position.floor() as usize;
    let left_index = left_index.min(source_frames.saturating_sub(1));
    let right_index = left_index
        .saturating_add(1)
        .min(source_frames.saturating_sub(1));
    if left_index == right_index {
        return Ok(downmix_source_frame_to_mono(source, left_index));
    }

    let fraction = (source_position - left_index as f64) as f32;
    let left_sample = downmix_source_frame_to_mono(source, left_index);
    let right_sample = downmix_source_frame_to_mono(source, right_index);
    Ok(left_sample + ((right_sample - left_sample) * fraction))
}

#[cfg(windows)]
fn max_playback_cursor_target(
    source: &CapturedAudioBuffer,
    config: &cpal::StreamConfig,
) -> Result<usize, TalkError> {
    let source_frames = source.samples.len() / usize::from(source.channels);
    let target_frames = resampled_frame_count(
        source_frames,
        source.sample_rate_hz,
        config.sample_rate.into(),
    )?;
    target_frames
        .checked_mul(usize::from(config.channels))
        .ok_or_else(|| native_windows_audio_error("playback sample buffer is too large"))
}

#[cfg(windows)]
fn native_windows_input_sample_format_supported(sample_format: cpal::SampleFormat) -> bool {
    matches!(
        sample_format,
        cpal::SampleFormat::I8
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::I24
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::I64
            | cpal::SampleFormat::U8
            | cpal::SampleFormat::U16
            | cpal::SampleFormat::U24
            | cpal::SampleFormat::U32
            | cpal::SampleFormat::U64
            | cpal::SampleFormat::F32
            | cpal::SampleFormat::F64
    )
}

#[cfg(windows)]
fn native_windows_output_sample_format_supported(sample_format: cpal::SampleFormat) -> bool {
    matches!(
        sample_format,
        cpal::SampleFormat::I8
            | cpal::SampleFormat::I16
            | cpal::SampleFormat::I24
            | cpal::SampleFormat::I32
            | cpal::SampleFormat::I64
            | cpal::SampleFormat::U8
            | cpal::SampleFormat::U16
            | cpal::SampleFormat::U24
            | cpal::SampleFormat::U32
            | cpal::SampleFormat::U64
            | cpal::SampleFormat::F32
            | cpal::SampleFormat::F64
    )
}

#[cfg(windows)]
fn build_native_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    samples: Arc<Mutex<Vec<f32>>>,
    max_samples: usize,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError> {
    match sample_format {
        cpal::SampleFormat::I8 => build_native_input_stream_for_sample::<i8>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::I16 => build_native_input_stream_for_sample::<i16>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::I24 => build_native_input_stream_for_sample::<cpal::I24>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::I32 => build_native_input_stream_for_sample::<i32>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::I64 => build_native_input_stream_for_sample::<i64>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::U8 => build_native_input_stream_for_sample::<u8>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::U16 => build_native_input_stream_for_sample::<u16>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::U24 => build_native_input_stream_for_sample::<cpal::U24>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::U32 => build_native_input_stream_for_sample::<u32>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::U64 => build_native_input_stream_for_sample::<u64>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::F32 => build_native_input_stream_for_sample::<f32>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::F64 => build_native_input_stream_for_sample::<f64>(
            device,
            config,
            samples,
            max_samples,
            stream_errors,
        ),
        cpal::SampleFormat::DsdU8 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU32 => Err(
            native_windows_audio_error(format!("unsupported input sample format {sample_format}")),
        ),
        _ => Err(native_windows_audio_error(format!(
            "unsupported input sample format {sample_format}"
        ))),
    }
}

#[cfg(windows)]
fn build_native_input_stream_for_sample<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Mutex<Vec<f32>>>,
    max_samples: usize,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| append_native_input_samples(data, &samples, max_samples),
            move |error| {
                if let Ok(mut errors) = stream_errors.lock() {
                    errors.push(error.to_string());
                }
            },
            None,
        )
        .map_err(|error| {
            native_windows_audio_error(format!("failed to build input stream: {error}"))
        })
}

#[cfg(windows)]
fn append_native_input_samples<T>(input: &[T], samples: &Arc<Mutex<Vec<f32>>>, max_samples: usize)
where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let Ok(mut samples) = samples.try_lock() else {
        return;
    };
    let remaining = max_samples.saturating_sub(samples.len());
    if remaining == 0 {
        return;
    }
    samples.extend(
        input
            .iter()
            .take(remaining)
            .map(|sample| f32::from_sample(*sample)),
    );
}

#[cfg(windows)]
fn resolve_native_windows_input_device(
    host: &cpal::Host,
    requested_device_name: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    if requested_device_name.is_none() {
        let Some(device) = host.default_input_device() else {
            return Err("no default input device is available".to_string());
        };
        let device_name = describe_native_windows_input_device(&device);
        return Ok((device, device_name));
    }

    let devices = host
        .input_devices()
        .map_err(|error| format!("failed to enumerate input devices: {error}"))?;
    let mut named_devices = devices
        .enumerate()
        .map(|(index, device)| {
            (
                describe_native_windows_input_device(&device)
                    .unwrap_or_else(|| format!("unnamed input device {}", index + 1)),
                device,
            )
        })
        .collect::<Vec<_>>();
    let available_device_names = named_devices
        .iter()
        .map(|(device_name, _)| device_name.clone())
        .collect::<Vec<_>>();
    let selected_device_name =
        select_native_windows_input_device_name(&available_device_names, requested_device_name)?
            .expect("requested device name should resolve to Some");
    let selected_index = available_device_names
        .iter()
        .position(|device_name| device_name == &selected_device_name)
        .expect("selected device name must exist in enumerated input devices");
    let (_, device) = named_devices.swap_remove(selected_index);
    Ok((device, Some(selected_device_name)))
}

#[cfg(windows)]
fn available_native_windows_input_device_names(host: &cpal::Host) -> Result<Vec<String>, String> {
    host.input_devices()
        .map_err(|error| format!("failed to enumerate input devices: {error}"))?
        .enumerate()
        .map(|(index, device)| {
            Ok(describe_native_windows_input_device(&device)
                .unwrap_or_else(|| format!("unnamed input device {}", index + 1)))
        })
        .collect()
}

#[cfg(windows)]
fn resolve_native_windows_output_device(
    host: &cpal::Host,
    requested_device_name: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    if requested_device_name.is_none() {
        let Some(device) = host.default_output_device() else {
            return Err("no default output device is available".to_string());
        };
        let device_name = describe_native_windows_output_device(&device);
        return Ok((device, device_name));
    }

    let devices = host
        .output_devices()
        .map_err(|error| format!("failed to enumerate output devices: {error}"))?;
    let mut named_devices = devices
        .enumerate()
        .map(|(index, device)| {
            (
                describe_native_windows_output_device(&device)
                    .unwrap_or_else(|| format!("unnamed output device {}", index + 1)),
                device,
            )
        })
        .collect::<Vec<_>>();
    let available_device_names = named_devices
        .iter()
        .map(|(device_name, _)| device_name.clone())
        .collect::<Vec<_>>();
    let selected_device_name =
        select_native_windows_output_device_name(&available_device_names, requested_device_name)?
            .expect("requested output device name should resolve to Some");
    let selected_index = available_device_names
        .iter()
        .position(|device_name| device_name == &selected_device_name)
        .expect("selected output device name must exist in enumerated output devices");
    let (_, device) = named_devices.swap_remove(selected_index);
    Ok((device, Some(selected_device_name)))
}

#[cfg(windows)]
fn describe_native_windows_output_device(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

#[cfg(windows)]
fn build_native_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    samples: Arc<Vec<f32>>,
    cursor: Arc<Mutex<usize>>,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError> {
    if !native_windows_output_sample_format_supported(sample_format) {
        return Err(native_windows_audio_error(format!(
            "unsupported output sample format {sample_format}"
        )));
    }

    match sample_format {
        cpal::SampleFormat::I8 => build_native_output_stream_for_sample::<i8>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::I16 => build_native_output_stream_for_sample::<i16>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::I24 => build_native_output_stream_for_sample::<cpal::I24>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::I32 => build_native_output_stream_for_sample::<i32>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::I64 => build_native_output_stream_for_sample::<i64>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::U8 => build_native_output_stream_for_sample::<u8>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::U16 => build_native_output_stream_for_sample::<u16>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::U24 => build_native_output_stream_for_sample::<cpal::U24>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::U32 => build_native_output_stream_for_sample::<u32>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::U64 => build_native_output_stream_for_sample::<u64>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::F32 => build_native_output_stream_for_sample::<f32>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::F64 => build_native_output_stream_for_sample::<f64>(
            device,
            config,
            samples,
            cursor,
            stream_errors,
        ),
        cpal::SampleFormat::DsdU8 | cpal::SampleFormat::DsdU16 | cpal::SampleFormat::DsdU32 => Err(
            native_windows_audio_error(format!("unsupported output sample format {sample_format}")),
        ),
        _ => Err(native_windows_audio_error(format!(
            "unsupported output sample format {sample_format}"
        ))),
    }
}

#[cfg(windows)]
fn build_native_output_stream_for_sample<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Vec<f32>>,
    cursor: Arc<Mutex<usize>>,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError>
where
    T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static,
{
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| append_native_output_samples(data, &samples, &cursor),
            move |error| {
                if let Ok(mut errors) = stream_errors.lock() {
                    errors.push(error.to_string());
                }
            },
            None,
        )
        .map_err(|error| {
            native_windows_audio_error(format!("failed to build output stream: {error}"))
        })
}

#[cfg(windows)]
fn append_native_output_samples<T>(
    output: &mut [T],
    samples: &Arc<Vec<f32>>,
    cursor: &Arc<Mutex<usize>>,
) where
    T: cpal::Sample + cpal::FromSample<f32>,
{
    let Ok(mut cursor) = cursor.lock() else {
        return;
    };
    for sample in output.iter_mut() {
        let value = if *cursor < samples.len() {
            samples[*cursor]
        } else {
            0.0
        };
        *sample = T::from_sample(value);
        *cursor = cursor.saturating_add(1);
    }
}

#[cfg(windows)]
fn describe_native_windows_input_device(device: &cpal::Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_string())
}

fn native_windows_input_device_subject(device_name: Option<&str>) -> String {
    match device_name {
        Some(device_name) => format!("device '{device_name}'"),
        None => "default input device".to_string(),
    }
}

fn native_windows_output_device_subject(device_name: Option<&str>) -> String {
    match device_name {
        Some(device_name) => format!("device '{device_name}'"),
        None => "default output device".to_string(),
    }
}

fn native_windows_audio_reason(message: impl Into<String>) -> String {
    format!("native_windows audio backend: {}", message.into())
}

fn native_windows_audio_error(message: impl Into<String>) -> TalkError {
    TalkError::Audio(native_windows_audio_reason(message))
}

#[cfg(test)]
mod tests {
    use super::{
        captured_audio_peak_abs, render_output_playback_samples,
        select_native_windows_input_device_name, select_native_windows_output_device_name,
        CapturedAudioBuffer,
    };

    #[test]
    fn selects_requested_native_input_device_by_case_insensitive_exact_match() {
        let available = vec![
            "Virtual Mic".to_string(),
            "麦克风".to_string(),
            "Virtual Mic Backup".to_string(),
        ];

        let selected = select_native_windows_input_device_name(&available, Some("virtual mic"))
            .expect("exact match should succeed");

        assert_eq!(selected.as_deref(), Some("Virtual Mic"));
    }

    #[test]
    fn rejects_requested_native_input_device_when_substring_match_is_ambiguous() {
        let available = vec![
            "Virtual Mic One".to_string(),
            "Virtual Mic Two".to_string(),
            "麦克风".to_string(),
        ];

        let error = select_native_windows_input_device_name(&available, Some("virtual mic"))
            .expect_err("ambiguous substring match must fail");
        let message = error.to_string();

        assert!(
            message.contains("matched multiple input devices"),
            "error={error}"
        );
        assert!(message.contains("Virtual Mic One"), "error={error}");
        assert!(message.contains("Virtual Mic Two"), "error={error}");
    }

    #[test]
    fn rejects_requested_native_input_device_when_no_match_exists() {
        let available = vec!["Virtual Mic".to_string(), "麦克风".to_string()];

        let error = select_native_windows_input_device_name(&available, Some("Line In"))
            .expect_err("missing device match must fail");
        let message = error.to_string();

        assert!(
            message.contains("did not match any input device"),
            "error={error}"
        );
        assert!(message.contains("Virtual Mic"), "error={error}");
        assert!(message.contains("麦克风"), "error={error}");
    }

    #[test]
    fn selects_requested_native_output_device_by_case_insensitive_exact_match() {
        let available = vec![
            "Virtual Speakers".to_string(),
            "扬声器".to_string(),
            "Virtual Speakers Backup".to_string(),
        ];

        let selected =
            select_native_windows_output_device_name(&available, Some("virtual speakers"))
                .expect("exact output-device match should succeed");

        assert_eq!(selected.as_deref(), Some("Virtual Speakers"));
    }

    #[test]
    fn rejects_requested_native_output_device_when_substring_match_is_ambiguous() {
        let available = vec![
            "Virtual Speakers One".to_string(),
            "Virtual Speakers Two".to_string(),
            "扬声器".to_string(),
        ];

        let error = select_native_windows_output_device_name(&available, Some("virtual speakers"))
            .expect_err("ambiguous output-device substring match must fail");
        let message = error.to_string();

        assert!(
            message.contains("matched multiple output devices"),
            "error={error}"
        );
        assert!(message.contains("Virtual Speakers One"), "error={error}");
        assert!(message.contains("Virtual Speakers Two"), "error={error}");
    }

    #[test]
    fn rejects_requested_native_output_device_when_no_match_exists() {
        let available = vec!["Virtual Speakers".to_string(), "扬声器".to_string()];

        let error = select_native_windows_output_device_name(&available, Some("HDMI Out"))
            .expect_err("missing output-device match must fail");
        let message = error.to_string();

        assert!(
            message.contains("did not match any output device"),
            "error={error}"
        );
        assert!(message.contains("Virtual Speakers"), "error={error}");
        assert!(message.contains("扬声器"), "error={error}");
    }

    #[test]
    fn render_output_playback_samples_linearly_interpolates_upsampled_audio() {
        let source = CapturedAudioBuffer {
            sample_rate_hz: 2,
            channels: 1,
            samples: vec![0.0, 1.0],
        };

        let rendered = render_output_playback_samples(&source, 4, 1)
            .expect("render playback samples for upsampled output");

        assert_eq!(rendered.len(), 4);
        assert!((rendered[0] - 0.0).abs() < 0.0001, "rendered={rendered:?}");
        assert!((rendered[1] - 0.5).abs() < 0.0001, "rendered={rendered:?}");
        assert!((rendered[2] - 1.0).abs() < 0.0001, "rendered={rendered:?}");
        assert!((rendered[3] - 1.0).abs() < 0.0001, "rendered={rendered:?}");
    }

    #[test]
    fn captured_audio_peak_abs_is_zero_for_silence() {
        let peak = captured_audio_peak_abs(&[0.0, 0.0, 0.0]);

        assert_eq!(peak, 0.0);
    }

    #[test]
    fn captured_audio_peak_abs_detects_nonzero_signal() {
        let peak = captured_audio_peak_abs(&[0.0, -0.25, 0.5, -0.1]);

        assert_eq!(peak, 0.5);
    }
}
