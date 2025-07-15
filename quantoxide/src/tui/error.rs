use std::result;

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TuiError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TuiError>;
