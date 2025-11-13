use std::result;

use hmac::digest::InvalidLength;
use hyper::{Method, StatusCode, header::InvalidHeaderValue};
use thiserror::Error;

// use super::models::error::FuturesTradeRequestValidationError;

#[derive(Error, Debug)]
pub enum RestApiError {
    #[error("Url parse error: {0}")]
    UrlParse(String),

    #[error("Unexpected schema error: {0}")]
    UnexpectedSchema(reqwest::Error),

    // #[error("Invalid futures trade request error: {0}")]
    // FuturesTradeRequestValidation(FuturesTradeRequestValidationError),
    #[error("Invalid header value error: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    #[error("Invalid secret HMAC error: {0}")]
    InvalidSecretHmac(InvalidLength),

    #[error("HTTP client `reqwest` error: {0}")]
    HttpClient(reqwest::Error),

    #[error("Response decoding `reqwest` error: {0}")]
    ResponseDecoding(reqwest::Error),

    #[error("Authentication required for request but no credentials provided")]
    MissingRequestCredentials,

    #[error("Tried to make a request with unsupported method: {0}")]
    UnsupportedMethod(Method),

    #[error("Failed to send request error: {0}")]
    SendFailed(reqwest::Error),

    #[error("Received error response. Status: {status}, text: {text}")]
    ErrorResponse { status: StatusCode, text: String },

    #[error("Response JSON deserialization failed. Raw response: '{raw_response}', error: {e}")]
    ResponseJsonDeserializeFailed {
        raw_response: String,
        e: serde_json::Error,
    },

    #[error("Request JSON serialization failed. Error: {0}")]
    RequestJsonSerializeFailed(serde_json::Error),
}

pub(in crate::api_v3) type Result<T> = result::Result<T, RestApiError>;
