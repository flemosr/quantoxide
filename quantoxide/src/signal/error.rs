use std::{result, sync::Arc};

use thiserror::Error;

use super::{process::error::SignalProcessFatalError, state::LiveSignalStatus};

#[derive(Error, Debug)]
pub enum SignalValidationError {
    #[error("`SignalName` cannot be an empty `String`")]
    InvalidSignalNameEmptyString,

    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,
}

pub type ValidationResult<T> = result::Result<T, SignalValidationError>;

#[derive(Error, Debug)]
pub enum SignalError {
    #[error(transparent)]
    SignalValidation(#[from] SignalValidationError),

    #[error("Live Signal process already shutdown error")]
    LiveSignalAlreadyShutdown,

    #[error("Live Signal process already terminated error, status: {0}")]
    LiveSignalAlreadyTerminated(LiveSignalStatus),

    #[error("Signal shutdown procedure failed: {0}")]
    SignalShutdownFailed(Arc<SignalProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SignalError>;
