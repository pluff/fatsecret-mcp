use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::Request,
    response::{IntoResponse, Response},
    routing::{any, get, post},
    serve as axum_serve,
};
use rmcp::transport::streamable_http_server::{
    StreamableHttpServerConfig, StreamableHttpService, session::never::NeverSessionManager,
};
use serde_json::json;
use thiserror::Error;
use tokio::net::TcpListener;
use tower::ServiceExt;
use tracing::info;

mod auth;
mod config;
mod logging;
mod oauth;
mod seal;

pub use config::Config;

use auth::{AuthSession, require_auth};

use crate::fatsecret::oauth1::OAuth1Credentials;
use crate::server::FatSecretServer;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub http: reqwest::Client,
    pub(crate) sealer: Arc<seal::Sealer>,
    pub(crate) fallback_host: String,
}

impl AppState {
    pub fn new(config: Config, port: u16) -> Self {
        let http = reqwest::Client::new();
        Self {
            sealer: Arc::new(seal::Sealer::new(&config.oauth_secret)),
            config: Arc::new(config),
            http,
            fallback_host: format!("localhost:{port}"),
        }
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

/// MCP transport handler (POST/GET/DELETE on `/mcp` and `/`), mounted behind [`require_auth`].
///
/// A server is built per request from the FatSecret app credentials and user tokens carried
/// inside the decrypted Bearer token; nothing is stored server-side. The transport is
/// stateless with plain JSON responses (no `Mcp-Session-Id`). A tool that emits progress
/// notifications (`save_weekly_food_diary`) makes rmcp answer that one request as SSE.
async fn mcp_handler(AuthSession(claims): AuthSession, req: Request) -> Response {
    let oauth1 = OAuth1Credentials {
        consumer_key: claims.app.client_id.clone(),
        consumer_secret: claims.app.consumer_secret,
        access_token: Some(claims.at),
        access_token_secret: Some(claims.ats),
    };
    let server =
        FatSecretServer::with_credentials(claims.app.client_id, claims.app.client_secret, oauth1);
    // Behind a reverse proxy the Host header is the public hostname; rmcp's default
    // loopback-only allowlist would reject it. Every request here carries a bearer token
    // (not ambient browser credentials), so DNS rebinding is not a threat.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .disable_allowed_hosts();
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    match service.oneshot(req).await {
        Ok(resp) => resp.map(axum::body::Body::new),
        Err(never) => match never {},
    }
}

/// OAuth/discovery routes plus the MCP transport behind [`require_auth`].
pub fn router(state: AppState) -> Router {
    let mcp_routes = Router::new()
        .route("/mcp", any(mcp_handler))
        .route("/", any(mcp_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_auth,
        ))
        .with_state(state.clone());
    Router::new()
        .route("/health", get(health))
        .route(
            "/.well-known/oauth-protected-resource",
            get(oauth::protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/*rest",
            get(oauth::protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(oauth::authorization_server_metadata),
        )
        .route(
            "/.well-known/oauth-authorization-server/*rest",
            get(oauth::authorization_server_metadata),
        )
        .route(
            "/.well-known/openid-configuration",
            get(oauth::authorization_server_metadata),
        )
        .route(
            "/.well-known/openid-configuration/*rest",
            get(oauth::authorization_server_metadata),
        )
        .route("/register", post(oauth::register))
        .route(
            "/authorize",
            get(oauth::authorize_get).post(oauth::authorize_post),
        )
        .route("/fatsecret/callback", get(oauth::fatsecret_callback))
        .route("/token", post(oauth::token))
        .with_state(state)
        .merge(mcp_routes)
        .layer(axum::middleware::from_fn(logging::log_requests))
}

pub async fn serve(addr: SocketAddr, config: Config) -> Result<(), HttpError> {
    let state = AppState::new(config, addr.port());
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|err| HttpError::Bind(err.to_string()))?;
    info!("HTTP MCP server listening on {}", addr);
    axum_serve(listener, router(state))
        .await
        .map_err(|err| HttpError::Serve(err.to_string()))?;
    Ok(())
}

#[derive(Error, Debug)]
pub enum HttpError {
    #[error("failed to bind HTTP listener: {0}")]
    Bind(String),
    #[error("HTTP server error: {0}")]
    Serve(String),
}

#[cfg(test)]
mod router_tests;
