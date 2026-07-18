use std::path::PathBuf;
use std::sync::Mutex;
use talk_audio::{
    capture_audio, probe_audio_signal, probe_native_windows_audio_readiness, read_wav_info,
    start_recording, summarize_recent_audio_level, summarize_recent_audio_waveform,
    write_captured_wav, write_silent_wav, AudioArtifact, AudioCaptureRequest, AudioInputLevel,
    AudioPlan, AudioSignalProbeRequest, CapturedAudioBuffer, NativeReadinessStatus,
    RecordingPcmCursor, WavSettings,
};
use talk_core::AudioBackendMode;

static NATIVE_AUDIO_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn audio_plan_builds_session_wav_path() {
    let plan = AudioPlan::new(PathBuf::from(".runtime/talk/audio"), "session-1");
    let artifact = plan.artifact();

    assert_eq!(
        artifact,
        AudioArtifact::new(
            PathBuf::from(".runtime/talk/audio/session-1.wav"),
            "audio/wav"
        )
    );
}

#[test]
fn write_silent_wav_creates_readable_pcm_wav() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-audio-contract-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp audio dir");

    let artifact = AudioArtifact::new(dir.join("sample.wav"), "audio/wav");
    write_silent_wav(&artifact, WavSettings::mono_16khz(), 320).expect("write silent wav");

    let info = read_wav_info(&artifact).expect("read wav info");
    assert_eq!(info.sample_rate_hz, 16_000);
    assert_eq!(info.channels, 1);
    assert_eq!(info.bits_per_sample, 16);
    assert_eq!(info.duration_samples, 320);
}

#[test]
fn write_silent_wav_rejects_invalid_wav_settings() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-invalid-silent-settings-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let artifact = AudioArtifact::new(dir.join("invalid.wav"), "audio/wav");
    let error = write_silent_wav(
        &artifact,
        WavSettings {
            sample_rate_hz: 0,
            channels: 1,
        },
        320,
    )
    .expect_err("invalid silent wav settings must fail");

    assert!(
        error
            .to_string()
            .contains("wav sample_rate_hz must be greater than 0"),
        "error={error}"
    );
    assert!(
        !artifact.path.exists(),
        "invalid silent wav settings must not create {}",
        artifact.path.display()
    );
}

#[test]
fn capture_audio_uses_silent_backend_for_readable_wav_artifacts() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-audio-silent-capture-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: dir.clone(),
        session_id: "silent-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };

    let artifact = capture_audio(&request).expect("capture silent audio");

    assert_eq!(artifact.path, dir.join("silent-session.wav"));
    let info = read_wav_info(&artifact).expect("read wav info");
    assert_eq!(info.sample_rate_hz, 16_000);
    assert_eq!(info.channels, 1);
    assert_eq!(info.duration_samples, 320);
}

#[test]
fn write_captured_wav_downmixes_and_resamples_to_requested_pcm_wav() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-captured-conversion-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let artifact = AudioArtifact::new(dir.join("captured.wav"), "audio/wav");
    let source = CapturedAudioBuffer {
        sample_rate_hz: 48_000,
        channels: 2,
        // 6 stereo frames at 48kHz. Downsampling to 16kHz should keep 2 mono samples.
        samples: vec![
            0.25, 0.75, // mono 0.50
            0.20, 0.20, // skipped by 3:1 downsample
            0.10, 0.10, // skipped by 3:1 downsample
            -0.25, -0.75, // mono -0.50
            0.30, 0.30, // skipped by 3:1 downsample
            0.40, 0.40, // skipped by 3:1 downsample
        ],
    };

    write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
        .expect("write converted captured wav");

    let info = read_wav_info(&artifact).expect("read wav info");
    assert_eq!(info.sample_rate_hz, 16_000);
    assert_eq!(info.channels, 1);
    assert_eq!(info.bits_per_sample, 16);
    assert_eq!(info.duration_samples, 2);
}

