use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BacktestError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, BacktestError>;
