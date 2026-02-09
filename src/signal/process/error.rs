use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{
    db::error::DbError, signal::error::SignalEvaluatorError,
    sync::process::error::SyncProcessFatalError,
};

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SignalProcessRecoverableError {
    #[error(transparent)]
    Evaluator(SignalEvaluatorError),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SignalProcessFatalError {
    #[error(transparent)]
    Evaluator(SignalEvaluatorError),

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown,

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>),

    #[error("`SyncRecvClosed` error")]
    SyncRecvClosed,

    #[error("TaskJoin error {0}")]
    LiveSignalProcessTaskJoin(JoinError),

    #[error("Shutdown `RecvError` error: {0}")]
    ShutdownSignalRecv(RecvError),

    #[error("Failed to send live signal process shutdown request error: {0}")]
    SendShutdownSignalFailed(SendError<()>),

    #[error("Live Signal shutdown timeout error")]
    ShutdownTimeout,
}

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum SignalProcessError {
    #[error(transparent)]
    Recoverable(#[from] SignalProcessRecoverableError),

    #[error(transparent)]
    Fatal(#[from] SignalProcessFatalError),
}

pub(crate) type ProcessResult<T> = result::Result<T, SignalProcessError>;
