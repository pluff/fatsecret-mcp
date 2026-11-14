//! OAuth 2.0 client_credentials token cache (app-level, scope `basic`).

use std::time::{Duration, Instant};

use reqwest::Client;
use serde::Deserialize;
use tokio::sync::Mutex;

use super::client::error::{FatSecretError, Result};

const TOKEN_URL: &str = "https://oauth.fatsecret.com/connect/token";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
}

struct CachedToken {
    token: String,
    expires_at: Instant,
}

pub struct OAuth2TokenCache {
    client_id: String,
    client_secret: String,
    cached: Mutex<Option<CachedToken>>,
}

impl OAuth2TokenCache {
    pub fn new(client_id: impl Into<String>, client_secret: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            cached: Mutex::new(None),
        }
    }

    /// Return a cached token or fetch a new one (expiry is `expires_in - 60s`).
    pub async fn get_token(&self, http: &Client) -> Result<String> {
        let mut guard = self.cached.lock().await;
        if let Some(c) = guard.as_ref()
            && Instant::now() < c.expires_at
        {
            return Ok(c.token.clone());
        }

        let response = http
            .post(TOKEN_URL)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("scope", "basic"),
            ])
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(FatSecretError::TokenRequest {
                status: status.as_u16(),
                body,
            });
        }
        let body = response.text().await?;
        let data: TokenResponse =
            serde_json::from_str(&body).map_err(|e| FatSecretError::SerializationWithBody {
                message: e.to_string(),
                body,
            })?;

        let ttl = (data.expires_in - 60).max(0) as u64;
        *guard = Some(CachedToken {
            token: data.access_token.clone(),
            expires_at: Instant::now() + Duration::from_secs(ttl),
        });
        Ok(data.access_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cached_token_is_reused_without_network() {
        let cache = OAuth2TokenCache::new("id", "secret");
        *cache.cached.lock().await = Some(CachedToken {
            token: "tok".into(),
            expires_at: Instant::now() + Duration::from_secs(300),
        });
        let t = cache.get_token(&Client::new()).await.unwrap();
        assert_eq!(t, "tok");
    }

    #[test]
    fn token_response_parses() {
        let r: TokenResponse =
            serde_json::from_str(r#"{"access_token":"a","expires_in":86400,"x":1}"#).unwrap();
        assert_eq!(r.access_token, "a");
        assert_eq!(r.expires_in, 86400);
    }
}
