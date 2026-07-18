use talk_core::{
    AudioBackendMode, ClipboardBackendMode, DesktopPasteShortcut, OpenAiTranscriptionTransport,
    OutputMode, ProviderKind, SpeculativeLocalAsrDaemonMode, SpeculativeSherpaOnlineModelFamily,
    TalkConfig, TriggerMode, VoiceMode,
};

fn read_example_config(name: &str) -> String {
    std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join(name),
    )
    .unwrap_or_else(|error| panic!("read example config {name}: {error}"))
}

#[test]
fn parses_dev_config_defaults() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "clipboard_paste"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid config");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.trigger.toggle_shortcut, "Ctrl+Alt+Space");
    assert_eq!(config.audio.max_recording_seconds, 60);
    assert_eq!(config.audio.sample_rate_hz, 16000);
    assert_eq!(config.audio.channels, 1);
    assert_eq!(config.audio.backend, AudioBackendMode::Silent);
    assert_eq!(config.provider.kind, ProviderKind::Mock);
    assert_eq!(
        config.provider.mock_transcript.as_deref(),
        Some("hello from talk mock")
    );
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert!(config.output.restore_clipboard);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::Fallback
    );
    assert_eq!(config.default_voice_mode(), VoiceMode::Smart);
}

#[test]
fn parses_disabled_speculative_dictation_config() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = false
local_asr = "mock"
cloud_correction = "disabled"
max_patch_age_ms = 2000
max_auto_patch_edit_ratio = 0.25
"#;

    let config = TalkConfig::from_toml_str(raw).expect("speculative config should parse");

    assert!(!config.speculative.enabled);
    assert_eq!(config.speculative.max_patch_age_ms, 2000);
}

#[test]
fn parses_speculative_external_local_asr_command() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = true
local_asr = "external_command"
cloud_correction = "disabled"
external_asr_command = "local-asr.exe --jsonl"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("external local ASR config should parse");

    assert_eq!(config.speculative.local_asr, "external_command");
    assert_eq!(
        config.speculative.external_asr_command.as_deref(),
        Some("local-asr.exe --jsonl")
    );
}

#[test]
fn parses_speculative_streaming_service_config() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "provider_text_processor"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
"#;

    let config = TalkConfig::from_toml_str(raw).expect("streaming service config should parse");
    let service = config
        .speculative
        .streaming_service
        .as_ref()
        .expect("streaming service settings should be present");

    assert_eq!(config.speculative.local_asr, "streaming_service");
    assert_eq!(service.endpoint, "ws://127.0.0.1:53171/asr");
    assert_eq!(service.sample_rate_hz, 16_000);
    assert_eq!(service.channels, 1);
    assert_eq!(service.connect_timeout_ms, 1_000);
    assert_eq!(service.idle_timeout_ms, 3_000);
    assert_eq!(service.final_timeout_ms, 7_000);
}

#[test]
fn parses_speculative_streaming_service_local_daemon_sherpa_config() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "provider_text_processor"

[speculative.streaming_service]
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000

[speculative.streaming_service.local_daemon]
mode = "sherpa-online"
model_family = "transducer"
model = "zipformer-bilingual-zh-en"
tokens = "C:/models/zipformer/tokens.txt"
encoder = "C:/models/zipformer/encoder.onnx"
decoder = "C:/models/zipformer/decoder.onnx"
joiner = "C:/models/zipformer/joiner.onnx"
provider = "cpu"
num_threads = 4
sample_rate_hz = 16000
decoding_method = "modified_beam_search"
"#;

    let config =
        TalkConfig::from_toml_str(raw).expect("streaming service daemon config should parse");
    let daemon = config
        .speculative
        .streaming_service
        .as_ref()
        .and_then(|service| service.local_daemon.as_ref())
        .expect("local daemon settings should be present");

    assert_eq!(daemon.mode, SpeculativeLocalAsrDaemonMode::SherpaOnline);
    assert_eq!(
        daemon.model_family,
        SpeculativeSherpaOnlineModelFamily::Transducer
    );
    assert_eq!(daemon.model.as_deref(), Some("zipformer-bilingual-zh-en"));
    assert_eq!(
        daemon.tokens.as_deref().unwrap().to_string_lossy(),
        "C:/models/zipformer/tokens.txt"
    );
    assert_eq!(
        daemon.encoder.as_deref().unwrap().to_string_lossy(),
        "C:/models/zipformer/encoder.onnx"
    );
    assert_eq!(
        daemon.decoder.as_deref().unwrap().to_string_lossy(),
        "C:/models/zipformer/decoder.onnx"
    );
    assert_eq!(
        daemon.joiner.as_deref().unwrap().to_string_lossy(),
        "C:/models/zipformer/joiner.onnx"
    );
    assert_eq!(daemon.provider.as_deref(), Some("cpu"));
    assert_eq!(daemon.num_threads, Some(4));
    assert_eq!(daemon.sample_rate_hz, Some(16_000));
    assert_eq!(
        daemon.decoding_method.as_deref(),
        Some("modified_beam_search")
    );
}

