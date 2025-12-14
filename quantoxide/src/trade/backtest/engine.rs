use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast::{self, error::RecvError};

use crate::{
    db::Database,
    shared::LookbackPeriod,
    signal::{ConfiguredSignalEvaluator, Signal},
    sync::PriceHistoryState,
    trade::backtest::config::BacktestConfig,
    tui::{TuiControllerShutdown, error::Result as TuiResult},
    util::{AbortOnDropHandle, DateTimeExt},
};

use super::{
    super::core::{
        RawOperator, SignalOperator, TradeExecutor, TradingState, WrappedRawOperator,
        WrappedSignalOperator,
    },
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

enum Operator {
    Signal {
        evaluators: Vec<(DateTime<Utc>, ConfiguredSignalEvaluator)>,
        signal_operator: WrappedSignalOperator,
    },
    Raw {
        last_eval: DateTime<Utc>,
        raw_operator: WrappedRawOperator,
    },
}

impl Operator {
    fn signal(
        start_time: DateTime<Utc>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        signal_operator: WrappedSignalOperator,
    ) -> Self {
        Self::Signal {
            evaluators: evaluators
                .into_iter()
                .map(|evaluator| (start_time, evaluator))
                .collect(),
            signal_operator,
        }
    }

    fn raw(start_time: DateTime<Utc>, raw_operator: WrappedRawOperator) -> Self {
        Self::Raw {
            last_eval: start_time,
            raw_operator,
        }
    }

    fn set_executor(&mut self, trade_executor: Arc<dyn TradeExecutor>) -> Result<()> {
        match self {
            Operator::Signal {
                evaluators: _,
                signal_operator: operator,
            } => {
                operator
                    .set_trade_executor(trade_executor)
                    .map_err(BacktestError::SetTradeExecutor)?;

                Ok(())
            }
            Operator::Raw {
                last_eval: _,
                raw_operator,
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(BacktestError::SetTradeExecutor)?;

                Ok(())
            }
        }
    }

    fn max_lookback(&self) -> Result<Option<LookbackPeriod>> {
        match self {
            Operator::Signal {
                evaluators,
                signal_operator: _,
            } => {
                let max_lookback = evaluators
                    .iter()
                    .map(|(_, evaluator)| evaluator.lookback())
                    .max()
                    .expect("`evaluators` must not be empty");

                Ok(max_lookback)
            }
            Operator::Raw {
                last_eval: _,
                raw_operator,
            } => raw_operator
                .lookback()
                .map_err(BacktestError::OperatorError),
        }
    }
}

/// Builder for configuring and executing a backtest simulation. Encapsulates the configuration,
/// database connection, operator, and time range for the backtest. The simulation is started when
/// [`start`](Self::start) is called, returning a [`BacktestController`] for monitoring progress.
pub struct BacktestEngine {
    config: BacktestConfig,
    db: Arc<Database>,
    operator: Operator,
    start_time: DateTime<Utc>,
    start_balance: u64,
    end_time: DateTime<Utc>,
    status_manager: Arc<BacktestStatusManager>,
    update_tx: BacktestTransmiter,
}

impl BacktestEngine {
    async fn new(
        config: BacktestConfig,
        db: Arc<Database>,
        operator: Operator,
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

        let max_lookback = operator.max_lookback()?;

        if max_lookback.is_some_and(|max| max.as_usize() > config.buffer_size()) {
            return Err(BacktestError::IncompatibleBufferSize {
                buffer_size: config.buffer_size(),
                max_lookback: max_lookback.expect("not `None`"),
            });
        }

        let price_history_state = PriceHistoryState::evaluate(&db)
            .await
            .map_err(BacktestError::PriceHistoryStateEvaluation)?;

        let lookback_time = if let Some(lookback) = max_lookback {
            start_time
                .checked_sub_signed(Duration::minutes(lookback.as_i64() - 1))
                .ok_or(BacktestError::DateRangeBufferOutOfRange)?
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
            });
        }

        let (update_tx, _) = broadcast::channel::<BacktestUpdate>(10_000);

        let status_manager = BacktestStatusManager::new(update_tx.clone());

        Ok(Self {
            config,
            db,
            operator,
            start_time,
            start_balance,
            end_time,
            status_manager,
            update_tx,
        })
    }

    /// Creates a new backtest engine using signal-based evaluation. Signal evaluators generate
    /// trading signals that are processed by the signal operator to execute trading actions.
    pub async fn with_signal_operator(
        config: BacktestConfig,
        db: Arc<Database>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        signal_operator: Box<dyn SignalOperator>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator = Operator::signal(start_time, evaluators, signal_operator.into());

        Self::new(config, db, operator, start_time, start_balance, end_time).await
    }

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
        let operator = Operator::raw(start_time, raw_operator.into());

        Self::new(config, db, operator, start_time, start_balance, end_time).await
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

        let mut operator = self.operator;

        let max_lookback = operator.max_lookback()?;

        let buffer_size = self.config.buffer_size();

        let get_buffers = |start_minute: DateTime<Utc>| {
            let db = &self.db;
            async move {
                // From `::new`, `max_lookback` must be lte `buffer_size`

                let from = start_minute
                    .checked_sub_signed(Duration::minutes(
                        max_lookback.map_or(0, |l| l.as_i64() - 1),
                    ))
                    .ok_or(BacktestError::DateRangeBufferOutOfRange)?;

                let to = from
                    .checked_add_signed(Duration::minutes(buffer_size as i64))
                    .ok_or(BacktestError::DateRangeBufferOutOfRange)?;

                let candle_buffer = db.ohlc_candles.get_candles(from, to).await?;

                // Some candles may have been skipped

                let mut buffer_cursor_idx = 0;
                while candle_buffer[buffer_cursor_idx].time < start_minute {
                    buffer_cursor_idx += 1;
                }

                Ok::<_, BacktestError>((candle_buffer, buffer_cursor_idx))
            }
        };

        let (mut candle_buffer, mut buffer_cursor_idx) = get_buffers(self.start_time).await?;

        let start_candle = &candle_buffer[buffer_cursor_idx];

        let trades_executor = Arc::new(SimulatedTradeExecutor::new(
            &self.config,
            start_candle,
            self.start_balance,
        ));

        operator.set_executor(trades_executor.clone())?;

        let mut time_cursor = start_candle.time + Duration::seconds(59);

        let mut send_next_update_at = time_cursor + self.config.update_interval();

        self.status_manager.update(BacktestStatus::Running);

        loop {
            match &mut operator {
                Operator::Signal {
                    evaluators,
                    signal_operator,
                } => {
                    for (last_eval, evaluator) in evaluators {
                        if time_cursor
                            < *last_eval + evaluator.min_iteration_interval().as_duration()
                        {
                            continue;
                        }

                        *last_eval = time_cursor;

                        let ctx_candles = if let Some(lookback) = evaluator.lookback() {
                            let lookback = lookback.as_usize();
                            &candle_buffer[buffer_cursor_idx + 1 - lookback..=buffer_cursor_idx]
                        } else {
                            &[]
                        };

                        let signal = Signal::try_evaluate(evaluator, time_cursor, ctx_candles)
                            .await
                            .map_err(BacktestError::SignalEvaluationError)?;

                        signal_operator
                            .process_signal(&signal)
                            .await
                            .map_err(BacktestError::SignalProcessingError)?;
                    }
                }
                Operator::Raw {
                    last_eval,
                    raw_operator,
                } => {
                    let min_iteration_interval = raw_operator
                        .min_iteration_interval()
                        .map_err(BacktestError::OperatorError)?
                        .as_duration();

                    if time_cursor >= *last_eval + min_iteration_interval {
                        *last_eval = time_cursor;

                        let lookback_opt = raw_operator
                            .lookback()
                            .map_err(BacktestError::OperatorError)?;

                        let ctx_candles = if let Some(lookback) = lookback_opt {
                            let lookback = lookback.as_usize();
                            &candle_buffer[buffer_cursor_idx + 1 - lookback..=buffer_cursor_idx]
                        } else {
                            &[]
                        };

                        raw_operator
                            .iterate(ctx_candles)
                            .await
                            .map_err(BacktestError::OperatorError)?;
                    }
                }
            }

            if time_cursor >= send_next_update_at {
                let trades_state = trades_executor
                    .trading_state()
                    .await
                    .map_err(BacktestError::ExecutorStateEvaluation)?;

                // Ignore no-receivers errors
                let _ = self.update_tx.send(trades_state.into());

                send_next_update_at += self.config.update_interval();
            }

            if time_cursor + Duration::minutes(1) >= self.end_time {
                break;
            }

            if buffer_cursor_idx < candle_buffer.len() - 1 {
                buffer_cursor_idx += 1;
            } else {
                (candle_buffer, buffer_cursor_idx) = get_buffers(time_cursor.next_minute()).await?;
            }

            let next_candle = &candle_buffer[buffer_cursor_idx];

            time_cursor = next_candle.time + Duration::seconds(59);

            if time_cursor >= self.end_time {
                break;
            }

            trades_executor
                .candle_update(next_candle)
                .await
                .map_err(BacktestError::ExecutorTickUpdate)?;
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
