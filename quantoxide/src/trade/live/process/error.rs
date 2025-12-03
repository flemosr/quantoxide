use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{
    db::error::DbError,
    signal::{error::SignalError, process::error::SignalProcessFatalError},
    sync::{error::SyncError, process::error::SyncProcessFatalError},
};

use super::super::{
    super::error::TradeCoreError,
    executor::error::{ExecutorProcessFatalError, LiveTradeExecutorError},
};

#[derive(Error, Debug)]
pub enum LiveProcessRecoverableError {
    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("Operator error: {0}")]
    OperatorError(TradeCoreError),

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },

    #[error("`SignalRecvLagged` error, skipped: {skipped}")]
    SignalRecvLagged { skipped: u64 },

    #[error("`ExecutorRecvLagged` error, skipped: {skipped}")]
    ExecutorRecvLagged { skipped: u64 },
    // #[error("Operator iteration time too long for iteration interval")]
    // OperatorIterationTimeTooLong,
}

#[derive(Error, Debug)]
pub enum LiveProcessFatalError {
    #[error("Launch executor error {0}")]
    LaunchExecutor(LiveTradeExecutorError),

    #[error("Setup operator error: {0}")]
    StartOperatorError(TradeCoreError),

    #[error("[TaskJoin] {0}")]
    LiveProcessTaskJoin(JoinError),

    #[error("`SyncRecvClosed` error")]
    SyncRecvClosed,

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>),

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown,

    #[error("`LiveSignal` process (dependency) was terminated with error: {0}")]
    LiveSignalProcessTerminated(Arc<SignalProcessFatalError>),

    #[error("`LiveSignal` process (dependency) was shutdown")]
    LiveSignalProcessShutdown,

    #[error("`SignalRecvClosed` error")]
    SignalRecvClosed,

    #[error("`LiveTradeExecutor` process (dependency) was terminated with error: {0}")]
    ExecutorProcessTerminated(Arc<ExecutorProcessFatalError>),

    #[error("`LiveTradeExecutor` process (dependency) was shutdown")]
    ExecutorProcessShutdown,

    #[error("`ExecutorRecvClosed` error")]
    ExecutorRecvClosed,

    #[error("Failed to send live trade process shutdown signal error: {0}")]
    SendShutdownSignalFailed(SendError<()>),

    #[error("Shutdown signal channel recv error: {0}")]
    ShutdownSignalRecv(RecvError),

    #[error("Live shutdown process timeout error")]
    ShutdownTimeout,

    #[error("`LiveTradeExecutor` shutdown error: {0}")]
    ExecutorShutdownError(LiveTradeExecutorError),

    #[error("Error while shutting down `LiveSignal`: {0}")]
    LiveSignalShutdown(SignalError),

    #[error("Error while shutting down `Sync`: {0}")]
    SyncShutdown(SyncError),
}

pub(crate) type LiveProcessFatalResult<T> = result::Result<T, LiveProcessFatalError>;

#[derive(Error, Debug)]
pub enum LiveProcessError {
    #[error(transparent)]
    Recoverable(#[from] LiveProcessRecoverableError),

    #[error(transparent)]
    Fatal(#[from] LiveProcessFatalError),
}

pub(super) type Result<T> = result::Result<T, LiveProcessError>;
