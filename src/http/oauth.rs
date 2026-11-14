//! Stateless OAuth 2.1 authorization server (PKCE, dynamic client registration).
//!
//! FatSecret has no OAuth 2.1 / MCP-compatible auth, so this server is its own authorization
//! server. The consent page collects the user's FatSecret app credentials; an OAuth 1.0a
//! handshake then links their FatSecret account. Every code, access token, refresh token,
//! registered client id and in-flight flow state is a ChaCha20-Poly1305 sealed blob (see
//! `seal.rs`) carrying what it needs, so nothing is stored server-side and tokens survive
//! restarts as long as `OAUTH_SECRET` stays the same.
//!
//! In-flight state (PKCE/redirect info, app credentials, FatSecret request token secret) lives
//! in a sealed, HttpOnly, `SameSite=Lax` cookie rather than in the `oauth_callback` query
//! params: the browser carries it to the callback (top-level GET navigation), it keeps the
//! app credentials out of URLs and access logs, and it does not depend on FatSecret
//! preserving a query string on the callback URL.

use axum::Json;
use axum::extract::{Form, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tracing::error;

use super::AppState;
use super::seal::{Expires, now_secs};
use crate::fatsecret::oauth1::{self, OAuth1Credentials};

pub(crate) const COOKIE_NAME: &str = "fs_oauth_pending";
const CODE_TTL: u64 = 300;
const PENDING_TTL: u64 = 600;
const ACCESS_TTL: u64 = 3600;
const REFRESH_TTL: u64 = 90 * 24 * 3600;
const CLIENT_TTL: u64 = 10 * 365 * 24 * 3600;

// ── Sealed payloads ──

/// FatSecret app credentials entered on the consent page.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AppCreds {
    pub client_id: String,
    pub client_secret: String,
    pub consumer_secret: String,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ClientBlob {
    pub exp: u64,
    pub uris: Vec<String>,
}

impl Expires for ClientBlob {
    fn exp(&self) -> u64 {
        self.exp
    }
}

/// Payload of codes, access tokens and refresh tokens (distinguished by AAD type).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Claims {
    pub exp: u64,
    /// Client id the token was issued to.
    #[serde(default)]
    pub cid: String,
    /// Redirect URI (codes only).
    #[serde(default)]
    pub ru: String,
    /// PKCE S256 challenge (codes only).
    #[serde(default)]
    pub cc: String,
    pub app: AppCreds,
    /// FatSecret OAuth 1.0a access token and secret.
    pub at: String,
    pub ats: String,
}

impl Expires for Claims {
    fn exp(&self) -> u64 {
        self.exp
    }
}

/// In-flight authorization state, sealed into the `fs_oauth_pending` cookie.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Pending {
    exp: u64,
    cid: String,
    ru: String,
    cc: String,
    /// The MCP client's `state`.
    #[serde(default)]
    st: String,
    app: AppCreds,
    /// FatSecret request token and secret.
    #[serde(default)]
    rt: String,
    #[serde(default)]
    rts: String,
}

impl Expires for Pending {
    fn exp(&self) -> u64 {
        self.exp
    }
}

// ── Helpers ──

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"error": error, "error_description": description})),
    )
        .into_response()
}

fn bad_request(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, msg.to_string()).into_response()
}

