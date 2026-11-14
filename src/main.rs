use std::env;
use std::io::stderr;
use std::net::AddrParseError;

use thiserror::Error;
use tracing::{Level, warn};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{fmt, registry};

use fatsecret_mcp_server::http;

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // LOG_LEVEL: error | warn | info | debug (default: info). Applies to this crate;
    // dependencies only log warnings and errors.
    let requested = env::var("LOG_LEVEL").ok();
    let (level, invalid_level) = match requested.as_deref().map(|v| v.trim().parse::<Level>()) {
        Some(Ok(level)) => (level, None),
        Some(Err(_)) => (Level::INFO, requested.clone()),
        None => (Level::INFO, None),
    };
    registry()
        .with(
            fmt::layer()
                .with_target(false)
                .with_writer(stderr)
                .with_filter(
                    Targets::new()
                        .with_default(Level::WARN)
                        .with_target("fatsecret_mcp_server", level),
                ),
        )
        .init();
    if let Some(value) = invalid_level {
        warn!("Invalid LOG_LEVEL {value:?}, using info (expected error, warn, info or debug)");
    }

    let port = match env::var("PORT") {
        Ok(port) => port,
        Err(env::VarError::NotPresent) => "8080".to_string(),
        Err(env::VarError::NotUnicode(_)) => {
            return Err(AppError::EnvVar("PORT must be valid UTF-8".to_string()));
        }
    };
    let addr = format!("0.0.0.0:{port}")
        .parse()
        .map_err(AppError::InvalidAddr)?;
    let config = http::Config::from_env();
    http::serve(addr, config)
        .await
        .map_err(|err| AppError::HttpServe(err.to_string()))?;

    Ok(())
}

#[derive(Error, Debug)]
enum AppError {
    #[error("{0}")]
    EnvVar(String),
    #[error("PORT must form a valid socket address: {0}")]
    InvalidAddr(AddrParseError),
    #[error("HTTP server error: {0}")]
    HttpServe(String),
}
