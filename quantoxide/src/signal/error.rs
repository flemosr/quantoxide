use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{sync::broadcast::error::SendError, task::JoinError};

use super::live::LiveSignalStatus;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("Generic error, {0}")]
    Generic(String),

    #[error("SignalTransmiterFailed failed error {0}")]
    SignalTransmiterFailed(SendError<Arc<LiveSignalStatus>>),

    #[error("TaskJoin error {0}")]
    TaskJoin(JoinError),
}

impl PartialEq for SignalError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for SignalError {}

pub type Result<T> = result::Result<T, SignalError>;
