use std::{result, sync::Arc};

use thiserror::Error;

use super::{process::error::SyncProcessFatalError, state::SyncStatus};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Sync process already terminated error, status: {0}")]
    SyncAlreadyTerminated(SyncStatus),

    #[error("Sync shutdown procedure failed: {0}")]
    SyncShutdownFailed(Arc<SyncProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SyncError>;
