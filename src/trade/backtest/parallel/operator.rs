use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    shared::{Lookback, OhlcResolution, Period},
    signal::{Signal, SignalEvaluator},
};

use super::super::{
    super::core::TradeExecutor,
    consolidator::MultiResolutionConsolidator,
    error::Result,
    operator::{
        RawOperatorPending, RawOperatorRunning, SignalOperatorPending, SignalOperatorRunning,
    },
};

/// Type-erased signal operator in pending state.
pub(super) trait AnySignalOperatorPending: Send {
    fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period>;
    fn max_lookback(&self) -> Option<Lookback>;
    fn start(
        self: Box<Self>,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<Box<dyn AnySignalOperatorRunning>>;
}

/// Type-erased signal operator in running state.
#[async_trait]
pub(super) trait AnySignalOperatorRunning: Send + Sync {
    async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()>;
}

impl<S: Signal> AnySignalOperatorPending for SignalOperatorPending<S> {
    fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        self.resolution_to_max_period()
    }

    fn max_lookback(&self) -> Option<Lookback> {
        self.max_lookback()
    }

    fn start(
        self: Box<Self>,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<Box<dyn AnySignalOperatorRunning>> {
        let running = (*self).start(start_time, trade_executor)?;
        Ok(Box::new(running))
    }
}

#[async_trait]
impl<S: Signal> AnySignalOperatorRunning for SignalOperatorRunning<S> {
    async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        self.iterate(time_cursor, consolidator).await
    }
}

/// Type-erased operator in pending state before starting.
pub(super) enum ParallelOperatorPending {
    Signal(Box<dyn AnySignalOperatorPending>),
    Raw(RawOperatorPending),
}

impl ParallelOperatorPending {
    /// Creates a new signal operator in pending state.
    pub fn signal<S: Signal>(
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: Box<dyn crate::trade::SignalOperator<S>>,
    ) -> Result<Self> {
        let pending = SignalOperatorPending::new(evaluators, signal_operator.into())?;
        Ok(Self::Signal(Box::new(pending)))
    }

    /// Creates a new raw operator in pending state.
    pub fn raw(raw_operator: Box<dyn crate::trade::RawOperator>) -> Result<Self> {
        let pending = RawOperatorPending::new(raw_operator.into())?;
        Ok(Self::Raw(pending))
    }

    /// Returns the resolution to max period mapping for this operator.
    pub fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        match self {
            Self::Signal(pending) => pending.resolution_to_max_period(),
            Self::Raw(pending) => pending.resolution_to_max_period(),
        }
    }

    /// Returns the maximum lookback for this operator.
    pub fn max_lookback(&self) -> Option<Lookback> {
        match self {
            Self::Signal(pending) => pending.max_lookback(),
            Self::Raw(pending) => pending.max_lookback(),
        }
    }

    /// Starts the operator, transitioning it to the running state.
    pub fn start(
        self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<ParallelOperatorRunning> {
        match self {
            Self::Signal(pending) => pending
                .start(start_time, trade_executor)
                .map(ParallelOperatorRunning::Signal),
            Self::Raw(pending) => pending
                .start(start_time, trade_executor)
                .map(ParallelOperatorRunning::Raw),
        }
    }
}

/// Type-erased operator in running state.
pub(super) enum ParallelOperatorRunning {
    Signal(Box<dyn AnySignalOperatorRunning>),
    Raw(RawOperatorRunning),
}

impl ParallelOperatorRunning {
    /// Iterates the operator with the given time cursor and consolidator.
    pub async fn iterate(
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
