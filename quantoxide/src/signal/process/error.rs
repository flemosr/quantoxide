use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{db::error::DbError, sync::SyncProcessFatalError, util::PanicPayload};

#[derive(Error, Debug)]
pub enum SignalProcessError {
    #[error("`SignalEvaluator::evaluate` panicked: {0}")]
    EvaluatePanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` error: {0}")]
    EvaluateError(String),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown, // Not recoverable

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>), // Not recoverable

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },

    #[error("`SyncRecvClosed` error")]
    SyncRecvClosed, // Not recoverable

    #[error("TaskJoin error {0}")]
    LiveSignalProcessTaskJoin(JoinError), // Not recoverable

    #[error("Shutdown `RecvError` error: {0}")]
    ShutdownSignalRecv(RecvError), // Not recoverable

    #[error("Failed to send live signal process shutdown request error: {0}")]
    SendShutdownSignalFailed(SendError<()>), // Not recoverable

    #[error("Live Signal shutdown timeout error")]
    ShutdownTimeout,
}

pub type ProcessResult<T> = result::Result<T, SignalProcessError>;
