use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    shared::{Lookback, OhlcResolution, Period},
    signal::{Signal, SignalEvaluator, WrappedSignalEvaluator, error::SignalOperatorError},
    trade::core::{TradeExecutor, WrappedRawOperator, WrappedSignalOperator},
};

use super::super::{
    consolidator::MultiResolutionConsolidator,
    error::{BacktestError, Result},
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

/// Signal operator in pending state for a specific signal type.
struct SignalOperatorPending<S: Signal> {
    evaluators: Vec<WrappedSignalEvaluator<S>>,
    signal_operator: WrappedSignalOperator<S>,
    resolution_to_max_period: HashMap<OhlcResolution, Period>,
    max_lookback: Option<Lookback>,
}

impl<S: Signal> SignalOperatorPending<S> {
    fn new(
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: WrappedSignalOperator<S>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(BacktestError::SignalOperator(
                SignalOperatorError::EmptyEvaluatorsVec,
            ));
        }

        let evaluators: Vec<_> = evaluators
            .into_iter()
            .map(WrappedSignalEvaluator::new)
            .collect();

        let mut resolution_map: HashMap<OhlcResolution, Period> = HashMap::new();
        let mut max_lookback: Option<Lookback> = None;

        for evaluator in &evaluators {
            if let Some(lookback) = evaluator
                .lookback()
                .map_err(BacktestError::SignalEvaluator)?
            {
                resolution_map
                    .entry(lookback.resolution())
                    .and_modify(|existing| {
                        if lookback.period() > *existing {
                            *existing = lookback.period();
                        }
                    })
                    .or_insert(lookback.period());

                if max_lookback
                    .is_none_or(|existing| existing.as_duration() < lookback.as_duration())
                {
                    max_lookback = Some(lookback);
                }
            }
        }

        Ok(Self {
            evaluators,
            signal_operator,
            resolution_to_max_period: resolution_map,
            max_lookback,
        })
    }
}

impl<S: Signal> AnySignalOperatorPending for SignalOperatorPending<S> {
    fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        &self.resolution_to_max_period
    }

    fn max_lookback(&self) -> Option<Lookback> {
        self.max_lookback
    }

    fn start(
        mut self: Box<Self>,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<Box<dyn AnySignalOperatorRunning>> {
        self.signal_operator
            .set_trade_executor(trade_executor)
            .map_err(BacktestError::SetTradeExecutor)?;

        let evaluators = self
            .evaluators
            .into_iter()
            .map(|ev| (start_time, ev))
            .collect();

        Ok(Box::new(SignalOperatorRunning {
            evaluators,
            signal_operator: self.signal_operator,
        }))
    }
}

/// Signal operator in running state for a specific signal type.
struct SignalOperatorRunning<S: Signal> {
    evaluators: Vec<(DateTime<Utc>, WrappedSignalEvaluator<S>)>,
    signal_operator: WrappedSignalOperator<S>,
}

#[async_trait]
impl<S: Signal> AnySignalOperatorRunning for SignalOperatorRunning<S> {
    async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        for (last_eval, evaluator) in &mut self.evaluators {
            let min_iteration_interval = evaluator
                .min_iteration_interval()
                .map_err(BacktestError::SignalEvaluator)?
                .as_duration();

            if time_cursor < *last_eval + min_iteration_interval {
                continue;
            }

            *last_eval = time_cursor;

            let lookback = evaluator
                .lookback()
                .map_err(BacktestError::SignalEvaluator)?;

            let eval_candles = match lookback {
                Some(lb) => {
                    let ctx_candles = consolidator
                        .and_then(|c| c.get_candles(lb.resolution()))
                        .expect("must not be `None` when evaluator has lookback");
                    let start_idx = ctx_candles.len().saturating_sub(lb.period().as_usize());
                    &ctx_candles[start_idx..]
                }
                None => &[],
            };

            let signal = evaluator
                .evaluate(eval_candles)
                .await
                .map_err(BacktestError::SignalEvaluator)?;

            self.signal_operator
                .process_signal(&signal)
                .await
                .map_err(BacktestError::SignalProcessingError)?;
        }

        Ok(())
    }
}

/// Type-erased operator in pending state before starting.
pub(super) enum ParallelOperatorPending {
    Signal(Box<dyn AnySignalOperatorPending>),
    Raw {
        raw_operator: WrappedRawOperator,
        resolution_to_max_period: HashMap<OhlcResolution, Period>,
        max_lookback: Option<Lookback>,
    },
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
        let raw_operator: WrappedRawOperator = raw_operator.into();

        let lookback = raw_operator
            .lookback()
            .map_err(BacktestError::OperatorError)?;

        let (resolution_to_max_period, max_lookback) = if let Some(lb) = lookback {
            let mut map = HashMap::new();
            map.insert(lb.resolution(), lb.period());
            (map, Some(lb))
        } else {
            (HashMap::new(), None)
        };

        Ok(Self::Raw {
            raw_operator,
            resolution_to_max_period,
            max_lookback,
        })
    }

    /// Returns the resolution to max period mapping for this operator.
    pub fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        match self {
            Self::Signal(pending) => pending.resolution_to_max_period(),
            Self::Raw {
                resolution_to_max_period,
                ..
            } => resolution_to_max_period,
        }
    }

    /// Returns the maximum lookback for this operator.
    pub fn max_lookback(&self) -> Option<Lookback> {
        match self {
            Self::Signal(pending) => pending.max_lookback(),
            Self::Raw { max_lookback, .. } => *max_lookback,
        }
    }

    /// Starts the operator, transitioning it to the running state.
    pub fn start(
        self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<ParallelOperatorRunning> {
        match self {
            Self::Signal(pending) => {
                let running = pending.start(start_time, trade_executor)?;
                Ok(ParallelOperatorRunning::Signal(running))
            }
            Self::Raw {
                mut raw_operator, ..
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(BacktestError::SetTradeExecutor)?;

                Ok(ParallelOperatorRunning::Raw {
                    last_eval: start_time,
                    raw_operator,
                })
            }
        }
    }
}

/// Type-erased operator in running state.
pub(super) enum ParallelOperatorRunning {
    Signal(Box<dyn AnySignalOperatorRunning>),
    Raw {
        last_eval: DateTime<Utc>,
        raw_operator: WrappedRawOperator,
    },
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
            Self::Raw {
                last_eval,
                raw_operator,
            } => {
                let min_iteration_interval = raw_operator
                    .min_iteration_interval()
                    .map_err(BacktestError::OperatorError)?
                    .as_duration();

                if time_cursor >= *last_eval + min_iteration_interval {
                    *last_eval = time_cursor;

                    let lookback = raw_operator
                        .lookback()
                        .map_err(BacktestError::OperatorError)?;

                    let ctx_candles = match lookback {
                        Some(lb) => consolidator
                            .and_then(|c| c.get_candles(lb.resolution()))
                            .expect("must not be `None` when evaluator has lookback"),
                        None => &[],
                    };

                    raw_operator
                        .iterate(ctx_candles)
                        .await
                        .map_err(BacktestError::OperatorError)?;
                }

                Ok(())
            }
        }
    }
}
