use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Init error: {0}")]
    Init(&'static str),
    #[error("Url parse error: {0}")]
    UrlParse(String),
    #[error("Request error: {0}")]
    Request(reqwest::Error),
    #[error("Unexpected response error: {0}")]
    UnexpectedResponse(reqwest::Error),
}

pub type Result<T> = result::Result<T, ApiError>;