pub(crate) fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub(crate) fn pkce_s256(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub(crate) fn verify_pkce(verifier: &str, challenge: &str) -> bool {
    constant_eq(pkce_s256(verifier).as_bytes(), challenge.as_bytes())
}

pub(crate) fn external_base_url(
    configured: Option<&str>,
    headers: &HeaderMap,
    fallback_host: &str,
) -> String {
    if let Some(url) = configured {
        return url.to_string();
    }
    let get = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let proto = get("x-forwarded-proto")
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("http");
    let host = get("x-forwarded-host")
        .or_else(|| get("host"))
        .unwrap_or(fallback_host);
    format!("{proto}://{host}")
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().find_map(|raw| {
        raw.to_str().ok()?.split(';').find_map(|part| {
            let (k, v) = part.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
    })
}

fn found(url: &str) -> Response {
    match HeaderValue::from_str(url) {
        Ok(value) => (
            StatusCode::FOUND,
            [
                (header::LOCATION, value),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn with_cookie(mut resp: Response, cookie: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(cookie) {
        resp.headers_mut().append(header::SET_COOKIE, value);
    }
    resp
}

fn cookie_attrs(base: &str) -> &'static str {
    if base.starts_with("https://") {
        "HttpOnly; Secure; SameSite=Lax; Path=/"
    } else {
        "HttpOnly; SameSite=Lax; Path=/"
    }
}

fn set_pending_cookie(state: &AppState, base: &str, p: &Pending) -> String {
    format!(
        "{COOKIE_NAME}={}; {}; Max-Age={PENDING_TTL}",
        state.sealer.seal("pending", p),
        cookie_attrs(base)
    )
}

fn clear_pending_cookie(base: &str) -> String {
    format!("{COOKIE_NAME}=; {}; Max-Age=0", cookie_attrs(base))
}

fn append_query(base: &str, params: &[(&str, &str)]) -> String {
    let mut url = base.to_string();
    let mut sep = if base.contains('?') { '&' } else { '?' };
    for (k, v) in params {
        url.push(sep);
        url.push_str(k);
        url.push('=');
        url.push_str(&urlencoding::encode(v));
        sep = '&';
    }
    url
}

/// Redirects back to the MCP client with an OAuth error and clears the flow cookie.
fn fail(base: &str, p: &Pending, err: &str, desc: &str) -> Response {
    let mut params = vec![("error", err), ("error_description", desc)];
    if !p.st.is_empty() {
        params.push(("state", p.st.as_str()));
    }
    with_cookie(
        found(&append_query(&p.ru, &params)),
        &clear_pending_cookie(base),
    )
}

fn session_expired() -> Response {
    bad_request("Session expired or missing. Restart the connection from your MCP client.")
}

impl AppState {
    pub(crate) fn base_url(&self, headers: &HeaderMap) -> String {
        external_base_url(
            self.config.public_url.as_deref(),
            headers,
            &self.fallback_host,
        )
    }

    fn redirect_allowed(&self, client_id: &str, redirect_uri: &str) -> bool {
        if let Some((id, _)) = &self.config.static_client
            && id == client_id
        {
            return self.config.redirect_allowed(redirect_uri);
        }
        self.sealer
            .open::<ClientBlob>("client", client_id)
            .is_some_and(|c| c.uris.iter().any(|u| u == redirect_uri))
    }

    fn client_authenticated(&self, client_id: &str, secret: Option<&str>) -> bool {
        match &self.config.static_client {
            Some((id, expected)) if id == client_id => {
                secret.is_some_and(|s| constant_eq(s.as_bytes(), expected.as_bytes()))
            }
            _ => true,
        }
    }

    fn open_pending(&self, headers: &HeaderMap) -> Option<Pending> {
        let raw = read_cookie(headers, COOKIE_NAME)?;
        self.sealer.open::<Pending>("pending", &raw)
    }

    /// Resolves a Bearer access token. `None` = no/invalid header shape,
    /// `Some(Err)` = token present but invalid or expired.
    pub(crate) fn bearer_claims(&self, headers: &HeaderMap) -> Option<Result<Claims, ()>> {
        let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
        let token = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))?
            .trim();
        Some(self.sealer.open::<Claims>("access", token).ok_or(()))
    }
}

// ── Discovery ──

pub(crate) async fn protected_resource(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<Value> {
    let base = state.base_url(&headers);
    Json(json!({
        "resource": base,
        "authorization_servers": [base],
        "bearer_methods_supported": ["header"],
    }))
}

pub(crate) async fn authorization_server_metadata(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Json<Value> {
    let base = state.base_url(&headers);
    Json(json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "registration_endpoint": format!("{base}/register"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code", "refresh_token"],
        "code_challenge_methods_supported": ["S256"],
        "token_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
        "scopes_supported": ["mcp"],
    }))
}

// ── /register (RFC 7591, stateless: the client id is a sealed blob of its redirect URIs) ──

pub(crate) async fn register(State(state): State<AppState>, Json(body): Json<Value>) -> Response {
    let uris: Vec<String> = body
        .get("redirect_uris")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    if uris.is_empty() {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            "redirect_uris required",
        );
    }
    if let Some(bad) = uris.iter().find(|u| !state.config.redirect_allowed(u)) {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_redirect_uri",
            &format!("redirect_uri not allowed: {bad}"),
        );
    }
    let client_id = state.sealer.seal(
        "client",
        &ClientBlob {
            exp: now_secs() + CLIENT_TTL,
            uris: uris.clone(),
        },
    );
    (
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "client_id": client_id,
            "client_name": body.get("client_name").cloned().unwrap_or(Value::Null),
            "redirect_uris": uris,
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        })),
    )
        .into_response()
}

