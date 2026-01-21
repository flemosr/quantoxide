use std::{result, sync::Arc};

use thiserror::Error;

use crate::shared::OhlcResolution;

use super::{
    process::error::{SignalProcessFatalError, SignalProcessRecoverableError},
    state::LiveSignalStatus,
};

#[derive(Error, Debug)]
pub enum SignalValidationError {
    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,

    #[error(transparent)]
    LookbackPanicked(#[from] SignalProcessRecoverableError),

    #[error("All evaluators must use the same resolution, found {0} and {1}")]
    MismatchedEvaluatorResolutions(OhlcResolution, OhlcResolution),
}

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
