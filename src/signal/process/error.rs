use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{db::error::DbError, sync::process::error::SyncProcessFatalError, util::PanicPayload};

#[derive(Error, Debug)]
pub enum SignalProcessRecoverableError {
    #[error("`SignalEvaluator::lookback` panicked: {0}")]
    LookbackPanicked(PanicPayload),

    #[error("`SignalEvaluator::min_iteration_interval` panicked: {0}")]
    MinIterationIntervalPanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` panicked: {0}")]
    EvaluatePanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` error: {0}")]
    EvaluateError(String),

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },
}

pub(crate) type ProcessRecoverableResult<T> = result::Result<T, SignalProcessRecoverableError>;

#[derive(Error, Debug)]
pub enum SignalProcessFatalError {
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
pub enum SignalProcessError {
    #[error(transparent)]
    Recoverable(#[from] SignalProcessRecoverableError),

    #[error(transparent)]
    Fatal(#[from] SignalProcessFatalError),
}

pub(crate) type ProcessResult<T> = result::Result<T, SignalProcessError>;
