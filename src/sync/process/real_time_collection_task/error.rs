use std::result;

use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;

use lnm_sdk::stream::v1::{StreamConnectionStatus, error::StreamApiError};

use crate::db::error::DbError;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum RealTimeCollectionError {
    #[error("[StreamApi] {0}")]
    StreamApi(#[from] StreamApiError),

    #[error("BadConnectionUpdate error: {0}")]
    BadConnectionUpdate(StreamConnectionStatus),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("RecvLagged error, skipped: {skipped}")]
    StreamRecvLagged { skipped: u64 },

    #[error("RecvLagged error")]
    StreamRecvClosed,

    #[error("Shutdown signal channel error: {0}")]
    ShutdownSignalRecv(RecvError),
}

pub(super) type Result<T> = result::Result<T, RealTimeCollectionError>;
