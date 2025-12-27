use std::{result, sync::Arc};

use thiserror::Error;

use lnm_sdk::api_v3::error::RestApiError;

use crate::signal::error::SignalError;

use super::{
    super::error::TradeCoreError, executor::error::LiveTradeExecutorError,
    process::error::LiveProcessFatalError, state::LiveTradeStatus,
};

#[derive(Error, Debug)]
pub enum LiveError {
    #[error("REST API client initialization error: {0}")]
    RestApiInit(RestApiError),

    #[error("Setup executor error {0}")]
    SetupExecutor(LiveTradeExecutorError),

    #[error("Setup operator error: {0}")]
    SetupOperatorError(TradeCoreError),

    #[error("Live trade process already shutdown error")]
    LiveAlreadyShutdown,

    #[error("Live trade process already terminated error, status: {0}")]
    LiveAlreadyTerminated(LiveTradeStatus),

    #[error("Lauch `LiveSignal` error: {0}")]
    LaunchLiveSignalEngine(SignalError),

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,

    #[error("Live shutdown procedure failed: {0}")]
    LiveShutdownFailed(Arc<LiveProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, LiveError>;
