//! OAuth 1.0 (HMAC-SHA1) signing and token exchange for FatSecret.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use reqwest::Client;
use sha1::Sha1;

use super::client::error::{FatSecretError, Result};

const REQUEST_TOKEN_URL: &str = "https://authentication.fatsecret.com/oauth/request_token";
const AUTHORIZE_URL: &str = "https://authentication.fatsecret.com/oauth/authorize";
const ACCESS_TOKEN_URL: &str = "https://authentication.fatsecret.com/oauth/access_token";

#[derive(Debug, Clone, Default)]
pub struct OAuth1Credentials {
    pub consumer_key: String,
    pub consumer_secret: String,
    pub access_token: Option<String>,
    pub access_token_secret: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuth1RequestTokenResult {
    pub oauth_token: String,
    pub oauth_token_secret: String,
    pub authorization_url: String,
}

#[derive(Debug, Clone)]
pub struct OAuth1AccessTokenResult {
    pub access_token: String,
    pub access_token_secret: String,
}

/// RFC 3986 percent-encoding: only `A-Z a-z 0-9 - . _ ~` are left as-is.
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Decode `application/x-www-form-urlencoded` text (`+` is a space).
pub(crate) fn form_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parse a query / form body into key-value pairs (order preserved).
pub(crate) fn parse_query(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (form_decode(k), form_decode(v)),
            None => (form_decode(p), String::new()),
        })
        .collect()
}