#[test]
fn rejects_speculative_streaming_service_sherpa_daemon_missing_model_paths() {
    let raw = speculative_streaming_service_config_with(
        r#"
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000

[speculative.streaming_service.local_daemon]
mode = "sherpa-online"
model_family = "transducer"
encoder = "C:/models/zipformer/encoder.onnx"
"#,
    );

    let error =
        TalkConfig::from_toml_str(&raw).expect_err("sherpa-online daemon must require model files");
    let message = error.to_string();

    assert!(
        message.contains("speculative.streaming_service.local_daemon.tokens must be set"),
        "error={error}"
    );
    assert!(
        message.contains("speculative.streaming_service.local_daemon.decoder must be set"),
        "error={error}"
    );
    assert!(
        message.contains("speculative.streaming_service.local_daemon.joiner must be set"),
        "error={error}"
    );
}

#[test]
fn speculative_streaming_service_config_defaults_to_localhost_pcm() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"
"#;

    let config =
        TalkConfig::from_toml_str(raw).expect("default streaming service config should parse");
    let service = config
        .speculative
        .streaming_service
        .as_ref()
        .expect("streaming service defaults should be available");

    assert_eq!(service.endpoint, "ws://127.0.0.1:53171/asr");
    assert_eq!(service.sample_rate_hz, 16_000);
    assert_eq!(service.channels, 1);
    assert_eq!(service.connect_timeout_ms, 1_000);
    assert_eq!(service.idle_timeout_ms, 3_000);
    assert_eq!(service.final_timeout_ms, 7_000);
}

#[test]
fn rejects_speculative_streaming_service_non_websocket_endpoint() {
    let raw = speculative_streaming_service_config_with(
        r#"
endpoint = "http://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
"#,
    );

    let error = TalkConfig::from_toml_str(&raw).expect_err("non-websocket endpoint must fail");

    assert!(
        error
            .to_string()
            .contains("speculative.streaming_service.endpoint must use ws or wss scheme"),
        "error={error}"
    );
}

#[test]
fn rejects_speculative_streaming_service_non_loopback_endpoint() {
    let raw = speculative_streaming_service_config_with(
        r#"
endpoint = "ws://192.168.1.50:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
"#,
    );

    let error = TalkConfig::from_toml_str(&raw).expect_err("non-loopback endpoint must fail");

    assert!(
        error
            .to_string()
            .contains("speculative.streaming_service.endpoint host must be loopback"),
        "error={error}"
    );
}

#[test]
fn rejects_speculative_streaming_service_zero_audio_shape() {
    let raw = speculative_streaming_service_config_with(
        r#"
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 0
channels = 0
connect_timeout_ms = 1000
idle_timeout_ms = 3000
final_timeout_ms = 7000
"#,
    );

    let error = TalkConfig::from_toml_str(&raw).expect_err("zero streaming audio shape must fail");
    let message = error.to_string();

    assert!(
        message.contains("speculative.streaming_service.sample_rate_hz must be greater than 0"),
        "error={error}"
    );
    assert!(
        message.contains("speculative.streaming_service.channels must be greater than 0"),
        "error={error}"
    );
}

