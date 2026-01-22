use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    db::{Database, models::OhlcCandleRow},
    shared::Lookback,
    signal::{Signal, SignalEvaluator, WrappedSignalEvaluator},
    sync::PriceHistoryState,
    trade::backtest::config::BacktestConfig,
    tui::{TuiControllerShutdown, error::Result as TuiResult},
    util::{AbortOnDropHandle, DateTimeExt},
};

use super::{
    super::core::{
        Raw, RawOperator, SignalOperator, TradeExecutor, TradingState, WrappedRawOperator,
        WrappedSignalOperator,
    },
    consolidator::RuntimeConsolidator,
    error::{BacktestError, Result},
    executor::SimulatedTradeExecutor,
    state::{
        BacktestReceiver, BacktestStatus, BacktestStatusManager, BacktestTransmiter, BacktestUpdate,
    },
};

/// Controller for managing and monitoring a running backtest simulation process.
///
/// Provides an interface to monitor backtest status, receive updates, and control the simulation
/// lifecycle including waiting for completion or aborting the process.
#[derive(Debug)]
pub struct BacktestController {
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    status_manager: Arc<BacktestStatusManager>,
}

impl BacktestController {
    fn new(handle: AbortOnDropHandle<()>, status_manager: Arc<BacktestStatusManager>) -> Arc<Self> {
        Arc::new(Self {
            handle: Mutex::new(Some(handle)),
            status_manager,
        })
    }

    /// Creates a new [`BacktestReceiver`] for subscribing to backtest status and trading state
    /// updates.
    pub fn receiver(&self) -> BacktestReceiver {
        self.status_manager.receiver()
    }

    /// Returns the current [`BacktestStatus`] as a snapshot.
    pub fn status_snapshot(&self) -> BacktestStatus {
        self.status_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`BacktestController` mutex can't be poisoned")
            .take()
    }

    /// Waits until the backtest has stopped and returns the final status.
    ///
    /// This method blocks until the backtest reaches a stopped state (finished, failed, or
    /// aborted).
    pub async fn until_stopped(&self) -> BacktestStatus {
        let mut backtest_rx = self.receiver();

        let status = self.status_snapshot();
        if status.is_stopped() {
            return status;
        }

        loop {
            match backtest_rx.recv().await {
                Ok(backtest_update) => {
                    if let BacktestUpdate::Status(status) = backtest_update
                        && status.is_stopped()
                    {
                        return status;
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let status = self.status_snapshot();
                    if status.is_stopped() {
                        return status;
                    }
                }
                Err(RecvError::Closed) => return self.status_snapshot(),
            }
        }
    }

    /// Consumes the task handle and aborts the backtest. This method can only be called once per
    /// controller instance.
    pub async fn abort(&self) -> Result<()> {
        if let Some(handle) = self.try_consume_handle() {
            if !handle.is_finished() {
                handle.abort();
                self.status_manager.update(BacktestStatus::Aborted);
            }

            return handle.await.map_err(BacktestError::TaskJoin);
        }

        Err(BacktestError::ProcessAlreadyConsumed)
    }
}

#[async_trait]
impl TuiControllerShutdown for BacktestController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        // A `TaskJoin` error is expected here and can be safely ignored.
        let _ = self.abort().await;
        Ok(())
    }
}

/// Pending operator state before starting.
enum OperatorPending<S: Signal> {
    Signal {
        evaluators: Vec<WrappedSignalEvaluator<S>>,
        signal_operator: WrappedSignalOperator<S>,
    },
    Raw {
        raw_operator: WrappedRawOperator,
    },
}

impl<S: Signal> OperatorPending<S> {
    fn signal(
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: WrappedSignalOperator<S>,
    ) -> Self {
        Self::Signal {
            evaluators: evaluators
                .into_iter()
                .map(WrappedSignalEvaluator::new)
                .collect(),
            signal_operator,
        }
    }

    fn raw(raw_operator: WrappedRawOperator) -> Self {
        Self::Raw { raw_operator }
    }

    fn max_lookback(&self) -> Result<Option<Lookback>> {
        match self {
            OperatorPending::Signal { evaluators, .. } => {
                let lookbacks: Vec<_> = evaluators
                    .iter()
                    .filter_map(|e| {
                        e.lookback()
                            .map_err(BacktestError::SignalEvaluationError)
                            .transpose()
                    })
                    .collect::<Result<_>>()?;

                Ok(lookbacks.into_iter().max_by_key(|l| l.as_duration()))
            }
            OperatorPending::Raw { raw_operator } => raw_operator
                .lookback()
                .map_err(BacktestError::OperatorError),
        }
    }

