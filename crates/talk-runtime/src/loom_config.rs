use talk_core::TalkConfig;

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
    let client = reqwest::Client::new();
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
    let client = reqwest::Client::new();
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
    let client = reqwest::Client::new();
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
