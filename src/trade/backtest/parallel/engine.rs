use std::{collections::HashMap, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;

use crate::{
    db::Database,
    shared::{Lookback, OhlcResolution, Period},
    signal::{Signal, SignalEvaluator},
    sync::PriceHistoryState,
    util::DateTimeExt,
};

use super::{
    super::{
        super::{RawOperator, SignalOperator, TradeExecutor},
        config::BacktestConfig,
        consolidator::MultiResolutionConsolidator,
        error::{BacktestError, Result},
        executor::SimulatedTradeExecutor,
        state::{
            BacktestParallelReceiver, BacktestParallelTransmitter, BacktestParallelUpdate,
            BacktestStatus, BacktestStatusManager,
        },
    },
    controller::BacktestParallelController,
    operator::ParallelOperatorPending,
};

/// Builder for configuring and executing a parallel backtest simulation.
///
/// This engine runs multiple trading operators in parallel over the same time period with shared
/// candle loading and consolidation, while maintaining isolated trade execution state per operator.
/// This is useful for comparing different strategies over the same historical data.
pub struct BacktestParallelEngine {
    config: BacktestConfig,
    db: Arc<Database>,
    operators: Vec<(String, ParallelOperatorPending)>,
    shared_resolution_map: HashMap<OhlcResolution, Period>,
    max_lookback: Option<Lookback>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    start_balance: u64,
    status_manager: Arc<BacktestStatusManager<BacktestParallelUpdate>>,
    update_tx: BacktestParallelTransmitter,
}

impl BacktestParallelEngine {
    /// Creates a new parallel backtest engine.
    ///
    /// The engine is initially empty and operators must be added using [`add_raw_operator`] or
    /// [`add_signal_operator`].
    pub async fn new(
        config: BacktestConfig,
        db: Arc<Database>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        start_balance: u64,
    ) -> Result<Self> {
        if !start_time.is_round_minute() || !end_time.is_round_minute() {
            return Err(BacktestError::InvalidTimeRangeNotRounded {
                start_time,
                end_time,
            });
        }

        if end_time <= start_time {
            return Err(BacktestError::InvalidTimeRangeSequence {
                start_time,
                end_time,
            });
        }

        let min_duration = Duration::days(1);
        if end_time - start_time < min_duration {
            let duration_hours = (end_time - start_time).num_hours();
            return Err(BacktestError::InvalidTimeRangeTooShort {
                min_duration,
                duration_hours,
            });
        }

        let (update_tx, _) = broadcast::channel::<BacktestParallelUpdate>(10_000);
        let status_manager = BacktestStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            db,
            operators: Vec::new(),
            shared_resolution_map: HashMap::new(),
            max_lookback: None,
            start_time,
            end_time,
            start_balance,
            status_manager,
            update_tx,
        })
    }

    /// Adds a raw operator to the backtest engine.
    ///
    /// Returns an error if the name is empty or if an operator with the same name already exists.
    pub fn add_raw_operator(
        mut self,
        name: impl Into<String>,
        operator: Box<dyn RawOperator>,
    ) -> Result<Self> {
        let name = name.into();
        self.validate_name(&name)?;

        let pending = ParallelOperatorPending::raw(operator)?;
        self.merge_resolution_map(pending.resolution_to_max_period());
        self.update_max_lookback(pending.max_lookback());

        self.operators.push((name, pending));
        Ok(self)
    }

    /// Adds a signal operator to the backtest engine.
    ///
    /// Returns an error if the name is empty or if an operator with the same name already exists.
    pub fn add_signal_operator<S: Signal>(
        mut self,
        name: impl Into<String>,
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        operator: Box<dyn SignalOperator<S>>,
    ) -> Result<Self> {
        let name = name.into();
        self.validate_name(&name)?;

        let pending = ParallelOperatorPending::signal(evaluators, operator)?;
        self.merge_resolution_map(pending.resolution_to_max_period());
        self.update_max_lookback(pending.max_lookback());

        self.operators.push((name, pending));
        Ok(self)
    }

    fn validate_name(&self, name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(BacktestError::ParallelEmptyOperatorName);
        }

        if self.operators.iter().any(|(n, _)| n == name) {
            return Err(BacktestError::ParallelDuplicateOperatorName {
                name: name.to_string(),
            });
        }

        Ok(())
    }

    fn merge_resolution_map(&mut self, operator_map: &HashMap<OhlcResolution, Period>) {
        for (resolution, period) in operator_map {
            self.shared_resolution_map
                .entry(*resolution)
                .and_modify(|existing| {
                    if *period > *existing {
                        *existing = *period;
                    }
                })
                .or_insert(*period);
        }
    }

    fn update_max_lookback(&mut self, lookback: Option<Lookback>) {
        if let Some(lb) = lookback
            && self
                .max_lookback
                .is_none_or(|existing| existing.as_duration() < lb.as_duration())
        {
            self.max_lookback = Some(lb);
        }
    }

    /// Returns the start time of the backtest simulation period.
    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    /// Returns the starting balance (in satoshis) for each operator in the backtest simulation.
    pub fn start_balance(&self) -> u64 {
        self.start_balance
    }

    /// Returns the end time of the backtest simulation period.
    pub fn end_time(&self) -> DateTime<Utc> {
        self.end_time
    }

    /// Creates a new receiver for subscribing to backtest status and trading state updates.
    pub fn receiver(&self) -> BacktestParallelReceiver {
        self.status_manager.receiver()
    }

    async fn run(self) -> Result<()> {
        if self.operators.is_empty() {
            return Err(BacktestError::ParallelNoOperators);
        }

        self.status_manager.update(BacktestStatus::Starting);

        let buffer_size = self.config.buffer_size() as i64;

        let max_lookback_duration = self
            .max_lookback
            .map(|lb| lb.as_duration())
            .unwrap_or(Duration::zero());

        // Validate price history is available
        let price_history_state = PriceHistoryState::evaluate(&self.db)
            .await
            .map_err(BacktestError::PriceHistoryStateEvaluation)?;

        let lookback_time = if let Some(lookback) = self.max_lookback {
            self.start_time
                .step_back_candles(lookback.resolution(), lookback.period().as_u64() - 1)
        } else {
            self.start_time
        };

        if !price_history_state
            .is_range_available(lookback_time, self.end_time)
            .map_err(BacktestError::PriceHistoryStateEvaluation)?
        {
            return Err(BacktestError::PriceHistoryUnavailable {
                lookback_time,
                end_time: self.end_time,
                history_start: price_history_state.bound_start(),
                history_end: price_history_state.bound_end(),
            });
        }

        let buffer_from = self.start_time - max_lookback_duration;
        let buffer_to = buffer_from + Duration::minutes(buffer_size);
        let mut minute_buffer = self
            .db
            .ohlc_candles
            .get_candles(buffer_from, buffer_to)
            .await?;

        // Find the index of the start_time minute candle, or the next available candle
        let start_candle_idx = minute_buffer
            .iter()
            .position(|c| c.time >= self.start_time)
            .ok_or(BacktestError::UnexpectedEmptyBuffer {
                time: self.start_time,
            })?;

        let start_candle = &minute_buffer[start_candle_idx];

        // Create per-operator trade executors
        let mut executors: Vec<(String, Arc<SimulatedTradeExecutor>)> = Vec::new();
        for (name, _) in &self.operators {
            let executor =
                SimulatedTradeExecutor::new(&self.config, start_candle, self.start_balance);
            executors.push((name.clone(), executor));
        }

        // Start all operators
        let mut running_operators: Vec<(
            String,
            super::operator::ParallelOperatorRunning,
            Arc<SimulatedTradeExecutor>,
        )> = Vec::new();

        for ((name, pending), (_, executor)) in self.operators.into_iter().zip(executors.iter()) {
            let running = pending
                .start(self.start_time, executor.clone())
                .map_err(|e| BacktestError::ParallelOperatorFailed {
                    operator_name: name.clone(),
                    source: Box::new(e),
                })?;
            running_operators.push((name, running, executor.clone()));
        }

        let mut time_cursor = start_candle.time + Duration::seconds(59);
        let mut minute_cursor_idx = start_candle_idx;

        let mut consolidator = if !self.shared_resolution_map.is_empty() {
            let initial_candles = &minute_buffer[..=start_candle_idx];
            Some(MultiResolutionConsolidator::new(
                self.shared_resolution_map,
                initial_candles,
                time_cursor,
            )?)
        } else {
            None
        };

        // Send initial trading state for all operators
        for (name, _, executor) in &running_operators {
            let initial_state = executor
                .trading_state()
                .await
                .map_err(BacktestError::ExecutorStateEvaluation)?;
            let _ = self.update_tx.send(BacktestParallelUpdate::TradingState {
                operator_name: name.clone(),
                state: Box::new(initial_state),
            });
        }

        // Next update will be at end of day (23:59:59), reported as midnight of following day
        let mut send_next_update_at = self.start_time + Duration::days(1) - Duration::seconds(1);

        self.status_manager.update(BacktestStatus::Running);

        loop {
            // Iterate all operators
            for (name, operator, _) in &mut running_operators {
                operator
                    .iterate(time_cursor, consolidator.as_ref())
                    .await
                    .map_err(|e| BacktestError::ParallelOperatorFailed {
                        operator_name: name.clone(),
                        source: Box::new(e),
                    })?;
            }

            if time_cursor >= send_next_update_at {
                // Report trading state as midnight UTC of each backtested day
                let update_time = send_next_update_at + Duration::seconds(1);

                for (name, _, executor) in &running_operators {
                    executor
                        .update_time(update_time)
                        .await
                        .map_err(BacktestError::ExecutorTickUpdate)?;

                    let trades_state = executor
                        .trading_state()
                        .await
                        .map_err(BacktestError::ExecutorStateEvaluation)?;

                    // Ignore no-receivers errors
                    let _ = self.update_tx.send(BacktestParallelUpdate::TradingState {
                        operator_name: name.clone(),
                        state: Box::new(trades_state),
                    });
                }

                send_next_update_at += Duration::days(1);
            }

            if time_cursor >= self.end_time - Duration::seconds(1) {
                break;
            }

            minute_cursor_idx += 1;

            // Refetch buffer when exhausted
            if minute_cursor_idx >= minute_buffer.len() {
                let new_buffer_to =
                    (time_cursor + Duration::minutes(buffer_size)).min(self.end_time);

                minute_buffer = self
                    .db
                    .ohlc_candles
                    .get_candles(time_cursor, new_buffer_to)
                    .await?;

                if minute_buffer.is_empty() {
                    return Err(BacktestError::UnexpectedEmptyBuffer { time: time_cursor });
                }

                minute_cursor_idx = 0;
            }

            // Advance time cursor to the end of the next candle's minute (skips gaps in data)
            time_cursor = minute_buffer[minute_cursor_idx].time + Duration::seconds(59);

            let next_minute_candle = &minute_buffer[minute_cursor_idx];

            // Update all executors with the new candle
            for (_, _, executor) in &running_operators {
                executor
                    .candle_update(next_minute_candle)
                    .await
                    .map_err(BacktestError::ExecutorTickUpdate)?;
            }

            if let Some(consolidator) = &mut consolidator {
                consolidator.push(next_minute_candle)?;
            }
        }

        Ok(())
    }

    /// Starts the backtest simulation and returns a [`BacktestParallelController`] for managing it.
    ///
    /// This consumes the engine and spawns the backtest task in the background.
    pub fn start(self) -> Arc<BacktestParallelController> {
        let status_manager = self.status_manager.clone();

        let handle = tokio::spawn(async move {
            let status_manager = self.status_manager.clone();

            let final_backtest_state = match self.run().await {
                Ok(_) => BacktestStatus::Finished,
                Err(e) => BacktestStatus::Failed(Arc::new(e)),
            };

            status_manager.update(final_backtest_state);
        })
        .into();

        BacktestParallelController::new(handle, status_manager)
    }
}
