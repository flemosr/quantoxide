use std::{result, sync::Arc};

use thiserror::Error;

use lnm_sdk::api_v2::error::RestApiError;

use super::{process::error::SyncProcessFatalError, state::SyncStatus};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("API initialization error: {0}")]
    ApiInit(RestApiError),

    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Sync process already terminated error, status: {0}")]
    SyncAlreadyTerminated(SyncStatus),

    #[error("Sync shutdown procedure failed: {0}")]
    SyncShutdownFailed(Arc<SyncProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SyncError>;
