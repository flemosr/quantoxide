use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;

use crate::{
    db::DbContext,
    signal::core::{ConfiguredSignalEvaluator, Signal},
    sync::PriceHistoryState,
    trade::backtest::config::BacktestConfig,
    tui::{Result as TuiResult, TuiControllerShutdown},
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

    pub fn receiver(&self) -> BacktestReceiver {
        self.status_manager.receiver()
    }

    pub fn status_snapshot(&self) -> Arc<BacktestStatus> {
        self.status_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`BacktestController` mutex can't be poisoned")
            .take()
    }

    /// Consumes the task handle and waits for the backtest to complete.
    /// This method can only be called once per controller instance.
    /// Returns an error if the internal task was not properly handled.
    pub async fn wait_for_completion(&self) -> Result<()> {
        if let Some(handle) = self.try_consume_handle() {
            return handle.await.map_err(BacktestError::TaskJoin);
        }

        return Err(BacktestError::ProcessAlreadyConsumed);
    }

    /// Consumes the task handle and aborts the backtest.
    /// This method can only be called once per controller instance.
    /// Returns an error if the internal task was not properly handled.
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

    fn max_ctx_window_secs(&self) -> Result<usize> {
        match self {
            Operator::Signal {
                evaluators,
                signal_operator: _,
            } => {
                let max_ctx_window = evaluators
                    .iter()
                    .map(|(_, evaluator)| evaluator.context_window_secs())
                    .max()
                    .expect("evaluators can't be empty");

                Ok(max_ctx_window)
            }
            Operator::Raw {
                last_eval: _,
                raw_operator,
            } => {
                let ctx_window = raw_operator
                    .context_window_secs()
                    .map_err(BacktestError::OperatorError)?;

                Ok(ctx_window)
            }
        }
    }
}

pub struct BacktestEngine {
    config: BacktestConfig,
    db: Arc<DbContext>,
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
        db: Arc<DbContext>,
        operator: Operator,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        if !start_time.is_round() || !end_time.is_round() {
            return Err(BacktestError::InvalidTimeRangeNotRounded);
        }

        if end_time - start_time < Duration::days(1) {
            let duration_hours = (end_time - start_time).num_hours();
            return Err(BacktestError::InvalidTimeRangeTooShort { duration_hours });
        }

        let max_ctx_window = operator.max_ctx_window_secs()?;

        if config.buffer_size() < max_ctx_window {
            return Err(BacktestError::IncompatibleBufferSize {
                buffer_size: config.buffer_size(),
                max_ctx_window,
            });
        }

        let price_history_state = PriceHistoryState::evaluate(&db)
            .await
            .map_err(BacktestError::PriceHistoryStateEvaluation)?;

        if !price_history_state
            .is_range_available(start_time, end_time)
            .map_err(BacktestError::PriceHistoryStateEvaluation)?
        {
            return Err(BacktestError::PriceHistoryUnavailable {
                start_time,
                end_time,
            });
        }

        let (update_tx, _) = broadcast::channel::<BacktestUpdate>(100);

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

    pub async fn with_signal_operator(
        config: BacktestConfig,
        db: Arc<DbContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        signal_operator: Box<dyn SignalOperator>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator = Operator::signal(start_time, evaluators, signal_operator.into());

        Self::new(config, db, operator, start_time, start_balance, end_time).await
    }

    pub async fn with_raw_operator(
        config: BacktestConfig,
        db: Arc<DbContext>,
        raw_operator: Box<dyn RawOperator>,
        start_time: DateTime<Utc>,
        start_balance: u64,
        end_time: DateTime<Utc>,
    ) -> Result<Self> {
        let operator = Operator::raw(start_time, raw_operator.into());

        Self::new(config, db, operator, start_time, start_balance, end_time).await
    }

    pub fn start_time(&self) -> DateTime<Utc> {
        self.start_time
    }

    pub fn start_balance(&self) -> u64 {
        self.start_balance
    }

    pub fn end_time(&self) -> DateTime<Utc> {
        self.end_time
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.status_manager.receiver()
    }

