use std::{result, sync::Arc};

use thiserror::Error;

use crate::{shared::OhlcResolution, util::PanicPayload};

use super::{process::error::SignalProcessFatalError, state::LiveSignalStatus};

#[derive(Error, Debug)]
pub enum SignalEvaluatorError {
    #[error("`SignalEvaluator::lookback` panicked: {0}")]
    LookbackPanicked(PanicPayload),

    #[error("`SignalEvaluator::min_iteration_interval` panicked: {0}")]
    MinIterationIntervalPanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` panicked: {0}")]
    EvaluatePanicked(PanicPayload),

    #[error("`SignalEvaluator::evaluate` error: {0}")]
    EvaluateError(String),
}

pub(crate) type SignalEvaluatorResult<T> = result::Result<T, SignalEvaluatorError>;

#[derive(Error, Debug)]
pub enum SignalOperatorError {
    #[error("At least one signal evaluator must be provided")]
    EmptyEvaluatorsVec,

    #[error("All evaluators must use the same resolution, found {0} and {1}")]
    MismatchedEvaluatorResolutions(OhlcResolution, OhlcResolution),
}

#[derive(Error, Debug)]
pub enum SignalError {
    #[error(transparent)]
    Evaluator(SignalEvaluatorError),

    #[error(transparent)]
    Operator(SignalOperatorError),

    #[error("Live Signal process already shutdown error")]
    LiveSignalAlreadyShutdown,

    #[error("Live Signal process already terminated error, status: {0}")]
    LiveSignalAlreadyTerminated(LiveSignalStatus),

    #[error("Signal shutdown procedure failed: {0}")]
    SignalShutdownFailed(Arc<SignalProcessFatalError>),
}

pub(super) type Result<T> = result::Result<T, SignalError>;
