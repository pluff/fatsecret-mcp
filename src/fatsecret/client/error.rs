use thiserror::Error;

#[derive(Error, Debug)]
pub enum FatSecretError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },

    #[error("FatSecret API credentials missing on server.")]
    MissingApiCredentials,

    #[error("Profile access not configured for this user. Re-authenticate via Claude.ai.")]
    ProfileNotAuthorized,

    #[error("OAuth2 token request failed: {status} {body}")]
    TokenRequest { status: u16, body: String },

    #[error("Failed to get request token: {status} {body}")]
    RequestToken { status: u16, body: String },

    #[error("Failed to get access token: {status} {body}")]
    AccessToken { status: u16, body: String },

    #[error("Invalid request token response: {0}")]
    InvalidRequestTokenResponse(String),

    #[error("Invalid access token response: {0}")]
    InvalidAccessTokenResponse(String),

    #[error("Serialization error: {message}. Body: {body}")]
    SerializationWithBody { message: String, body: String },
}

impl FatSecretError {
    /// HTTP 429, or FatSecret API error code 12 (rate limit) in the response body.
    pub fn is_rate_limited(&self) -> bool {
        let FatSecretError::Api { status, message } = self else {
            return false;
        };
        if *status == 429 {
            return true;
        }
        serde_json::from_str::<serde_json::Value>(message)
            .ok()
            .and_then(|v| {
                v["error"]["code"]
                    .as_i64()
                    .or_else(|| v["error"]["code"].as_str()?.parse().ok())
            })
            == Some(12)
    }
}

pub type Result<T> = std::result::Result<T, FatSecretError>;
