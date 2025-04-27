use std::result;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TradeError {
    #[error("Generic error, {0}")]
    Generic(String),
}

pub type Result<T> = result::Result<T, TradeError>;