// ── /authorize: consent page ──

#[derive(Deserialize)]
pub(crate) struct AuthQuery {
    response_type: Option<String>,
    client_id: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
}

pub(crate) fn consent_page(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
    error: &str,
) -> Response {
    let error_html = if error.is_empty() {
        String::new()
    } else {
        format!("<p style=\"color:#c00\">{}</p>", esc(error))
    };
    Html(format!(
        r#"<!doctype html><html><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Connect FatSecret</title>
<style>body{{font-family:system-ui,sans-serif;max-width:420px;margin:8vh auto;padding:0 16px}}
input{{width:100%;padding:10px;margin:6px 0 14px;box-sizing:border-box}}
label{{font-size:14px}}
button{{padding:10px 16px;width:100%}}</style></head><body>
<h2>Connect FatSecret</h2>
<p>Enter your FatSecret app credentials (<a href="https://platform.fatsecret.com/my-account/api-key" target="_blank" rel="noopener">platform.fatsecret.com</a> &rarr; My Account &rarr; API Keys) to authorize this client. You will then be sent to FatSecret to authorize your account.</p>
{error_html}
<form method="post" action="/authorize">
<input type="hidden" name="client_id" value="{client_id}">
<input type="hidden" name="redirect_uri" value="{redirect_uri}">
<input type="hidden" name="state" value="{state}">
<input type="hidden" name="code_challenge" value="{challenge}">
<label>Client ID</label>
<input type="text" name="fs_client_id" required autofocus autocomplete="off">
<label>Client Secret (OAuth 2.0)</label>
<input type="password" name="fs_client_secret" required autocomplete="off">
<label>Consumer Secret (OAuth 1.0)</label>
<input type="password" name="fs_consumer_secret" required autocomplete="off">
<button type="submit">Authorize</button>
</form></body></html>"#,
        client_id = esc(client_id),
        redirect_uri = esc(redirect_uri),
        state = esc(state),
        challenge = esc(challenge),
    ))
    .into_response()
}

pub(crate) async fn authorize_get(
    State(state): State<AppState>,
    Query(q): Query<AuthQuery>,
) -> Response {
    let (Some(client_id), Some(redirect_uri)) = (q.client_id, q.redirect_uri) else {
        return bad_request("client_id and redirect_uri required");
    };
    if !state.redirect_allowed(&client_id, &redirect_uri) {
        return bad_request("unknown client or redirect_uri");
    }
    if q.response_type.as_deref() != Some("code") {
        return bad_request("response_type must be code");
    }
    let Some(challenge) = q.code_challenge.filter(|c| !c.is_empty()) else {
        return bad_request("PKCE code_challenge required");
    };
    if q.code_challenge_method.as_deref().unwrap_or("S256") != "S256" {
        return bad_request("only S256 code_challenge_method supported");
    }
    consent_page(
        &client_id,
        &redirect_uri,
        q.state.as_deref().unwrap_or(""),
        &challenge,
        "",
    )
}

#[derive(Deserialize)]
pub(crate) struct AuthForm {
    client_id: String,
    redirect_uri: String,
    #[serde(default)]
    state: String,
    code_challenge: String,
    fs_client_id: String,
    fs_client_secret: String,
    fs_consumer_secret: String,
}

