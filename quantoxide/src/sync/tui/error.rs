use std::result;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum SyncTuiError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, SyncTuiError>;
