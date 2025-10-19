use std::{result, sync::Arc};

use thiserror::Error;
use tokio::{
    sync::broadcast::error::{RecvError, SendError},
    task::JoinError,
};

use crate::{db::error::DbError, util::PanicPayload};

use super::live::LiveSignalStatus;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error("SignalTransmiterFailed failed error {0}")]
    SignalTransmiterFailed(SendError<Arc<LiveSignalStatus>>),

    #[error("TaskJoin error {0}")]
    TaskJoin(JoinError),

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
    SyncProcessShutdown,

    #[error("`Sync` `RecvError` error: {0}")]
    SyncRecv(RecvError),

    #[error("Shutdown `RecvError` error: {0}")]
    ShutdownRecv(RecvError),

    #[error("Live Signal already shutdown error")]
    LiveSignalAlreadyShutdown,

    #[error("Failed to send live signal process shutdown request error: {0}")]
    SendShutdownFailed(SendError<()>),

    #[error("Live Signal shutdown timeout error")]
    ShutdownTimeout,

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,
}

impl PartialEq for SignalError {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for SignalError {}

pub type Result<T> = result::Result<T, SignalError>;
