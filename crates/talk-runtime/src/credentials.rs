use std::env;
use std::path::{Path, PathBuf};

use talk_core::{ProviderKind, TalkConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderCredentialSource {
    ExplicitConfig,
    Environment,
    LegacyJson,
    Unavailable,
}

pub(crate) struct ProviderCredential {
    source: ProviderCredentialSource,
    api_key: Option<String>,
}

impl ProviderCredential {
    pub(crate) fn is_available(&self) -> bool {
        self.source != ProviderCredentialSource::Unavailable && self.api_key.is_some()
    }

    #[cfg(test)]
    pub(crate) fn source(&self) -> ProviderCredentialSource {
        self.source
    }

    #[cfg(test)]
    pub(crate) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub(crate) fn into_api_key(self) -> Option<String> {
        self.api_key
    }
}

pub(crate) fn resolve_provider_credential(config: &TalkConfig) -> ProviderCredential {
    let legacy_json_path = default_legacy_json_path();
    resolve_provider_credential_with(
        config,
        |name| env::var(name).ok(),
        legacy_json_path.as_deref(),
    )
}

pub(crate) fn resolve_provider_credential_with<F>(
    config: &TalkConfig,
    env_lookup: F,
    legacy_json_path: Option<&Path>,
) -> ProviderCredential
where
    F: Fn(&str) -> Option<String>,
{
    if config.provider.kind != ProviderKind::OpenAiCompatible {
        return unavailable();
    }

    if let Some(api_key) = config.provider.api_key.as_deref().and_then(valid_key) {
        return available(ProviderCredentialSource::ExplicitConfig, api_key);
    }

    if let Some(env_name) = config.provider.api_key_env.as_deref() {
        if let Some(value) = env_lookup(env_name) {
            if let Some(api_key) = valid_key(&value) {
                return available(ProviderCredentialSource::Environment, api_key);
            }
        }
    }

    if is_dashscope_config(config) {
        if let Some(value) = legacy_json_path.and_then(read_legacy_json_api_key) {
            if let Some(api_key) = valid_key(&value) {
                return available(ProviderCredentialSource::LegacyJson, api_key);
            }
        }
    }

    unavailable()
}

fn available(source: ProviderCredentialSource, api_key: &str) -> ProviderCredential {
    ProviderCredential {
        source,
        api_key: Some(api_key.to_string()),
    }
}

fn unavailable() -> ProviderCredential {
    ProviderCredential {
        source: ProviderCredentialSource::Unavailable,
        api_key: None,
    }
}

fn valid_key(value: &str) -> Option<&str> {
    (!value.trim().is_empty() && value.trim() == value).then_some(value)
}

fn read_legacy_json_api_key(path: &Path) -> Option<String> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<serde_json::Value>(&contents).ok()?;
    let object = value.as_object()?;

    ["apiKey", "api_key", "key"]
        .into_iter()
        .filter_map(|field| object.get(field).and_then(serde_json::Value::as_str))
        .find_map(|value| valid_key(value).map(str::to_string))
}

fn is_dashscope_config(config: &TalkConfig) -> bool {
    is_dashscope_endpoint(config.provider.audio_transcriptions_endpoint.as_deref())
        && is_dashscope_endpoint(config.provider.chat_completions_endpoint.as_deref())
}

fn is_dashscope_endpoint(value: Option<&str>) -> bool {
    let Some(value) = value else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(value) else {
        return false;
    };
    url.scheme() == "https" && url.host_str() == Some("dashscope.aliyuncs.com")
}

