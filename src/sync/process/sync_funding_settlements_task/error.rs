use std::{num::NonZeroU64, result};

use chrono::{DateTime, Utc};
use thiserror::Error;

use lnm_sdk::api_v3::error::RestApiError;

use crate::db::error::DbError;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SyncFundingSettlementsError {
    #[error("RestApiMaxTrialsReached error: error {error}, trials {trials}")]
    RestApiMaxTrialsReached {
        error: RestApiError,
        trials: NonZeroU64,
    },

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error(
        "Unreachable missing settlement detected. Missing at {time}, configured reach at {reach}"
    )]
    UnreachableMissingSettlement {
        time: DateTime<Utc>,
        reach: DateTime<Utc>,
    },

    #[error("HistoryUpdateHandlerFailed error")]
    HistoryUpdateHandlerFailed,

    #[error(
        "Funding settlements state `reach` was not set, and it is required to evaluate DB gaps"
    )]
    FundingSettlementsStateReachNotSet,

    #[error("API funding settlements before {history_start} are not available")]
    ApiSettlementsNotAvailableBeforeHistoryStart { history_start: DateTime<Utc> },

    #[error("Invalid funding settlement time received from API: {time}")]
    InvalidSettlementTime { time: DateTime<Utc> },
}

pub(super) type Result<T> = result::Result<T, SyncFundingSettlementsError>;
