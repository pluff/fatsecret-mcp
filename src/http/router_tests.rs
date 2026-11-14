//! Non-network tests driving `router(state)` with `tower::ServiceExt::oneshot`.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util_shim::to_bytes;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::config::test_config;
use super::oauth::{AppCreds, Claims, pkce_s256};
use super::seal::now_secs;
use super::{AppState, router};

mod http_body_util_shim {
    use axum::body::Body;
    pub async fn to_bytes(body: Body) -> Vec<u8> {
        axum::body::to_bytes(body, usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }
}

const REDIRECT: &str = "https://claude.ai/api/mcp/auth_callback";
const VERIFIER: &str = "verifier-verifier-verifier-verifier-0123456789";

fn state() -> AppState {
    AppState::new(test_config(), 8080)
}

async fn send(state: &AppState, req: Request<Body>) -> Response {
    router(state.clone()).oneshot(req).await.unwrap()
}

async fn json_body(resp: Response) -> Value {
    serde_json::from_slice(&to_bytes(resp.into_body()).await).unwrap_or(Value::Null)
}

async fn text_body(resp: Response) -> String {
    String::from_utf8(to_bytes(resp.into_body()).await).unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn form_req(uri: &str, form: &str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(form.to_string()))
        .unwrap()
}

fn token_req(form: &str) -> Request<Body> {
    form_req("/token", form)
}

fn mcp_req(token: Option<&str>, body: Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        .header(header::HOST, "mcp.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-03-26","capabilities":{},
        "clientInfo":{"name":"test","version":"0"}}})
}

fn claims(cid: &str, exp: u64) -> Claims {
    Claims {
        exp,
        cid: cid.into(),
        ru: REDIRECT.into(),
        cc: pkce_s256(VERIFIER),
        app: AppCreds {
            client_id: "fs-id".into(),
            client_secret: "fs-secret".into(),
            consumer_secret: "fs-consumer".into(),
        },
        at: "at".into(),
        ats: "ats".into(),
    }
}

/// Registers a dynamic client and returns its sealed client id.
async fn register(st: &AppState) -> String {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/register")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"redirect_uris":[REDIRECT],"client_name":"t"}).to_string(),
        ))
        .unwrap();
    let resp = send(st, req).await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    json_body(resp).await["client_id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Mints an auth code (as `/fatsecret/callback` would), exchanges it at `/token` (PKCE) and
/// returns the token response body.
async fn exchange_code(st: &AppState) -> Value {
    let cid = register(st).await;
    let code = st.sealer.seal("code", &claims(&cid, now_secs() + 60));
    let form = |v: &str| {
        format!(
            "grant_type=authorization_code&client_id={cid}&code={code}&redirect_uri={REDIRECT}&code_verifier={v}"
        )
    };
    let resp = send(st, token_req(&form("wrong-verifier"))).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(resp).await["error"], "invalid_grant");

    let resp = send(st, token_req(&form(VERIFIER))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["token_type"], "Bearer");
    assert!(body["refresh_token"].as_str().is_some());
    body
}