fn default_legacy_json_path() -> Option<PathBuf> {
    let home = env::var_os("USERPROFILE").or_else(|| env::var_os("HOME"))?;
    Some(
        PathBuf::from(home)
            .join(".neuro")
            .join("qwen-platform")
            .join("qwen-dashscope-openai")
            .join("api-key")
            .join("manual-live.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::{resolve_provider_credential_with, ProviderCredential, ProviderCredentialSource};
    use std::path::{Path, PathBuf};
    use talk_core::TalkConfig;
    use uuid::Uuid;

    fn openai_config() -> TalkConfig {
        TalkConfig::from_toml_str(
            r#"
[trigger]
mode = "toggle"
toggle_shortcut = "Alt"

[audio]
backend = "silent"
max_recording_seconds = 60
sample_rate_hz = 16000
channels = 1
temp_dir = ".runtime/audio"

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
clipboard_backend = "fallback"

[logging]
dir = ".runtime/logs"
"#,
        )
        .expect("credential test config should parse")
    }

    fn write_legacy_json(contents: &str) -> PathBuf {
        let root = std::env::temp_dir()
            .join("talk-runtime-credentials")
            .join(Uuid::new_v4().to_string());
        std::fs::create_dir_all(&root).expect("create credential fixture directory");
        let path = root.join("manual-live.json");
        std::fs::write(&path, contents).expect("write credential fixture");
        path
    }

    fn resolve_without_environment(
        config: &TalkConfig,
        legacy_json_path: Option<&Path>,
    ) -> ProviderCredential {
        resolve_provider_credential_with(config, |_| None, legacy_json_path)
    }

    #[test]
    fn explicit_config_key_has_highest_precedence() {
        let legacy_path = write_legacy_json(r#"{"apiKey":"legacy-key"}"#);
        let mut config = openai_config();
        config.provider.api_key = Some("configured-key".to_string());

        let credential = resolve_provider_credential_with(
            &config,
            |name| (name == "TALK_PROVIDER_API_KEY").then(|| "environment-key".to_string()),
            Some(&legacy_path),
        );

        assert_eq!(
            credential.source(),
            ProviderCredentialSource::ExplicitConfig
        );
        assert_eq!(credential.api_key(), Some("configured-key"));
    }

    #[test]
    fn environment_key_precedes_legacy_json() {
        let legacy_path = write_legacy_json(r#"{"apiKey":"legacy-key"}"#);
        let config = openai_config();

        let credential = resolve_provider_credential_with(
            &config,
            |name| (name == "TALK_PROVIDER_API_KEY").then(|| "environment-key".to_string()),
            Some(&legacy_path),
        );

        assert_eq!(credential.source(), ProviderCredentialSource::Environment);
        assert_eq!(credential.api_key(), Some("environment-key"));
    }

    #[test]
    fn legacy_json_accepts_supported_key_fields() {
        let config = openai_config();

        for field in ["apiKey", "api_key", "key"] {
            let legacy_path = write_legacy_json(&format!(r#"{{"{field}":"legacy-key"}}"#));
            let credential = resolve_without_environment(&config, Some(&legacy_path));

            assert_eq!(credential.source(), ProviderCredentialSource::LegacyJson);
            assert_eq!(credential.api_key(), Some("legacy-key"));
        }
    }

    #[test]
    fn invalid_legacy_values_are_unavailable() {
        let config = openai_config();

        for contents in [
            "not-json",
            r#"{"apiKey":""}"#,
            r#"{"apiKey":"   "}"#,
            r#"{"apiKey":" padded-key "}"#,
            r#"{"other":"value"}"#,
        ] {
            let legacy_path = write_legacy_json(contents);
            let credential = resolve_without_environment(&config, Some(&legacy_path));

            assert_eq!(credential.source(), ProviderCredentialSource::Unavailable);
            assert_eq!(credential.api_key(), None);
        }
    }

    #[test]
    fn legacy_json_is_not_used_for_non_dashscope_endpoints() {
        let legacy_path = write_legacy_json(r#"{"apiKey":"legacy-key"}"#);
        let mut config = openai_config();
        config.provider.audio_transcriptions_endpoint =
            Some("https://example.invalid/v1/audio/transcriptions".to_string());
        config.provider.chat_completions_endpoint =
            Some("https://example.invalid/v1/chat/completions".to_string());

        let credential = resolve_without_environment(&config, Some(&legacy_path));

        assert_eq!(credential.source(), ProviderCredentialSource::Unavailable);
        assert_eq!(credential.api_key(), None);
    }

    #[test]
    fn missing_legacy_json_is_unavailable() {
        let config = openai_config();
        let missing = std::env::temp_dir()
            .join("talk-runtime-credentials")
            .join(Uuid::new_v4().to_string())
            .join("manual-live.json");

        let credential = resolve_without_environment(&config, Some(&missing));

        assert_eq!(credential.source(), ProviderCredentialSource::Unavailable);
        assert_eq!(credential.api_key(), None);
    }
}