#[test]
fn write_captured_wav_rejects_non_frame_aligned_source_samples() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-captured-unaligned-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let artifact = AudioArtifact::new(dir.join("captured.wav"), "audio/wav");
    let source = CapturedAudioBuffer {
        sample_rate_hz: 48_000,
        channels: 2,
        samples: vec![0.25, 0.75, 0.10],
    };

    let error = write_captured_wav(&artifact, &source, WavSettings::mono_16khz())
        .expect_err("non-frame-aligned captured audio must fail");

    assert!(
        error
            .to_string()
            .contains("captured audio samples must be frame-aligned with channels"),
        "error={error}"
    );
    assert!(
        !artifact.path.exists(),
        "unaligned captured audio must not create {}",
        artifact.path.display()
    );
}

#[test]
fn capture_audio_native_windows_backend_disabled_is_not_silent_fallback() {
    let _guard = NATIVE_AUDIO_ENV_LOCK
        .lock()
        .expect("native audio env mutex");
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-audio-native-disabled-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let previous = std::env::var_os("TALK_DISABLE_NATIVE_AUDIO");
    std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", "1");

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::NativeWindows,
        temp_dir: dir.clone(),
        session_id: "native-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };
    let error = capture_audio(&request).expect_err("native audio should fail when disabled");

    match previous {
        Some(value) => std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", value),
        None => std::env::remove_var("TALK_DISABLE_NATIVE_AUDIO"),
    }

    assert!(error.to_string().contains("native_windows"));
    let wav_path = dir.join("native-session.wav");
    assert!(
        !wav_path.exists(),
        "native audio failure must not create silent wav artifact at {}",
        wav_path.display()
    );
}

#[test]
fn start_recording_silent_backend_writes_readable_wav_when_finished() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-audio-live-silent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: dir.clone(),
        session_id: "live-silent-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };

    let recording = start_recording(&request).expect("start silent recording");
    let artifact = recording.finish().expect("finish silent recording");

    assert_eq!(artifact.path, dir.join("live-silent-session.wav"));
    let info = read_wav_info(&artifact).expect("read live silent wav info");
    assert_eq!(info.sample_rate_hz, 16_000);
    assert_eq!(info.channels, 1);
    assert_eq!(info.duration_samples, 320);
}

#[test]
fn summarize_recent_audio_level_uses_only_the_latest_frames() {
    let source = CapturedAudioBuffer {
        sample_rate_hz: 16_000,
        channels: 1,
        samples: vec![0.0, 0.0, 0.2, -0.4, 0.1, 0.5],
    };

    let level = summarize_recent_audio_level(&source, 3).expect("summarize recent audio level");

    assert_eq!(
        level,
        AudioInputLevel {
            peak: 0.5,
            rms: (0.14_f32).sqrt(),
        }
    );
}

#[test]
fn summarize_recent_audio_waveform_uses_latest_frames_and_preserves_bucket_shape() {
    let source = CapturedAudioBuffer {
        sample_rate_hz: 16_000,
        channels: 1,
        samples: vec![0.0, 0.0, 0.2, -0.5, 0.1, 0.9],
    };

    let waveform =
        summarize_recent_audio_waveform(&source, 4, 4).expect("summarize recent audio waveform");

    assert_eq!(waveform, vec![0.2, 0.5, 0.1, 0.9]);
}

#[test]
fn start_recording_silent_backend_reports_zero_live_level_before_finish() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-live-level-silent-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: dir,
        session_id: "silent-live-level".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };

    let recording = start_recording(&request).expect("start silent recording");
    let level = recording.current_level().expect("silent live level");

    assert_eq!(
        level,
        AudioInputLevel {
            peak: 0.0,
            rms: 0.0
        }
    );
    let waveform = recording.current_waveform(6).expect("silent live waveform");
    assert_eq!(waveform, vec![0.0; 6]);
}

