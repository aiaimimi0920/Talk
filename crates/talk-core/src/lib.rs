use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub mod speculative;
pub use speculative::{
    SpeculativeCorrectionPatch, SpeculativeEdit, SpeculativeEditKind, SpeculativeMode,
    SpeculativeSegment, SpeculativeSegmentState,
};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TalkError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("insert error: {0}")]
    Insert(String),
    #[error("audio error: {0}")]
    Audio(String),
    #[error("hotkey error: {0}")]
    Hotkey(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("invalid transition: state={state:?} event={event:?}")]
    InvalidTransition {
        state: SessionStatus,
        event: VoiceEventKind,
    },
    #[error("session is terminal: state={state:?} event={event:?}")]
    TerminalTransition {
        state: SessionStatus,
        event: VoiceEventKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMode {
    Transcribe,
    Document,
    Generate,
    Smart,
    #[serde(alias = "dictation")]
    Dictate,
    Polish,
    Translate,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    Toggle,
    PushToTalk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Mock,
    Http,
    #[serde(rename = "openai_compatible")]
    OpenAiCompatible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenAiTranscriptionTransport {
    AudioTranscriptions,
    ChatCompletionsAudioInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    ClipboardPaste,
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardBackendMode {
    Fallback,
    NativeWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioBackendMode {
    Silent,
    NativeWindows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DesktopPasteShortcut {
    #[serde(rename = "ctrl_v")]
    ControlV,
    #[serde(rename = "ctrl_shift_v")]
    ControlShiftV,
    #[serde(rename = "shift_insert")]
    ShiftInsert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReadinessStatus {
    Ready,
    Unavailable,
}

impl NativeReadinessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TalkConfig {
    pub trigger: TriggerConfig,
    #[serde(default)]
    pub desktop: DesktopConfig,
    pub audio: AudioConfig,
    pub provider: ProviderConfig,
    pub output: OutputConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub speculative: SpeculativeConfig,
    #[serde(default = "default_voice_mode")]
    pub voice_mode: VoiceMode,
}

impl TalkConfig {
    pub fn from_toml_str(raw: &str) -> Result<Self, TalkError> {
        let config: Self =
            toml::from_str(raw).map_err(|error| TalkError::InvalidConfig(error.to_string()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), TalkError> {
        let mut problems = Vec::<String>::new();

        if self.trigger.toggle_shortcut.trim().is_empty() {
            problems.push("trigger.toggle_shortcut must not be empty".to_string());
        } else if self.trigger.toggle_shortcut.trim() != self.trigger.toggle_shortcut {
            problems.push(
                "trigger.toggle_shortcut must not have leading or trailing whitespace".to_string(),
            );
        }
        validate_desktop_shortcuts(&self.desktop.shortcuts, &mut problems);
        for (index, override_rule) in self.desktop.paste.shortcut_overrides.iter().enumerate() {
            let prefix = format!("desktop.paste.shortcut_overrides[{index}]");
            validate_optional_trimmed_config_string(
                override_rule.process_name.as_deref(),
                &format!("{prefix}.process_name"),
                &mut problems,
            );
            validate_optional_trimmed_config_string(
                override_rule.focus_class_name.as_deref(),
                &format!("{prefix}.focus_class_name"),
                &mut problems,
            );
            validate_optional_trimmed_config_string(
                override_rule.automation_framework_id.as_deref(),
                &format!("{prefix}.automation_framework_id"),
                &mut problems,
            );
            validate_optional_trimmed_config_string(
                override_rule.automation_control_type.as_deref(),
                &format!("{prefix}.automation_control_type"),
                &mut problems,
            );
            if override_rule.process_name.is_none()
                && override_rule.focus_class_name.is_none()
                && override_rule.automation_framework_id.is_none()
                && override_rule.automation_control_type.is_none()
            {
                problems.push(format!("{prefix} must declare at least one matcher field"));
            }
        }
        if desktop_any_shortcut_configured(&self.desktop.shortcuts)
            && self.trigger.mode != TriggerMode::Toggle
        {
            problems
                .push("desktop.shortcuts currently require trigger.mode = \"toggle\"".to_string());
        }
        if desktop_shortcut_values_are_not_unique(self) {
            problems.push(
                "desktop shortcut values must be unique across trigger.toggle_shortcut and desktop.shortcuts"
                    .to_string(),
            );
        }
        if self.audio.max_recording_seconds == 0 {
            problems.push("audio.max_recording_seconds must be greater than 0".to_string());
        }
        if self.audio.sample_rate_hz == 0 {
            problems.push("audio.sample_rate_hz must be greater than 0".to_string());
        }
        if self.audio.channels == 0 {
            problems.push("audio.channels must be greater than 0".to_string());
        }
        if self.speculative.max_patch_age_ms == 0 {
            problems.push("speculative.max_patch_age_ms must be greater than 0".to_string());
        }
        if !(0.0..=1.0).contains(&self.speculative.max_auto_patch_edit_ratio) {
            problems
                .push("speculative.max_auto_patch_edit_ratio must be between 0 and 1".to_string());
        }
        validate_optional_trimmed_config_string(
            self.speculative.external_asr_command.as_deref(),
            "speculative.external_asr_command",
            &mut problems,
        );
        if self.speculative.enabled
            && self.speculative.local_asr == "external_command"
            && self
                .speculative
                .external_asr_command
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            problems.push(
                "speculative.external_asr_command must be set when local_asr is external_command"
                    .to_string(),
            );
        }
        if self.speculative.enabled
            && self
                .speculative
                .local_asr
                .trim()
                .eq_ignore_ascii_case("streaming_service")
        {
            let Some(service) = self.speculative.streaming_service.as_ref() else {
                problems.push(
                    "speculative.streaming_service must be set when local_asr is streaming_service"
                        .to_string(),
                );
                return Err(TalkError::InvalidConfig(problems.join("; ")));
            };
            if let Err(message) = validate_websocket_loopback_endpoint(
                &service.endpoint,
                "speculative.streaming_service.endpoint",
            ) {
                problems.push(message);
            }
            if service.sample_rate_hz == 0 {
                problems.push(
                    "speculative.streaming_service.sample_rate_hz must be greater than 0"
                        .to_string(),
                );
            }
            if service.channels == 0 {
                problems.push(
                    "speculative.streaming_service.channels must be greater than 0".to_string(),
                );
            }
            if service.connect_timeout_ms == 0 {
                problems.push(
                    "speculative.streaming_service.connect_timeout_ms must be greater than 0"
                        .to_string(),
                );
            }
            if service.idle_timeout_ms == 0 {
                problems.push(
                    "speculative.streaming_service.idle_timeout_ms must be greater than 0"
                        .to_string(),
                );
            }
            if service.final_timeout_ms == 0 {
                problems.push(
                    "speculative.streaming_service.final_timeout_ms must be greater than 0"
                        .to_string(),
                );
            }
            validate_speculative_local_asr_daemon_config(
                service.local_daemon.as_ref(),
                &mut problems,
            );
        }
        if path_is_blank(&self.audio.temp_dir) {
            problems.push("audio.temp_dir must not be empty".to_string());
        }
        validate_optional_trimmed_config_string(
            self.audio.input_device.as_deref(),
            "audio.input_device",
            &mut problems,
        );
        if path_is_blank(&self.logging.dir) {
            problems.push("logging.dir must not be empty".to_string());
        }
        if self.provider.kind == ProviderKind::Mock {
            match self.provider.mock_transcript.as_deref() {
                Some(transcript) if transcript.trim().is_empty() => {
                    problems
                        .push("provider.mock_transcript must be set for mock provider".to_string());
                }
                Some(transcript) if transcript.trim() != transcript => problems.push(
                    "provider.mock_transcript must not have leading or trailing whitespace"
                        .to_string(),
                ),
                Some(_) => {}
                None => problems
                    .push("provider.mock_transcript must be set for mock provider".to_string()),
            }
        }
        if self.provider.kind == ProviderKind::Mock && self.provider.endpoint.is_some() {
            problems.push("provider.endpoint must not be set for mock provider".to_string());
        }
        if self.provider.kind == ProviderKind::Http {
            let endpoint = self.provider.endpoint.as_deref().unwrap_or_default();
            if endpoint.is_empty() {
                problems.push("provider.endpoint must be set for http provider".to_string());
            } else if let Err(message) = validate_provider_endpoint(endpoint) {
                problems.push(message);
            }
        }
        if self.provider.kind == ProviderKind::OpenAiCompatible {
            validate_required_provider_endpoint(
                self.provider.audio_transcriptions_endpoint.as_deref(),
                "provider.audio_transcriptions_endpoint",
                &mut problems,
            );
            validate_required_provider_endpoint(
                self.provider.chat_completions_endpoint.as_deref(),
                "provider.chat_completions_endpoint",
                &mut problems,
            );
            validate_required_trimmed_provider_string(
                self.provider.transcription_model.as_deref(),
                "provider.transcription_model",
                &mut problems,
            );
            validate_required_trimmed_provider_string(
                self.provider.chat_model.as_deref(),
                "provider.chat_model",
                &mut problems,
            );
            validate_optional_trimmed_provider_string(
                self.provider.api_key.as_deref(),
                "provider.api_key",
                &mut problems,
            );
            validate_optional_trimmed_provider_string(
                self.provider.api_key_env.as_deref(),
                "provider.api_key_env",
                &mut problems,
            );
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(TalkError::InvalidConfig(problems.join("; ")))
        }
    }

    pub fn default_voice_mode(&self) -> VoiceMode {
        self.voice_mode
    }
}

fn path_is_blank(path: &Path) -> bool {
    path.as_os_str().is_empty() || path.as_os_str().to_string_lossy().trim().is_empty()
}

fn validate_provider_endpoint(endpoint: &str) -> Result<(), String> {
    validate_http_endpoint_with_blank_message(
        endpoint,
        "provider.endpoint",
        "provider.endpoint must not have leading or trailing whitespace".to_string(),
    )
}

fn validate_required_provider_endpoint(
    endpoint: Option<&str>,
    subject: &str,
    problems: &mut Vec<String>,
) {
    let endpoint = endpoint.unwrap_or_default();
    if endpoint.is_empty() {
        problems.push(format!(
            "{subject} must be set for openai_compatible provider"
        ));
    } else if let Err(message) = validate_http_endpoint(endpoint, subject) {
        problems.push(message);
    }
}

fn validate_required_trimmed_provider_string(
    value: Option<&str>,
    subject: &str,
    problems: &mut Vec<String>,
) {
    match value {
        Some(value) if value.trim().is_empty() => {
            problems.push(format!(
                "{subject} must be set for openai_compatible provider"
            ));
        }
        Some(value) if value.trim() != value => {
            problems.push(format!(
                "{subject} must not have leading or trailing whitespace"
            ));
        }
        Some(_) => {}
        None => problems.push(format!(
            "{subject} must be set for openai_compatible provider"
        )),
    }
}

fn validate_optional_trimmed_provider_string(
    value: Option<&str>,
    subject: &str,
    problems: &mut Vec<String>,
) {
    match value {
        Some(value) if value.trim().is_empty() => {
            problems.push(format!("{subject} must not be blank"));
        }
        Some(value) if value.trim() != value => {
            problems.push(format!(
                "{subject} must not have leading or trailing whitespace"
            ));
        }
        _ => {}
    }
}

fn validate_optional_trimmed_config_string(
    value: Option<&str>,
    subject: &str,
    problems: &mut Vec<String>,
) {
    match value {
        Some(value) if value.trim().is_empty() => {
            problems.push(format!("{subject} must not be blank"));
        }
        Some(value) if value.trim() != value => {
            problems.push(format!(
                "{subject} must not have leading or trailing whitespace"
            ));
        }
        _ => {}
    }
}

fn validate_desktop_shortcuts(shortcuts: &DesktopShortcutConfig, problems: &mut Vec<String>) {
    for (subject, value) in [
        (
            "desktop.shortcuts.transcribe_shortcut",
            shortcuts.transcribe_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.document_shortcut",
            shortcuts.document_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.command_shortcut",
            shortcuts.command_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.generate_shortcut",
            shortcuts.generate_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.smart_shortcut",
            shortcuts.smart_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.translate_shortcut",
            shortcuts.translate_shortcut.as_deref(),
        ),
        (
            "desktop.shortcuts.ask_shortcut",
            shortcuts.ask_shortcut.as_deref(),
        ),
    ] {
        validate_optional_trimmed_config_string(value, subject, problems);
    }
}

fn desktop_any_shortcut_configured(shortcuts: &DesktopShortcutConfig) -> bool {
    [
        shortcuts.transcribe_shortcut.as_ref(),
        shortcuts.document_shortcut.as_ref(),
        shortcuts.command_shortcut.as_ref(),
        shortcuts.generate_shortcut.as_ref(),
        shortcuts.smart_shortcut.as_ref(),
        shortcuts.translate_shortcut.as_ref(),
        shortcuts.ask_shortcut.as_ref(),
    ]
    .into_iter()
    .any(|shortcut| shortcut.is_some())
}

fn validate_speculative_local_asr_daemon_config(
    config: Option<&SpeculativeLocalAsrDaemonConfig>,
    problems: &mut Vec<String>,
) {
    let Some(config) = config else {
        return;
    };
    let prefix = "speculative.streaming_service.local_daemon";
    validate_optional_trimmed_config_string(
        config.engine.as_deref(),
        &format!("{prefix}.engine"),
        problems,
    );
    validate_optional_trimmed_config_string(
        config.model.as_deref(),
        &format!("{prefix}.model"),
        problems,
    );
    validate_optional_trimmed_config_string(
        config.dry_run_text.as_deref(),
        &format!("{prefix}.dry_run_text"),
        problems,
    );
    validate_optional_trimmed_config_string(
        config.dry_run_partial_text.as_deref(),
        &format!("{prefix}.dry_run_partial_text"),
        problems,
    );
    validate_optional_trimmed_config_string(
        config.provider.as_deref(),
        &format!("{prefix}.provider"),
        problems,
    );
    validate_optional_trimmed_config_string(
        config.decoding_method.as_deref(),
        &format!("{prefix}.decoding_method"),
        problems,
    );
    if let Some(decoding_method) = config.decoding_method.as_deref() {
        if !matches!(decoding_method, "greedy_search" | "modified_beam_search") {
            problems.push(format!(
                "{prefix}.decoding_method must be greedy_search or modified_beam_search"
            ));
        }
    }
    if config.num_threads == Some(0) {
        problems.push(format!("{prefix}.num_threads must be greater than 0"));
    }
    if config.sample_rate_hz == Some(0) {
        problems.push(format!("{prefix}.sample_rate_hz must be greater than 0"));
    }
    validate_optional_config_path(
        config.tokens.as_ref(),
        &format!("{prefix}.tokens"),
        problems,
    );
    validate_optional_config_path(
        config.encoder.as_ref(),
        &format!("{prefix}.encoder"),
        problems,
    );
    validate_optional_config_path(
        config.decoder.as_ref(),
        &format!("{prefix}.decoder"),
        problems,
    );
    validate_optional_config_path(
        config.joiner.as_ref(),
        &format!("{prefix}.joiner"),
        problems,
    );
    validate_optional_config_path(
        config.hotwords_file.as_ref(),
        &format!("{prefix}.hotwords_file"),
        problems,
    );
    validate_optional_config_path(
        config.rule_fsts.as_ref(),
        &format!("{prefix}.rule_fsts"),
        problems,
    );
    validate_optional_config_path(
        config.rule_fars.as_ref(),
        &format!("{prefix}.rule_fars"),
        problems,
    );

    if config.mode == SpeculativeLocalAsrDaemonMode::SherpaOnline {
        if config
            .model
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        {
            problems.push(format!("{prefix}.model must be set for sherpa-online mode"));
        }
        require_config_path(
            config.tokens.as_ref(),
            &format!("{prefix}.tokens"),
            problems,
        );
        require_config_path(
            config.encoder.as_ref(),
            &format!("{prefix}.encoder"),
            problems,
        );
        require_config_path(
            config.decoder.as_ref(),
            &format!("{prefix}.decoder"),
            problems,
        );
        if config.model_family == SpeculativeSherpaOnlineModelFamily::Transducer {
            require_config_path(
                config.joiner.as_ref(),
                &format!("{prefix}.joiner"),
                problems,
            );
        }
    }
}

fn validate_optional_config_path(
    path: Option<&PathBuf>,
    subject: &str,
    problems: &mut Vec<String>,
) {
    if let Some(path) = path {
        if path_is_blank(path) {
            problems.push(format!("{subject} must not be blank"));
        }
    }
}

fn require_config_path(path: Option<&PathBuf>, subject: &str, problems: &mut Vec<String>) {
    match path {
        Some(path) if path_is_blank(path) => problems.push(format!("{subject} must not be blank")),
        Some(_) => {}
        None => problems.push(format!("{subject} must be set")),
    }
}

fn desktop_shortcut_values_are_not_unique(config: &TalkConfig) -> bool {
    let mut identities = std::collections::BTreeSet::<String>::new();
    for value in [
        Some(config.trigger.toggle_shortcut.as_str()),
        config.desktop.shortcuts.transcribe_shortcut.as_deref(),
        config.desktop.shortcuts.document_shortcut.as_deref(),
        config.desktop.shortcuts.command_shortcut.as_deref(),
        config.desktop.shortcuts.generate_shortcut.as_deref(),
        config.desktop.shortcuts.smart_shortcut.as_deref(),
        config.desktop.shortcuts.translate_shortcut.as_deref(),
        config.desktop.shortcuts.ask_shortcut.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        let identity = shortcut_identity(value);
        if !identities.insert(identity) {
            return true;
        }
    }
    false
}

fn shortcut_identity(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

pub fn validate_http_endpoint(endpoint: &str, subject: &str) -> Result<(), String> {
    validate_http_endpoint_with_blank_message(
        endpoint,
        subject,
        format!("{subject} must not be blank"),
    )
}

fn validate_http_endpoint_with_blank_message(
    endpoint: &str,
    subject: &str,
    blank_message: String,
) -> Result<(), String> {
    if endpoint.trim().is_empty() {
        return Err(blank_message);
    }
    if endpoint.trim() != endpoint {
        return Err(format!(
            "{subject} must not have leading or trailing whitespace"
        ));
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Err(format!("{subject} must not contain whitespace"));
    }
    if !uses_http_scheme(endpoint) {
        return Err(format!("{subject} must use http or https scheme"));
    }
    if endpoint.contains('#') {
        return Err(format!("{subject} must not include a URL fragment"));
    }
    let authority = http_endpoint_authority(endpoint)
        .ok_or_else(|| format!("{subject} must use http or https scheme"))?;
    if authority.contains('@') {
        return Err(format!("{subject} must not include user info"));
    }
    let (host, port) = split_http_endpoint_host_and_port(authority, subject)?;
    if host.is_empty() {
        return Err(format!("{subject} must include a host"));
    }
    let Some(port) = port else {
        return Ok(());
    };
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{subject} port must be numeric"));
    }
    match port.parse::<u16>() {
        Ok(0) | Err(_) => Err(format!("{subject} port must be between 1 and 65535")),
        Ok(_) => Ok(()),
    }
}

fn uses_http_scheme(endpoint: &str) -> bool {
    endpoint.split_once("://").is_some_and(|(scheme, rest)| {
        (scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
            && !rest.trim().is_empty()
    })
}

fn http_endpoint_authority(endpoint: &str) -> Option<&str> {
    let (_, rest) = endpoint.split_once("://")?;
    let end = rest
        .find(|ch| ['/', '?', '#'].contains(&ch))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn split_http_endpoint_host_and_port<'a>(
    authority: &'a str,
    subject: &str,
) -> Result<(&'a str, Option<&'a str>), String> {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let Some(closing_bracket) = bracketed.find(']') else {
            return Err(format!("{subject} must include a host"));
        };
        let host = &bracketed[..closing_bracket];
        if !host.is_empty() && host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(format!(
                "{subject} bracketed host must be a valid IPv6 address"
            ));
        }
        let remainder = &bracketed[closing_bracket + 1..];
        if remainder.is_empty() {
            return Ok((host, None));
        }
        let Some(port) = remainder.strip_prefix(':') else {
            return Err(format!("{subject} port must be numeric"));
        };
        return Ok((host, Some(port)));
    }
    if authority.matches(':').count() > 1 {
        return Err(format!("{subject} IPv6 hosts must use [brackets]"));
    }
    if let Some((host, port)) = authority.rsplit_once(':') {
        Ok((host, Some(port)))
    } else {
        Ok((authority, None))
    }
}

fn validate_websocket_loopback_endpoint(endpoint: &str, subject: &str) -> Result<(), String> {
    if endpoint.trim().is_empty() {
        return Err(format!("{subject} must not be blank"));
    }
    if endpoint.trim() != endpoint {
        return Err(format!(
            "{subject} must not have leading or trailing whitespace"
        ));
    }
    if endpoint.chars().any(char::is_whitespace) {
        return Err(format!("{subject} must not contain whitespace"));
    }
    if endpoint.contains('#') {
        return Err(format!("{subject} must not include a URL fragment"));
    }
    if !uses_websocket_scheme(endpoint) {
        return Err(format!("{subject} must use ws or wss scheme"));
    }

    let authority = websocket_endpoint_authority(endpoint)
        .ok_or_else(|| format!("{subject} must use ws or wss scheme"))?;
    if authority.contains('@') {
        return Err(format!("{subject} must not include user info"));
    }
    let (host, port) = split_http_endpoint_host_and_port(authority, subject)?;
    if host.is_empty() {
        return Err(format!("{subject} must include a host"));
    }
    if !websocket_endpoint_host_is_loopback(host) {
        return Err(format!("{subject} host must be loopback"));
    }
    let Some(port) = port else {
        return Ok(());
    };
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{subject} port must be numeric"));
    }
    match port.parse::<u16>() {
        Ok(0) | Err(_) => Err(format!("{subject} port must be between 1 and 65535")),
        Ok(_) => Ok(()),
    }
}

fn uses_websocket_scheme(endpoint: &str) -> bool {
    endpoint.split_once("://").is_some_and(|(scheme, rest)| {
        (scheme.eq_ignore_ascii_case("ws") || scheme.eq_ignore_ascii_case("wss"))
            && !rest.trim().is_empty()
    })
}

fn websocket_endpoint_authority(endpoint: &str) -> Option<&str> {
    let (_, rest) = endpoint.split_once("://")?;
    let end = rest
        .find(|ch| ['/', '?', '#'].contains(&ch))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

fn websocket_endpoint_host_is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub mode: TriggerMode,
    pub toggle_shortcut: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopConfig {
    #[serde(default)]
    pub shortcuts: DesktopShortcutConfig,
    #[serde(default)]
    pub paste: DesktopPasteConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopShortcutConfig {
    #[serde(default)]
    pub transcribe_shortcut: Option<String>,
    #[serde(default)]
    pub document_shortcut: Option<String>,
    #[serde(default)]
    pub command_shortcut: Option<String>,
    #[serde(default)]
    pub generate_shortcut: Option<String>,
    #[serde(default)]
    pub smart_shortcut: Option<String>,
    #[serde(default)]
    pub translate_shortcut: Option<String>,
    #[serde(default)]
    pub ask_shortcut: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopPasteConfig {
    #[serde(default)]
    pub shortcut_overrides: Vec<DesktopPasteShortcutOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopPasteShortcutOverride {
    #[serde(default)]
    pub process_name: Option<String>,
    #[serde(default)]
    pub focus_class_name: Option<String>,
    #[serde(default)]
    pub automation_framework_id: Option<String>,
    #[serde(default)]
    pub automation_control_type: Option<String>,
    pub paste_shortcut: DesktopPasteShortcut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    #[serde(default = "default_audio_backend")]
    pub backend: AudioBackendMode,
    #[serde(default)]
    pub input_device: Option<String>,
    pub max_recording_seconds: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub temp_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    #[serde(default)]
    pub mock_transcript: Option<String>,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub audio_transcriptions_endpoint: Option<String>,
    #[serde(default)]
    pub chat_completions_endpoint: Option<String>,
    #[serde(default = "default_openai_transcription_transport")]
    pub transcription_transport: OpenAiTranscriptionTransport,
    #[serde(default)]
    pub transcription_model: Option<String>,
    #[serde(default)]
    pub chat_model: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputConfig {
    pub mode: OutputMode,
    pub restore_clipboard: bool,
    #[serde(default = "default_clipboard_backend")]
    pub clipboard_backend: ClipboardBackendMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeculativeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_speculative_local_asr")]
    pub local_asr: String,
    #[serde(default = "default_speculative_cloud_correction")]
    pub cloud_correction: String,
    #[serde(default)]
    pub external_asr_command: Option<String>,
    #[serde(default = "default_speculative_streaming_service")]
    pub streaming_service: Option<SpeculativeStreamingServiceConfig>,
    #[serde(default = "default_speculative_max_patch_age_ms")]
    pub max_patch_age_ms: u64,
    #[serde(default = "default_speculative_max_auto_patch_edit_ratio")]
    pub max_auto_patch_edit_ratio: f32,
}

impl Eq for SpeculativeConfig {}

impl Default for SpeculativeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            local_asr: default_speculative_local_asr(),
            cloud_correction: default_speculative_cloud_correction(),
            external_asr_command: None,
            streaming_service: default_speculative_streaming_service(),
            max_patch_age_ms: default_speculative_max_patch_age_ms(),
            max_auto_patch_edit_ratio: default_speculative_max_auto_patch_edit_ratio(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeculativeStreamingServiceConfig {
    #[serde(default = "default_speculative_streaming_service_endpoint")]
    pub endpoint: String,
    #[serde(default = "default_speculative_streaming_service_sample_rate_hz")]
    pub sample_rate_hz: u32,
    #[serde(default = "default_speculative_streaming_service_channels")]
    pub channels: u16,
    #[serde(default = "default_speculative_streaming_service_connect_timeout_ms")]
    pub connect_timeout_ms: u64,
    #[serde(default = "default_speculative_streaming_service_idle_timeout_ms")]
    pub idle_timeout_ms: u64,
    #[serde(default = "default_speculative_streaming_service_final_timeout_ms")]
    pub final_timeout_ms: u64,
    #[serde(default)]
    pub local_daemon: Option<SpeculativeLocalAsrDaemonConfig>,
}

impl Default for SpeculativeStreamingServiceConfig {
    fn default() -> Self {
        Self {
            endpoint: default_speculative_streaming_service_endpoint(),
            sample_rate_hz: default_speculative_streaming_service_sample_rate_hz(),
            channels: default_speculative_streaming_service_channels(),
            connect_timeout_ms: default_speculative_streaming_service_connect_timeout_ms(),
            idle_timeout_ms: default_speculative_streaming_service_idle_timeout_ms(),
            final_timeout_ms: default_speculative_streaming_service_final_timeout_ms(),
            local_daemon: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeculativeLocalAsrDaemonMode {
    DryRun,
    SherpaOnline,
}

impl SpeculativeLocalAsrDaemonMode {
    pub fn as_daemon_arg(self) -> &'static str {
        match self {
            Self::DryRun => "dry-run",
            Self::SherpaOnline => "sherpa-online",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeculativeSherpaOnlineModelFamily {
    Transducer,
    Paraformer,
}

impl SpeculativeSherpaOnlineModelFamily {
    pub fn as_daemon_arg(self) -> &'static str {
        match self {
            Self::Transducer => "transducer",
            Self::Paraformer => "paraformer",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpeculativeLocalAsrDaemonConfig {
    #[serde(default = "default_speculative_local_asr_daemon_mode")]
    pub mode: SpeculativeLocalAsrDaemonMode,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub dry_run_text: Option<String>,
    #[serde(default)]
    pub dry_run_partial_text: Option<String>,
    #[serde(default = "default_speculative_sherpa_online_model_family")]
    pub model_family: SpeculativeSherpaOnlineModelFamily,
    #[serde(default)]
    pub tokens: Option<PathBuf>,
    #[serde(default)]
    pub encoder: Option<PathBuf>,
    #[serde(default)]
    pub decoder: Option<PathBuf>,
    #[serde(default)]
    pub joiner: Option<PathBuf>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub num_threads: Option<u32>,
    #[serde(default)]
    pub sample_rate_hz: Option<u32>,
    #[serde(default)]
    pub decoding_method: Option<String>,
    #[serde(default)]
    pub hotwords_file: Option<PathBuf>,
    #[serde(default)]
    pub rule_fsts: Option<PathBuf>,
    #[serde(default)]
    pub rule_fars: Option<PathBuf>,
}

impl Default for SpeculativeLocalAsrDaemonConfig {
    fn default() -> Self {
        Self {
            mode: default_speculative_local_asr_daemon_mode(),
            engine: None,
            model: None,
            dry_run_text: None,
            dry_run_partial_text: None,
            model_family: default_speculative_sherpa_online_model_family(),
            tokens: None,
            encoder: None,
            decoder: None,
            joiner: None,
            provider: None,
            num_threads: None,
            sample_rate_hz: None,
            decoding_method: None,
            hotwords_file: None,
            rule_fsts: None,
            rule_fars: None,
        }
    }
}

fn default_voice_mode() -> VoiceMode {
    VoiceMode::Smart
}

fn default_speculative_local_asr() -> String {
    "mock".to_string()
}

fn default_speculative_cloud_correction() -> String {
    "disabled".to_string()
}

fn default_speculative_streaming_service() -> Option<SpeculativeStreamingServiceConfig> {
    Some(SpeculativeStreamingServiceConfig::default())
}

fn default_speculative_streaming_service_endpoint() -> String {
    "ws://127.0.0.1:53171/asr".to_string()
}

fn default_speculative_streaming_service_sample_rate_hz() -> u32 {
    16_000
}

fn default_speculative_streaming_service_channels() -> u16 {
    1
}

fn default_speculative_streaming_service_connect_timeout_ms() -> u64 {
    1_000
}

fn default_speculative_streaming_service_idle_timeout_ms() -> u64 {
    3_000
}

fn default_speculative_streaming_service_final_timeout_ms() -> u64 {
    7_000
}

fn default_speculative_local_asr_daemon_mode() -> SpeculativeLocalAsrDaemonMode {
    SpeculativeLocalAsrDaemonMode::DryRun
}

fn default_speculative_sherpa_online_model_family() -> SpeculativeSherpaOnlineModelFamily {
    SpeculativeSherpaOnlineModelFamily::Transducer
}

fn default_speculative_max_patch_age_ms() -> u64 {
    2_000
}

fn default_speculative_max_auto_patch_edit_ratio() -> f32 {
    0.25
}

fn default_clipboard_backend() -> ClipboardBackendMode {
    ClipboardBackendMode::Fallback
}

fn default_audio_backend() -> AudioBackendMode {
    AudioBackendMode::Silent
}

fn default_openai_transcription_transport() -> OpenAiTranscriptionTransport {
    OpenAiTranscriptionTransport::AudioTranscriptions
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Idle,
    Recording,
    Transcribing,
    Processing,
    Inserting,
    Completed,
    Failed,
    Cancelled,
}

impl SessionStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceEventKind {
    TriggerStart,
    TriggerStop,
    TriggerCancel,
    TranscriptReady,
    ProcessedTextReady,
    InsertSucceeded,
    InsertFailed,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoiceEvent {
    TriggerStart,
    TriggerStop,
    TriggerCancel,
    TranscriptReady { text: String },
    ProcessedTextReady { text: String },
    InsertSucceeded,
    InsertFailed { reason: String },
    Error { reason: String },
}

impl VoiceEvent {
    pub fn kind(&self) -> VoiceEventKind {
        match self {
            VoiceEvent::TriggerStart => VoiceEventKind::TriggerStart,
            VoiceEvent::TriggerStop => VoiceEventKind::TriggerStop,
            VoiceEvent::TriggerCancel => VoiceEventKind::TriggerCancel,
            VoiceEvent::TranscriptReady { .. } => VoiceEventKind::TranscriptReady,
            VoiceEvent::ProcessedTextReady { .. } => VoiceEventKind::ProcessedTextReady,
            VoiceEvent::InsertSucceeded => VoiceEventKind::InsertSucceeded,
            VoiceEvent::InsertFailed { .. } => VoiceEventKind::InsertFailed,
            VoiceEvent::Error { .. } => VoiceEventKind::Error,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceSession {
    id: String,
    status: SessionStatus,
    transcript: Option<String>,
    output_text: Option<String>,
    error: Option<String>,
}

impl VoiceSession {
    pub fn new_for_test(id: impl Into<String>) -> Self {
        Self::new(id)
    }

    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status: SessionStatus::Idle,
            transcript: None,
            output_text: None,
            error: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn status(&self) -> SessionStatus {
        self.status
    }

    pub fn transcript(&self) -> Option<&str> {
        self.transcript.as_deref()
    }

    pub fn output_text(&self) -> Option<&str> {
        self.output_text.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn apply(&mut self, event: VoiceEvent) -> Result<(), TalkError> {
        if self.status.is_terminal() {
            return Err(TalkError::TerminalTransition {
                state: self.status,
                event: event.kind(),
            });
        }

        match (self.status, event) {
            (SessionStatus::Idle, VoiceEvent::TriggerStart) => {
                self.status = SessionStatus::Recording;
                Ok(())
            }
            (SessionStatus::Recording, VoiceEvent::TriggerStop) => {
                self.status = SessionStatus::Transcribing;
                Ok(())
            }
            (SessionStatus::Transcribing, VoiceEvent::TranscriptReady { text }) => {
                self.transcript = Some(text);
                self.status = SessionStatus::Processing;
                Ok(())
            }
            (SessionStatus::Processing, VoiceEvent::ProcessedTextReady { text }) => {
                self.output_text = Some(text);
                self.status = SessionStatus::Inserting;
                Ok(())
            }
            (SessionStatus::Inserting, VoiceEvent::InsertSucceeded) => {
                self.status = SessionStatus::Completed;
                Ok(())
            }
            (_, VoiceEvent::TriggerCancel) => {
                self.status = SessionStatus::Cancelled;
                Ok(())
            }
            (_, VoiceEvent::Error { reason }) | (_, VoiceEvent::InsertFailed { reason }) => {
                self.error = Some(reason);
                self.status = SessionStatus::Failed;
                Ok(())
            }
            (state, event) => Err(TalkError::InvalidTransition {
                state,
                event: event.kind(),
            }),
        }
    }
}

impl fmt::Display for VoiceSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{:?}", self.id, self.status)
    }
}
