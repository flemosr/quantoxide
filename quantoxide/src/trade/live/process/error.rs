use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{
    db::error::DbError,
    signal::{error::SignalError, process::error::SignalProcessFatalError},
    sync::{SyncError, SyncProcessFatalError},
};

use super::super::{super::error::TradeCoreError, executor::error::LiveTradeExecutorError};

#[derive(Error, Debug)]
pub enum LiveProcessError {
    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("[TaskJoin] {0}")]
    LiveProcessTaskJoin(JoinError), // Not recoverable

    #[error("Operator error: {0}")]
    OperatorError(TradeCoreError),

    #[error("`SyncRecvClosed` error")]
    SyncRecvClosed, // Not recoverable

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>), // Not recoverable

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown, // Not recoverable

    #[error("`LiveSignal` process (dependency) was terminated with error: {0}")]
    LiveSignalProcessTerminated(Arc<SignalProcessFatalError>), // Not recoverable

    #[error("`LiveSignal` process (dependency) was shutdown")]
    LiveSignalProcessShutdown, // Not recoverable

    #[error("`SignalRecvClosed` error")]
    SignalRecvClosed, // Not recoverable

    #[error("`SignalRecvLagged` error, skipped: {skipped}")]
    SignalRecvLagged { skipped: u64 },

    #[error("Shutdown signal channel recv error: {0}")]
    ShutdownSignalRecv(RecvError), // Not recoverable

    #[error("Failed to send live trade process shutdown signal error: {0}")]
    SendShutdownSignalFailed(SendError<()>),

    #[error("Live shutdown process timeout error")]
    ShutdownTimeout, // Not recoverable

    #[error("`LiveTradeExecutor` shutdown error: {0}")]
    ExecutorShutdownError(LiveTradeExecutorError), // Not recoverable

    #[error("Error while shutting down `LiveSignal`: {0}")]
    LiveSignalShutdown(SignalError), // Not recoverable

    #[error("Error while shutting down `Sync`: {0}")]
    SyncShutdown(SyncError), // Not recoverable

    #[error("Operator iteration time too long for iteration interval")]
    OperatorIterationTimeTooLong,
}

pub type Result<T> = result::Result<T, LiveProcessError>;