pub(crate) async fn authorize_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(f): Form<AuthForm>,
) -> Response {
    if !state.redirect_allowed(&f.client_id, &f.redirect_uri) || f.code_challenge.is_empty() {
        return bad_request("unknown client or redirect_uri");
    }
    let app = AppCreds {
        client_id: f.fs_client_id.trim().to_string(),
        client_secret: f.fs_client_secret.trim().to_string(),
        consumer_secret: f.fs_consumer_secret.trim().to_string(),
    };
    let page = |msg: &str| {
        consent_page(
            &f.client_id,
            &f.redirect_uri,
            &f.state,
            &f.code_challenge,
            msg,
        )
    };
    if app.client_id.is_empty() || app.client_secret.is_empty() || app.consumer_secret.is_empty() {
        return page("All three FatSecret credentials are required.");
    }
    let pending = Pending {
        exp: now_secs() + PENDING_TTL,
        cid: f.client_id.clone(),
        ru: f.redirect_uri.clone(),
        cc: f.code_challenge.clone(),
        st: f.state.clone(),
        app,
        rt: String::new(),
        rts: String::new(),
    };
    let base = state.base_url(&headers);

    match start_fatsecret(&state, &base, pending).await {
        Ok(resp) => resp,
        Err(err) => {
            error!("fatsecret request_token failed: {err}");
            page("FatSecret rejected these credentials (request token failed).")
        }
    }
}

fn app_oauth1(app: &AppCreds) -> OAuth1Credentials {
    OAuth1Credentials {
        consumer_key: app.client_id.clone(),
        consumer_secret: app.consumer_secret.clone(),
        access_token: None,
        access_token_secret: None,
    }
}

/// Obtains a FatSecret request token, seals it into the flow cookie and redirects the user
/// to FatSecret's authorize page.
async fn start_fatsecret(
    state: &AppState,
    base: &str,
    mut pending: Pending,
) -> Result<Response, crate::fatsecret::client::error::FatSecretError> {
    let result = oauth1::request_token(
        &state.http,
        &app_oauth1(&pending.app),
        &format!("{base}/fatsecret/callback"),
    )
    .await?;
    pending.rt = result.oauth_token;
    pending.rts = result.oauth_token_secret;
    pending.exp = now_secs() + PENDING_TTL;
    Ok(with_cookie(
        found(&result.authorization_url),
        &set_pending_cookie(state, base, &pending),
    ))
}

// ── /fatsecret/callback ──

#[derive(Deserialize)]
pub(crate) struct FatSecretCallbackQuery {
    oauth_token: Option<String>,
    oauth_verifier: Option<String>,
}

pub(crate) async fn fatsecret_callback(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<FatSecretCallbackQuery>,
) -> Response {
    let Some(pending) = state.open_pending(&headers).filter(|p| !p.rt.is_empty()) else {
        return session_expired();
    };
    let base = state.base_url(&headers);
    let verifier = q.oauth_verifier.as_deref().filter(|v| !v.is_empty());
    let (true, Some(verifier)) = (
        q.oauth_token.as_deref() == Some(pending.rt.as_str()),
        verifier,
    ) else {
        return fail(
            &base,
            &pending,
            "access_denied",
            "fatsecret token/verifier mismatch or authorization denied",
        );
    };
    let tokens = match oauth1::access_token(
        &state.http,
        &app_oauth1(&pending.app),
        &pending.rt,
        &pending.rts,
        verifier,
    )
    .await
    {
        Ok(t) => t,
        Err(err) => {
            error!("fatsecret access_token failed: {err}");
            return fail(
                &base,
                &pending,
                "server_error",
                "fatsecret access_token failed",
            );
        }
    };
    let code = state.sealer.seal(
        "code",
        &Claims {
            exp: now_secs() + CODE_TTL,
            cid: pending.cid.clone(),
            ru: pending.ru.clone(),
            cc: pending.cc.clone(),
            app: pending.app.clone(),
            at: tokens.access_token,
            ats: tokens.access_token_secret,
        },
    );
    let mut params = vec![("code", code.as_str())];
    if !pending.st.is_empty() {
        params.push(("state", pending.st.as_str()));
    }
    with_cookie(
        found(&append_query(&pending.ru, &params)),
        &clear_pending_cookie(&base),
    )
}

// ── /token ──

#[derive(Deserialize, Default)]
pub(crate) struct TokenForm {
    grant_type: String,
    code: Option<String>,
    redirect_uri: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    code_verifier: Option<String>,
    refresh_token: Option<String>,
}

fn basic_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = String::from_utf8(STANDARD.decode(encoded.trim()).ok()?).ok()?;
    let (id, secret) = decoded.split_once(':')?;
    Some((
        urlencoding::decode(id).ok()?.into_owned(),
        urlencoding::decode(secret).ok()?.into_owned(),
    ))
}