    async fn run(self) -> Result<TradingState> {
        self.status_manager.update(BacktestStatus::Starting);

        let trades_executor = {
            let start_time_entry = self
                .db
                .price_history
                .get_latest_entry_at_or_before(self.start_time)
                .await?
                .ok_or(BacktestError::DatabaseNoEntriesBeforeStartTime)?;

            Arc::new(SimulatedTradeExecutor::new(
                &self.config,
                self.start_time,
                start_time_entry.value,
                self.start_balance,
            ))
        };

        let mut operator = self.operator;

        operator.set_executor(trades_executor.clone())?;

        let max_ctx_window = operator.max_ctx_window_secs()?;

        let buffer_size = self.config.buffer_size();

        let get_buffers = |time_cursor: DateTime<Utc>| {
            let db = &self.db;
            async move {
                let buffer_end = time_cursor
                    .checked_add_signed(Duration::seconds(
                        buffer_size as i64 - max_ctx_window as i64,
                    ))
                    .ok_or(BacktestError::DateRangeBufferOutOfRange)?;

                let locf_buffer = if max_ctx_window == 0 {
                    Vec::new()
                } else {
                    db.price_ticks
                        .compute_locf_entries_for_range(buffer_end, buffer_size)
                        .await?
                };

                let price_ticks = db
                    .price_history
                    .get_entries_between(time_cursor, buffer_end + Duration::seconds(1))
                    .await?;

                let locf_buffer_cursor_idx = max_ctx_window.checked_sub(1).unwrap_or(0);

                Ok::<_, BacktestError>((
                    buffer_end,
                    locf_buffer,
                    locf_buffer_cursor_idx,
                    price_ticks,
                    0,
                ))
            }
        };

        let mut time_cursor = self.start_time;

        let (
            mut buffer_end,
            mut locf_buffer,
            mut locf_buffer_cursor_idx,
            mut price_ticks,
            mut price_ticks_cursor_idx,
        ) = get_buffers(time_cursor).await?;

        let mut send_next_update_at = self.start_time + self.config.update_interval();

        self.status_manager.update(BacktestStatus::Running);

        loop {
            match &mut operator {
                Operator::Signal {
                    evaluators,
                    signal_operator,
                } => {
                    for (last_eval, evaluator) in evaluators {
                        if time_cursor < *last_eval + evaluator.evaluation_interval() {
                            continue;
                        }

                        *last_eval = time_cursor;

                        let ctx_window_size = evaluator.context_window_secs();

                        let ctx_entries = if ctx_window_size == 0 {
                            &[]
                        } else {
                            &locf_buffer[locf_buffer_cursor_idx + 1 - ctx_window_size
                                ..=locf_buffer_cursor_idx]
                        };

                        let signal = Signal::try_evaluate(evaluator, time_cursor, ctx_entries)
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
                    let iteration_interval = raw_operator
                        .iteration_interval()
                        .map_err(BacktestError::OperatorError)?;

                    if time_cursor >= *last_eval + iteration_interval {
                        *last_eval = time_cursor;

                        let ctx_window_size = raw_operator
                            .context_window_secs()
                            .map_err(BacktestError::OperatorError)?;

                        let ctx_entries = if ctx_window_size == 0 {
                            &[]
                        } else {
                            &locf_buffer[locf_buffer_cursor_idx + 1 - ctx_window_size
                                ..=locf_buffer_cursor_idx]
                        };

                        raw_operator
                            .iterate(ctx_entries)
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

            time_cursor += Duration::seconds(1);

            if time_cursor >= self.end_time {
                break;
            }

            // Update `SimulatedTradeExecutor` with all the price ticks with time lte the new
            // `time_cursor`.
            while let Some(next_price_tick) = price_ticks.get(price_ticks_cursor_idx) {
                if next_price_tick.time > time_cursor {
                    break;
                }

                trades_executor
                    .tick_update(next_price_tick.time, next_price_tick.value)
                    .await
                    .map_err(BacktestError::ExecutorTickUpdate)?;

                price_ticks_cursor_idx += 1;
            }

            if time_cursor <= buffer_end {
                locf_buffer_cursor_idx += 1;
            } else {
                (
                    buffer_end,
                    locf_buffer,
                    locf_buffer_cursor_idx,
                    price_ticks,
                    price_ticks_cursor_idx,
                ) = get_buffers(time_cursor).await?;
            }

            trades_executor
                .time_update(time_cursor)
                .await
                .map_err(BacktestError::ExecutorTimeUpdate)?;
        }

        let final_state = trades_executor
            .trading_state()
            .await
            .map_err(BacktestError::ExecutorStateEvaluation)?;

        Ok(final_state)
    }

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
                Err(e) => BacktestStatus::Failed(e),
            };

            status_manager.update(final_backtest_state);
        })
        .into();

        BacktestController::new(handle, status_manager)
    }
}