#[test]
fn rejects_speculative_streaming_service_zero_timeouts() {
    let raw = speculative_streaming_service_config_with(
        r#"
endpoint = "ws://127.0.0.1:53171/asr"
sample_rate_hz = 16000
channels = 1
connect_timeout_ms = 0
idle_timeout_ms = 0
final_timeout_ms = 0
"#,
    );

    let error =
        TalkConfig::from_toml_str(&raw).expect_err("zero streaming service timeouts must fail");
    let message = error.to_string();

    assert!(
        message.contains("speculative.streaming_service.connect_timeout_ms must be greater than 0"),
        "error={error}"
    );
    assert!(
        message.contains("speculative.streaming_service.idle_timeout_ms must be greater than 0"),
        "error={error}"
    );
    assert!(
        message.contains("speculative.streaming_service.final_timeout_ms must be greater than 0"),
        "error={error}"
    );
}

fn speculative_streaming_service_config_with(streaming_service_table: &str) -> String {
    format!(
        r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
temp_dir = ".runtime/audio"
max_recording_seconds = 15
sample_rate_hz = 16000
channels = 1

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/logs"

[speculative]
enabled = true
local_asr = "streaming_service"
cloud_correction = "disabled"

[speculative.streaming_service]
{streaming_service_table}
"#
    )
}

#[test]
fn parses_optional_desktop_shortcut_routes() {
    let raw = r#"
voice_mode = "dictate"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[desktop.shortcuts]
translate_shortcut = "RightAlt+/"
ask_shortcut = "RightAlt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("desktop shortcut config should parse");

    assert_eq!(
        config.desktop.shortcuts.translate_shortcut.as_deref(),
        Some("RightAlt+/")
    );
    assert_eq!(
        config.desktop.shortcuts.ask_shortcut.as_deref(),
        Some("RightAlt+Space")
    );
}

#[test]
fn parses_five_user_facing_voice_modes_and_legacy_aliases() {
    for (raw_mode, expected_mode) in [
        ("transcribe", VoiceMode::Transcribe),
        ("document", VoiceMode::Document),
        ("command", VoiceMode::Command),
        ("generate", VoiceMode::Generate),
        ("smart", VoiceMode::Smart),
        ("dictate", VoiceMode::Dictate),
        ("dictation", VoiceMode::Dictate),
        ("polish", VoiceMode::Polish),
        ("translate", VoiceMode::Translate),
    ] {
        let raw = format!(
            r#"
voice_mode = "{raw_mode}"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#
        );

        let config = TalkConfig::from_toml_str(&raw).expect("voice mode should parse");
        assert_eq!(
            config.default_voice_mode(),
            expected_mode,
            "raw_mode={raw_mode}"
        );
    }
}

#[test]
fn parses_five_mode_direct_entry_shortcuts() {
    let raw = r#"
voice_mode = "smart"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[desktop.shortcuts]
transcribe_shortcut = "RightCtrl+1"
document_shortcut = "RightCtrl+2"
command_shortcut = "RightCtrl+3"
generate_shortcut = "RightCtrl+4"
smart_shortcut = "RightCtrl+5"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("five mode shortcuts should parse");

    assert_eq!(
        config.desktop.shortcuts.transcribe_shortcut.as_deref(),
        Some("RightCtrl+1")
    );
    assert_eq!(
        config.desktop.shortcuts.document_shortcut.as_deref(),
        Some("RightCtrl+2")
    );
    assert_eq!(
        config.desktop.shortcuts.command_shortcut.as_deref(),
        Some("RightCtrl+3")
    );
    assert_eq!(
        config.desktop.shortcuts.generate_shortcut.as_deref(),
        Some("RightCtrl+4")
    );
    assert_eq!(
        config.desktop.shortcuts.smart_shortcut.as_deref(),
        Some("RightCtrl+5")
    );
}

