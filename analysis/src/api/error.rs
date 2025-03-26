use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Init error: {0}")]
    Init(&'static str),
    #[error("Url parse error: {0}")]
    UrlParse(String),
    #[error("Response error: {0}")]
    Response(reqwest::Error),
    #[error("Unexpected schema error: {0}")]
    UnexpectedSchema(reqwest::Error),
    #[error("WebSocket generic error: {0}")]
    WebSocketGeneric(String),
}

pub type Result<T> = result::Result<T, ApiError>;