pub(crate) async fn token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(f): Form<TokenForm>,
) -> Response {
    let invalid_grant = |d: &str| oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", d);
    let (client_id, client_secret) = match basic_credentials(&headers) {
        Some((id, secret)) => (Some(id), Some(secret)),
        None => (f.client_id.clone(), f.client_secret.clone()),
    };

    let claims = match f.grant_type.as_str() {
        "authorization_code" => {
            let Some(claims) = f
                .code
                .as_deref()
                .and_then(|c| state.sealer.open::<Claims>("code", c))
            else {
                return invalid_grant("invalid or expired code");
            };
            if client_id.as_deref().is_some_and(|id| id != claims.cid)
                || f.redirect_uri.as_deref() != Some(claims.ru.as_str())
            {
                return invalid_grant("client or redirect_uri mismatch");
            }
            if !f
                .code_verifier
                .as_deref()
                .is_some_and(|v| verify_pkce(v, &claims.cc))
            {
                return invalid_grant("PKCE verification failed");
            }
            claims
        }
        "refresh_token" => {
            let Some(claims) = f
                .refresh_token
                .as_deref()
                .and_then(|t| state.sealer.open::<Claims>("refresh", t))
            else {
                return invalid_grant("invalid or expired refresh_token");
            };
            if client_id.as_deref().is_some_and(|id| id != claims.cid) {
                return invalid_grant("client mismatch");
            }
            claims
        }
        _ => {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "unsupported_grant_type",
                "use authorization_code or refresh_token",
            );
        }
    };

    if !state.client_authenticated(&claims.cid, client_secret.as_deref()) {
        return oauth_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "bad client credentials",
        );
    }

    let issue = |typ: &str, ttl: u64| {
        state.sealer.seal(
            typ,
            &Claims {
                exp: now_secs() + ttl,
                ru: String::new(),
                cc: String::new(),
                ..claims.clone()
            },
        )
    };
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({
            "access_token": issue("access", ACCESS_TTL),
            "token_type": "Bearer",
            "expires_in": ACCESS_TTL,
            "refresh_token": issue("refresh", REFRESH_TTL),
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_rfc7636_vector() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM";
        assert_eq!(pkce_s256(verifier), challenge);
        assert!(verify_pkce(verifier, challenge));
        assert!(!verify_pkce("other", challenge));
        assert!(!verify_pkce(verifier, ""));
    }

    #[test]
    fn external_base_url_precedence() {
        let mut h = HeaderMap::new();
        h.insert("host", "internal:8080".parse().unwrap());
        assert_eq!(external_base_url(None, &h, "fb"), "http://internal:8080");
        h.insert("x-forwarded-host", "mcp.example.com".parse().unwrap());
        h.insert("x-forwarded-proto", "https, http".parse().unwrap());
        assert_eq!(external_base_url(None, &h, "fb"), "https://mcp.example.com");
        assert_eq!(
            external_base_url(Some("https://pub.example"), &h, "fb"),
            "https://pub.example"
        );
        assert_eq!(
            external_base_url(None, &HeaderMap::new(), "fb"),
            "http://fb"
        );
    }

    #[test]
    fn consent_page_escapes_and_has_fields() {
        let resp = consent_page("c\"id", "https://x/cb?a=1&b=<2>", "s'1", "chal", "<oops>");
        let body = futures_body(resp);
        assert!(body.contains("value=\"c&quot;id\""));
        assert!(body.contains("a=1&amp;b=&lt;2&gt;"));
        assert!(body.contains("s&#39;1"));
        assert!(body.contains("&lt;oops&gt;"));
        for name in [
            "fs_client_id",
            "fs_client_secret",
            "fs_consumer_secret",
            "code_challenge",
        ] {
            assert!(body.contains(&format!("name=\"{name}\"")), "{name}");
        }
        assert!(body.contains("sent to FatSecret"));
    }

    fn futures_body(resp: Response) -> String {
        let (_, body) = resp.into_parts();
        let bytes = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(axum::body::to_bytes(body, usize::MAX))
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[test]
    fn cookie_parsing() {
        let mut h = HeaderMap::new();
        h.insert(
            header::COOKIE,
            "a=1; fs_oauth_pending=abc; b=2".parse().unwrap(),
        );
        assert_eq!(read_cookie(&h, COOKIE_NAME).as_deref(), Some("abc"));
        assert_eq!(read_cookie(&h, "zzz"), None);
    }
}