#[test]
fn parses_desktop_paste_shortcut_overrides() {
    let raw = r#"
voice_mode = "dictate"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[desktop.shortcuts]
translate_shortcut = "RightAlt+/"
ask_shortcut = "RightAlt+Space"

[[desktop.paste.shortcut_overrides]]
process_name = "tabby"
paste_shortcut = "ctrl_shift_v"

[[desktop.paste.shortcut_overrides]]
automation_framework_id = "Chrome"
automation_control_type = "edit"
paste_shortcut = "ctrl_v"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("desktop paste overrides should parse");

    assert_eq!(config.desktop.paste.shortcut_overrides.len(), 2);
    assert_eq!(
        config.desktop.paste.shortcut_overrides[0]
            .process_name
            .as_deref(),
        Some("tabby")
    );
    assert_eq!(
        config.desktop.paste.shortcut_overrides[0].paste_shortcut,
        DesktopPasteShortcut::ControlShiftV
    );
    assert_eq!(
        config.desktop.paste.shortcut_overrides[1]
            .automation_framework_id
            .as_deref(),
        Some("Chrome")
    );
    assert_eq!(
        config.desktop.paste.shortcut_overrides[1]
            .automation_control_type
            .as_deref(),
        Some("edit")
    );
    assert_eq!(
        config.desktop.paste.shortcut_overrides[1].paste_shortcut,
        DesktopPasteShortcut::ControlV
    );
}

#[test]
fn rejects_desktop_paste_shortcut_override_without_any_matcher() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[[desktop.paste.shortcut_overrides]]
paste_shortcut = "ctrl_shift_v"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("desktop paste override without matchers");

    assert!(
        error.to_string().contains(
            "desktop.paste.shortcut_overrides[0] must declare at least one matcher field"
        ),
        "error={error}"
    );
}

#[test]
fn rejects_duplicate_desktop_shortcuts() {
    let raw = r#"
voice_mode = "dictate"

[trigger]
mode = "toggle"
toggle_shortcut = "RightAlt"

[desktop.shortcuts]
translate_shortcut = "RightAlt"
ask_shortcut = "RightAlt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("duplicate desktop shortcuts must fail");

    assert!(
        error
            .to_string()
            .contains("desktop shortcut values must be unique across trigger.toggle_shortcut and desktop.shortcuts"),
        "error={error}"
    );
}

#[test]
fn parses_explicit_native_windows_audio_backend() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "native_windows"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid native audio config");

    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.audio.sample_rate_hz, 16_000);
    assert_eq!(config.audio.channels, 1);
}

#[test]
fn parses_explicit_native_windows_input_device() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "native_windows"
input_device = "Virtual Mic"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid native input-device config");

    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.audio.input_device.as_deref(), Some("Virtual Mic"));
}

#[test]
fn parses_explicit_native_windows_clipboard_backend() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello from talk mock"

[output]
mode = "clipboard_paste"
restore_clipboard = true
clipboard_backend = "native_windows"

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid native clipboard config");

    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert!(config.output.restore_clipboard);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::NativeWindows
    );
}

#[test]
fn rejects_invalid_audio_config() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 0
sample_rate_hz = 0
channels = 0
temp_dir = ""

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "clipboard_paste"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    let message = error.to_string();

    assert!(message.contains("max_recording_seconds"));
    assert!(message.contains("sample_rate_hz"));
    assert!(message.contains("channels"));
    assert!(message.contains("temp_dir"));
}

#[test]
fn rejects_native_windows_input_device_with_surrounding_whitespace() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "native_windows"
input_device = " Virtual Mic "
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("input_device whitespace must fail validation");

    assert!(
        error
            .to_string()
            .contains("audio.input_device must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[test]
fn rejects_whitespace_only_runtime_paths() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = "   "

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = "	 "
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("blank runtime paths must fail");
    let message = error.to_string();

    assert!(
        message.contains("audio.temp_dir must not be empty"),
        "error={error}"
    );
    assert!(
        message.contains("logging.dir must not be empty"),
        "error={error}"
    );
}

#[test]
fn rejects_trigger_shortcut_with_surrounding_whitespace() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = " Ctrl+Alt+Space "

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("shortcut whitespace must fail validation");

    assert!(
        error
            .to_string()
            .contains("trigger.toggle_shortcut must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[test]
fn rejects_mock_provider_endpoint() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = "hello"
endpoint = "http://127.0.0.1:3000/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("mock provider endpoint should fail validation");

    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not be set for mock provider"),
        "error={error}"
    );
}

#[test]
fn rejects_mock_provider_transcript_with_surrounding_whitespace() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "mock"
mock_transcript = " hello "

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("mock transcript whitespace must fail");

    assert!(
        error
            .to_string()
            .contains("provider.mock_transcript must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_without_endpoint() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must be set for http provider"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_without_http_scheme() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "provider.local/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must use http or https scheme"),
        "error={error}"
    );
}

