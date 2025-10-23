use std::{result, sync::Arc};

use thiserror::Error;

use crate::signal::error::SignalError;

use super::{
    super::error::TradeCoreError, executor::error::LiveTradeExecutorError,
    process::error::LiveProcessFatalError,
};

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("Launch executor error {0}")]
    LauchExecutor(LiveTradeExecutorError),

    #[error("Setup operator error: {0}")]
    SetupOperatorError(TradeCoreError),

    #[error("Live trade process already shutdown error")]
    LiveAlreadyShutdown,

    #[error("Lauch `LiveSignal` error: {0}")]
    LaunchLiveSignalEngine(SignalError),

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,

    #[error("Live shutdown procedure failed: {0}")]
    LiveShutdownFailed(Arc<LiveProcessFatalError>),
}

pub type Result<T> = result::Result<T, LiveError>;
