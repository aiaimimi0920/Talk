#[cfg(windows)]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
#[cfg(windows)]
use cpal::Sample;
#[cfg(windows)]
use crossbeam_queue::ArrayQueue;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
pub use talk_core::NativeReadinessStatus;
use talk_core::{AudioBackendMode, TalkError};

mod streaming_converter;
#[cfg(windows)]
use streaming_converter::StreamingCaptureConverter;

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

#[derive(Clone)]
pub struct RecordingPcmSource {
    backend: RecordingPcmSourceBackend,
}

#[derive(Clone)]
enum RecordingPcmSourceBackend {
    Silent {
        settings: WavSettings,
        samples: usize,
    },
    #[cfg(windows)]
    NativeWindows {
        wav_settings: WavSettings,
        sample_rate_hz: u32,
        channels: u16,
        samples: Arc<Mutex<VecDeque<f32>>>,
        archived_samples: Arc<Mutex<Vec<f32>>>,
    },
}

#[derive(Default)]
pub struct RecordingPcmCursor {
    source_sample_offset: usize,
    next_sequence: u64,
    sample_scratch: Vec<f32>,
    resampled_scratch: Vec<f32>,
    channel_energy_scratch: Vec<f64>,
    pcm_bytes_scratch: Vec<u8>,
}

impl std::fmt::Debug for RecordingPcmCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecordingPcmCursor")
            .field("source_sample_offset", &self.source_sample_offset)
            .field("next_sequence", &self.next_sequence)
            .finish_non_exhaustive()
    }
}

impl Clone for RecordingPcmCursor {
    fn clone(&self) -> Self {
        Self {
            source_sample_offset: self.source_sample_offset,
            next_sequence: self.next_sequence,
            sample_scratch: Vec::new(),
            resampled_scratch: Vec::new(),
            channel_energy_scratch: Vec::new(),
            pcm_bytes_scratch: Vec::new(),
        }
    }
}

impl PartialEq for RecordingPcmCursor {
    fn eq(&self, other: &Self) -> bool {
        self.source_sample_offset == other.source_sample_offset
            && self.next_sequence == other.next_sequence
    }
}

impl Eq for RecordingPcmCursor {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingPcmChunk {
    pub sequence: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingPcmCursorCheckpoint {
    source_sample_offset: usize,
    next_sequence: u64,
}

impl RecordingPcmCursor {
    fn next_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }

    pub fn checkpoint(&self) -> RecordingPcmCursorCheckpoint {
        RecordingPcmCursorCheckpoint {
            source_sample_offset: self.source_sample_offset,
            next_sequence: self.next_sequence,
        }
    }

    pub fn restore_checkpoint(&mut self, checkpoint: RecordingPcmCursorCheckpoint) {
        self.source_sample_offset = checkpoint.source_sample_offset;
        self.next_sequence = checkpoint.next_sequence;
    }

    pub fn recycle_pcm_chunk(&mut self, mut chunk: RecordingPcmChunk) {
        chunk.bytes.clear();
        if chunk.bytes.capacity() >= self.pcm_bytes_scratch.capacity() {
            self.pcm_bytes_scratch = chunk.bytes;
        }
    }
}

#[cfg(test)]
fn discard_captured_samples_before_cursor(
    samples: &mut VecDeque<f32>,
    cursor: &mut RecordingPcmCursor,
) {
    let consumed_samples = cursor.source_sample_offset.min(samples.len());
    if consumed_samples == 0 {
        return;
    }
    for _ in 0..consumed_samples {
        samples.pop_front();
    }
    cursor.source_sample_offset -= consumed_samples;
}

fn archive_consumed_samples_before_cursor(
    samples: &mut VecDeque<f32>,
    archived_samples: &mut Vec<f32>,
    cursor: &mut RecordingPcmCursor,
) {
    let consumed_samples = cursor.source_sample_offset.min(samples.len());
    if consumed_samples == 0 {
        return;
    }
    archived_samples.reserve(consumed_samples);
    archived_samples.extend(samples.drain(..consumed_samples));
    cursor.source_sample_offset -= consumed_samples;
}

fn take_complete_captured_samples(
    archived_samples: &mut Vec<f32>,
    live_samples: &mut VecDeque<f32>,
) -> Vec<f32> {
    archived_samples.reserve(live_samples.len());
    archived_samples.extend(live_samples.drain(..));
    std::mem::take(archived_samples)
}

fn native_input_sample_count_to_append(
    input_sample_count: usize,
    total_captured_samples: usize,
    max_samples: usize,
) -> usize {
    input_sample_count.min(max_samples.saturating_sub(total_captured_samples))
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
    samples: Arc<Mutex<VecDeque<f32>>>,
    archived_samples: Arc<Mutex<Vec<f32>>>,
    capture_collector_stop: Arc<AtomicBool>,
    capture_collector: Option<std::thread::JoinHandle<Result<(), String>>>,
    overflowed_samples: Arc<AtomicUsize>,
    stream_errors: Arc<Mutex<Vec<String>>>,
    stream: Option<cpal::Stream>,
}

#[cfg(windows)]
#[derive(Clone)]
struct NativeInputCaptureLimit {
    max_samples: usize,
    total_captured_samples: Arc<AtomicUsize>,
}

#[cfg(not(windows))]
struct NativeWindowsRecording;

impl std::fmt::Debug for RecordingSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RecordingSession(..)")
    }
}

impl std::fmt::Debug for RecordingPcmSource {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RecordingPcmSource(..)")
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWindowsDeviceDirection {
    Input,
    Output,
}

impl NativeWindowsDeviceDirection {
    fn as_str(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }
}

#[cfg(test)]
fn select_native_windows_input_device_name(
    available_device_names: &[String],
    requested_device_name: Option<&str>,
) -> Result<Option<String>, String> {
    select_native_windows_device_name(
        NativeWindowsDeviceDirection::Input,
        available_device_names,
        requested_device_name,
    )
}

#[cfg(test)]
fn select_native_windows_output_device_name(
    available_device_names: &[String],
    requested_device_name: Option<&str>,
) -> Result<Option<String>, String> {
    select_native_windows_device_name(
        NativeWindowsDeviceDirection::Output,
        available_device_names,
        requested_device_name,
    )
}

fn select_native_windows_device_name(
    direction: NativeWindowsDeviceDirection,
    available_device_names: &[String],
    requested_device_name: Option<&str>,
) -> Result<Option<String>, String> {
    let direction = direction.as_str();
    let Some(requested_device_name) = requested_device_name else {
        return Ok(None);
    };
    if requested_device_name.trim().is_empty() {
        return Err(format!(
            "requested {direction} device name must not be blank"
        ));
    }
    if requested_device_name.trim() != requested_device_name {
        return Err(format!(
            "requested {direction} device name must not have leading or trailing whitespace"
        ));
    }
    if available_device_names.is_empty() {
        return Err(format!("no {direction} devices are available"));
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
            "requested {direction} device '{requested_device_name}' did not match any {direction} device; available: {}",
            available_device_names.join(", ")
        )),
        [matched] => Ok(Some(matched.clone())),
        matches => Err(format!(
            "requested {direction} device '{requested_device_name}' matched multiple {direction} devices: {}",
            matches.join(", ")
        )),
    }
}

impl RecordingSession {
    pub fn streaming_pcm_source(&self) -> Result<RecordingPcmSource, TalkError> {
        match &self.backend {
            RecordingBackend::Silent {
                settings, samples, ..
            } => Ok(RecordingPcmSource {
                backend: RecordingPcmSourceBackend::Silent {
                    settings: *settings,
                    samples: *samples,
                },
            }),
            #[cfg(windows)]
            RecordingBackend::NativeWindows(recording) => Ok(RecordingPcmSource {
                backend: RecordingPcmSourceBackend::NativeWindows {
                    wav_settings: recording.wav_settings,
                    sample_rate_hz: recording.sample_rate_hz,
                    channels: recording.channels,
                    samples: Arc::clone(&recording.samples),
                    archived_samples: Arc::clone(&recording.archived_samples),
                },
            }),
            #[cfg(not(windows))]
            RecordingBackend::NativeWindows(_) => Err(native_windows_audio_error(
                "native_windows audio backend is only available on Windows",
            )),
        }
    }

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