#[test]
fn accepts_http_provider_endpoint_with_uppercase_https_scheme() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "HTTPS://api.example.com/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid config");
    assert_eq!(
        config.provider.endpoint.as_deref(),
        Some("HTTPS://api.example.com/transcribe")
    );
}

#[test]
fn accepts_http_provider_endpoint_with_ipv6_host_without_port() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://[::1]/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid config");
    assert_eq!(
        config.provider.endpoint.as_deref(),
        Some("https://[::1]/transcribe")
    );
}

#[test]
fn rejects_http_provider_endpoint_with_invalid_bracketed_ipv6_host() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://[not-ip]/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint bracketed host must be a valid IPv6 address"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_unbracketed_ipv6_host() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://::1/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint IPv6 hosts must use [brackets]"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_user_info() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://user@example.com/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not include user info"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_user_info_and_password() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://user:secret@example.com/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not include user info"),
        "error={error}"
    );
}

#[test]
fn accepts_http_provider_endpoint_with_numeric_port_before_query() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://api.example.com:8443?mode=dictate"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid config");
    assert_eq!(
        config.provider.endpoint.as_deref(),
        Some("https://api.example.com:8443?mode=dictate")
    );
}

#[test]
fn rejects_http_provider_endpoint_with_surrounding_whitespace() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = " https://api.example.com/transcribe "

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not have leading or trailing whitespace"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_embedded_whitespace() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://api example.com/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not contain whitespace"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_without_host() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https:///transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must include a host"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_port_but_without_host() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://:8443/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must include a host"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_fragment() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://api.example.com/transcribe#fragment"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint must not include a URL fragment"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_non_numeric_port() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://api.example.com:not-a-port/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint port must be numeric"),
        "error={error}"
    );
}

#[test]
fn rejects_http_provider_endpoint_with_out_of_range_port() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "http"
endpoint = "https://api.example.com:70000/transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("invalid config must fail");
    assert!(
        error
            .to_string()
            .contains("provider.endpoint port must be between 1 and 65535"),
        "error={error}"
    );
}

#[test]
fn parses_openai_compatible_provider_with_explicit_endpoints_and_models() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"
chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"
transcription_model = "gpt-4o-mini-transcribe"
chat_model = "gpt-4o-mini"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config = TalkConfig::from_toml_str(raw).expect("valid openai-compatible config");

    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/audio/transcriptions")
    );
    assert_eq!(
        config.provider.chat_completions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/chat/completions")
    );
    assert_eq!(
        config.provider.transcription_model.as_deref(),
        Some("gpt-4o-mini-transcribe")
    );
    assert_eq!(config.provider.chat_model.as_deref(), Some("gpt-4o-mini"));
    assert_eq!(
        config.provider.api_key_env.as_deref(),
        Some("TALK_PROVIDER_API_KEY")
    );
    assert_eq!(
        config.provider.transcription_transport,
        OpenAiTranscriptionTransport::AudioTranscriptions
    );
}

#[test]
fn rejects_openai_compatible_provider_without_audio_transcriptions_endpoint() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"
transcription_model = "gpt-4o-mini-transcribe"
chat_model = "gpt-4o-mini"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("missing transcriptions endpoint must fail");
    assert!(
        error.to_string().contains(
            "provider.audio_transcriptions_endpoint must be set for openai_compatible provider"
        ),
        "error={error}"
    );
}

#[test]
fn parses_openai_compatible_provider_with_chat_completions_audio_input_transport() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
transcription_transport = "chat_completions_audio_input"
audio_transcriptions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
chat_completions_endpoint = "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions"
transcription_model = "qwen3-asr-flash"
chat_model = "qwen3.7-plus"
api_key_env = "TALK_PROVIDER_API_KEY"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let config =
        TalkConfig::from_toml_str(raw).expect("chat-completions audio-input config should parse");

    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.transcription_transport,
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput
    );
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
    );
    assert_eq!(
        config.provider.transcription_model.as_deref(),
        Some("qwen3-asr-flash")
    );
    assert_eq!(config.provider.chat_model.as_deref(), Some("qwen3.7-plus"));
}

