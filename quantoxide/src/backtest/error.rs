use std::{result, sync::Arc};

use thiserror::Error;
use tokio::sync::broadcast::error::SendError;

use super::BacktestState;

#[derive(Error, Debug)]
pub enum BacktestError {
    #[error("TransmiterFailed error {0}")]
    TransmiterFailed(SendError<Arc<BacktestState>>),
    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for BacktestError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for BacktestError {}

pub type Result<T> = result::Result<T, BacktestError>;