    /// Releases PCM that a streaming consumer has already sent from the live queue while retaining
    /// it in the final recording archive.
    pub fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<(), TalkError> {
        match &self.backend {
            RecordingBackend::Silent { .. } => Ok(()),
            RecordingBackend::NativeWindows(recording) => recording.discard_consumed_pcm(cursor),
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
        let mut waveform = vec![0.0; bucket_count];
        self.current_waveform_into(&mut waveform)?;
        Ok(waveform)
    }

    pub fn current_waveform_into(&self, waveform: &mut [f32]) -> Result<(), TalkError> {
        match &self.backend {
            RecordingBackend::Silent { .. } => {
                waveform.fill(0.0);
                Ok(())
            }
            RecordingBackend::NativeWindows(recording) => recording.current_waveform_into(waveform),
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

#[cfg(windows)]
fn try_lock_capture_samples(
    samples: &Mutex<VecDeque<f32>>,
) -> Result<Option<std::sync::MutexGuard<'_, VecDeque<f32>>>, TalkError> {
    match samples.try_lock() {
        Ok(guard) => Ok(Some(guard)),
        Err(std::sync::TryLockError::WouldBlock) => Ok(None),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Ok(Some(poisoned.into_inner())),
    }
}

impl RecordingPcmSource {
    pub fn drain_pcm_chunk(
        &self,
        cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        match &self.backend {
            RecordingPcmSourceBackend::Silent { settings, samples } => {
                drain_silent_pcm_chunk(cursor, *settings, *samples)
            }
            #[cfg(windows)]
            RecordingPcmSourceBackend::NativeWindows {
                wav_settings,
                sample_rate_hz,
                channels,
                samples,
                ..
            } => drain_native_windows_pcm_chunk(
                samples,
                *wav_settings,
                *sample_rate_hz,
                *channels,
                cursor,
            ),
        }
    }

    pub fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<(), TalkError> {
        match &self.backend {
            RecordingPcmSourceBackend::Silent { .. } => Ok(()),
            #[cfg(windows)]
            RecordingPcmSourceBackend::NativeWindows {
                samples,
                archived_samples,
                ..
            } => discard_native_windows_pcm(samples, archived_samples, cursor),
        }
    }

    pub fn current_waveform_into(&self, waveform: &mut [f32]) -> Result<(), TalkError> {
        match &self.backend {
            RecordingPcmSourceBackend::Silent { .. } => {
                waveform.fill(0.0);
                Ok(())
            }
            #[cfg(windows)]
            RecordingPcmSourceBackend::NativeWindows {
                sample_rate_hz,
                channels,
                samples,
                ..
            } => {
                let Some(samples) = try_lock_capture_samples(samples)? else {
                    return Ok(());
                };
                summarize_recent_interleaved_audio_waveform_queue_into(
                    &samples,
                    *channels,
                    live_waveform_trailing_frames(*sample_rate_hz),
                    waveform,
                )
            }
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
    mut source: CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<(), TalkError> {
    // Lift quiet-but-valid captures toward a consistent speech level before
    // encoding. Gain is bounded and skipped for weak captures so the provider
    // weak-signal reject still fires (see `normalized_capture_gain`). Streaming
    // chunks intentionally skip this: per-chunk gain would pump between chunks.
    // Encode already downmixes in place, so take ownership instead of cloning
    // the full PCM buffer on the dictation finalize path.
    let normalization_gain = normalized_capture_gain(captured_audio_peak_abs(&source.samples));
    if normalization_gain != 1.0 {
        for sample in source.samples.iter_mut() {
            *sample = (*sample * normalization_gain).clamp(-1.0, 1.0);
        }
    }

    let mut resampled_scratch = Vec::new();
    let mut channel_energy_scratch = Vec::new();
    let mut pcm_bytes = Vec::new();
    encode_captured_pcm_bytes_reusing_scratch_into(
        &mut source,
        settings,
        &mut resampled_scratch,
        &mut channel_energy_scratch,
        &mut pcm_bytes,
    )?;
    let wav_bytes = wav_bytes_from_encoded_pcm(&pcm_bytes, settings)?;

    ensure_artifact_parent_dir(artifact)?;
    std::fs::write(&artifact.path, wav_bytes).map_err(|error| TalkError::Audio(error.to_string()))
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
    let frame_range = prepared_frame_range(&source);
    summarize_prepared_wav_buffer(&source, frame_range)
}

pub fn trim_wav_silence_bytes(path: &std::path::Path) -> Result<Option<Vec<u8>>, TalkError> {
    let source = read_playback_wav_buffer(path)?;
    let frame_range = prepared_frame_range(&source);
    trim_wav_silence_buffer(&source, frame_range)
}

/// Prepared-upload signal summary plus trimmed WAV bytes produced from a
/// single decode and a single `prepared_frame_range` pass over in-memory WAV
/// bytes. `trimmed_wav_bytes` is `None` when trimming is unnecessary or fails,
/// mirroring how callers of `trim_wav_silence_bytes` fall back to the
/// original bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedWavUpload {
    pub summary: PreparedWavSignalSummary,
    pub trimmed_wav_bytes: Option<Vec<u8>>,
}

pub fn summarize_and_trim_prepared_wav_bytes(
    wav_bytes: &[u8],
) -> Result<PreparedWavUpload, TalkError> {
    let source = read_playback_wav_buffer_from_bytes(wav_bytes)?;
    let frame_range = prepared_frame_range(&source);
    Ok(PreparedWavUpload {
        summary: summarize_prepared_wav_buffer(&source, frame_range)?,
        trimmed_wav_bytes: trim_wav_silence_buffer(&source, frame_range).unwrap_or(None),
    })
}

fn summarize_prepared_wav_buffer(
    source: &CapturedAudioBuffer,
    frame_range: Option<(usize, usize, bool)>,
) -> Result<PreparedWavSignalSummary, TalkError> {
    let Some((start_frame, end_frame, trimmed)) = frame_range else {
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

fn trim_wav_silence_buffer(
    source: &CapturedAudioBuffer,
    frame_range: Option<(usize, usize, bool)>,
) -> Result<Option<Vec<u8>>, TalkError> {
    let channels = usize::from(source.channels);
    if channels == 0 || source.samples.is_empty() {
        return Ok(None);
    }
    if !source.samples.len().is_multiple_of(channels) {
        return Err(TalkError::Audio(
            "playback wav samples must be frame-aligned with channels".to_string(),
        ));
    }

    let Some((start_frame, end_frame, trimmed)) = frame_range else {
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
    let reader =
        hound::WavReader::open(path).map_err(|error| TalkError::Audio(error.to_string()))?;
    decode_playback_wav_buffer(reader)
}

fn read_playback_wav_buffer_from_bytes(wav_bytes: &[u8]) -> Result<CapturedAudioBuffer, TalkError> {
    let reader = hound::WavReader::new(std::io::Cursor::new(wav_bytes))
        .map_err(|error| TalkError::Audio(error.to_string()))?;
    decode_playback_wav_buffer(reader)
}

fn decode_playback_wav_buffer<R>(
    mut reader: hound::WavReader<R>,
) -> Result<CapturedAudioBuffer, TalkError>
where
    R: std::io::Read,
{
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

/// Resample a mono f32 signal to exactly `target_frames` samples using
/// band-limited interpolation.
///
/// Downsampling applies a Hann-weighted moving average whose width tracks the
/// decimation ratio, which attenuates energy above the target Nyquist
/// frequency. Nearest-neighbour picking (the previous behaviour) instead folds
/// that high-frequency energy back into the speech band as aliasing, degrading
/// ASR features on any capture whose device rate is not already the target
/// rate. Upsampling uses linear interpolation (no aliasing on upsample), and an
/// unchanged sample rate is a bit-identical passthrough.
#[cfg(test)]
fn resample_mono_to_len(
    source_mono: &[f32],
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
    target_frames: usize,
) -> Vec<f32> {
    let mut resampled = Vec::with_capacity(target_frames);
    resample_mono_to_len_into(
        source_mono,
        source_sample_rate_hz,
        target_sample_rate_hz,
        target_frames,
        &mut resampled,
    );
    resampled
}

fn resample_mono_to_len_into(
    source_mono: &[f32],
    source_sample_rate_hz: u32,
    target_sample_rate_hz: u32,
    target_frames: usize,
    resampled: &mut Vec<f32>,
) {
    resampled.clear();
    if source_mono.is_empty() || target_frames == 0 {
        return;
    }
    resampled.reserve(target_frames);
    if source_sample_rate_hz == target_sample_rate_hz && source_mono.len() == target_frames {
        resampled.extend_from_slice(source_mono);
        return;
    }

    let last_index = source_mono.len() - 1;
    let step = f64::from(source_sample_rate_hz) / f64::from(target_sample_rate_hz);

    if target_sample_rate_hz >= source_sample_rate_hz {
        // Upsample / same-rate: linear interpolation between adjacent samples.
        for target_index in 0..target_frames {
            let source_position = target_index as f64 * step;
            let lower = source_position.floor() as usize;
            if lower >= last_index {
                resampled.push(source_mono[last_index]);
                continue;
            }
            let frac = (source_position - lower as f64) as f32;
            resampled.push(source_mono[lower] * (1.0 - frac) + source_mono[lower + 1] * frac);
        }
        return;
    }

    // Downsample: Hann-weighted moving average around each fractional source
    // position. The window radius scales with the decimation ratio so the
    // effective low-pass cutoff sits near the target Nyquist frequency.
    let radius = step.ceil().max(1.0);
    let window = radius + 1.0;
    for target_index in 0..target_frames {
        let center = target_index as f64 * step;
        let first = (center - radius).floor() as isize;
        let last = (center + radius).ceil() as isize;
        let mut weighted_sum = 0.0_f64;
        let mut weight_total = 0.0_f64;
        for tap in first..=last {
            let distance = (tap as f64 - center).abs();
            if distance >= window {
                continue;
            }
            let weight = 0.5 * (1.0 + (std::f64::consts::PI * distance / window).cos());
            let index = tap.clamp(0, last_index as isize) as usize;
            weighted_sum += f64::from(source_mono[index]) * weight;
            weight_total += weight;
        }
        let sample = if weight_total > 0.0 {
            (weighted_sum / weight_total) as f32
        } else {
            source_mono[center.round().clamp(0.0, last_index as f64) as usize]
        };
        resampled.push(sample);
    }
}

/// Peak below which capture gain is left untouched. This sits at the provider
/// weak-signal reject threshold so genuinely weak captures stay weak (and keep
/// being rejected upstream) rather than being amplified up to speech level.
const CAPTURE_NORMALIZE_MIN_PEAK: f32 = 0.05;
/// Target peak that quiet-but-valid captures are lifted toward.
const CAPTURE_NORMALIZE_TARGET_PEAK: f32 = 0.9;
/// Upper bound on applied gain, so the noise floor of a quiet capture is not
/// amplified without limit.
const CAPTURE_NORMALIZE_MAX_GAIN: f32 = 4.0;

/// Gain that lifts a quiet-but-valid capture toward the target peak, bounded so
/// weak captures and the noise floor are preserved. Returns `1.0` (no change)
/// when the signal is silent/weak (`< CAPTURE_NORMALIZE_MIN_PEAK`) or already
/// hot (`>= CAPTURE_NORMALIZE_TARGET_PEAK`).
fn normalized_capture_gain(peak: f32) -> f32 {
    if !peak.is_finite()
        || !(CAPTURE_NORMALIZE_MIN_PEAK..CAPTURE_NORMALIZE_TARGET_PEAK).contains(&peak)
    {
        return 1.0;
    }
    (CAPTURE_NORMALIZE_TARGET_PEAK / peak).min(CAPTURE_NORMALIZE_MAX_GAIN)
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

#[cfg(test)]
fn phase_safe_downmix_channel(source: &CapturedAudioBuffer) -> Option<usize> {
    let mut channel_energy_scratch = Vec::new();
    phase_safe_downmix_channel_reusing_scratch(source, &mut channel_energy_scratch)
}

fn phase_safe_downmix_channel_reusing_scratch(
    source: &CapturedAudioBuffer,
    channel_energy_scratch: &mut Vec<f64>,
) -> Option<usize> {
    const MAX_AVERAGE_TO_STRONGEST_ENERGY_RATIO: f64 = 0.25;

    channel_energy_scratch.clear();
    let channel_count = usize::from(source.channels);
    if channel_count <= 1 || source.samples.is_empty() {
        return None;
    }

    channel_energy_scratch.resize(channel_count, 0.0);
    let mut average_energy = 0.0_f64;
    for frame in source.samples.chunks_exact(channel_count) {
        let mut sum = 0.0_f64;
        for (channel_index, sample) in frame.iter().enumerate() {
            let sample = f64::from(*sample);
            channel_energy_scratch[channel_index] += sample * sample;
            sum += sample;
        }
        let average = sum / channel_count as f64;
        average_energy += average * average;
    }

    let (strongest_channel, strongest_energy) = channel_energy_scratch
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))?;
    let strongest_energy = *strongest_energy;
    if strongest_energy <= f64::EPSILON
        || average_energy > strongest_energy * MAX_AVERAGE_TO_STRONGEST_ENERGY_RATIO
    {
        return None;
    }

    Some(strongest_channel)
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
    if channels == 0 || source.samples.is_empty() || !source.samples.len().is_multiple_of(channels)
    {
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
    if !source.samples.len().is_multiple_of(source_channels) {
        return Err(TalkError::Audio(
            "captured audio samples must be frame-aligned with channels".to_string(),
        ));
    }
    let mut pcm_bytes = Vec::with_capacity(source.samples.len() * std::mem::size_of::<i16>());
    for sample in &source.samples {
        pcm_bytes.extend_from_slice(&float_sample_to_i16(*sample).to_le_bytes());
    }
    wav_bytes_from_encoded_pcm(&pcm_bytes, settings)
}

fn wav_bytes_from_encoded_pcm(
    pcm_bytes: &[u8],
    settings: WavSettings,
) -> Result<Vec<u8>, TalkError> {
    validate_wav_settings(settings)?;
    let data_size = u32::try_from(pcm_bytes.len())
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
    bytes.extend_from_slice(pcm_bytes);
    Ok(bytes)
}

fn captured_pcm_conversion_dimensions(
    source: &CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<(usize, usize, usize), TalkError> {
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
    if !source.samples.len().is_multiple_of(source_channels) {
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
    Ok((source_channels, source_frames, target_frames))
}

// Kept as the allocating reference implementation that the contract tests
// compare the zero-copy scratch encoder against.
#[cfg(test)]
fn encode_captured_pcm_bytes(
    source: &CapturedAudioBuffer,
    settings: WavSettings,
) -> Result<Vec<u8>, TalkError> {
    let (source_channels, source_frames, target_frames) =
        captured_pcm_conversion_dimensions(source, settings)?;
    if source.sample_rate_hz == settings.sample_rate_hz && source_channels == 1 {
        return encode_same_rate_mono_pcm_bytes(&source.samples, settings);
    }

    let mut bytes = Vec::with_capacity(
        target_frames * usize::from(settings.channels) * std::mem::size_of::<i16>(),
    );
    let selected_downmix_channel = phase_safe_downmix_channel(source);
    let source_mono: Vec<f32> = (0..source_frames)
        .map(|source_frame_index| match selected_downmix_channel {
            Some(channel_index) => {
                source.samples[(source_frame_index * source_channels) + channel_index]
            }
            None => downmix_source_frame_to_mono(source, source_frame_index),
        })
        .collect();
    let resampled_mono = resample_mono_to_len(
        &source_mono,
        source.sample_rate_hz,
        settings.sample_rate_hz,
        target_frames,
    );

    for mono_sample in resampled_mono {
        for _ in 0..settings.channels {
            bytes.extend_from_slice(&float_sample_to_i16(mono_sample).to_le_bytes());
        }
    }

    Ok(bytes)
}

fn downmix_captured_samples_in_place(
    source: &mut CapturedAudioBuffer,
    source_channels: usize,
    source_frames: usize,
    selected_downmix_channel: Option<usize>,
) {
    for source_frame_index in 0..source_frames {
        let mono_sample = match selected_downmix_channel {
            Some(channel_index) => {
                source.samples[(source_frame_index * source_channels) + channel_index]
            }
            None => downmix_source_frame_to_mono(source, source_frame_index),
        };
        source.samples[source_frame_index] = mono_sample;
    }
    source.samples.truncate(source_frames);
}

#[cfg(test)]
fn encode_captured_pcm_bytes_reusing_scratch(
    source: &mut CapturedAudioBuffer,
    settings: WavSettings,
    resampled_scratch: &mut Vec<f32>,
    channel_energy_scratch: &mut Vec<f64>,
) -> Result<Vec<u8>, TalkError> {
    let mut pcm_bytes = Vec::new();
    encode_captured_pcm_bytes_reusing_scratch_into(
        source,
        settings,
        resampled_scratch,
        channel_energy_scratch,
        &mut pcm_bytes,
    )?;
    Ok(pcm_bytes)
}

fn encode_captured_pcm_bytes_reusing_scratch_into(
    source: &mut CapturedAudioBuffer,
    settings: WavSettings,
    resampled_scratch: &mut Vec<f32>,
    channel_energy_scratch: &mut Vec<f64>,
    pcm_bytes: &mut Vec<u8>,
) -> Result<(), TalkError> {
    resampled_scratch.clear();
    pcm_bytes.clear();
    let (source_channels, source_frames, target_frames) =
        captured_pcm_conversion_dimensions(source, settings)?;
    if source.sample_rate_hz == settings.sample_rate_hz && source_channels == 1 {
        return encode_same_rate_mono_pcm_bytes_into(&source.samples, settings, pcm_bytes);
    }

    let selected_downmix_channel =
        phase_safe_downmix_channel_reusing_scratch(source, channel_energy_scratch);
    downmix_captured_samples_in_place(
        source,
        source_channels,
        source_frames,
        selected_downmix_channel,
    );
    if source.sample_rate_hz == settings.sample_rate_hz {
        return encode_same_rate_mono_pcm_bytes_into(&source.samples, settings, pcm_bytes);
    }

    resample_mono_to_len_into(
        &source.samples,
        source.sample_rate_hz,
        settings.sample_rate_hz,
        target_frames,
        resampled_scratch,
    );
    encode_same_rate_mono_pcm_bytes_into(resampled_scratch, settings, pcm_bytes)
}

#[cfg(test)]
fn encode_same_rate_mono_pcm_bytes(
    mono_samples: &[f32],
    settings: WavSettings,
) -> Result<Vec<u8>, TalkError> {
    let mut bytes = Vec::new();
    encode_same_rate_mono_pcm_bytes_into(mono_samples, settings, &mut bytes)?;
    Ok(bytes)
}

fn encode_same_rate_mono_pcm_bytes_into(
    mono_samples: &[f32],
    settings: WavSettings,
    bytes: &mut Vec<u8>,
) -> Result<(), TalkError> {
    validate_wav_settings(settings)?;
    bytes.clear();
    let output_sample_count = mono_samples
        .len()
        .checked_mul(usize::from(settings.channels))
        .ok_or_else(|| TalkError::Audio("captured PCM sample count overflow".to_string()))?;
    let byte_capacity = output_sample_count
        .checked_mul(std::mem::size_of::<i16>())
        .ok_or_else(|| TalkError::Audio("captured PCM byte count overflow".to_string()))?;
    bytes.try_reserve(byte_capacity).map_err(|error| {
        TalkError::Audio(format!(
            "failed to reserve captured PCM byte buffer: {error}"
        ))
    })?;
    for mono_sample in mono_samples.iter().copied() {
        let encoded_sample = float_sample_to_i16(mono_sample).to_le_bytes();
        for _ in 0..settings.channels {
            bytes.extend_from_slice(&encoded_sample);
        }
    }
    Ok(())
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
    let remaining_samples = (samples - cursor.source_sample_offset).min(
        streaming_pcm_chunk_sample_count(settings.sample_rate_hz, settings.channels),
    );
    let byte_count = remaining_samples
        .checked_mul(std::mem::size_of::<i16>())
        .ok_or_else(|| TalkError::Audio("captured PCM byte count overflow".to_string()))?;
    let mut bytes = std::mem::take(&mut cursor.pcm_bytes_scratch);
    bytes.clear();
    if let Err(error) = bytes.try_reserve(byte_count) {
        cursor.pcm_bytes_scratch = bytes;
        return Err(TalkError::Audio(format!(
            "failed to reserve captured PCM byte buffer: {error}"
        )));
    }
    bytes.resize(byte_count, 0);
    cursor.source_sample_offset = cursor
        .source_sample_offset
        .saturating_add(remaining_samples);
    let sequence = cursor.next_sequence();
    Ok(Some(RecordingPcmChunk {
        sequence,
        sample_rate_hz: settings.sample_rate_hz,
        channels: settings.channels,
        bytes,
    }))
}

const STREAMING_PCM_CHUNK_DURATION_MS: usize = 80;

fn streaming_pcm_chunk_sample_count(sample_rate_hz: u32, channels: u16) -> usize {
    let channels = usize::from(channels).max(1);
    ((usize::try_from(sample_rate_hz)
        .unwrap_or(usize::MAX)
        .saturating_mul(channels)
        .saturating_mul(STREAMING_PCM_CHUNK_DURATION_MS)
        / 1_000)
        / channels)
        .max(1)
        .saturating_mul(channels)
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
    if !samples.len().is_multiple_of(channel_count) {
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
    let mut waveform = vec![0.0; bucket_count];
    summarize_recent_interleaved_audio_waveform_into(
        samples,
        channels,
        trailing_frames,
        &mut waveform,
    )?;
    Ok(waveform)
}

fn summarize_recent_interleaved_audio_waveform_into(
    samples: &[f32],
    channels: u16,
    trailing_frames: usize,
    waveform: &mut [f32],
) -> Result<(), TalkError> {
    if channels == 0 {
        return Err(TalkError::Audio(
            "live audio waveform channels must be greater than 0".to_string(),
        ));
    }
    if waveform.is_empty() {
        return Ok(());
    }

    let channel_count = usize::from(channels);
    if samples.is_empty() || trailing_frames == 0 {
        waveform.fill(0.0);
        return Ok(());
    }
    if !samples.len().is_multiple_of(channel_count) {
        return Err(TalkError::Audio(
            "live audio waveform samples must be frame-aligned with channels".to_string(),
        ));
    }

    let frame_count = samples.len() / channel_count;
    let recent_frame_count = trailing_frames.min(frame_count);
    let recent_start_frame = frame_count - recent_frame_count;
    let bucket_count = waveform.len();
    let frames_per_bucket = recent_frame_count as f32 / bucket_count as f32;

    for (bucket_index, bucket) in waveform.iter_mut().enumerate() {
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
        *bucket = peak.clamp(0.0, 1.0);
    }

    Ok(())
}

#[cfg(windows)]
fn summarize_recent_interleaved_audio_level_queue(
    samples: &VecDeque<f32>,
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
    if !samples.len().is_multiple_of(channel_count) {
        return Err(TalkError::Audio(
            "live audio level samples must be frame-aligned with channels".to_string(),
        ));
    }

    let frame_count = samples.len() / channel_count;
    let recent_frame_count = trailing_frames.min(frame_count);
    let recent_start = (frame_count - recent_frame_count) * channel_count;
    let recent_sample_count = samples.len() - recent_start;
    let mut peak = 0.0_f32;
    let mut sum_squares = 0.0_f32;
    for sample in samples.range(recent_start..).copied() {
        peak = peak.max(sample.abs());
        sum_squares += sample * sample;
    }

    Ok(AudioInputLevel {
        peak,
        rms: (sum_squares / recent_sample_count as f32).sqrt(),
    })
}

#[cfg(windows)]
fn summarize_recent_interleaved_audio_waveform_queue_into(
    samples: &VecDeque<f32>,
    channels: u16,
    trailing_frames: usize,
    waveform: &mut [f32],
) -> Result<(), TalkError> {
    if channels == 0 {
        return Err(TalkError::Audio(
            "live audio waveform channels must be greater than 0".to_string(),
        ));
    }
    if waveform.is_empty() {
        return Ok(());
    }

    let channel_count = usize::from(channels);
    if samples.is_empty() || trailing_frames == 0 {
        waveform.fill(0.0);
        return Ok(());
    }
    if !samples.len().is_multiple_of(channel_count) {
        return Err(TalkError::Audio(
            "live audio waveform samples must be frame-aligned with channels".to_string(),
        ));
    }

    let frame_count = samples.len() / channel_count;
    let recent_frame_count = trailing_frames.min(frame_count);
    let recent_start_frame = frame_count - recent_frame_count;
    let bucket_count = waveform.len();
    let frames_per_bucket = recent_frame_count as f32 / bucket_count as f32;

    for (bucket_index, bucket) in waveform.iter_mut().enumerate() {
        let start_offset = (bucket_index as f32 * frames_per_bucket).floor() as usize;
        let mut end_offset = ((bucket_index + 1) as f32 * frames_per_bucket).floor() as usize;
        if end_offset <= start_offset {
            end_offset = (start_offset + 1).min(recent_frame_count);
        }
        let start_frame = (recent_start_frame + start_offset).min(frame_count.saturating_sub(1));
        let end_frame = (recent_start_frame + end_offset).min(frame_count);
        let start_sample = start_frame * channel_count;
        let end_sample = end_frame * channel_count;
        let peak = samples
            .range(start_sample..end_sample)
            .copied()
            .map(f32::abs)
            .fold(0.0_f32, f32::max);
        *bucket = peak.clamp(0.0, 1.0);
    }

    Ok(())
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
    if !sample_count.is_multiple_of(usize::from(channels)) {
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
    fn discard_consumed_pcm(&self, cursor: &mut RecordingPcmCursor) -> Result<(), TalkError> {
        discard_native_windows_pcm(&self.samples, &self.archived_samples, cursor)
    }

    fn drain_pcm_chunk(
        &self,
        cursor: &mut RecordingPcmCursor,
    ) -> Result<Option<RecordingPcmChunk>, TalkError> {
        drain_native_windows_pcm_chunk(
            &self.samples,
            self.wav_settings,
            self.sample_rate_hz,
            self.channels,
            cursor,
        )
    }

    fn current_level(&self) -> Result<AudioInputLevel, TalkError> {
        let samples = self
            .samples
            .lock()
            .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
        summarize_recent_interleaved_audio_level_queue(
            &samples,
            self.channels,
            live_level_trailing_frames(self.sample_rate_hz),
        )
    }

    fn current_waveform_into(&self, waveform: &mut [f32]) -> Result<(), TalkError> {
        let Some(samples) = try_lock_capture_samples(&self.samples)? else {
            return Ok(());
        };
        summarize_recent_interleaved_audio_waveform_queue_into(
            &samples,
            self.channels,
            live_waveform_trailing_frames(self.sample_rate_hz),
            waveform,
        )
    }

    fn finish(&mut self) -> Result<AudioArtifact, TalkError> {
        let captured = self.finish_captured_audio(true)?;
        write_captured_wav(&self.artifact, captured, self.wav_settings).map_err(|error| {
            native_windows_audio_error(format!("failed to write captured WAV: {error}"))
        })?;
        Ok(self.artifact.clone())
    }

    fn cancel(&mut self) -> Result<(), TalkError> {
        self.shutdown_capture_collector(false)
    }

    fn finish_probe(&mut self) -> Result<AudioSignalProbe, TalkError> {
        let captured = self.finish_captured_audio(false)?;
        let signal = summarize_captured_audio(&captured).map_err(|error| {
            native_windows_audio_error(format!("failed to summarize captured audio: {error}"))
        })?;
        write_captured_wav(&self.artifact, captured, self.wav_settings).map_err(|error| {
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
        self.shutdown_capture_collector(true)?;

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

        let samples = take_complete_native_windows_samples(&self.archived_samples, &self.samples)?;
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

    fn shutdown_capture_collector(&mut self, report_overflow: bool) -> Result<(), TalkError> {
        // Drop the CPAL stream first so no callback can enqueue after the
        // collector observes the stop flag and drains the queue.
        self.stream.take();
        self.capture_collector_stop.store(true, Ordering::Release);

        let collector_result = match self.capture_collector.take() {
            Some(handle) => handle
                .join()
                .map_err(|_| native_windows_audio_error("native capture collector thread panicked"))
                .and_then(|result| {
                    result.map_err(|error| {
                        native_windows_audio_error(format!(
                            "native capture collector failed: {error}"
                        ))
                    })
                }),
            None => Ok(()),
        };

        let overflowed_samples = self.overflowed_samples.load(Ordering::Acquire);
        if report_overflow && overflowed_samples > 0 {
            return Err(native_windows_audio_error(format!(
                "native capture queue overflowed and lost {overflowed_samples} sample(s)"
            )));
        }

        collector_result
    }
}

#[cfg(windows)]
impl Drop for NativeWindowsRecording {
    fn drop(&mut self) {
        let _ = self.shutdown_capture_collector(false);
    }
}

#[cfg(windows)]
fn take_complete_native_windows_samples(
    archived_samples: &Arc<Mutex<Vec<f32>>>,
    live_samples: &Arc<Mutex<VecDeque<f32>>>,
) -> Result<Vec<f32>, TalkError> {
    let mut archived_samples = archived_samples
        .lock()
        .map_err(|_| native_windows_audio_error("archived sample buffer lock was poisoned"))?;
    let mut live_samples = live_samples
        .lock()
        .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
    Ok(take_complete_captured_samples(
        &mut archived_samples,
        &mut live_samples,
    ))
}

#[cfg(windows)]
fn discard_native_windows_pcm(
    samples: &Arc<Mutex<VecDeque<f32>>>,
    archived_samples: &Arc<Mutex<Vec<f32>>>,
    cursor: &mut RecordingPcmCursor,
) -> Result<(), TalkError> {
    let mut archived_samples = archived_samples
        .lock()
        .map_err(|_| native_windows_audio_error("archived sample buffer lock was poisoned"))?;
    let mut samples = samples
        .lock()
        .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
    archive_consumed_samples_before_cursor(&mut samples, &mut archived_samples, cursor);
    Ok(())
}

#[cfg(windows)]
fn drain_native_windows_pcm_chunk(
    samples: &Arc<Mutex<VecDeque<f32>>>,
    wav_settings: WavSettings,
    sample_rate_hz: u32,
    channels: u16,
    cursor: &mut RecordingPcmCursor,
) -> Result<Option<RecordingPcmChunk>, TalkError> {
    let channel_count = usize::from(channels);
    if channel_count == 0 {
        return Err(native_windows_audio_error(
            "captured audio channels must be greater than 0",
        ));
    }
    let max_chunk_samples = streaming_pcm_chunk_sample_count(sample_rate_hz, channels);
    cursor.sample_scratch.clear();
    cursor
        .sample_scratch
        .try_reserve(max_chunk_samples)
        .map_err(|error| {
            native_windows_audio_error(format!(
                "failed to reserve captured PCM scratch buffer: {error}"
            ))
        })?;
    let samples = samples
        .lock()
        .map_err(|_| native_windows_audio_error("captured sample buffer lock was poisoned"))?;
    let aligned_available = samples.len() - (samples.len() % channel_count);
    if cursor.source_sample_offset >= aligned_available {
        return Ok(None);
    }
    let start = cursor.source_sample_offset - (cursor.source_sample_offset % channel_count);
    let end = start
        .saturating_add(max_chunk_samples)
        .min(aligned_available);
    cursor
        .sample_scratch
        .extend(samples.range(start..end).copied());
    drop(samples);

    let mut source = CapturedAudioBuffer {
        sample_rate_hz,
        channels,
        samples: std::mem::take(&mut cursor.sample_scratch),
    };
    let mut bytes = std::mem::take(&mut cursor.pcm_bytes_scratch);
    let encoded = encode_captured_pcm_bytes_reusing_scratch_into(
        &mut source,
        wav_settings,
        &mut cursor.resampled_scratch,
        &mut cursor.channel_energy_scratch,
        &mut bytes,
    );
    cursor.sample_scratch = source.samples;
    if let Err(error) = encoded {
        cursor.pcm_bytes_scratch = bytes;
        return Err(native_windows_audio_error(format!(
            "failed to encode captured PCM chunk: {error}"
        )));
    }
    if bytes.is_empty() {
        cursor.pcm_bytes_scratch = bytes;
        return Ok(None);
    }
    cursor.source_sample_offset = end;
    let sequence = cursor.next_sequence();
    Ok(Some(RecordingPcmChunk {
        sequence,
        sample_rate_hz: wav_settings.sample_rate_hz,
        channels: wav_settings.channels,
        bytes,
    }))
}

#[cfg(not(windows))]
impl NativeWindowsRecording {
    fn discard_consumed_pcm(&self, _cursor: &mut RecordingPcmCursor) -> Result<(), TalkError> {
        Err(native_windows_audio_error(
            "native_windows audio backend is only available on Windows",
        ))
    }

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

    fn current_waveform_into(&self, _waveform: &mut [f32]) -> Result<(), TalkError> {
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
    if !native_windows_sample_format_supported(sample_format) {
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
        render_output_playback_samples(&source, config.sample_rate, config.channels)?;

    let cursor = Arc::new(AtomicUsize::new(0));
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
    // Bound the wait so a device that stops delivering callbacks (e.g. it was
    // unplugged mid-playback without reporting a stream error) cannot hang the
    // caller forever.
    let duration_seconds = audio_signal_duration_seconds(
        source.sample_rate_hz,
        source.channels,
        source.samples.len(),
    )?;
    let max_wait =
        Duration::from_secs_f64(duration_seconds * 2.0).saturating_add(Duration::from_secs(1));
    let wait_started = std::time::Instant::now();
    loop {
        if cursor.load(Ordering::Relaxed) >= playback_target {
            break;
        }
        let pending_errors = stream_errors
            .lock()
            .map_err(|_| native_windows_audio_error("output stream error lock was poisoned"))?;
        if !pending_errors.is_empty() {
            return Err(native_windows_audio_error(format!(
                "output stream reported errors: {}",
                pending_errors.join("; ")
            )));
        }
        drop(pending_errors);
        if wait_started.elapsed() > max_wait {
            return Err(native_windows_audio_error(format!(
                "playback stalled: cursor did not reach the end of the audio within {:.1}s",
                max_wait.as_secs_f64()
            )));
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeRecordingLimit {
    Unlimited,
    Seconds(u64),
}

fn resolve_native_recording_limit(
    configured_max_seconds: u64,
    override_seconds: Option<u64>,
) -> Result<NativeRecordingLimit, TalkError> {
    if override_seconds == Some(0) {
        return Err(native_windows_audio_error(
            "TALK_NATIVE_AUDIO_SECONDS must be greater than 0",
        ));
    }

    Ok(match (configured_max_seconds, override_seconds) {
        (0, None) => NativeRecordingLimit::Unlimited,
        (0, Some(seconds)) => NativeRecordingLimit::Seconds(seconds),
        (configured_max_seconds, None) => NativeRecordingLimit::Seconds(configured_max_seconds),
        (configured_max_seconds, Some(seconds)) => {
            NativeRecordingLimit::Seconds(seconds.min(configured_max_seconds))
        }
    })
}

#[cfg(windows)]
fn native_windows_recording_limit(
    request: &AudioCaptureRequest,
) -> Result<NativeRecordingLimit, TalkError> {
    let override_seconds = std::env::var_os("TALK_NATIVE_AUDIO_SECONDS")
        .map(|raw| {
            raw.to_string_lossy()
                .trim()
                .parse::<u64>()
                .map_err(|error| {
                    native_windows_audio_error(format!(
                        "TALK_NATIVE_AUDIO_SECONDS must be a positive integer: {error}"
                    ))
                })
        })
        .transpose()?;
    resolve_native_recording_limit(request.max_recording_seconds, override_seconds)
}

fn resolved_native_capture_sample_limit(
    sample_rate_hz: u32,
    channels: u16,
    recording_limit: NativeRecordingLimit,
) -> Result<usize, TalkError> {
    let NativeRecordingLimit::Seconds(recording_seconds) = recording_limit else {
        return Ok(usize::MAX);
    };

    let frames = u128::from(sample_rate_hz) * u128::from(recording_seconds);
    let samples = frames * u128::from(channels);
    usize::try_from(samples)
        .map_err(|_| native_windows_audio_error("requested native recording duration is too large"))
}

#[cfg(windows)]
fn max_native_capture_samples(
    config: &cpal::StreamConfig,
    recording_limit: NativeRecordingLimit,
) -> Result<usize, TalkError> {
    resolved_native_capture_sample_limit(config.sample_rate, config.channels, recording_limit)
}

#[cfg(windows)]
fn native_capture_queue_capacity(config: &cpal::StreamConfig, max_samples: usize) -> usize {
    const CAPTURE_QUEUE_HEADROOM_SECONDS: usize = 2;

    let headroom = usize::try_from(config.sample_rate)
        .unwrap_or(usize::MAX)
        .saturating_mul(usize::from(config.channels))
        .saturating_mul(CAPTURE_QUEUE_HEADROOM_SECONDS);
    headroom.min(max_samples).max(usize::from(config.channels))
}

#[cfg(windows)]
fn spawn_native_capture_collector(
    capture_queue: Arc<ArrayQueue<f32>>,
    samples: Arc<Mutex<VecDeque<f32>>>,
    stop: Arc<AtomicBool>,
    source_sample_rate_hz: u32,
    source_channels: u16,
    target_settings: WavSettings,
) -> Result<std::thread::JoinHandle<Result<(), String>>, TalkError> {
    std::thread::Builder::new()
        .name("talk-native-capture-collector".to_string())
        .spawn(move || {
            let mut converter = StreamingCaptureConverter::new(
                source_sample_rate_hz,
                source_channels,
                target_settings.sample_rate_hz,
                target_settings.channels,
            )?;
            let mut raw_samples = Vec::with_capacity(8_192);
            let mut converted_samples = Vec::with_capacity(4_096);

            loop {
                raw_samples.clear();
                while raw_samples.len() < raw_samples.capacity() {
                    let Some(sample) = capture_queue.pop() else {
                        break;
                    };
                    raw_samples.push(sample);
                }

                if !raw_samples.is_empty() {
                    converter.push_interleaved(&raw_samples, &mut converted_samples)?;
                    if !converted_samples.is_empty() {
                        samples
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .extend(converted_samples.iter().copied());
                    }
                    continue;
                }

                if stop.load(Ordering::Acquire) && capture_queue.is_empty() {
                    converter.finish(&mut converted_samples)?;
                    if !converted_samples.is_empty() {
                        samples
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .extend(converted_samples.iter().copied());
                    }
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        })
        .map_err(|error| {
            native_windows_audio_error(format!("failed to spawn native capture collector: {error}"))
        })
}

#[cfg(windows)]
fn start_native_windows_recording(
    request: &AudioCaptureRequest,
    artifact: AudioArtifact,
) -> Result<NativeWindowsRecording, TalkError> {
    let recording_limit = native_windows_recording_limit(request)?;
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
    let max_samples = max_native_capture_samples(&config, recording_limit)?;

    validate_wav_settings(request.wav_settings)?;

    let samples = Arc::new(Mutex::new(VecDeque::<f32>::with_capacity(
        usize::try_from(request.wav_settings.sample_rate_hz)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::from(request.wav_settings.channels))
            .saturating_mul(2)
            .min(max_samples),
    )));
    let archived_samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let capture_queue = Arc::new(ArrayQueue::new(native_capture_queue_capacity(
        &config,
        max_samples,
    )));
    let overflowed_samples = Arc::new(AtomicUsize::new(0));
    let capture_collector_stop = Arc::new(AtomicBool::new(false));
    let sample_limit = NativeInputCaptureLimit {
        max_samples,
        total_captured_samples: Arc::new(AtomicUsize::new(0)),
    };
    let stream_errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let stream = build_native_input_stream(
        &device,
        &config,
        sample_format,
        Arc::clone(&capture_queue),
        sample_limit,
        Arc::clone(&overflowed_samples),
        Arc::clone(&stream_errors),
    )?;
    let capture_collector = spawn_native_capture_collector(
        Arc::clone(&capture_queue),
        Arc::clone(&samples),
        Arc::clone(&capture_collector_stop),
        config.sample_rate,
        config.channels,
        request.wav_settings,
    )?;

    if let Err(error) = stream.play() {
        capture_collector_stop.store(true, Ordering::Release);
        let _ = capture_collector.join();
        return Err(native_windows_audio_error(format!(
            "failed to start input stream: {error}"
        )));
    }

    Ok(NativeWindowsRecording {
        artifact,
        wav_settings: request.wav_settings,
        sample_rate_hz: request.wav_settings.sample_rate_hz,
        channels: request.wav_settings.channels,
        samples,
        archived_samples,
        capture_collector_stop,
        capture_collector: Some(capture_collector),
        overflowed_samples,
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
    if !source.samples.len().is_multiple_of(source_channels) {
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
    let target_frames =
        resampled_frame_count(source_frames, source.sample_rate_hz, config.sample_rate)?;
    target_frames
        .checked_mul(usize::from(config.channels))
        .ok_or_else(|| native_windows_audio_error("playback sample buffer is too large"))
}

#[cfg(windows)]
fn native_windows_sample_format_supported(sample_format: cpal::SampleFormat) -> bool {
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

/// Dispatches a CPAL sample-format value to a monomorphized stream builder.
/// Each arm may move the same captured arguments because exactly one arm runs,
/// which also removes the per-arm clones the hand-written matches needed.
#[cfg(windows)]
macro_rules! build_native_stream_for_sample_format {
    ($sample_format:expr, $direction:literal, $builder:ident, ($($arg:expr),* $(,)?)) => {
        match $sample_format {
            cpal::SampleFormat::I8 => $builder::<i8>($($arg),*),
            cpal::SampleFormat::I16 => $builder::<i16>($($arg),*),
            cpal::SampleFormat::I24 => $builder::<cpal::I24>($($arg),*),
            cpal::SampleFormat::I32 => $builder::<i32>($($arg),*),
            cpal::SampleFormat::I64 => $builder::<i64>($($arg),*),
            cpal::SampleFormat::U8 => $builder::<u8>($($arg),*),
            cpal::SampleFormat::U16 => $builder::<u16>($($arg),*),
            cpal::SampleFormat::U24 => $builder::<cpal::U24>($($arg),*),
            cpal::SampleFormat::U32 => $builder::<u32>($($arg),*),
            cpal::SampleFormat::U64 => $builder::<u64>($($arg),*),
            cpal::SampleFormat::F32 => $builder::<f32>($($arg),*),
            cpal::SampleFormat::F64 => $builder::<f64>($($arg),*),
            other => Err(native_windows_audio_error(format!(
                concat!("unsupported ", $direction, " sample format {}"),
                other
            ))),
        }
    };
}

#[cfg(windows)]
fn build_native_input_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    capture_queue: Arc<ArrayQueue<f32>>,
    sample_limit: NativeInputCaptureLimit,
    overflowed_samples: Arc<AtomicUsize>,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError> {
    build_native_stream_for_sample_format!(
        sample_format,
        "input",
        build_native_input_stream_for_sample,
        (
            device,
            config,
            capture_queue,
            sample_limit,
            overflowed_samples,
            stream_errors
        )
    )
}

#[cfg(windows)]
fn build_native_input_stream_for_sample<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    capture_queue: Arc<ArrayQueue<f32>>,
    sample_limit: NativeInputCaptureLimit,
    overflowed_samples: Arc<AtomicUsize>,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError>
where
    T: cpal::SizedSample + Send + 'static,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                append_native_input_samples(
                    data,
                    &capture_queue,
                    &sample_limit,
                    &overflowed_samples,
                )
            },
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
fn append_native_input_samples<T>(
    input: &[T],
    capture_queue: &ArrayQueue<f32>,
    sample_limit: &NativeInputCaptureLimit,
    overflowed_samples: &AtomicUsize,
) where
    T: cpal::Sample,
    f32: cpal::FromSample<T>,
{
    let append_count = loop {
        let total_captured_samples = sample_limit.total_captured_samples.load(Ordering::Relaxed);
        let append_count = native_input_sample_count_to_append(
            input.len(),
            total_captured_samples,
            sample_limit.max_samples,
        );
        if append_count == 0 {
            return;
        }
        if sample_limit
            .total_captured_samples
            .compare_exchange_weak(
                total_captured_samples,
                total_captured_samples.saturating_add(append_count),
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
        {
            break append_count;
        }
    };
    if append_count == 0 {
        return;
    }

    let mut queued = 0usize;
    for sample in input.iter().take(append_count) {
        if capture_queue.push(f32::from_sample(*sample)).is_err() {
            break;
        }
        queued += 1;
    }
    let overflowed = append_count.saturating_sub(queued);
    if overflowed > 0 {
        sample_limit
            .total_captured_samples
            .fetch_sub(overflowed, Ordering::AcqRel);
        overflowed_samples.fetch_add(overflowed, Ordering::Relaxed);
    }
}

#[cfg(windows)]
fn resolve_native_windows_input_device(
    host: &cpal::Host,
    requested_device_name: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    resolve_native_windows_device(
        host,
        NativeWindowsDeviceDirection::Input,
        requested_device_name,
    )
}

#[cfg(windows)]
fn resolve_native_windows_device(
    host: &cpal::Host,
    direction: NativeWindowsDeviceDirection,
    requested_device_name: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    let direction_name = direction.as_str();
    if requested_device_name.is_none() {
        let default_device = match direction {
            NativeWindowsDeviceDirection::Input => host.default_input_device(),
            NativeWindowsDeviceDirection::Output => host.default_output_device(),
        };
        let Some(device) = default_device else {
            return Err(format!("no default {direction_name} device is available"));
        };
        let device_name = describe_native_windows_device(&device);
        return Ok((device, device_name));
    }

    let devices: Vec<cpal::Device> = match direction {
        NativeWindowsDeviceDirection::Input => host
            .input_devices()
            .map_err(|error| format!("failed to enumerate {direction_name} devices: {error}"))?
            .collect(),
        NativeWindowsDeviceDirection::Output => host
            .output_devices()
            .map_err(|error| format!("failed to enumerate {direction_name} devices: {error}"))?
            .collect(),
    };
    let mut named_devices = devices
        .into_iter()
        .enumerate()
        .map(|(index, device)| {
            (
                describe_native_windows_device(&device)
                    .unwrap_or_else(|| format!("unnamed {direction_name} device {}", index + 1)),
                device,
            )
        })
        .collect::<Vec<_>>();
    let available_device_names = named_devices
        .iter()
        .map(|(device_name, _)| device_name.clone())
        .collect::<Vec<_>>();
    let selected_device_name = select_native_windows_device_name(
        direction,
        &available_device_names,
        requested_device_name,
    )?
    .expect("requested device name should resolve to Some");
    let selected_index = available_device_names
        .iter()
        .position(|device_name| device_name == &selected_device_name)
        .expect("selected device name must exist in enumerated devices");
    let (_, device) = named_devices.swap_remove(selected_index);
    Ok((device, Some(selected_device_name)))
}

#[cfg(windows)]
fn available_native_windows_input_device_names(host: &cpal::Host) -> Result<Vec<String>, String> {
    host.input_devices()
        .map_err(|error| format!("failed to enumerate input devices: {error}"))?
        .enumerate()
        .map(|(index, device)| {
            Ok(describe_native_windows_device(&device)
                .unwrap_or_else(|| format!("unnamed input device {}", index + 1)))
        })
        .collect()
}

#[cfg(windows)]
fn resolve_native_windows_output_device(
    host: &cpal::Host,
    requested_device_name: Option<&str>,
) -> Result<(cpal::Device, Option<String>), String> {
    resolve_native_windows_device(
        host,
        NativeWindowsDeviceDirection::Output,
        requested_device_name,
    )
}

#[cfg(windows)]
fn build_native_output_stream(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_format: cpal::SampleFormat,
    samples: Arc<Vec<f32>>,
    cursor: Arc<AtomicUsize>,
    stream_errors: Arc<Mutex<Vec<String>>>,
) -> Result<cpal::Stream, TalkError> {
    build_native_stream_for_sample_format!(
        sample_format,
        "output",
        build_native_output_stream_for_sample,
        (device, config, samples, cursor, stream_errors)
    )
}

#[cfg(windows)]
fn build_native_output_stream_for_sample<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Arc<Vec<f32>>,
    cursor: Arc<AtomicUsize>,
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
    cursor: &Arc<AtomicUsize>,
) where
    T: cpal::Sample + cpal::FromSample<f32>,
{
    let mut index = cursor.load(Ordering::Relaxed);
    for sample in output.iter_mut() {
        let value = samples.get(index).copied().unwrap_or(0.0);
        *sample = T::from_sample(value);
        index = index.saturating_add(1);
    }
    cursor.store(index, Ordering::Relaxed);
}

#[cfg(windows)]
fn describe_native_windows_device(device: &cpal::Device) -> Option<String> {
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
    use std::collections::VecDeque;

    use super::{
        archive_consumed_samples_before_cursor, captured_audio_peak_abs, captured_audio_rms,
        discard_captured_samples_before_cursor, drain_silent_pcm_chunk, encode_captured_pcm_bytes,
        encode_captured_pcm_bytes_reusing_scratch, native_input_sample_count_to_append,
        normalized_capture_gain, phase_safe_downmix_channel,
        phase_safe_downmix_channel_reusing_scratch, render_output_playback_samples,
        resample_mono_to_len, resampled_frame_count, resolve_native_recording_limit,
        resolved_native_capture_sample_limit, select_native_windows_input_device_name,
        select_native_windows_output_device_name, streaming_pcm_chunk_sample_count,
        take_complete_captured_samples, CapturedAudioBuffer, NativeRecordingLimit,
        RecordingPcmCursor, RecordingPcmSource, RecordingPcmSourceBackend, WavSettings,
    };

    #[cfg(windows)]
    use super::{
        append_native_input_samples, drain_native_windows_pcm_chunk,
        spawn_native_capture_collector, summarize_recent_interleaved_audio_level,
        summarize_recent_interleaved_audio_level_queue,
        summarize_recent_interleaved_audio_waveform_into,
        summarize_recent_interleaved_audio_waveform_queue_into,
        take_complete_native_windows_samples, NativeInputCaptureLimit,
    };

    #[test]
    fn zero_configured_recording_seconds_resolves_to_unlimited_capture() {
        let limit = resolve_native_recording_limit(0, None)
            .expect("zero configured max should disable the recording limit");

        assert_eq!(limit, NativeRecordingLimit::Unlimited);
    }

    #[test]
    fn consumed_pcm_archiving_uses_one_range_drain() {
        let source = include_str!("lib.rs");
        let start = source
            .find("fn archive_consumed_samples_before_cursor")
            .expect("consumed PCM archiving helper");
        let end = source[start..]
            .find("fn take_complete_captured_samples")
            .map(|offset| start + offset)
            .expect("function following consumed PCM archiving helper");
        let helper_source = &source[start..end];

        assert!(
            helper_source.contains("archived_samples.extend(samples.drain(..consumed_samples))")
        );
        assert!(!helper_source.contains("pop_front()"));
    }

    #[test]
    fn cloned_silent_pcm_source_clears_reused_waveform_storage() {
        let source = RecordingPcmSource {
            backend: RecordingPcmSourceBackend::Silent {
                settings: WavSettings::mono_16khz(),
                samples: 16_000,
            },
        };
        let mut waveform = [1.0; 9];

        source
            .clone()
            .current_waveform_into(&mut waveform)
            .expect("silent PCM waveform");

        assert_eq!(waveform, [0.0; 9]);
    }

    #[test]
    fn native_recording_seconds_override_can_bound_an_unlimited_capture() {
        let limit = resolve_native_recording_limit(0, Some(45))
            .expect("positive override should bound an unlimited capture");

        assert_eq!(limit, NativeRecordingLimit::Seconds(45));
    }

    #[test]
    fn rejects_zero_native_recording_seconds_override() {
        let error = resolve_native_recording_limit(0, Some(0))
            .expect_err("zero override must not silently change override semantics");

        assert!(
            error
                .to_string()
                .contains("TALK_NATIVE_AUDIO_SECONDS must be greater than 0"),
            "error={error}"
        );
    }

    #[test]
    fn unlimited_native_capture_has_no_sample_count_cutoff() {
        let max_samples =
            resolved_native_capture_sample_limit(48_000, 2, NativeRecordingLimit::Unlimited)
                .expect("unlimited capture should resolve without arithmetic failure");

        assert_eq!(max_samples, usize::MAX);
    }

    #[test]
    fn discarding_consumed_samples_releases_prefix_and_rebases_cursor() {
        let mut samples = [0.1, 0.2, 0.3, 0.4, 0.5].into_iter().collect();
        let mut cursor = RecordingPcmCursor {
            source_sample_offset: 3,
            next_sequence: 7,
            ..RecordingPcmCursor::default()
        };

        discard_captured_samples_before_cursor(&mut samples, &mut cursor);

        assert_eq!(samples.into_iter().collect::<Vec<_>>(), vec![0.4, 0.5]);
        assert_eq!(cursor.source_sample_offset, 0);
        assert_eq!(cursor.next_sequence, 7);
    }

    #[test]
    fn streaming_compaction_preserves_consumed_samples_for_the_final_recording() {
        let original = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        let mut samples = original.iter().copied().collect();
        let mut archived_samples = Vec::new();
        let mut cursor = RecordingPcmCursor {
            source_sample_offset: 3,
            next_sequence: 7,
            ..RecordingPcmCursor::default()
        };

        archive_consumed_samples_before_cursor(&mut samples, &mut archived_samples, &mut cursor);

        assert_eq!(archived_samples, vec![0.1, 0.2, 0.3]);
        assert_eq!(samples.iter().copied().collect::<Vec<_>>(), vec![0.4, 0.5]);
        let complete = take_complete_captured_samples(&mut archived_samples, &mut samples);

        assert_eq!(complete, original);
        assert!(archived_samples.is_empty());
        assert!(samples.is_empty());
        assert_eq!(cursor.source_sample_offset, 0);
        assert_eq!(cursor.next_sequence, 7);
    }

    #[test]
    fn streaming_ring_queue_reuses_capacity_across_wrapped_compaction() {
        let mut samples = VecDeque::with_capacity(8);
        samples.extend((0..8).map(|value| value as f32));
        let initial_capacity = samples.capacity();
        let mut archived_samples = Vec::new();
        let mut cursor = RecordingPcmCursor::default();
        let mut observed_wrapped_storage = false;

        for batch in 0..64 {
            cursor.source_sample_offset = 4;
            archive_consumed_samples_before_cursor(
                &mut samples,
                &mut archived_samples,
                &mut cursor,
            );
            samples.extend((0..4).map(|offset| (8 + batch * 4 + offset) as f32));
            observed_wrapped_storage |= !samples.as_slices().1.is_empty();

            assert_eq!(samples.len(), 8);
            assert_eq!(samples.capacity(), initial_capacity);
            assert_eq!(cursor.source_sample_offset, 0);
        }

        let complete = take_complete_captured_samples(&mut archived_samples, &mut samples);
        let expected = (0..264).map(|value| value as f32).collect::<Vec<_>>();
        assert!(observed_wrapped_storage);
        assert_eq!(complete, expected);
        assert!(samples.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn wrapped_ring_queue_level_and_waveform_match_contiguous_samples() {
        let contiguous = vec![
            0.1_f32, -0.2, 0.3, -0.4, 0.9, -0.8, -0.7, 0.6, 0.2, -0.1, 0.5, -0.3,
        ];
        let split = 5;
        let mut wrapped = contiguous[split..]
            .iter()
            .chain(contiguous[..split].iter())
            .copied()
            .collect::<VecDeque<_>>();
        wrapped.rotate_right(split);
        assert!(
            !wrapped.as_slices().1.is_empty(),
            "test queue must cross the ring boundary"
        );

        let expected_level = summarize_recent_interleaved_audio_level(&contiguous, 2, 4)
            .expect("summarize contiguous level");
        let actual_level = summarize_recent_interleaved_audio_level_queue(&wrapped, 2, 4)
            .expect("summarize wrapped level");
        assert_eq!(actual_level, expected_level);

        let mut expected_waveform = [0.0_f32; 5];
        summarize_recent_interleaved_audio_waveform_into(&contiguous, 2, 6, &mut expected_waveform)
            .expect("summarize contiguous waveform");
        let mut actual_waveform = [0.0_f32; 5];
        summarize_recent_interleaved_audio_waveform_queue_into(
            &wrapped,
            2,
            6,
            &mut actual_waveform,
        )
        .expect("summarize wrapped waveform");
        assert_eq!(actual_waveform, expected_waveform);
        assert!(
            !wrapped.as_slices().1.is_empty(),
            "level and waveform analysis must not make the ring contiguous"
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_complete_sample_take_releases_locks_before_signal_analysis() {
        use std::sync::{Arc, Mutex};

        let archived_samples = Arc::new(Mutex::new(vec![0.1_f32, 0.2]));
        let live_samples = Arc::new(Mutex::new(
            [0.3_f32, -0.5].into_iter().collect::<VecDeque<_>>(),
        ));

        let complete = take_complete_native_windows_samples(&archived_samples, &live_samples)
            .expect("take complete native samples");

        let archived_guard = archived_samples
            .try_lock()
            .expect("archived samples lock must be released before analysis");
        let live_guard = live_samples
            .try_lock()
            .expect("live samples lock must be released before analysis");
        assert!(archived_guard.is_empty());
        assert!(live_guard.is_empty());
        drop(live_guard);
        drop(archived_guard);

        assert_eq!(complete, vec![0.1, 0.2, 0.3, -0.5]);
        assert_eq!(captured_audio_peak_abs(&complete), 0.5);
    }

    #[cfg(windows)]
    #[test]
    fn native_input_callback_queues_samples_without_waiting_for_the_consumer_buffer() {
        use crossbeam_queue::ArrayQueue;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;

        let consumer_buffer = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let consumer_guard = consumer_buffer.lock().expect("hold consumer buffer");
        let capture_queue = Arc::new(ArrayQueue::new(8));
        let overflowed_samples = Arc::new(AtomicUsize::new(0));
        let sample_limit = NativeInputCaptureLimit {
            max_samples: usize::MAX,
            total_captured_samples: Arc::new(AtomicUsize::new(0)),
        };
        let callback_queue = Arc::clone(&capture_queue);
        let callback_overflow = Arc::clone(&overflowed_samples);
        let callback_limit = sample_limit.clone();
        let (started_sender, started_receiver) = mpsc::channel();
        let (done_sender, done_receiver) = mpsc::channel();
        let callback = std::thread::spawn(move || {
            started_sender.send(()).expect("announce callback start");
            append_native_input_samples(
                &[0.25_f32, -0.5_f32],
                &callback_queue,
                &callback_limit,
                &callback_overflow,
            );
            done_sender.send(()).expect("announce callback completion");
        });

        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("callback should start");
        done_receiver
            .recv_timeout(Duration::from_millis(25))
            .expect("callback must not wait for the consumer buffer lock");
        drop(consumer_guard);
        callback.join().expect("callback thread should finish");

        assert_eq!(capture_queue.pop(), Some(0.25));
        assert_eq!(capture_queue.pop(), Some(-0.5));
        assert_eq!(capture_queue.pop(), None);
        assert_eq!(
            sample_limit.total_captured_samples.load(Ordering::Relaxed),
            2
        );
        assert_eq!(overflowed_samples.load(Ordering::Relaxed), 0);
    }

    #[cfg(windows)]
    #[test]
    fn native_input_callback_reports_queue_overflow_without_blocking() {
        use crossbeam_queue::ArrayQueue;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let capture_queue = ArrayQueue::new(1);
        let overflowed_samples = AtomicUsize::new(0);
        let sample_limit = NativeInputCaptureLimit {
            max_samples: usize::MAX,
            total_captured_samples: Arc::new(AtomicUsize::new(0)),
        };

        append_native_input_samples(
            &[0.25_f32, -0.5_f32],
            &capture_queue,
            &sample_limit,
            &overflowed_samples,
        );

        assert_eq!(capture_queue.pop(), Some(0.25));
        assert_eq!(capture_queue.pop(), None);
        assert_eq!(
            sample_limit.total_captured_samples.load(Ordering::Relaxed),
            1
        );
        assert_eq!(overflowed_samples.load(Ordering::Relaxed), 1);
    }

    #[cfg(windows)]
    #[test]
    fn native_capture_collector_drains_and_converts_every_queued_sample() {
        use crossbeam_queue::ArrayQueue;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let capture_queue = Arc::new(ArrayQueue::new(16));
        let samples = Arc::new(Mutex::new(VecDeque::<f32>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let overflowed_samples = AtomicUsize::new(0);
        let sample_limit = NativeInputCaptureLimit {
            max_samples: usize::MAX,
            total_captured_samples: Arc::new(AtomicUsize::new(0)),
        };
        let collector = spawn_native_capture_collector(
            Arc::clone(&capture_queue),
            Arc::clone(&samples),
            Arc::clone(&stop),
            16_000,
            2,
            WavSettings::mono_16khz(),
        );

        append_native_input_samples(
            &[0.25_f32, 0.25_f32, -0.5_f32, -0.5_f32],
            &capture_queue,
            &sample_limit,
            &overflowed_samples,
        );
        stop.store(true, Ordering::Release);
        collector
            .expect("spawn native capture collector")
            .join()
            .expect("collector thread should not panic")
            .expect("collector should finish conversion");

        assert_eq!(
            samples
                .lock()
                .expect("consumer buffer")
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![0.25, -0.5]
        );
        assert_eq!(overflowed_samples.load(Ordering::Relaxed), 0);
    }

    #[cfg(windows)]
    #[test]
    fn hud_waveform_skips_a_frame_instead_of_blocking_on_a_contended_capture_buffer() {
        use std::sync::{mpsc, Arc, Mutex};
        use std::time::Duration;

        let samples = Arc::new(Mutex::new(VecDeque::from([0.25_f32, -0.5_f32])));
        let capture_guard = samples.lock().expect("hold capture buffer");
        let source = RecordingPcmSource {
            backend: RecordingPcmSourceBackend::NativeWindows {
                wav_settings: WavSettings::mono_16khz(),
                sample_rate_hz: 16_000,
                channels: 1,
                samples: Arc::clone(&samples),
                archived_samples: Arc::new(Mutex::new(Vec::new())),
            },
        };
        let (started_sender, started_receiver) = mpsc::channel();
        let (done_sender, done_receiver) = mpsc::channel();
        let hud = std::thread::spawn(move || {
            started_sender.send(()).expect("announce hud start");
            let mut waveform = [1.0_f32; 4];
            source
                .current_waveform_into(&mut waveform)
                .expect("contended waveform read must not fail");
            done_sender.send(waveform).expect("announce hud completion");
        });

        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("hud should start");
        let waveform = done_receiver
            .recv_timeout(Duration::from_millis(25))
            .expect("hud waveform must return immediately when the capture buffer is locked");
        drop(capture_guard);
        hud.join().expect("hud thread should finish");
        assert_eq!(
            waveform, [1.0; 4],
            "a contended HUD frame must keep the previous waveform instead of blocking"
        );
    }

    #[test]
    fn native_capture_limit_uses_total_samples_after_streaming_buffer_compaction() {
        assert_eq!(native_input_sample_count_to_append(4, 3, 5), 2);
        assert_eq!(native_input_sample_count_to_append(4, 5, 5), 0);
        assert_eq!(native_input_sample_count_to_append(4, 3, usize::MAX), 4);
    }

    #[test]
    fn streaming_pcm_chunks_are_bounded_to_eighty_milliseconds() {
        assert_eq!(streaming_pcm_chunk_sample_count(16_000, 1), 1_280);
        assert_eq!(streaming_pcm_chunk_sample_count(48_000, 2), 7_680);
    }

    #[test]
    fn same_rate_mono_pcm_encoding_preserves_samples_and_expands_target_channels() {
        let source = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 1,
            samples: vec![0.0, 0.5, -0.5, 1.0],
        };
        let bytes = encode_captured_pcm_bytes(
            &source,
            WavSettings {
                sample_rate_hz: 16_000,
                channels: 2,
            },
        )
        .expect("encode same-rate mono PCM");
        let samples = bytes
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
            .collect::<Vec<_>>();

        assert_eq!(
            samples,
            vec![0, 0, 16_384, 16_384, -16_384, -16_384, 32_767, 32_767]
        );
    }

    #[test]
    fn phase_safe_channel_energy_scratch_resets_and_reuses_capacity() {
        let mut channel_energy_scratch = Vec::new();
        let four_channel = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 4,
            samples: vec![0.1, -0.2, 0.3, -0.8, -0.1, 0.2, -0.3, 0.8],
        };
        assert_eq!(phase_safe_downmix_channel(&four_channel), Some(3));
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(&four_channel, &mut channel_energy_scratch,),
            Some(3)
        );
        assert_eq!(channel_energy_scratch.len(), 4);
        let scratch_pointer = channel_energy_scratch.as_ptr();
        let scratch_capacity = channel_energy_scratch.capacity();

        let strongest_left = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 2,
            samples: vec![0.9, -0.1, -0.8, 0.1],
        };
        assert_eq!(phase_safe_downmix_channel(&strongest_left), Some(0));
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(
                &strongest_left,
                &mut channel_energy_scratch,
            ),
            Some(0)
        );
        assert_eq!(channel_energy_scratch.len(), 2);
        assert_eq!(channel_energy_scratch.as_ptr(), scratch_pointer);
        assert_eq!(channel_energy_scratch.capacity(), scratch_capacity);

        let anti_phase_tie = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 2,
            samples: vec![0.8, -0.8, -0.4, 0.4],
        };
        assert_eq!(phase_safe_downmix_channel(&anti_phase_tie), Some(1));
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(
                &anti_phase_tie,
                &mut channel_energy_scratch,
            ),
            Some(1)
        );
        assert_eq!(channel_energy_scratch.as_ptr(), scratch_pointer);
        assert_eq!(channel_energy_scratch.capacity(), scratch_capacity);

        let in_phase = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 2,
            samples: vec![0.5, 0.5, -0.25, -0.25],
        };
        assert_eq!(phase_safe_downmix_channel(&in_phase), None);
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(&in_phase, &mut channel_energy_scratch,),
            None
        );
        assert_eq!(channel_energy_scratch, [0.3125, 0.3125]);

        let mono = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 1,
            samples: vec![0.25, -0.25],
        };
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(&mono, &mut channel_energy_scratch),
            None
        );
        assert!(channel_energy_scratch.is_empty());
        assert_eq!(channel_energy_scratch.as_ptr(), scratch_pointer);
        assert_eq!(channel_energy_scratch.capacity(), scratch_capacity);

        let empty_stereo = CapturedAudioBuffer {
            sample_rate_hz: 16_000,
            channels: 2,
            samples: Vec::new(),
        };
        assert_eq!(
            phase_safe_downmix_channel_reusing_scratch(&empty_stereo, &mut channel_energy_scratch,),
            None
        );
        assert!(channel_energy_scratch.is_empty());
        assert_eq!(channel_energy_scratch.capacity(), scratch_capacity);
    }

    #[test]
    fn reusable_pcm_encoder_matches_allocating_encoder_across_audio_formats() {
        let cases = [
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 16_000,
                    channels: 1,
                    samples: vec![0.0, 0.5, -0.5, 1.0],
                },
                WavSettings {
                    sample_rate_hz: 16_000,
                    channels: 2,
                },
            ),
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 16_000,
                    channels: 2,
                    samples: vec![0.25, 0.75, -0.25, -0.75],
                },
                WavSettings::mono_16khz(),
            ),
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 16_000,
                    channels: 2,
                    samples: vec![0.5, -0.5, -0.25, 0.25],
                },
                WavSettings::mono_16khz(),
            ),
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 16_000,
                    channels: 4,
                    samples: vec![0.1, -0.2, 0.3, -0.8, -0.1, 0.2, -0.3, 0.8],
                },
                WavSettings::mono_16khz(),
            ),
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 48_000,
                    channels: 2,
                    samples: (0..192)
                        .map(|index| ((index % 19) as f32 - 9.0) / 9.0)
                        .collect(),
                },
                WavSettings::mono_16khz(),
            ),
            (
                CapturedAudioBuffer {
                    sample_rate_hz: 44_100,
                    channels: 1,
                    samples: (0..147)
                        .map(|index| ((index % 13) as f32 - 6.0) / 6.0)
                        .collect(),
                },
                WavSettings::mono_16khz(),
            ),
        ];

        for (source, settings) in cases {
            let expected = encode_captured_pcm_bytes(&source, settings)
                .expect("allocating PCM encoder should succeed");
            let mut optimized_source = source.clone();
            let mut resampled_scratch = Vec::new();
            let mut channel_energy_scratch = Vec::new();
            let actual = encode_captured_pcm_bytes_reusing_scratch(
                &mut optimized_source,
                settings,
                &mut resampled_scratch,
                &mut channel_energy_scratch,
            )
            .expect("reusable PCM encoder should succeed");

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn reusable_pcm_encoder_retains_resample_capacity_for_equal_and_smaller_chunks() {
        let settings = WavSettings::mono_16khz();
        let make_source = |frames: usize, offset: usize| CapturedAudioBuffer {
            sample_rate_hz: 48_000,
            channels: 2,
            samples: (0..frames * 2)
                .map(|index| (((index + offset) % 23) as f32 - 11.0) / 11.0)
                .collect(),
        };
        let mut resampled_scratch = Vec::new();
        let mut channel_energy_scratch = Vec::new();

        let mut first_source = make_source(96, 0);
        let first_expected = encode_captured_pcm_bytes(&first_source, settings)
            .expect("encode first reference chunk");
        let first_actual = encode_captured_pcm_bytes_reusing_scratch(
            &mut first_source,
            settings,
            &mut resampled_scratch,
            &mut channel_energy_scratch,
        )
        .expect("encode first reusable chunk");
        assert_eq!(first_actual, first_expected);
        assert_eq!(first_source.samples.len(), 96);
        let scratch_pointer = resampled_scratch.as_ptr();
        let scratch_capacity = resampled_scratch.capacity();
        let energy_pointer = channel_energy_scratch.as_ptr();
        let energy_capacity = channel_energy_scratch.capacity();

        let mut equal_source = make_source(96, 19);
        let equal_expected = encode_captured_pcm_bytes(&equal_source, settings)
            .expect("encode equal-size reference chunk");
        let equal_actual = encode_captured_pcm_bytes_reusing_scratch(
            &mut equal_source,
            settings,
            &mut resampled_scratch,
            &mut channel_energy_scratch,
        )
        .expect("encode equal-size reusable chunk");
        assert_eq!(equal_actual, equal_expected);
        assert_eq!(resampled_scratch.as_ptr(), scratch_pointer);
        assert_eq!(resampled_scratch.capacity(), scratch_capacity);
        assert_eq!(channel_energy_scratch.as_ptr(), energy_pointer);
        assert_eq!(channel_energy_scratch.capacity(), energy_capacity);

        let mut smaller_source = make_source(24, 3);
        let smaller_expected = encode_captured_pcm_bytes(&smaller_source, settings)
            .expect("encode smaller reference chunk");
        let smaller_actual = encode_captured_pcm_bytes_reusing_scratch(
            &mut smaller_source,
            settings,
            &mut resampled_scratch,
            &mut channel_energy_scratch,
        )
        .expect("encode smaller reusable chunk");
        assert_eq!(smaller_actual, smaller_expected);
        assert_eq!(resampled_scratch.as_ptr(), scratch_pointer);
        assert_eq!(resampled_scratch.capacity(), scratch_capacity);
        assert_eq!(channel_energy_scratch.as_ptr(), energy_pointer);
        assert_eq!(channel_energy_scratch.capacity(), energy_capacity);
    }

    #[test]
    fn silent_pcm_drain_reuses_recycled_byte_buffer_without_changing_chunks() {
        let settings = WavSettings::mono_16khz();
        let mut cursor = RecordingPcmCursor::default();

        let first = drain_silent_pcm_chunk(&mut cursor, settings, 2_000)
            .expect("drain first silent PCM chunk")
            .expect("first silent chunk");
        assert_eq!(first.sequence, 0);
        assert_eq!(first.bytes.len(), 2_560);
        assert!(first.bytes.iter().all(|byte| *byte == 0));
        let bytes_pointer = first.bytes.as_ptr();
        let bytes_capacity = first.bytes.capacity();
        cursor.recycle_pcm_chunk(first);
        assert_eq!(cursor.pcm_bytes_scratch.as_ptr(), bytes_pointer);
        assert_eq!(cursor.pcm_bytes_scratch.capacity(), bytes_capacity);

        let second = drain_silent_pcm_chunk(&mut cursor, settings, 2_000)
            .expect("drain second silent PCM chunk")
            .expect("second silent chunk");
        assert_eq!(second.sequence, 1);
        assert_eq!(second.bytes.len(), 1_440);
        assert!(second.bytes.iter().all(|byte| *byte == 0));
        assert_eq!(second.bytes.as_ptr(), bytes_pointer);
        assert_eq!(second.bytes.capacity(), bytes_capacity);
        cursor.recycle_pcm_chunk(second);

        assert!(drain_silent_pcm_chunk(&mut cursor, settings, 2_000)
            .expect("drain exhausted silent PCM")
            .is_none());
        assert_eq!(cursor.pcm_bytes_scratch.as_ptr(), bytes_pointer);
        assert_eq!(cursor.pcm_bytes_scratch.capacity(), bytes_capacity);
    }

    #[test]
    fn recording_pcm_cursor_clone_preserves_position_without_copying_scratch_samples() {
        let mut cursor = RecordingPcmCursor {
            source_sample_offset: 1_280,
            next_sequence: 4,
            ..RecordingPcmCursor::default()
        };
        cursor.sample_scratch.extend_from_slice(&[0.25, -0.5]);
        cursor.resampled_scratch.extend_from_slice(&[0.125, -0.25]);
        cursor.channel_energy_scratch.extend_from_slice(&[1.0, 2.0]);
        cursor.pcm_bytes_scratch.extend_from_slice(&[1, 2, 3, 4]);

        let cloned = cursor.clone();

        assert_eq!(cloned, cursor);
        assert_eq!(cloned.source_sample_offset, 1_280);
        assert_eq!(cloned.next_sequence, 4);
        assert!(cloned.sample_scratch.is_empty());
        assert!(cloned.resampled_scratch.is_empty());
        assert!(cloned.channel_energy_scratch.is_empty());
        assert!(cloned.pcm_bytes_scratch.is_empty());
    }

    #[test]
    fn recording_pcm_cursor_checkpoint_restores_position_without_dropping_scratch() {
        let mut cursor = RecordingPcmCursor {
            source_sample_offset: 1_280,
            next_sequence: 4,
            ..RecordingPcmCursor::default()
        };
        cursor.sample_scratch.extend_from_slice(&[0.25, -0.5]);
        cursor.pcm_bytes_scratch.extend_from_slice(&[1, 2, 3, 4]);
        let scratch_pointer = cursor.pcm_bytes_scratch.as_ptr();
        let scratch_capacity = cursor.pcm_bytes_scratch.capacity();

        let checkpoint = cursor.checkpoint();
        cursor.source_sample_offset = 2_560;
        cursor.next_sequence = 9;
        cursor.restore_checkpoint(checkpoint);

        assert_eq!(cursor.source_sample_offset, 1_280);
        assert_eq!(cursor.next_sequence, 4);
        assert_eq!(cursor.pcm_bytes_scratch.as_ptr(), scratch_pointer);
        assert_eq!(cursor.pcm_bytes_scratch.capacity(), scratch_capacity);
        assert_eq!(cursor.sample_scratch, vec![0.25, -0.5]);
    }

    #[cfg(windows)]
    #[test]
    fn native_pcm_drain_reuses_sample_scratch_without_changing_chunk_bytes() {
        use std::sync::{Arc, Mutex};

        let source_samples = (0..2_600)
            .map(|index| ((index % 17) as f32 - 8.0) / 8.0)
            .collect::<Vec<_>>();
        let samples = Arc::new(Mutex::new(
            source_samples.iter().copied().collect::<VecDeque<_>>(),
        ));
        let settings = WavSettings {
            sample_rate_hz: 16_000,
            channels: 1,
        };
        let mut cursor = RecordingPcmCursor::default();

        let first = drain_native_windows_pcm_chunk(&samples, settings, 16_000, 1, &mut cursor)
            .expect("drain first native PCM chunk")
            .expect("first chunk");
        let scratch_pointer = cursor.sample_scratch.as_ptr();
        let scratch_capacity = cursor.sample_scratch.capacity();
        let expected_first = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 16_000,
                channels: 1,
                samples: source_samples[..1_280].to_vec(),
            },
            settings,
        )
        .expect("encode expected first chunk");

        assert_eq!(first.sequence, 0);
        assert_eq!(first.bytes, expected_first);
        assert_eq!(cursor.source_sample_offset, 1_280);

        let second = drain_native_windows_pcm_chunk(&samples, settings, 16_000, 1, &mut cursor)
            .expect("drain second native PCM chunk")
            .expect("second chunk");
        let expected_second = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 16_000,
                channels: 1,
                samples: source_samples[1_280..2_560].to_vec(),
            },
            settings,
        )
        .expect("encode expected second chunk");
        assert_eq!(cursor.sample_scratch.as_ptr(), scratch_pointer);
        assert_eq!(cursor.sample_scratch.capacity(), scratch_capacity);
        assert_eq!(second.sequence, 1);
        assert_eq!(second.bytes, expected_second);
        assert_eq!(cursor.source_sample_offset, 2_560);

        let final_chunk =
            drain_native_windows_pcm_chunk(&samples, settings, 16_000, 1, &mut cursor)
                .expect("drain final native PCM chunk")
                .expect("final chunk");
        let expected_final = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 16_000,
                channels: 1,
                samples: source_samples[2_560..].to_vec(),
            },
            settings,
        )
        .expect("encode expected final chunk");

        assert_eq!(cursor.sample_scratch.as_ptr(), scratch_pointer);
        assert_eq!(cursor.sample_scratch.capacity(), scratch_capacity);
        assert_eq!(final_chunk.sequence, 2);
        assert_eq!(final_chunk.bytes, expected_final);
        assert_eq!(cursor.source_sample_offset, source_samples.len());
        assert!(
            drain_native_windows_pcm_chunk(&samples, settings, 16_000, 1, &mut cursor)
                .expect("drain exhausted native PCM")
                .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn native_pcm_drain_reads_wrapped_ring_storage_in_logical_order() {
        use std::sync::{Arc, Mutex};

        let source_samples = (0..2_600)
            .map(|index| ((index % 17) as f32 - 8.0) / 8.0)
            .collect::<Vec<_>>();
        let split = 731;
        let mut wrapped_samples = source_samples[split..]
            .iter()
            .chain(source_samples[..split].iter())
            .copied()
            .collect::<VecDeque<_>>();
        wrapped_samples.rotate_right(split);
        assert_eq!(
            wrapped_samples.iter().copied().collect::<Vec<_>>(),
            source_samples
        );
        assert!(
            !wrapped_samples.as_slices().1.is_empty(),
            "test queue must cross the ring boundary"
        );

        let samples = Arc::new(Mutex::new(wrapped_samples));
        let settings = WavSettings::mono_16khz();
        let mut cursor = RecordingPcmCursor::default();
        let first = drain_native_windows_pcm_chunk(&samples, settings, 16_000, 1, &mut cursor)
            .expect("drain wrapped native PCM chunk")
            .expect("wrapped chunk");
        let expected = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 16_000,
                channels: 1,
                samples: source_samples[..1_280].to_vec(),
            },
            settings,
        )
        .expect("encode wrapped queue reference chunk");

        assert_eq!(first.bytes, expected);
        assert_eq!(cursor.source_sample_offset, 1_280);
    }

    #[cfg(windows)]
    #[test]
    fn native_pcm_drain_reuses_sample_scratch_for_format_conversion() {
        use std::sync::{Arc, Mutex};

        let source_samples = (0..15_360)
            .map(|index| ((index % 29) as f32 - 14.0) / 14.0)
            .collect::<Vec<_>>();
        let samples = Arc::new(Mutex::new(
            source_samples.iter().copied().collect::<VecDeque<_>>(),
        ));
        let settings = WavSettings {
            sample_rate_hz: 16_000,
            channels: 1,
        };
        let mut cursor = RecordingPcmCursor::default();

        let first = drain_native_windows_pcm_chunk(&samples, settings, 48_000, 2, &mut cursor)
            .expect("drain first converted native PCM chunk")
            .expect("first converted chunk");
        let scratch_pointer = cursor.sample_scratch.as_ptr();
        let scratch_capacity = cursor.sample_scratch.capacity();
        let resampled_pointer = cursor.resampled_scratch.as_ptr();
        let resampled_capacity = cursor.resampled_scratch.capacity();
        let energy_pointer = cursor.channel_energy_scratch.as_ptr();
        let energy_capacity = cursor.channel_energy_scratch.capacity();
        let expected_first = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 48_000,
                channels: 2,
                samples: source_samples[..7_680].to_vec(),
            },
            settings,
        )
        .expect("encode expected first converted chunk");

        assert_eq!(first.sample_rate_hz, 16_000);
        assert_eq!(first.channels, 1);
        assert_eq!(first.bytes, expected_first);

        let second = drain_native_windows_pcm_chunk(&samples, settings, 48_000, 2, &mut cursor)
            .expect("drain second converted native PCM chunk")
            .expect("second converted chunk");
        let expected_second = encode_captured_pcm_bytes(
            &CapturedAudioBuffer {
                sample_rate_hz: 48_000,
                channels: 2,
                samples: source_samples[7_680..].to_vec(),
            },
            settings,
        )
        .expect("encode expected second converted chunk");

        assert_eq!(cursor.sample_scratch.as_ptr(), scratch_pointer);
        assert_eq!(cursor.sample_scratch.capacity(), scratch_capacity);
        assert_eq!(cursor.resampled_scratch.as_ptr(), resampled_pointer);
        assert_eq!(cursor.resampled_scratch.capacity(), resampled_capacity);
        assert_eq!(cursor.channel_energy_scratch.as_ptr(), energy_pointer);
        assert_eq!(cursor.channel_energy_scratch.capacity(), energy_capacity);
        assert_eq!(second.sequence, 1);
        assert_eq!(second.bytes, expected_second);
        assert_eq!(cursor.source_sample_offset, source_samples.len());
    }

    #[cfg(windows)]
    #[test]
    fn native_pcm_drain_rejects_zero_source_channels_without_advancing_cursor() {
        use std::sync::{Arc, Mutex};

        let samples = Arc::new(Mutex::new(
            [0.25_f32, -0.5_f32].into_iter().collect::<VecDeque<_>>(),
        ));
        let mut cursor = RecordingPcmCursor::default();

        let error = drain_native_windows_pcm_chunk(
            &samples,
            WavSettings::mono_16khz(),
            16_000,
            0,
            &mut cursor,
        )
        .expect_err("zero source channels must fail");

        assert!(
            error
                .to_string()
                .contains("captured audio channels must be greater than 0"),
            "error={error}"
        );
        assert_eq!(cursor, RecordingPcmCursor::default());
    }

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

    fn resample_to_target_rate(
        source_mono: &[f32],
        source_sample_rate_hz: u32,
        target_sample_rate_hz: u32,
    ) -> Vec<f32> {
        let target_frames = resampled_frame_count(
            source_mono.len(),
            source_sample_rate_hz,
            target_sample_rate_hz,
        )
        .expect("resampled frame count");
        resample_mono_to_len(
            source_mono,
            source_sample_rate_hz,
            target_sample_rate_hz,
            target_frames,
        )
    }

    #[test]
    fn resample_downsample_attenuates_energy_above_target_nyquist() {
        // A 12 kHz tone at 48 kHz is above the 8 kHz Nyquist of 16 kHz. Nearest
        // neighbour decimation folds it back to a strong 4 kHz alias; the
        // band-limited resampler must attenuate it toward silence instead.
        let source: Vec<f32> = (0..4_800)
            .map(|index| {
                (0.8 * (2.0 * std::f64::consts::PI * 12_000.0 * index as f64 / 48_000.0).sin())
                    as f32
            })
            .collect();

        let resampled = resample_to_target_rate(&source, 48_000, 16_000);

        assert_eq!(resampled.len(), source.len() / 3);
        let output_rms = captured_audio_rms(&resampled);
        assert!(
            output_rms < 0.3,
            "above-Nyquist tone must be attenuated, got rms {output_rms}"
        );
    }

    #[test]
    fn resample_preserves_low_frequency_tone_amplitude() {
        // A 1 kHz tone is well within the 16 kHz passband and must survive
        // downsampling with most of its amplitude intact.
        let source: Vec<f32> = (0..4_800)
            .map(|index| {
                (0.8 * (2.0 * std::f64::consts::PI * 1_000.0 * index as f64 / 48_000.0).sin())
                    as f32
            })
            .collect();

        let resampled = resample_to_target_rate(&source, 48_000, 16_000);

        let output_peak = captured_audio_peak_abs(&resampled);
        assert!(
            output_peak > 0.6,
            "in-band tone must be preserved, got peak {output_peak}"
        );
    }

    #[test]
    fn resample_non_integer_ratio_matches_frame_count_and_is_finite() {
        let source: Vec<f32> = (0..441).map(|index| (index as f32 / 441.0) - 0.5).collect();

        let resampled = resample_to_target_rate(&source, 44_100, 16_000);

        assert_eq!(resampled.len(), 160);
        assert!(resampled.iter().all(|sample| sample.is_finite()));
    }

    #[test]
    fn resample_same_rate_is_bit_identical_passthrough() {
        let source = vec![0.0, 0.9, -0.9, 0.42, -0.17, 0.5];

        let resampled = resample_to_target_rate(&source, 16_000, 16_000);

        assert_eq!(resampled, source);
    }

    #[test]
    fn resample_upsample_uses_linear_interpolation() {
        let source = vec![0.0, 1.0];

        let resampled = resample_mono_to_len(&source, 8_000, 16_000, 4);

        assert_eq!(resampled.len(), 4);
        assert!((resampled[0] - 0.0).abs() < 1e-6);
        assert!((resampled[1] - 0.5).abs() < 1e-6);
        assert!((resampled[2] - 1.0).abs() < 1e-6);
        assert!((resampled[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn normalized_capture_gain_lifts_quiet_valid_signal() {
        let gain = normalized_capture_gain(0.3);

        assert!((gain - 3.0).abs() < 1e-6, "gain={gain}");
    }

    #[test]
    fn normalized_capture_gain_preserves_weak_signal_below_reject_threshold() {
        assert_eq!(normalized_capture_gain(0.03), 1.0);
        assert_eq!(normalized_capture_gain(0.049), 1.0);
    }

    #[test]
    fn normalized_capture_gain_is_noop_for_hot_signal() {
        assert_eq!(normalized_capture_gain(0.9), 1.0);
        assert_eq!(normalized_capture_gain(0.99), 1.0);
    }

    #[test]
    fn normalized_capture_gain_respects_max_gain_cap() {
        let gain = normalized_capture_gain(0.1);

        assert!((gain - 4.0).abs() < 1e-6, "gain={gain}");
    }
}
