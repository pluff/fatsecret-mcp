//! Bearer-token authentication for protected routes (the MCP transport mounts behind this).

use axum::Json;
use axum::async_trait;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::{HeaderMap, StatusCode, header, request::Parts};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::AppState;
use super::oauth::Claims;

/// The decrypted access token (FatSecret app creds + user tokens), inserted by [`require_auth`].
#[derive(Clone, Debug)]
pub(crate) struct AuthSession(pub Claims);

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthSession {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthSession>()
            .cloned()
            .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())
    }
}

pub(crate) fn unauthorized(state: &AppState, headers: &HeaderMap, invalid_token: bool) -> Response {
    let base = state.base_url(headers);
    let mut challenge =
        format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource\"");
    if invalid_token {
        challenge.push_str(", error=\"invalid_token\"");
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, challenge)],
        Json(json!({"error": "Unauthorized"})),
    )
        .into_response()
}

/// Middleware: 401 + `WWW-Authenticate` unless a valid access token is presented;
/// otherwise inserts [`AuthSession`] into the request extensions.
pub(crate) async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    match state.bearer_claims(req.headers()) {
        Some(Ok(claims)) => {
            req.extensions_mut().insert(AuthSession(claims));
            next.run(req).await
        }
        Some(Err(())) => unauthorized(&state, req.headers(), true),
        None => unauthorized(&state, req.headers(), false),
    }
}
