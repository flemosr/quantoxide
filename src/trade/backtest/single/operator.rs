use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};

use crate::{
    shared::{Lookback, OhlcResolution, Period},
    signal::{Signal, SignalEvaluator},
};

use super::super::{
    super::core::{TradeExecutor, WrappedRawOperator, WrappedSignalOperator},
    consolidator::MultiResolutionConsolidator,
    error::Result,
    operator::{
        RawOperatorPending, RawOperatorRunning, SignalOperatorPending, SignalOperatorRunning,
    },
};

/// Pending operator state before starting.
pub(super) enum OperatorPending<S: Signal> {
    Signal(SignalOperatorPending<S>),
    Raw(RawOperatorPending),
}

impl<S: Signal> OperatorPending<S> {
    pub(super) fn signal(
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: WrappedSignalOperator<S>,
    ) -> Result<Self> {
        SignalOperatorPending::new(evaluators, signal_operator).map(Self::Signal)
    }

    pub(super) fn raw(raw_operator: WrappedRawOperator) -> Result<Self> {
        RawOperatorPending::new(raw_operator).map(Self::Raw)
    }

    pub(super) fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        match self {
            Self::Signal(pending) => pending.resolution_to_max_period(),
            Self::Raw(pending) => pending.resolution_to_max_period(),
        }
    }

    pub(super) fn max_lookback(&self) -> Option<Lookback> {
        match self {
            Self::Signal(pending) => pending.max_lookback(),
            Self::Raw(pending) => pending.max_lookback(),
        }
    }

    pub(super) fn start(
        self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<OperatorRunning<S>> {
        match self {
            Self::Signal(pending) => pending
                .start(start_time, trade_executor)
                .map(OperatorRunning::Signal),
            Self::Raw(pending) => pending
                .start(start_time, trade_executor)
                .map(OperatorRunning::Raw),
        }
    }
}

/// Running operator state.
pub(super) enum OperatorRunning<S: Signal> {
    Signal(SignalOperatorRunning<S>),
    Raw(RawOperatorRunning),
}

impl<S: Signal> OperatorRunning<S> {
    pub(super) async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        match self {
            Self::Signal(running) => running.iterate(time_cursor, consolidator).await,
            Self::Raw(running) => running.iterate(time_cursor, consolidator).await,
        }
    }
}
