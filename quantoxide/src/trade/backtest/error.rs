use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{sync::broadcast::error::SendError, task::JoinError};

use super::{BacktestState, manager::error::SimulatedTradeManagerError};

#[derive(Error, Debug)]
pub enum BacktestError {
    #[error("[Manager] {0}")]
    Manager(#[from] SimulatedTradeManagerError),

    #[error("TransmiterFailed error {0}")]
    TransmiterFailed(SendError<Arc<BacktestState>>),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

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