/// Serialize pairs as a query string using RFC 3986 encoding.
pub fn build_query(params: &[(String, String)]) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn generate_nonce() -> String {
    let bytes: [u8; 16] = rand::random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn generate_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

fn build_base_string(method: &str, url: &str, params: &BTreeMap<String, String>) -> String {
    // BTreeMap iterates in sorted key order, matching the JS key sort.
    let param_string = params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");
    format!(
        "{}&{}&{}",
        method.to_uppercase(),
        percent_encode(url),
        percent_encode(&param_string)
    )
}

fn sign(base_string: &str, consumer_secret: &str, token_secret: &str) -> String {
    use base64::Engine;
    let key = format!(
        "{}&{}",
        percent_encode(consumer_secret),
        percent_encode(token_secret)
    );
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(base_string.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
}

/// Build the full parameter set (oauth_* plus `extra`) with a signature.
pub fn build_oauth1_params(
    method: &str,
    url: &str,
    credentials: &OAuth1Credentials,
    extra: &[(String, String)],
) -> BTreeMap<String, String> {
    build_oauth1_params_with(
        method,
        url,
        credentials,
        extra,
        &generate_nonce(),
        &generate_timestamp(),
    )
}

pub(crate) fn build_oauth1_params_with(
    method: &str,
    url: &str,
    credentials: &OAuth1Credentials,
    extra: &[(String, String)],
    nonce: &str,
    timestamp: &str,
) -> BTreeMap<String, String> {
    let mut oauth = BTreeMap::new();
    oauth.insert(
        "oauth_consumer_key".to_string(),
        credentials.consumer_key.clone(),
    );
    oauth.insert("oauth_nonce".to_string(), nonce.to_string());
    oauth.insert(
        "oauth_signature_method".to_string(),
        "HMAC-SHA1".to_string(),
    );
    oauth.insert("oauth_timestamp".to_string(), timestamp.to_string());
    oauth.insert("oauth_version".to_string(), "1.0".to_string());
    if let Some(token) = credentials
        .access_token
        .as_deref()
        .filter(|t| !t.is_empty())
    {
        oauth.insert("oauth_token".to_string(), token.to_string());
    }

    // Extra params override oauth params on key collision, as in the JS spread.
    let mut all = oauth;
    for (k, v) in extra {
        all.insert(k.clone(), v.clone());
    }

    let base = build_base_string(method, url, &all);
    let signature = sign(
        &base,
        &credentials.consumer_secret,
        credentials.access_token_secret.as_deref().unwrap_or(""),
    );
    all.insert("oauth_signature".to_string(), signature);
    all
}

fn token_pair(body: &str) -> Option<(String, String)> {
    let params = parse_query(body);
    let get = |name: &str| {
        params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
    };
    Some((get("oauth_token")?, get("oauth_token_secret")?))
}

async fn signed_get(
    http: &Client,
    url: &str,
    credentials: &OAuth1Credentials,
    extra: &[(String, String)],
) -> Result<(u16, String)> {
    let params = build_oauth1_params("GET", url, credentials, extra);
    let pairs: Vec<(String, String)> = params.into_iter().collect();
    let response = http
        .get(format!("{url}?{}", build_query(&pairs)))
        .send()
        .await?;
    let status = response.status().as_u16();
    let body = response.text().await?;
    Ok((status, body))
}

pub async fn request_token(
    http: &Client,
    credentials: &OAuth1Credentials,
    callback_url: &str,
) -> Result<OAuth1RequestTokenResult> {
    let (status, body) = signed_get(
        http,
        REQUEST_TOKEN_URL,
        credentials,
        &[("oauth_callback".to_string(), callback_url.to_string())],
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(FatSecretError::RequestToken { status, body });
    }
    let (oauth_token, oauth_token_secret) =
        token_pair(&body).ok_or(FatSecretError::InvalidRequestTokenResponse(body))?;
    Ok(OAuth1RequestTokenResult {
        authorization_url: format!("{AUTHORIZE_URL}?oauth_token={oauth_token}"),
        oauth_token,
        oauth_token_secret,
    })
}

pub async fn access_token(
    http: &Client,
    credentials: &OAuth1Credentials,
    oauth_token: &str,
    oauth_token_secret: &str,
    verifier: &str,
) -> Result<OAuth1AccessTokenResult> {
    let token_credentials = OAuth1Credentials {
        access_token: Some(oauth_token.to_string()),
        access_token_secret: Some(oauth_token_secret.to_string()),
        ..credentials.clone()
    };
    let (status, body) = signed_get(
        http,
        ACCESS_TOKEN_URL,
        &token_credentials,
        &[("oauth_verifier".to_string(), verifier.to_string())],
    )
    .await?;
    if !(200..300).contains(&status) {
        return Err(FatSecretError::AccessToken { status, body });
    }
    let (access_token, access_token_secret) =
        token_pair(&body).ok_or(FatSecretError::InvalidAccessTokenResponse(body))?;
    Ok(OAuth1AccessTokenResult {
        access_token,
        access_token_secret,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_rfc3986() {
        assert_eq!(percent_encode("abc-._~123"), "abc-._~123");
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_encode("!'()*"), "%21%27%28%29%2A");
        assert_eq!(percent_encode("a+b/c=d&e"), "a%2Bb%2Fc%3Dd%26e");
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    #[test]
    fn form_decode_roundtrip() {
        assert_eq!(form_decode("a%20b+c%2B"), "a b c+");
        assert_eq!(form_decode("100%"), "100%");
        assert_eq!(form_decode("%zz"), "%zz");
        assert_eq!(form_decode("%41"), "A");
    }

    #[test]
    fn known_signature_vector() {
        // Twitter "Creating a signature" documentation example.
        let creds = OAuth1Credentials {
            consumer_key: "xvz1evFS4wEEPTGEFPHBog".into(),
            consumer_secret: "kAcSOqF21Fu85e7zjz7ZN2U4ZRhfV3WpwPAoE3Z7kBw".into(),
            access_token: Some("370773112-GmHxMAgYyLbNEtIKZeRNFsMKPR9EyMZeS9weJAEb".into()),
            access_token_secret: Some("LswwdoUaIvS8ltyTt5jkRh4J50vUPVVHtR2YPi5kE".into()),
        };
        let extra = vec![
            ("include_entities".to_string(), "true".to_string()),
            (
                "status".to_string(),
                "Hello Ladies + Gentlemen, a signed OAuth request!".to_string(),
            ),
        ];
        let params = build_oauth1_params_with(
            "POST",
            "https://api.twitter.com/1.1/statuses/update.json",
            &creds,
            &extra,
            "kYjzVBB8Y0ZFabxSWbWovY3uYSQ2pTgmZeNu2VS4cg",
            "1318622958",
        );
        assert_eq!(params["oauth_signature"], "hCtSmYh+iHYCEqBWrE7C7hYmtUk=");
    }

    #[test]
    fn params_without_token() {
        let creds = OAuth1Credentials {
            consumer_key: "ck".into(),
            consumer_secret: "cs".into(),
            ..Default::default()
        };
        let p = build_oauth1_params("GET", "https://x.test/a", &creds, &[]);
        assert!(!p.contains_key("oauth_token"));
        assert_eq!(p["oauth_signature_method"], "HMAC-SHA1");
        assert_eq!(p["oauth_version"], "1.0");
        assert_eq!(p["oauth_nonce"].len(), 32);
        assert!(p.contains_key("oauth_signature"));
    }

    #[test]
    fn token_pair_parsing() {
        assert_eq!(
            token_pair("oauth_token=a&oauth_token_secret=b&x=1"),
            Some(("a".into(), "b".into()))
        );
        assert_eq!(token_pair("oauth_token=a"), None);
    }
}
