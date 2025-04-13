use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, SignalError>;
