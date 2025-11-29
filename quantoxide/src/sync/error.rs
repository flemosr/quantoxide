use std::{result, sync::Arc};

use thiserror::Error;

use lnm_sdk::api_v3::error::RestApiError;

use super::{engine::LookbackPeriod, process::error::SyncProcessFatalError, state::SyncStatus};

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("REST API client initialization error: {0}")]
    RestApiInit(RestApiError),

    #[error("Invalid lookback period, must be at least {}", LookbackPeriod::MIN)]
    InvalidLookbackPeriodTooShort,

    #[error("Invalid lookback period, must be at most {}", LookbackPeriod::MAX)]
    InvalidLookbackPeriodTooLong,

    #[error("Sync already shutdown error")]
    SyncAlreadyShutdown,

    #[error("Sync process already terminated error, status: {0}")]
    SyncAlreadyTerminated(SyncStatus),

    #[error("Sync shutdown procedure failed: {0}")]
    SyncShutdownFailed(Arc<SyncProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SyncError>;
