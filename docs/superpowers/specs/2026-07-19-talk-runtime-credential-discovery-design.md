# Talk Runtime Credential Discovery Design

**Status:** Approved for implementation

**Goal:** Restore the complete local-ASR-to-cloud-processing Talk workflow when users launch `Talk.exe` directly, while continuing to support credential-free releases and local transcript fallback.

## Root Cause

The existing local DashScope credential is stored in:

```text
%USERPROFILE%\.neuro\qwen-platform\qwen-dashscope-openai\api-key\manual-live.json
```

The former PowerShell launcher discovered that file and injected its `apiKey` value into `TALK_PROVIDER_API_KEY` before starting Talk. The single-EXE release intentionally bypasses the launcher and contains only `api_key_env = "TALK_PROVIDER_API_KEY"`, so a process started by double-clicking the EXE cannot see the local JSON credential when no Windows environment variable is configured.

## Runtime Resolution

For `openai_compatible` providers, Talk will resolve credentials in this order:

1. Nonblank `provider.api_key` from `talk.toml`.
2. A nonblank process environment value named by `provider.api_key_env`.
3. The nonblank `apiKey`, `api_key`, or `key` field from the existing per-user `manual-live.json` file.
4. No credential.

The legacy JSON fallback is intended for the packaged DashScope configuration. It will only be considered when the configured OpenAI-compatible endpoints use the `dashscope.aliyuncs.com` HTTPS host, preventing an automatically discovered DashScope credential from being sent to an unrelated custom endpoint.

The same resolver will be used by both the availability check and the provider client construction. This prevents Talk from deciding that cloud processing is available through one code path and then failing to obtain the key through another.

## Failure Behavior

- A missing credential file means credentials are unavailable; it is not a runtime error.
- Invalid JSON, unreadable files, blank recognized fields, or whitespace-padded values are ignored as unavailable credentials.
- Local ASR remains usable without cloud credentials, preserving the recognized transcript as the final output.
- If a credential is resolved but the provider request fails, the existing provider error handling remains in effect.
- Credential values must not be included in error messages, session logs, release manifests, or test output.

## Release Contract

Published packages remain exactly:

```text
Talk.exe
talk.toml
```

The release configuration continues to use `api_key_env = "TALK_PROVIDER_API_KEY"` and must not contain `provider.api_key`. Credential discovery happens only at runtime on the user's machine; packaging and GitHub Actions do not copy local credentials into an artifact.

## Test Strategy

Tests will establish the following behavior before implementation:

- An explicit config key takes precedence over every other source.
- The configured environment variable takes precedence over the legacy JSON file.
- A valid legacy JSON file supplies a credential when the environment variable is absent.
- All supported legacy JSON field names are recognized.
- Missing, invalid, blank, or whitespace-padded JSON values result in unavailable credentials without failing the local transcript path.
- The legacy JSON credential is not used for non-DashScope endpoints.
- Error and debug representations do not expose the credential value.
- Release packaging still produces only `Talk.exe` and `talk.toml`, with no inline API key.

## Acceptance Criteria

- Double-clicking the packaged `Talk.exe` on the current machine reuses the existing local DashScope credential.
- A real Alt-triggered session completes local recognition, cloud smart processing, and output insertion.
- Removing all credential sources still completes with the local transcript rather than a failed session.
- Existing explicit config and environment-variable setups keep working with their current precedence.
- The generated release contains no API key and no PowerShell launcher.
