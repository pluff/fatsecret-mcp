//! Request/response logging middleware.
//!
//! - `info`:  one line per request (method, path, status, latency)
//! - `debug`: plus headers and bodies, with secrets redacted

use std::time::Instant;

use axum::body::{Body, to_bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, header};
use axum::middleware::Next;
use axum::response::Response;
use serde_json::Value;
use tracing::{Level, debug, error, info, warn};

const SENSITIVE: &[&str] = &[
    "api_key",
    "apikey",
    "api-key",
    "access_token",
    "refresh_token",
    "code",
    "code_verifier",
    "client_secret",
    "id_token",
    "oauth_token",
    "oauth_verifier",
    "password",
];
const MAX_BODY_LOG: usize = 4096;

fn is_sensitive(key: &str) -> bool {
    SENSITIVE.iter().any(|s| key.eq_ignore_ascii_case(s))
}

fn redact_form(s: &str) -> String {
    s.split('&')
        .map(|pair| match pair.split_once('=') {
            Some((k, _)) if is_sensitive(k) => format!("{k}=<redacted>"),
            _ => pair.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn redact_value(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, val) in map.iter_mut() {
                if is_sensitive(k) && val.is_string() {
                    *val = Value::String("<redacted>".into());
                } else {
                    redact_value(val);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        _ => {}
    }
}

fn format_body(content_type: &str, bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "<empty>".to_string();
    }
    let text = String::from_utf8_lossy(bytes);
    let mut out = if content_type.starts_with("application/x-www-form-urlencoded") {
        redact_form(&text)
    } else if let Ok(mut json) = serde_json::from_str::<Value>(&text) {
        redact_value(&mut json);
        json.to_string()
    } else {
        text.into_owned()
    };
    if out.len() > MAX_BODY_LOG {
        let mut end = MAX_BODY_LOG;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
        out.push_str("...<truncated>");
    }
    out
}

fn format_headers(headers: &HeaderMap) -> String {
    headers
        .iter()
        .map(|(name, value)| {
            let value = value.to_str().unwrap_or("<binary>");
            let shown = if name == header::AUTHORIZATION
                || name == header::COOKIE
                || name == header::SET_COOKIE
            {
                "<redacted>".to_string()
            } else if name == header::LOCATION {
                match value.split_once('?') {
                    Some((base, query)) => format!("{base}?{}", redact_form(query)),
                    None => value.to_string(),
                }
            } else {
                value.to_string()
            };
            format!("{name}: {shown}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn content_type(headers: &HeaderMap) -> &str {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}

pub(crate) async fn log_requests(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(redact_form);
    let verbose = tracing::enabled!(Level::DEBUG);

    let req = if verbose {
        debug!(
            "-> {method} {path} query={} headers=[{}]",
            query.as_deref().unwrap_or("-"),
            format_headers(req.headers())
        );
        let (parts, body) = req.into_parts();
        match to_bytes(body, usize::MAX).await {
            Ok(bytes) => {
                debug!(
                    "-> body: {}",
                    format_body(content_type(&parts.headers), &bytes)
                );
                Request::from_parts(parts, Body::from(bytes))
            }
            Err(err) => {
                warn!("-> failed to read request body: {err}");
                Request::from_parts(parts, Body::empty())
            }
        }
    } else {
        req
    };

    let response = next.run(req).await;
    let status = response.status();
    let ms = start.elapsed().as_millis();
    let line = format!("{method} {path} -> {} ({ms} ms)", status.as_u16());
    if status.is_server_error() {
        error!("{line}");
    } else if status.is_client_error() {
        warn!("{line}");
    } else {
        info!("{line}");
    }

    if !verbose {
        return response;
    }
    debug!("<- headers=[{}]", format_headers(response.headers()));
    let (parts, body) = response.into_parts();
    match to_bytes(body, usize::MAX).await {
        Ok(bytes) => {
            debug!(
                "<- body: {}",
                format_body(content_type(&parts.headers), &bytes)
            );
            Response::from_parts(parts, Body::from(bytes))
        }
        Err(err) => {
            warn!("<- failed to read response body: {err}");
            Response::from_parts(parts, Body::empty())
        }
    }
}
