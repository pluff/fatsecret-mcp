//! Environment configuration for the HTTP / OAuth authorization server.

use std::collections::HashMap;
use std::env;

use tracing::warn;
use uuid::Uuid;

const DEFAULT_REDIRECT_URIS: &str =
    "https://claude.ai/api/mcp/auth_callback,https://claude.com/api/mcp/auth_callback";

#[derive(Clone)]
pub struct Config {
    /// Key material for sealing codes and tokens (`OAUTH_SECRET`, random if unset).
    pub oauth_secret: String,
    /// Redirect URIs accepted at dynamic registration and for the static client.
    pub allowed_redirect_uris: Vec<String>,
    /// Optional pre-registered client (`OAUTH_CLIENT_ID` / `OAUTH_CLIENT_SECRET`).
    pub static_client: Option<(String, String)>,
    /// `https://host` override for externally visible URLs (`PUBLIC_HOST`).
    pub public_url: Option<String>,
}

fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

impl Config {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| env::var(key).ok())
    }

    pub fn from_map(map: &HashMap<String, String>) -> Self {
        Self::from_lookup(|key| map.get(key).cloned())
    }

    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let nonempty = |name: &str| get(name).filter(|v| !v.trim().is_empty());
        let oauth_secret = nonempty("OAUTH_SECRET").unwrap_or_else(|| {
            warn!(
                "OAUTH_SECRET not set: using a random secret, all OAuth tokens become invalid on restart"
            );
            format!("{}{}", Uuid::new_v4(), Uuid::new_v4())
        });

        let static_client = match (nonempty("OAUTH_CLIENT_ID"), get("OAUTH_CLIENT_SECRET")) {
            (Some(id), Some(secret)) => Some((id, secret)),
            _ => None,
        };
        let allowed_redirect_uris = split_list(
            &nonempty("OAUTH_ALLOWED_REDIRECT_URIS")
                .unwrap_or_else(|| DEFAULT_REDIRECT_URIS.to_string()),
        );
        let public_url = nonempty("PUBLIC_HOST")
            .map(|h| {
                let h = h.trim().trim_end_matches('/');
                let h = h.strip_prefix("https://").unwrap_or(h);
                format!("https://{h}")
            })
            .filter(|u| u.len() > "https://".len());

        Self {
            oauth_secret,
            allowed_redirect_uris,
            static_client,
            public_url,
        }
    }

    pub fn redirect_allowed(&self, uri: &str) -> bool {
        self.allowed_redirect_uris.iter().any(|u| u == uri)
    }
}

#[cfg(test)]
pub(crate) fn config_from(pairs: &[(&str, &str)]) -> Config {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    Config::from_map(&map)
}

/// Config with a fixed `OAUTH_SECRET`.
#[cfg(test)]
pub(crate) fn test_config() -> Config {
    config_from(&[("OAUTH_SECRET", "test-secret")])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_env_is_valid() {
        let c = config_from(&[]);
        assert!(c.static_client.is_none());
        assert!(c.public_url.is_none());
        // Random fallback secret is generated.
        assert!(c.oauth_secret.len() >= 64);
    }

    #[test]
    fn defaults_and_normalization() {
        let c = test_config();
        assert_eq!(c.oauth_secret, "test-secret");
        assert!(c.redirect_allowed("https://claude.ai/api/mcp/auth_callback"));
        assert!(c.redirect_allowed("https://claude.com/api/mcp/auth_callback"));
        assert!(!c.redirect_allowed("https://evil.example/cb"));
        assert!(!c.redirect_allowed("https://claude.ai/api/mcp/auth_callback/"));
    }

    #[test]
    fn custom_redirects_static_client_public_host() {
        let c = config_from(&[
            (
                "OAUTH_ALLOWED_REDIRECT_URIS",
                " https://a/cb , https://b/cb ",
            ),
            ("OAUTH_CLIENT_ID", "sid"),
            ("OAUTH_CLIENT_SECRET", "ssecret"),
            ("PUBLIC_HOST", "mcp.example.com/"),
        ]);
        assert!(c.redirect_allowed("https://a/cb"));
        assert!(!c.redirect_allowed("https://claude.ai/api/mcp/auth_callback"));
        assert_eq!(c.static_client, Some(("sid".into(), "ssecret".into())));
        assert_eq!(c.public_url.as_deref(), Some("https://mcp.example.com"));
    }
}
