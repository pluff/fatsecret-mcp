//! Environment configuration for the HTTP / OAuth authorization server.

use std::collections::HashMap;
use std::env;

use tracing::warn;
use uuid::Uuid;

/// Claude and Gemini (Google's custom-MCP redirect proxy: prod, sandbox, test).
const DEFAULT_REDIRECT_URIS: &str = "https://claude.ai/api/mcp/auth_callback,\
https://claude.com/api/mcp/auth_callback,\
https://oauth-redirect.googleusercontent.com/*,\
https://oauth-redirect-sandbox.googleusercontent.com/*,\
https://oauth-redirect-test.googleusercontent.com/*";

#[derive(Clone)]
pub struct Config {
    /// Key material for sealing codes and tokens (`OAUTH_SECRET`, random if unset).
    pub oauth_secret: String,
    /// Redirect URIs accepted at dynamic registration and for the static client.
    pub allowed_redirect_uris: Vec<String>,
    /// Also accept loopback redirects (`http://localhost|127.0.0.1|[::1]:<any port>/...`) for
    /// native clients such as Gemini CLI. On only when `OAUTH_ALLOWED_REDIRECT_URIS` is unset.
    pub allow_loopback_redirects: bool,
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
        let custom_redirects = nonempty("OAUTH_ALLOWED_REDIRECT_URIS");
        let allow_loopback_redirects = custom_redirects.is_none();
        let allowed_redirect_uris =
            split_list(&custom_redirects.unwrap_or_else(|| DEFAULT_REDIRECT_URIS.to_string()));
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
            allow_loopback_redirects,
            static_client,
            public_url,
        }
    }

    pub fn redirect_allowed(&self, uri: &str) -> bool {
        self.allowed_redirect_uris
            .iter()
            .any(|u| entry_matches(u, uri))
            || (self.allow_loopback_redirects && is_loopback_redirect(uri))
    }
}

/// Exact match, or prefix match when the entry ends in `/*` (the `*` stands for anything after
/// that slash, so the scheme and host must match exactly).
fn entry_matches(entry: &str, uri: &str) -> bool {
    match entry.strip_suffix('*') {
        Some(prefix) if prefix.ends_with('/') => uri.starts_with(prefix),
        _ => entry == uri,
    }
}

/// `http://localhost`, `http://127.0.0.1` or `http://[::1]`, optional port, optional path.
/// No userinfo, so `http://localhost@evil.com` does not match.
fn is_loopback_redirect(uri: &str) -> bool {
    let Some(rest) = uri.strip_prefix("http://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = if let Some(v6) = authority.strip_prefix("[::1]") {
        if !(v6.is_empty() || v6.starts_with(':')) {
            return false;
        }
        return v6.is_empty() || v6[1..].bytes().all(|b| b.is_ascii_digit());
    } else {
        match authority.split_once(':') {
            Some((h, port)) => {
                if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
                    return false;
                }
                h
            }
            None => authority,
        }
    };
    matches!(host, "localhost" | "127.0.0.1")
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
    fn google_prefix_defaults_and_wildcards() {
        let c = test_config();
        for ok in [
            "https://oauth-redirect.googleusercontent.com/r/user_bound_custom-mcp-abc",
            "https://oauth-redirect-sandbox.googleusercontent.com/a/user_bound_custom-mcp-abc",
            "https://oauth-redirect-test.googleusercontent.com/r/x",
        ] {
            assert!(c.redirect_allowed(ok), "{ok}");
        }
        for bad in [
            "https://oauth-redirect.googleusercontent.com.evil.com/r/x",
            "https://oauth-redirect.googleusercontent.com@evil.com/r/x",
            "http://oauth-redirect.googleusercontent.com/r/x",
            "https://evil.com/https://oauth-redirect.googleusercontent.com/r/x",
            "https://oauth-redirect.googleusercontent.com",
        ] {
            assert!(!c.redirect_allowed(bad), "{bad}");
        }
        // `*` only counts as a wildcard after a slash.
        let c = config_from(&[(
            "OAUTH_ALLOWED_REDIRECT_URIS",
            "https://a.com*,https://b.com/x/*",
        )]);
        assert!(!c.redirect_allowed("https://a.com.evil.com/x"));
        assert!(c.redirect_allowed("https://b.com/x/y"));
        assert!(!c.redirect_allowed("https://b.com/xy"));
    }

    #[test]
    fn loopback_redirects_default_only() {
        let c = test_config();
        for ok in [
            "http://localhost:7777/oauth/callback",
            "http://127.0.0.1:54321/cb",
            "http://localhost/cb",
            "http://[::1]:8080/cb",
        ] {
            assert!(c.redirect_allowed(ok), "{ok}");
        }
        for bad in [
            "https://localhost:7777/cb",
            "http://localhost@evil.com/cb",
            "http://localhost.evil.com/cb",
            "http://127.0.0.1.evil.com:80/cb",
            "http://localhost:/cb",
            "http://localhost:abc/cb",
            "http://[::1]x/cb",
        ] {
            assert!(!c.redirect_allowed(bad), "{bad}");
        }
        let custom = config_from(&[("OAUTH_ALLOWED_REDIRECT_URIS", "https://a/cb")]);
        assert!(!custom.redirect_allowed("http://localhost:7777/cb"));
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
