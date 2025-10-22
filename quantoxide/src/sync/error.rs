use std::result;

use thiserror::Error;
use tokio::{sync::broadcast::error::SendError, task::JoinError};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("[SyncProcessTaskJoin] {0}")]
    SyncProcessTaskJoin(JoinError),

    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Failed to send sync shutdown signal error: {0}")]
    SendShutdownSignalFailed(SendError<()>),

    #[error("Sync shutdown timeout error")]
    ShutdownTimeout,
}

pub type Result<T> = result::Result<T, SyncError>;