#[test]
fn rejects_openai_compatible_provider_without_chat_completions_endpoint() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"
transcription_model = "gpt-4o-mini-transcribe"
chat_model = "gpt-4o-mini"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error =
        TalkConfig::from_toml_str(raw).expect_err("missing chat completions endpoint must fail");
    assert!(
        error.to_string().contains(
            "provider.chat_completions_endpoint must be set for openai_compatible provider"
        ),
        "error={error}"
    );
}

#[test]
fn rejects_openai_compatible_provider_without_transcription_model() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"
chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"
chat_model = "gpt-4o-mini"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("missing transcription model must fail");
    assert!(
        error
            .to_string()
            .contains("provider.transcription_model must be set for openai_compatible provider"),
        "error={error}"
    );
}

#[test]
fn rejects_openai_compatible_provider_without_chat_model() {
    let raw = r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Ctrl+Alt+Space"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/talk/audio"

[provider]
kind = "openai_compatible"
audio_transcriptions_endpoint = "http://127.0.0.1:4200/v1/audio/transcriptions"
chat_completions_endpoint = "http://127.0.0.1:4200/v1/chat/completions"
transcription_model = "gpt-4o-mini-transcribe"

[output]
mode = "dry_run"
restore_clipboard = true

[logging]
dir = ".runtime/talk/logs"
"#;

    let error = TalkConfig::from_toml_str(raw).expect_err("missing chat model must fail");
    assert!(
        error
            .to_string()
            .contains("provider.chat_model must be set for openai_compatible provider"),
        "error={error}"
    );
}

#[test]
fn parses_desktop_http_safe_example_config() {
    let raw = read_example_config("desktop-http-safe-config.toml");

    let config = TalkConfig::from_toml_str(&raw).expect("safe desktop http example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::Silent);
    assert_eq!(config.provider.kind, ProviderKind::Http);
    assert_eq!(
        config.provider.endpoint.as_deref(),
        Some("http://127.0.0.1:18080/provider")
    );
    assert_eq!(config.output.mode, OutputMode::DryRun);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::Fallback
    );
    assert_eq!(config.default_voice_mode(), VoiceMode::Dictate);
}

#[test]
fn parses_desktop_http_live_example_config() {
    let raw = read_example_config("desktop-http-live-config.toml");

    let config = TalkConfig::from_toml_str(&raw).expect("live desktop http example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.provider.kind, ProviderKind::Http);
    assert_eq!(
        config.provider.endpoint.as_deref(),
        Some("http://127.0.0.1:18080/provider")
    );
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::NativeWindows
    );
    assert_eq!(config.default_voice_mode(), VoiceMode::Dictate);
    assert_eq!(
        config.desktop.shortcuts.translate_shortcut.as_deref(),
        Some("RightAlt+/")
    );
    assert_eq!(
        config.desktop.shortcuts.ask_shortcut.as_deref(),
        Some("RightAlt+Space")
    );
}

#[test]
fn parses_desktop_openai_compatible_safe_example_config() {
    let raw = read_example_config("desktop-openai-compatible-safe-config.toml");

    let config = TalkConfig::from_toml_str(&raw)
        .expect("safe desktop openai-compatible example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::Silent);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/audio/transcriptions")
    );
    assert_eq!(
        config.provider.chat_completions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/chat/completions")
    );
    assert_eq!(
        config.provider.transcription_model.as_deref(),
        Some("gpt-4o-mini-transcribe")
    );
    assert_eq!(config.provider.chat_model.as_deref(), Some("gpt-4o-mini"));
    assert_eq!(
        config.provider.api_key_env.as_deref(),
        Some("TALK_PROVIDER_API_KEY")
    );
    assert_eq!(config.output.mode, OutputMode::DryRun);
    assert_eq!(config.default_voice_mode(), VoiceMode::Command);
}

