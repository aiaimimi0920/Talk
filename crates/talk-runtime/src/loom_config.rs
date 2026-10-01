use std::sync::OnceLock;
use std::time::Duration;
use talk_core::TalkConfig;

/// Fail fast when the Loom host is unreachable instead of hanging startup.
const LOOM_HTTP_CONNECT_TIMEOUT_SECS: u64 = 5;
/// Bound the whole request so a stalled or half-open connection cannot hang
/// the startup configuration path indefinitely (`reqwest::Client::new()` has
/// no timeout at all).
const LOOM_HTTP_REQUEST_TIMEOUT_SECS: u64 = 15;

fn shared_loom_http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(LOOM_HTTP_CONNECT_TIMEOUT_SECS))
            .timeout(Duration::from_secs(LOOM_HTTP_REQUEST_TIMEOUT_SECS))
            .build()
            .expect("static Loom HTTP client configuration must be valid")
    })
}

#[derive(Debug, serde::Deserialize)]
struct LoomClaim {
    managed: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct LoomTalkConfigResponse {
    pub created: bool,
    pub document: LoomDocumentMetadata,
    pub config: TalkConfig,
}

#[derive(Debug, serde::Deserialize)]
pub struct LoomDocumentMetadata {
    pub revision: u64,
}

#[derive(Debug, serde::Serialize)]
struct PutTalkConfigRequest<'a> {
    expected_revision: u64,
    config: &'a TalkConfig,
}

pub async fn is_talk_managed(base_url: &str, auth_token: Option<&str>) -> Result<bool, String> {
    let client = shared_loom_http_client();
    let mut request = client.get(format!(
        "{}/v1/configuration/claims?app=talk",
        base_url.trim_end_matches('/')
    ));
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }
    let claim = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<LoomClaim>()
        .await
        .map_err(|error| error.to_string())?;
    Ok(claim.managed)
}

pub async fn read_talk_config(
    base_url: &str,
    auth_token: Option<&str>,
) -> Result<LoomTalkConfigResponse, String> {
    let client = shared_loom_http_client();
    let mut request = client.get(format!(
        "{}/v1/configuration/apps/talk",
        base_url.trim_end_matches('/')
    ));
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<LoomTalkConfigResponse>()
        .await
        .map_err(|error| error.to_string())?;
    response
        .config
        .validate()
        .map_err(|error| error.to_string())?;
    Ok(response)
}

pub async fn write_talk_config(
    base_url: &str,
    auth_token: Option<&str>,
    expected_revision: u64,
    config: &TalkConfig,
) -> Result<LoomTalkConfigResponse, String> {
    let client = shared_loom_http_client();
    let mut request = client
        .put(format!(
            "{}/v1/configuration/apps/talk",
            base_url.trim_end_matches('/')
        ))
        .json(&PutTalkConfigRequest {
            expected_revision,
            config,
        });
    if let Some(token) = auth_token {
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<LoomTalkConfigResponse>()
        .await
        .map_err(|error| error.to_string())?;
    response
        .config
        .validate()
        .map_err(|error| error.to_string())?;
    Ok(response)
}