    fn start(
        self,
        start_time: DateTime<Utc>,
        trade_executor: Arc<dyn TradeExecutor>,
    ) -> Result<OperatorRunning<S>> {
        match self {
            OperatorPending::Signal {
                evaluators,
                mut signal_operator,
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
            OperatorPending::Raw { mut raw_operator } => {
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
enum OperatorRunning<S: Signal> {
    Signal {
        evaluators: Vec<(DateTime<Utc>, WrappedSignalEvaluator<S>)>,
        signal_operator: WrappedSignalOperator<S>,
    },
    Raw {
        last_eval: DateTime<Utc>,
        raw_operator: WrappedRawOperator,
    },
}

impl<S: Signal> OperatorRunning<S> {
    async fn iterate(
        &mut self,
        time_cursor: DateTime<Utc>,
        ctx_candles: &[OhlcCandleRow],
    ) -> Result<()> {
        match self {
            OperatorRunning::Signal {
                evaluators,
                signal_operator,
            } => Self::iterate_signal(evaluators, signal_operator, time_cursor, ctx_candles).await,
            OperatorRunning::Raw {
                last_eval,
                raw_operator,
            } => Self::iterate_raw(last_eval, raw_operator, time_cursor, ctx_candles).await,
        }
    }

    async fn iterate_signal(
        evaluators: &mut [(DateTime<Utc>, WrappedSignalEvaluator<S>)],
        signal_operator: &WrappedSignalOperator<S>,
        time_cursor: DateTime<Utc>,
        ctx_candles: &[OhlcCandleRow],
    ) -> Result<()> {
        for (last_eval, evaluator) in evaluators {
            let min_interval = evaluator
                .min_iteration_interval()
                .map_err(BacktestError::SignalEvaluationError)?;
            if time_cursor < *last_eval + min_interval.as_duration() {
                continue;
            }

            *last_eval = time_cursor;

            let eval_candles = match evaluator
                .lookback()
                .map_err(BacktestError::SignalEvaluationError)?
            {
                Some(lookback) => {
                    let start_idx = ctx_candles
                        .len()
                        .saturating_sub(lookback.period().as_usize());
                    &ctx_candles[start_idx..]
                }
                None => &[],
            };

            let signal = evaluator
                .evaluate(eval_candles)
                .await
                .map_err(BacktestError::SignalEvaluationError)?;

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
        ctx_candles: &[OhlcCandleRow],
    ) -> Result<()> {
        let min_iteration_interval = raw_operator
            .min_iteration_interval()
            .map_err(BacktestError::OperatorError)?
            .as_duration();

        if time_cursor >= *last_eval + min_iteration_interval {
            *last_eval = time_cursor;

            raw_operator
                .iterate(ctx_candles)
                .await
                .map_err(BacktestError::OperatorError)?;
        }

        Ok(())
    }
}

/// Builder for configuring and executing a backtest simulation. Encapsulates the configuration,
/// database connection, operator, and time range for the backtest. The simulation is started when
/// [`start`](Self::start) is called, returning a [`BacktestController`] for monitoring progress.
pub struct BacktestEngine<S: Signal> {
    config: BacktestConfig,
    db: Arc<Database>,
    operator_pending: OperatorPending<S>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    end_time: DateTime<Utc>,
    status_manager: Arc<BacktestStatusManager>,
    update_tx: BacktestTransmiter,
}

impl<S: Signal> BacktestEngine<S> {
    /// Creates a new backtest engine using signal-based evaluation. Signal evaluators generate
    /// trading signals that are processed by the signal operator to execute trading actions.
    ///
    /// The generic parameter `S` ensures type safety between evaluators and operator. They must
    /// produce and consume the same signal type.
    pub async fn with_signal_operator(
        config: BacktestConfig,
        db: Arc<Database>,
        evaluators: Vec<Box<dyn SignalEvaluator<S>>>,
        signal_operator: Box<dyn SignalOperator<S>>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator_pending = OperatorPending::signal(evaluators, signal_operator.into());

        Self::new(
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
        )
        .await
    }

    async fn new(
        config: BacktestConfig,
        db: Arc<Database>,
        operator_pending: OperatorPending<S>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
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

        let max_lookback = operator_pending.max_lookback()?;

        let price_history_state = PriceHistoryState::evaluate(&db)
            .await
            .map_err(BacktestError::PriceHistoryStateEvaluation)?;

        let lookback_time = if let Some(lookback) = max_lookback {
            start_time.step_back_candles(lookback.resolution(), lookback.period().as_u64() - 1)
        } else {
            start_time
        };

        if !price_history_state
            .is_range_available(lookback_time, end_time)
            .map_err(BacktestError::PriceHistoryStateEvaluation)?
        {
            return Err(BacktestError::PriceHistoryUnavailable {
                lookback_time,
                end_time,
                history_start: price_history_state.bound_start(),
                history_end: price_history_state.bound_end(),
            });
        }

        let (update_tx, _) = broadcast::channel::<BacktestUpdate>(10_000);

        let status_manager = BacktestStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
            status_manager,
            update_tx,
        })
    }

    /// Returns the start time of the backtest simulation period.
    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    /// Returns the starting balance (in satoshis) for the backtest simulation.
    pub fn start_balance(&self) -> u64 {
        self.start_balance
    }

    /// Returns the end time of the backtest simulation period.
    pub fn end_time(&self) -> DateTime<Utc> {
        self.end_time
    }

    /// Creates a new receiver for subscribing to backtest status and trading state updates.
    pub fn receiver(&self) -> BacktestReceiver {
        self.status_manager.receiver()
    }

    async fn run(self) -> Result<TradingState> {
        self.status_manager.update(BacktestStatus::Starting);

        let max_lookback = self.operator_pending.max_lookback()?;

        let buffer_size = self.config.buffer_size() as i64;

        let lookback_minutes = max_lookback
            .map(|l| l.period().as_u64() as i64 * l.resolution().as_minutes() as i64)
            .unwrap_or(0);

        let buffer_from = self.start_time - Duration::minutes(lookback_minutes);
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

        let trades_executor =
            SimulatedTradeExecutor::new(&self.config, start_candle, self.start_balance);

        let mut operator = self
            .operator_pending
            .start(self.start_time, trades_executor.clone())?;

        let mut time_cursor = start_candle.time + Duration::seconds(59);
        let mut minute_cursor_idx = start_candle_idx;

        // If lookback is set, create consolidator with lookback candles up to start position
        let mut consolidator = if let Some(lookback) = max_lookback {
            let initial_candles = &minute_buffer[..=start_candle_idx];
            Some(RuntimeConsolidator::new(
                lookback,
                initial_candles,
                time_cursor,
            )?)
        } else {
            None
        };

        // Send initial trading state at start_time (midnight UTC)
        let initial_state = trades_executor
            .trading_state()
            .await
            .map_err(BacktestError::ExecutorStateEvaluation)?;
        let _ = self.update_tx.send(initial_state.into());

        // Next update will be at end of day (23:59:59), reported as midnight of following day
        let mut send_next_update_at = time_cursor + Duration::days(1);

        self.status_manager.update(BacktestStatus::Running);

        loop {
            let ctx_candles = consolidator
                .as_ref()
                .map(|c| c.get_candles())
                .unwrap_or(&[]);

            operator.iterate(time_cursor, ctx_candles).await?;

            if time_cursor >= send_next_update_at {
                // Report trading state as midnight UTC of each backtested day
                let update_time = send_next_update_at + Duration::seconds(1);
                trades_executor
                    .update_time(update_time)
                    .await
                    .map_err(BacktestError::ExecutorTickUpdate)?;
                let trades_state = trades_executor
                    .trading_state()
                    .await
                    .map_err(BacktestError::ExecutorStateEvaluation)?;

                // Ignore no-receivers errors
                let _ = self.update_tx.send(trades_state.into());

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
            trades_executor
                .candle_update(next_minute_candle)
                .await
                .map_err(BacktestError::ExecutorTickUpdate)?;

            if let Some(consolidator) = &mut consolidator {
                consolidator.push(next_minute_candle)?;
            }
        }

        let final_state = trades_executor
            .trading_state()
            .await
            .map_err(BacktestError::ExecutorStateEvaluation)?;

        Ok(final_state)
    }

    /// Starts the backtest simulation and returns a [`BacktestController`] for managing it. This
    /// consumes the engine and spawns the backtest task in the background.
    pub fn start(self) -> Arc<BacktestController> {
        let status_manager = self.status_manager.clone();

        let handle = tokio::spawn(async move {
            let status_manager = self.status_manager.clone();
            let update_tx = self.update_tx.clone();

            let final_backtest_state = match self.run().await {
                Ok(final_trade_state) => {
                    // Ignore no-receivers errors
                    let _ = update_tx.send(final_trade_state.into());

                    BacktestStatus::Finished
                }
                Err(e) => BacktestStatus::Failed(Arc::new(e)),
            };

            status_manager.update(final_backtest_state);
        })
        .into();

        BacktestController::new(handle, status_manager)
    }
}

impl BacktestEngine<Raw> {
    /// Creates a new backtest engine using a raw operator. The raw operator directly implements
    /// trading logic without intermediate signal generation.
    pub async fn with_raw_operator(
        config: BacktestConfig,
        db: Arc<Database>,
        raw_operator: Box<dyn RawOperator>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator_pending = OperatorPending::raw(raw_operator.into());

        Self::new(
            config,
            db,
            operator_pending,
            start_time,
            start_balance,
            end_time,
        )
        .await
    }
}