async fn assert_42_tools(st: &AppState, access: &str) {
    let resp = send(st, mcp_req(Some(access), initialize())).await;
    let status = resp.status();
    let raw = to_bytes(resp.into_body()).await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&raw));
    let body: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(body["result"]["serverInfo"]["name"], "fatsecret-mcp");

    let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
    let resp = send(st, mcp_req(Some(access), list)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body = json_body(resp).await;
    assert_eq!(body["result"]["tools"].as_array().unwrap().len(), 42);

    // Also reachable at `/`.
    let mut req = mcp_req(Some(access), initialize());
    *req.uri_mut() = "/".parse().unwrap();
    assert_eq!(send(st, req).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn health_ok() {
    let resp = send(&state(), get("/health")).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(json_body(resp).await["status"], "ok");
}

#[tokio::test]
async fn mcp_requires_valid_bearer() {
    let st = state();
    for uri in ["/mcp", "/"] {
        let req = Request::builder()
            .method(Method::POST)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        let resp = send(&st, req).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{uri}");
        let www = resp.headers()[header::WWW_AUTHENTICATE].to_str().unwrap();
        assert!(www.contains("resource_metadata="), "{www}");
        assert!(www.contains("/.well-known/oauth-protected-resource"));
        assert!(!www.contains("invalid_token"));
    }
    let resp = send(&st, mcp_req(Some("garbage"), initialize())).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let www = resp.headers()[header::WWW_AUTHENTICATE].to_str().unwrap();
    assert!(www.contains("invalid_token"));

    // Wrong blob type (a code or refresh token is not an access token).
    let code = st.sealer.seal("code", &claims("c", now_secs() + 60));
    assert_eq!(
        send(&st, mcp_req(Some(&code), initialize())).await.status(),
        StatusCode::UNAUTHORIZED
    );
    // Expired access token.
    let expired = st.sealer.seal("access", &claims("c", now_secs() - 1));
    assert_eq!(
        send(&st, mcp_req(Some(&expired), initialize()))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    // Sealed with a different secret.
    let other = AppState::new(
        super::config::config_from(&[("OAUTH_SECRET", "different")]),
        8080,
    );
    let foreign = other.sealer.seal("access", &claims("c", now_secs() + 60));
    assert_eq!(
        send(&st, mcp_req(Some(&foreign), initialize()))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn discovery_endpoints() {
    let st = state();
    for uri in [
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-protected-resource/mcp",
    ] {
        let pr = send(&st, get(uri)).await;
        assert_eq!(pr.status(), StatusCode::OK, "{uri}");
        assert!(json_body(pr).await.get("authorization_servers").is_some());
    }
    for uri in [
        "/.well-known/oauth-authorization-server",
        "/.well-known/openid-configuration",
    ] {
        let resp = send(&st, get(uri)).await;
        assert_eq!(resp.status(), StatusCode::OK, "{uri}");
        let body = json_body(resp).await;
        for (key, suffix) in [
            ("authorization_endpoint", "/authorize"),
            ("token_endpoint", "/token"),
            ("registration_endpoint", "/register"),
        ] {
            assert!(body[key].as_str().unwrap().ends_with(suffix), "{key}");
        }
    }
}

#[tokio::test]
async fn registration_enforces_redirect_allowlist() {
    let st = state();
    let post = |body: Value| {
        Request::builder()
            .method(Method::POST)
            .uri("/register")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let resp = send(
        &st,
        post(json!({"redirect_uris":["https://evil.example/cb"]})),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(resp).await["error"], "invalid_redirect_uri");
    let resp = send(&st, post(json!({}))).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(!register(&st).await.is_empty());
}

#[tokio::test]
async fn authorize_shows_consent_page_and_validates() {
    let st = state();
    let cid = register(&st).await;
    let q = |extra: &str| {
        format!("/authorize?response_type=code&client_id={cid}&redirect_uri={REDIRECT}{extra}")
    };

    let resp = send(
        &st,
        get(&q(
            "&code_challenge=abc&code_challenge_method=S256&state=s1",
        )),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = text_body(resp).await;
    assert!(html.contains("fs_client_id") && html.contains("fs_consumer_secret"));
    assert!(html.contains("value=\"s1\""));
    assert!(!html.contains("Google"));

    // Missing PKCE, wrong method, wrong response_type, unknown redirect, forged client id.
    for bad in [
        q(""),
        q("&code_challenge=abc&code_challenge_method=plain"),
        format!(
            "/authorize?response_type=token&client_id={cid}&redirect_uri={REDIRECT}&code_challenge=abc"
        ),
        format!(
            "/authorize?response_type=code&client_id={cid}&redirect_uri=https://evil.example/cb&code_challenge=abc"
        ),
        format!(
            "/authorize?response_type=code&client_id=forged&redirect_uri={REDIRECT}&code_challenge=abc"
        ),
    ] {
        let resp = send(&st, get(&bad)).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{bad}");
        assert!(resp.headers().get(header::LOCATION).is_none());
    }

    // A client registered for one redirect cannot be used with another allowed one.
    let st2 = AppState::new(
        super::config::config_from(&[
            ("OAUTH_SECRET", "test-secret"),
            ("OAUTH_ALLOWED_REDIRECT_URIS", "https://a/cb,https://b/cb"),
        ]),
        8080,
    );
    let cid_a = st2.sealer.seal(
        "client",
        &super::oauth::ClientBlob {
            exp: now_secs() + 60,
            uris: vec!["https://a/cb".into()],
        },
    );
    let uri = |ru: &str| {
        format!(
            "/authorize?response_type=code&client_id={cid_a}&redirect_uri={ru}&code_challenge=x"
        )
    };
    assert_eq!(
        send(&st2, get(&uri("https://a/cb"))).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        send(&st2, get(&uri("https://b/cb"))).await.status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn authorize_post_validates_form() {
    let st = state();
    let cid = register(&st).await;
    // Bad redirect.
    let resp = send(
        &st,
        form_req(
            "/authorize",
            &format!(
                "client_id={cid}&redirect_uri=https%3A%2F%2Fevil.example%2Fcb&code_challenge=x&fs_client_id=a&fs_client_secret=b&fs_consumer_secret=c"
            ),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    // Blank creds: page re-rendered with an error, no redirect, no network.
    let resp = send(
        &st,
        form_req(
            "/authorize",
            &format!(
                "client_id={cid}&redirect_uri={REDIRECT}&code_challenge=x&fs_client_id=a&fs_client_secret=+&fs_consumer_secret=c"
            ),
        ),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        text_body(resp)
            .await
            .contains("All three FatSecret credentials")
    );
}

#[tokio::test]
async fn callbacks_without_flow_cookie_are_rejected() {
    let st = state();
    let uri = "/fatsecret/callback?oauth_token=t&oauth_verifier=v";
    assert_eq!(send(&st, get(uri)).await.status(), StatusCode::BAD_REQUEST);
    // The Google callback no longer exists.
    assert_eq!(
        send(&st, get("/google/callback?code=x&state=y"))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    // A tampered cookie is rejected too.
    let req = Request::builder()
        .uri("/fatsecret/callback?oauth_token=t&oauth_verifier=v")
        .header(
            header::COOKIE,
            "fs_oauth_pending=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        )
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(&state(), req).await.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn token_rejects_bad_requests() {
    let st = state();
    let cid = register(&st).await;
    let code = st.sealer.seal("code", &claims(&cid, now_secs() + 60));
    let base = format!("client_id={cid}&code={code}&redirect_uri={REDIRECT}");

    let cases: Vec<(String, StatusCode, &str)> = vec![
        (
            "grant_type=password".into(),
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
        ),
        (
            format!(
                "grant_type=authorization_code&client_id={cid}&code=bogus&code_verifier={VERIFIER}"
            ),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            // no verifier
            format!("grant_type=authorization_code&{base}"),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            // redirect mismatch
            format!(
                "grant_type=authorization_code&client_id={cid}&code={code}&redirect_uri=https://x/cb&code_verifier={VERIFIER}"
            ),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            // different client than the code was issued to
            format!(
                "grant_type=authorization_code&client_id=other&code={code}&redirect_uri={REDIRECT}&code_verifier={VERIFIER}"
            ),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
        (
            // refresh with a code (wrong blob type)
            format!("grant_type=refresh_token&refresh_token={code}"),
            StatusCode::BAD_REQUEST,
            "invalid_grant",
        ),
    ];
    for (form, status, err) in cases {
        let resp = send(&st, token_req(&form)).await;
        assert_eq!(resp.status(), status, "{form}");
        assert_eq!(json_body(resp).await["error"], err, "{form}");
    }

    // Expired code.
    let expired = st.sealer.seal("code", &claims(&cid, now_secs() - 1));
    let resp = send(
        &st,
        token_req(&format!(
            "grant_type=authorization_code&client_id={cid}&code={expired}&redirect_uri={REDIRECT}&code_verifier={VERIFIER}"
        )),
    )
    .await;
    assert_eq!(json_body(resp).await["error"], "invalid_grant");
}

#[tokio::test]
async fn static_client_requires_secret() {
    let st = AppState::new(
        super::config::config_from(&[
            ("OAUTH_SECRET", "s"),
            ("OAUTH_CLIENT_ID", "sid"),
            ("OAUTH_CLIENT_SECRET", "ssecret"),
        ]),
        8080,
    );
    let code = st.sealer.seal("code", &claims("sid", now_secs() + 60));
    let form = |extra: &str| {
        format!(
            "grant_type=authorization_code&client_id=sid{extra}&code={code}&redirect_uri={REDIRECT}&code_verifier={VERIFIER}"
        )
    };
    let resp = send(&st, token_req(&form(""))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(resp).await["error"], "invalid_client");
    let resp = send(&st, token_req(&form("&client_secret=wrong"))).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let resp = send(&st, token_req(&form("&client_secret=ssecret"))).await;
    assert_eq!(resp.status(), StatusCode::OK);
    // Static client may use the allowlisted redirect on /authorize.
    let resp = send(
        &st,
        get(&format!(
            "/authorize?response_type=code&client_id=sid&redirect_uri={REDIRECT}&code_challenge=x"
        )),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn full_flow_initialize_and_list_42_tools() {
    let st = state();
    let body = exchange_code(&st).await;
    let access = body["access_token"].as_str().unwrap();
    assert_42_tools(&st, access).await;

    // Tokens are stateless: a fresh AppState with the same secret accepts them.
    let restarted = state();
    assert_42_tools(&restarted, access).await;

    // Refresh yields a working access token.
    let refresh = body["refresh_token"].as_str().unwrap();
    let resp = send(
        &restarted,
        token_req(&format!("grant_type=refresh_token&refresh_token={refresh}")),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let refreshed = json_body(resp).await;
    assert_42_tools(&restarted, refreshed["access_token"].as_str().unwrap()).await;
    // Refresh token is not usable as a Bearer.
    assert_eq!(
        send(&restarted, mcp_req(Some(refresh), initialize()))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
}
