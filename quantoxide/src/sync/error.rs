use std::{result, sync::Arc};

use thiserror::Error;

use super::process::SyncProcessFatalError;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Sync shutdown procedure failed: {0}")]
    SyncShutdownFailed(Arc<SyncProcessFatalError>),
}

pub type Result<T> = result::Result<T, SyncError>;
