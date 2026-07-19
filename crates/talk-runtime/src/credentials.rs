#[cfg(test)]
mod tests {
    use super::{
        resolve_provider_credential_with, ProviderCredential, ProviderCredentialSource,
    };
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
            |name| {
                (name == "TALK_PROVIDER_API_KEY").then(|| "environment-key".to_string())
            },
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
            |name| {
                (name == "TALK_PROVIDER_API_KEY").then(|| "environment-key".to_string())
            },
            Some(&legacy_path),
        );

        assert_eq!(
            credential.source(),
            ProviderCredentialSource::Environment
        );
        assert_eq!(credential.api_key(), Some("environment-key"));
    }

    #[test]
    fn legacy_json_accepts_supported_key_fields() {
        let config = openai_config();

        for field in ["apiKey", "api_key", "key"] {
            let legacy_path = write_legacy_json(&format!(r#"{{"{field}":"legacy-key"}}"#));
            let credential = resolve_without_environment(&config, Some(&legacy_path));

            assert_eq!(
                credential.source(),
                ProviderCredentialSource::LegacyJson
            );
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

            assert_eq!(
                credential.source(),
                ProviderCredentialSource::Unavailable
            );
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

        assert_eq!(
            credential.source(),
            ProviderCredentialSource::Unavailable
        );
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

        assert_eq!(
            credential.source(),
            ProviderCredentialSource::Unavailable
        );
        assert_eq!(credential.api_key(), None);
    }
}
