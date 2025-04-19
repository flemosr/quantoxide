use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RestApiError {
    #[error("Url parse error: {0}")]
    UrlParse(String),
    #[error("Response error: {0}")]
    Response(reqwest::Error),
    #[error("Unexpected schema error: {0}")]
    UnexpectedSchema(reqwest::Error),
    #[error("RestApi generic error: {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, RestApiError>;
