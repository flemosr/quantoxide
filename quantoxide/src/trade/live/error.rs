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

use super::{super::error::TradeCoreError, executor::error::LiveTradeExecutorError};

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("[TaskJoin] {0}")]
    TaskJoin(JoinError),

    #[error("Launch executor error {0}")]
    LauchExecutor(LiveTradeExecutorError),

    #[error("Operator error: {0}")]
    OperatorError(TradeCoreError),

    #[error("`Sync` `RecvError` error: {0}")]
    SyncRecv(RecvError), // TODO: Split

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>), // Not recoverable

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown, // Not recoverable

    #[error("`LiveSignal` process (dependency) was terminated with error: {0}")]
    LiveSignalProcessTerminated(Arc<SignalProcessFatalError>), // Not recoverable

    #[error("`LiveSignal` process (dependency) was shutdown")]
    LiveSignalProcessShutdown,

    #[error("`LiveSignal` `RecvError` error: {0}")]
    SignalRecv(RecvError), // Split

    #[error("`LiveTradeExecutor` `RecvError` error: {0}")]
    ExecutorRecv(RecvError),

    #[error("[ShutdownRecv] {0}")]
    ShutdownRecv(RecvError),

    #[error("Live trade process already shutdown error")]
    LiveAlreadyShutdown,

    #[error("Failed to send live trade process shutdown request error: {0}")]
    SendShutdownFailed(SendError<()>),

    #[error("Live shutdown timeout error")]
    ShutdownTimeout,

    #[error("`LiveTradeExecutor` shutdown error: {0}")]
    ExecutorShutdownError(LiveTradeExecutorError),

    #[error("Error while shutting down `LiveSignal`: {0}")]
    LiveSignalShutdown(SignalError), // Not recoverable

    #[error("Error while shutting down `Sync`: {0}")]
    SyncShutdown(SyncError),

    #[error("Lauch `LiveSignal` error: {0}")]
    LaunchLiveSignalEngine(SignalError),

    #[error("Operator iteration time too long for iteration interval")]
    OperatorIterationTimeTooLong,

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,
}

pub type Result<T> = result::Result<T, LiveError>;
