use std::result;

use thiserror::Error;
use tokio::task::JoinError;

use super::executor::error::LiveTradeExecutorError;

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("Executor error {0}")]
    Executor(#[from] LiveTradeExecutorError),

    #[error("Generic error, {0}")]
    Generic(String),
}

impl PartialEq for LiveError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for LiveError {}

pub type Result<T> = result::Result<T, LiveError>;
