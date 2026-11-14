use reqwest::{Client, Method};
use serde_json::Value;

use super::error::{FatSecretError, Result};
use crate::fatsecret::auth::OAuth2TokenCache;
use crate::fatsecret::oauth1::{OAuth1Credentials, build_oauth1_params, build_query};

pub const FATSECRET_BASE_URL: &str = "https://platform.fatsecret.com/rest";

pub struct FatSecretClient {
    http: Client,
    client_id: String,
    client_secret: String,
    oauth1: OAuth1Credentials,
    token_cache: OAuth2TokenCache,
    base_url: String,
}

impl FatSecretClient {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        oauth1: OAuth1Credentials,
    ) -> Self {
        let client_id = client_id.into();
        let client_secret = client_secret.into();
        Self {
            http: Client::new(),
            token_cache: OAuth2TokenCache::new(client_id.clone(), client_secret.clone()),
            client_id,
            client_secret,
            oauth1,
            base_url: FATSECRET_BASE_URL.to_string(),
        }
    }

    pub fn has_api_credentials(&self) -> bool {
        !self.client_id.is_empty()
            && !self.client_secret.is_empty()
            && !self.oauth1.consumer_secret.is_empty()
    }

    pub fn has_profile_tokens(&self) -> bool {
        self.oauth1
            .access_token
            .as_deref()
            .is_some_and(|t| !t.is_empty())
            && self
                .oauth1
                .access_token_secret
                .as_deref()
                .is_some_and(|t| !t.is_empty())
    }

    pub fn ensure_api_credentials(&self) -> Result<()> {
        if self.has_api_credentials() {
            Ok(())
        } else {
            Err(FatSecretError::MissingApiCredentials)
        }
    }

    pub fn ensure_profile_auth(&self) -> Result<()> {
        self.ensure_api_credentials()?;
        if self.has_profile_tokens() {
            Ok(())
        } else {
            Err(FatSecretError::ProfileNotAuthorized)
        }
    }

    async fn handle(&self, response: reqwest::Response) -> Result<Value> {
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(FatSecretError::Api {
                status: status.as_u16(),
                message: body,
            });
        }
        if body.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&body).map_err(|e| FatSecretError::SerializationWithBody {
            message: e.to_string(),
            body,
        })
    }

    fn with_format_json(mut query: Vec<(String, String)>) -> Vec<(String, String)> {
        if !query.iter().any(|(k, _)| k == "format") {
            query.push(("format".to_string(), "json".to_string()));
        }
        query
    }

    /// Public API GET (OAuth2 Bearer). `format=json` is added to the query.
    pub async fn public_get(&self, path: &str, query: &[(String, String)]) -> Result<Value> {
        self.ensure_api_credentials()?;
        let token = self.token_cache.get_token(&self.http).await?;
        let query = Self::with_format_json(query.to_vec());
        let response = self
            .http
            .get(format!("{}{}", self.base_url, path))
            .query(&query)
            .bearer_auth(token)
            .send()
            .await?;
        self.handle(response).await
    }

    /// Profile API request. OAuth1 params are signed into the query string
    /// (existing query parameters are included in the signature).
    pub async fn profile_request(
        &self,
        method: Method,
        path: &str,
        query: &[(String, String)],
    ) -> Result<Value> {
        self.ensure_profile_auth()?;
        let url = format!("{}{}", self.base_url, path);
        let query = Self::with_format_json(query.to_vec());
        let signed = build_oauth1_params(method.as_str(), &url, &self.oauth1, &query);
        let pairs: Vec<(String, String)> = signed.into_iter().collect();
        let full = format!("{url}?{}", build_query(&pairs));
        let response = self.http.request(method, full).send().await?;
        self.handle(response).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(secret: &str) -> OAuth1Credentials {
        OAuth1Credentials {
            consumer_key: "ck".into(),
            consumer_secret: secret.into(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn missing_credentials_message() {
        let c = FatSecretClient::new("", "", creds(""));
        let err = c.public_get("/x", &[]).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "FatSecret API credentials missing on server."
        );
    }

    #[tokio::test]
    async fn missing_profile_tokens_message() {
        let c = FatSecretClient::new("id", "sec", creds("cs"));
        assert!(c.has_api_credentials());
        let err = c.profile_request(Method::GET, "/x", &[]).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "Profile access not configured for this user. Re-authenticate via Claude.ai."
        );
    }

    #[test]
    fn format_json_added_once() {
        let q = FatSecretClient::with_format_json(vec![("a".into(), "1".into())]);
        assert!(q.contains(&("format".to_string(), "json".to_string())));
        let q = FatSecretClient::with_format_json(q);
        assert_eq!(q.iter().filter(|(k, _)| k == "format").count(), 1);
    }
}
