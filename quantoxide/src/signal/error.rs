use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{db::error::DbError, sync::SyncProcessFatalError, util::PanicPayload};

use super::live::LiveSignalStatus;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("SignalTransmiterFailed failed error {0}")]
    SignalTransmiterFailed(SendError<Arc<LiveSignalStatus>>),

    #[error("TaskJoin error {0}")]
    LiveSignalProcessTaskJoin(JoinError), // Not recoverable

    #[error("It was not possible to convert `evaluation_interval_secs` to `NonZeroU64`")]
    InvalidEvaluationInterval,

    #[error("`SignalEvaluator::evaluate` panicked: {0}")]
    EvaluatePanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` error: {0}")]
    EvaluateError(String),

    #[error("`SignalName` cannot be an empty `String`")]
    InvalidSignalNameEmptyString,

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

    #[error("Live Signal already shutdown error")]
    LiveSignalAlreadyShutdown,

    #[error("Shutdown `RecvError` error: {0}")]
    ShutdownSignalRecv(RecvError), // Not recoverable

    #[error("Failed to send live signal process shutdown request error: {0}")]
    SendShutdownSignalFailed(SendError<()>), // Not recoverable

    #[error("Live Signal shutdown timeout error")]
    ShutdownTimeout,

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,
}

pub type Result<T> = result::Result<T, SignalError>;
