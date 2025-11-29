use std::{result, sync::Arc};

use thiserror::Error;

use lnm_sdk::api_v2::error::RestApiError;

use super::{engine::SyncMode, process::error::SyncProcessFatalError, state::SyncStatus};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("REST API client initialization error: {0}")]
    RestApiInit(RestApiError),

    #[error("Invalid live range, must be in round minutes")]
    InvalidLiveRangeNotRoundMinutes,

    #[error("Invalid live range, must be at least {}", SyncMode::MIN_LIVE_RANGE)]
    InvalidLiveRangeTooShort,

    #[error("Invalid live range, must be at most {}", SyncMode::MAX_LIVE_RANGE)]
    InvalidLiveRangeTooLong,

    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Sync process already terminated error, status: {0}")]
    SyncAlreadyTerminated(SyncStatus),

    #[error("Sync shutdown procedure failed: {0}")]
    SyncShutdownFailed(Arc<SyncProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SyncError>;
