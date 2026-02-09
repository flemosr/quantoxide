use std::{collections::HashMap, sync::Arc};

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

/// Pending operator state before starting.
pub(super) enum OperatorPending<S: Signal> {
    Signal {
        evaluators: Vec<WrappedSignalEvaluator<S>>,
        signal_operator: WrappedSignalOperator<S>,
        /// Max period per resolution
        resolution_to_max_period: HashMap<OhlcResolution, Period>,
        max_lookback: Option<Lookback>,
    },
    Raw {
        raw_operator: WrappedRawOperator,
        /// Max period per resolution
        resolution_to_max_period: HashMap<OhlcResolution, Period>,
        max_lookback: Option<Lookback>,
    },
}

impl<S: Signal> OperatorPending<S> {
    pub(super) fn signal(
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

        Ok(Self::Signal {
            evaluators,
            signal_operator,
            resolution_to_max_period: resolution_map,
            max_lookback,
        })
    }

    pub(super) fn raw(raw_operator: WrappedRawOperator) -> Result<Self> {
        let lookback = raw_operator
            .lookback()
            .map_err(BacktestError::OperatorError)?;

        let (resolution_map, max_lookback) = if let Some(lb) = lookback {
            let mut map = HashMap::new();
            map.insert(lb.resolution(), lb.period());
            (map, Some(lb))
        } else {
            (HashMap::new(), None)
        };

        Ok(Self::Raw {
            raw_operator,
            resolution_to_max_period: resolution_map,
            max_lookback,
        })
    }

    pub(super) fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        match self {
            OperatorPending::Signal {
                resolution_to_max_period,
                ..
            } => resolution_to_max_period,
            OperatorPending::Raw {
                resolution_to_max_period,
                ..
            } => resolution_to_max_period,
        }
    }

    pub(super) fn max_lookback(&self) -> Option<Lookback> {
        match self {
            OperatorPending::Signal { max_lookback, .. } => *max_lookback,
            OperatorPending::Raw { max_lookback, .. } => *max_lookback,
        }
    }

    pub(super) fn start(
        self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<OperatorRunning<S>> {
        match self {
            OperatorPending::Signal {
                evaluators,
                mut signal_operator,
                ..
            } => {
                signal_operator
                    .set_trade_executor(trade_executor)
                    .map_err(BacktestError::SetTradeExecutor)?;

                let evaluators = evaluators.into_iter().map(|ev| (start_time, ev)).collect();

                Ok(OperatorRunning::Signal {
                    evaluators,
                    signal_operator,
                })
            }
            OperatorPending::Raw {
                mut raw_operator, ..
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(BacktestError::SetTradeExecutor)?;

                Ok(OperatorRunning::Raw {
                    last_eval: start_time,
                    raw_operator,
                })
            }
        }
    }
}

/// Running operator state.
pub(super) enum OperatorRunning<S: Signal> {
    Signal {
        /// (last_eval_time, evaluator)
        evaluators: Vec<(DateTime<Utc>, WrappedSignalEvaluator<S>)>,
        signal_operator: WrappedSignalOperator<S>,
    },
    Raw {
        last_eval: DateTime<Utc>,
        raw_operator: WrappedRawOperator,
    },
}

impl<S: Signal> OperatorRunning<S> {
    pub(super) async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        match self {
            OperatorRunning::Signal {
                evaluators,
                signal_operator,
            } => Self::iterate_signal(evaluators, signal_operator, time_cursor, consolidator).await,
            OperatorRunning::Raw {
                last_eval,
                raw_operator,
            } => Self::iterate_raw(last_eval, raw_operator, time_cursor, consolidator).await,
        }
    }

    async fn iterate_signal(
        evaluators: &mut [(DateTime<Utc>, WrappedSignalEvaluator<S>)],
        signal_operator: &WrappedSignalOperator<S>,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        for (last_eval, evaluator) in evaluators {
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

            signal_operator
                .process_signal(&signal)
                .await
                .map_err(BacktestError::SignalProcessingError)?;
        }

        Ok(())
    }

    async fn iterate_raw(
        last_eval: &mut DateTime<Utc>,
        raw_operator: &WrappedRawOperator,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
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
