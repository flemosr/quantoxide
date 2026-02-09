use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Utc};

use crate::{
    shared::{Lookback, OhlcResolution, Period},
    signal::{Signal, SignalEvaluator, WrappedSignalEvaluator, error::SignalOperatorError},
    trade::core::{TradeExecutor, WrappedRawOperator, WrappedSignalOperator},
};

use super::{
    consolidator::MultiResolutionConsolidator,
    error::{BacktestError, Result},
};

/// Signal operator in pending state for a specific signal type.
pub(super) struct SignalOperatorPending<S: Signal> {
    evaluators: Vec<WrappedSignalEvaluator<S>>,
    signal_operator: WrappedSignalOperator<S>,
    resolution_to_max_period: HashMap<OhlcResolution, Period>,
    max_lookback: Option<Lookback>,
}

impl<S: Signal> SignalOperatorPending<S> {
    pub(super) fn new(
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

    pub(super) fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        &self.resolution_to_max_period
    }

    pub(super) fn max_lookback(&self) -> Option<Lookback> {
        self.max_lookback
    }

    pub(super) fn start(
        mut self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<SignalOperatorRunning<S>> {
        self.signal_operator
            .set_trade_executor(trade_executor)
            .map_err(BacktestError::SetTradeExecutor)?;

        let evaluators = self
            .evaluators
            .into_iter()
            .map(|ev| (start_time, ev))
            .collect();

        Ok(SignalOperatorRunning {
            evaluators,
            signal_operator: self.signal_operator,
        })
    }
}

/// Signal operator in running state for a specific signal type.
pub(super) struct SignalOperatorRunning<S: Signal> {
    evaluators: Vec<(DateTime<Utc>, WrappedSignalEvaluator<S>)>,
    signal_operator: WrappedSignalOperator<S>,
}

impl<S: Signal> SignalOperatorRunning<S> {
    pub(super) async fn iterate(
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
                        .expect("not `None` when evaluator has lookback");
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

/// Raw operator in pending state.
pub(super) struct RawOperatorPending {
    raw_operator: WrappedRawOperator,
    resolution_to_max_period: HashMap<OhlcResolution, Period>,
    max_lookback: Option<Lookback>,
}

impl RawOperatorPending {
    pub(super) fn new(raw_operator: WrappedRawOperator) -> Result<Self> {
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

        Ok(Self {
            raw_operator,
            resolution_to_max_period,
            max_lookback,
        })
    }

    pub(super) fn resolution_to_max_period(&self) -> &HashMap<OhlcResolution, Period> {
        &self.resolution_to_max_period
    }

    pub(super) fn max_lookback(&self) -> Option<Lookback> {
        self.max_lookback
    }

    pub(super) fn start(
        mut self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<RawOperatorRunning> {
        self.raw_operator
            .set_trade_executor(trade_executor)
            .map_err(BacktestError::SetTradeExecutor)?;

        Ok(RawOperatorRunning {
            last_eval: start_time,
            raw_operator: self.raw_operator,
        })
    }
}

/// Raw operator in running state.
pub(super) struct RawOperatorRunning {
    last_eval: DateTime<Utc>,
    raw_operator: WrappedRawOperator,
}

impl RawOperatorRunning {
    pub(super) async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        consolidator: Option<&MultiResolutionConsolidator>,
    ) -> Result<()> {
        let min_iteration_interval = self
            .raw_operator
            .min_iteration_interval()
            .map_err(BacktestError::OperatorError)?
            .as_duration();

        if time_cursor >= self.last_eval + min_iteration_interval {
            self.last_eval = time_cursor;

            let lookback = self
                .raw_operator
                .lookback()
                .map_err(BacktestError::OperatorError)?;

            let ctx_candles = match lookback {
                Some(lb) => consolidator
                    .and_then(|c| c.get_candles(lb.resolution()))
                    .expect("not `None` when evaluator has lookback"),
                None => &[],
            };

            self.raw_operator
                .iterate(ctx_candles)
                .await
                .map_err(BacktestError::OperatorError)?;
        }

        Ok(())
    }
}