#[test]
fn recording_session_drains_raw_pcm_chunks_without_waiting_for_finish() {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-streaming-pcm-silent-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: dir,
        session_id: "silent-streaming-pcm".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };

    let recording = start_recording(&request).expect("start silent recording");
    let mut cursor = RecordingPcmCursor::default();
    let chunk = recording
        .drain_pcm_chunk(&mut cursor)
        .expect("drain silent PCM chunk")
        .expect("silent backend should expose its available PCM once");

    assert_eq!(chunk.sequence, 0);
    assert_eq!(chunk.sample_rate_hz, 16_000);
    assert_eq!(chunk.channels, 1);
    assert_eq!(chunk.bytes.len(), 320 * 2);
    assert!(chunk.bytes.iter().all(|byte| *byte == 0));
    assert!(recording
        .drain_pcm_chunk(&mut cursor)
        .expect("second drain should succeed")
        .is_none());
}

#[test]
fn start_recording_native_windows_backend_disabled_is_not_silent_fallback() {
    let _guard = NATIVE_AUDIO_ENV_LOCK
        .lock()
        .expect("native audio env mutex");
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "talk-audio-live-native-disabled-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);

    let previous = std::env::var_os("TALK_DISABLE_NATIVE_AUDIO");
    std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", "1");

    let request = AudioCaptureRequest {
        backend: AudioBackendMode::NativeWindows,
        temp_dir: dir.clone(),
        session_id: "live-native-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        max_recording_seconds: 60,
        silent_samples: 320,
    };
    let error = start_recording(&request).expect_err("disabled native recording must fail");

    match previous {
        Some(value) => std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", value),
        None => std::env::remove_var("TALK_DISABLE_NATIVE_AUDIO"),
    }

    assert!(error.to_string().contains("native_windows"));
    let wav_path = dir.join("live-native-session.wav");
    assert!(
        !wav_path.exists(),
        "disabled native live recording must not create silent wav artifact at {}",
        wav_path.display()
    );
}

#[test]
fn native_windows_audio_readiness_reports_disabled_env_before_device_probe() {
    let _guard = NATIVE_AUDIO_ENV_LOCK
        .lock()
        .expect("native audio env mutex");
    let previous = std::env::var_os("TALK_DISABLE_NATIVE_AUDIO");
    std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", "1");

    let readiness = probe_native_windows_audio_readiness();

    match previous {
        Some(value) => std::env::set_var("TALK_DISABLE_NATIVE_AUDIO", value),
        None => std::env::remove_var("TALK_DISABLE_NATIVE_AUDIO"),
    }

    assert_eq!(readiness.status, NativeReadinessStatus::Unavailable);
    assert_eq!(
        readiness.reason.as_deref(),
        Some("native_windows audio backend disabled by TALK_DISABLE_NATIVE_AUDIO")
    );
    assert_eq!(readiness.device_name, None);
    assert_eq!(readiness.default_sample_rate_hz, None);
    assert_eq!(readiness.default_channels, None);
    assert_eq!(readiness.sample_format, None);
}

#[test]
fn probe_audio_signal_reports_zero_metrics_for_silent_backend() {
    let mut dir = std::env::temp_dir();
    dir.push(format!("talk-audio-probe-silent-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let probe = probe_audio_signal(&AudioSignalProbeRequest {
        backend: AudioBackendMode::Silent,
        temp_dir: dir.clone(),
        session_id: "probe-silent-session".to_string(),
        input_device: None,
        wav_settings: WavSettings::mono_16khz(),
        capture_seconds: 2,
    })
    .expect("probe silent audio");

    assert_eq!(probe.artifact.path, dir.join("probe-silent-session.wav"));
    assert_eq!(probe.signal.sample_rate_hz, 16_000);
    assert_eq!(probe.signal.channels, 1);
    assert_eq!(probe.signal.duration_seconds, 2.0);
    assert_eq!(probe.signal.peak, 0.0);
    assert_eq!(probe.signal.rms, 0.0);
    assert!(probe.signal.silent);
}