#[test]
fn parses_desktop_openai_compatible_live_example_config() {
    let raw = read_example_config("desktop-openai-compatible-live-config.toml");

    let config = TalkConfig::from_toml_str(&raw)
        .expect("live desktop openai-compatible example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/audio/transcriptions")
    );
    assert_eq!(
        config.provider.chat_completions_endpoint.as_deref(),
        Some("http://127.0.0.1:4200/v1/chat/completions")
    );
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::NativeWindows
    );
    assert_eq!(config.default_voice_mode(), VoiceMode::Dictate);
    assert_eq!(
        config.desktop.shortcuts.translate_shortcut.as_deref(),
        Some("RightAlt+/")
    );
    assert_eq!(
        config.desktop.shortcuts.ask_shortcut.as_deref(),
        Some("RightAlt+Space")
    );
}

#[test]
fn parses_once_qwen_audio_input_safe_example_config() {
    let raw = read_example_config("once-qwen-audio-input-safe-config.toml");

    let config =
        TalkConfig::from_toml_str(&raw).expect("once qwen audio input safe example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::Silent);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.transcription_transport,
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput
    );
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
    );
    assert_eq!(
        config.provider.chat_completions_endpoint.as_deref(),
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
    );
    assert_eq!(
        config.provider.transcription_model.as_deref(),
        Some("qwen3-asr-flash")
    );
    assert_eq!(config.provider.chat_model.as_deref(), Some("qwen3.7-plus"));
    assert_eq!(config.output.mode, OutputMode::DryRun);
    assert_eq!(config.default_voice_mode(), VoiceMode::Command);
}

#[test]
fn parses_desktop_qwen_audio_input_live_example_config() {
    let raw = read_example_config("desktop-qwen-audio-input-live-config.toml");

    let config = TalkConfig::from_toml_str(&raw)
        .expect("desktop qwen audio input live example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(
        config.provider.transcription_transport,
        OpenAiTranscriptionTransport::ChatCompletionsAudioInput
    );
    assert_eq!(
        config.provider.audio_transcriptions_endpoint.as_deref(),
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
    );
    assert_eq!(
        config.provider.chat_completions_endpoint.as_deref(),
        Some("https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions")
    );
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert_eq!(
        config.output.clipboard_backend,
        ClipboardBackendMode::NativeWindows
    );
    assert_eq!(config.default_voice_mode(), VoiceMode::Dictate);
    assert_eq!(
        config.desktop.shortcuts.translate_shortcut.as_deref(),
        Some("RightAlt+/")
    );
    assert_eq!(
        config.desktop.shortcuts.ask_shortcut.as_deref(),
        Some("RightAlt+Space")
    );
}

#[test]
fn parses_desktop_external_asr_speculative_example_config() {
    let raw = read_example_config("desktop-external-asr-speculative-config.toml");

    let config = TalkConfig::from_toml_str(&raw)
        .expect("desktop external ASR speculative example should parse");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert!(config.speculative.enabled);
    assert_eq!(config.speculative.local_asr, "external_command");
    assert_eq!(
        config.speculative.cloud_correction,
        "provider_text_processor"
    );
    assert!(config
        .speculative
        .external_asr_command
        .as_deref()
        .is_some_and(|value| value.contains("external-asr-jsonl-smoke.ps1")));
}

#[test]
fn parses_desktop_streaming_service_speculative_example_config() {
    let raw = read_example_config("desktop-streaming-service-speculative-config.toml");

    let config = TalkConfig::from_toml_str(&raw)
        .expect("desktop streaming service speculative example should parse");
    let service = config
        .speculative
        .streaming_service
        .as_ref()
        .expect("streaming service settings");

    assert_eq!(config.trigger.mode, TriggerMode::Toggle);
    assert_eq!(config.audio.backend, AudioBackendMode::NativeWindows);
    assert_eq!(config.provider.kind, ProviderKind::OpenAiCompatible);
    assert_eq!(config.output.mode, OutputMode::ClipboardPaste);
    assert!(config.speculative.enabled);
    assert_eq!(config.speculative.local_asr, "streaming_service");
    assert_eq!(
        config.speculative.cloud_correction,
        "provider_text_processor"
    );
    assert_eq!(service.endpoint, "ws://127.0.0.1:53171/asr");
    assert_eq!(service.sample_rate_hz, 16_000);
    assert_eq!(service.channels, 1);
}
