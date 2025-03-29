use chrono::{DateTime, Utc};
use std::result;
use thiserror::Error;

use crate::{
    api::{error::ApiError, websocket::error::WebSocketApiError},
    db::error::DbError,
};

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] DbError),
    #[error("API error: {0}")]
    Api(#[from] ApiError),
    #[error("API Max Trials reached error. Api Error: {api_error}, Max Trials: {max_trials}")]
    ApiMaxTrialsReached {
        api_error: ApiError,
        max_trials: u32,
    },
    #[error("Unexpected LNM payload error: {0}")]
    UnexpectedLNMPayload(String),
    #[error("Unreachable DB gap error. Earliest Gap: {earliest_gap}, Limit {limit}")]
    UnreachableDbGap {
        earliest_gap: DateTime<Utc>,
        limit: DateTime<Utc>,
    },
}

impl From<WebSocketApiError> for AppError {
    fn from(err: WebSocketApiError) -> Self {
        AppError::Api(ApiError::from(err))
    }
}

pub type Result<T> = result::Result<T, AppError>;
