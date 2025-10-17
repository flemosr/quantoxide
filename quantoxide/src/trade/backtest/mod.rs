use std::{
    fmt,
    sync::{Arc, Mutex, MutexGuard},
};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;

use lnm_sdk::api::rest::models::BoundedPercentage;

use crate::{
    db::DbContext,
    signal::core::{ConfiguredSignalEvaluator, Signal},
    sync::PriceHistoryState,
    tui::{Result as TuiResult, TuiControllerShutdown},
    util::{AbortOnDropHandle, DateTimeExt},
};

use super::core::{
    RawOperator, SignalOperator, TradeExecutor, TradingState, WrappedRawOperator,
    WrappedSignalOperator,
};

pub mod error;
mod executor;

use error::{BacktestError, Result};
use executor::SimulatedTradeExecutor;

#[derive(Debug)]
pub enum BacktestStatus {
    NotInitiated,
    Starting,
    Running,
    Finished,
    Failed(BacktestError),
    Aborted,
}

impl BacktestStatus {
    pub fn is_not_initiated(&self) -> bool {
        matches!(self, Self::NotInitiated)
    }

    pub fn is_starting(&self) -> bool {
        matches!(self, Self::Starting)
    }

    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }

    pub fn is_finished(&self) -> bool {
        matches!(self, Self::Finished)
    }

    pub fn is_failed(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }
}

impl fmt::Display for BacktestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitiated => write!(f, "Not initiated"),
            Self::Starting => write!(f, "Starting"),
            Self::Running => write!(f, "Running"),
            Self::Finished => write!(f, "Finished"),
            Self::Failed(error) => write!(f, "Failed: {error}"),
            Self::Aborted => write!(f, "Aborted"),
        }
    }
}

#[derive(Clone)]
pub enum BacktestUpdate {
    Status(Arc<BacktestStatus>),
    TradingState(TradingState),
}

impl From<Arc<BacktestStatus>> for BacktestUpdate {
    fn from(value: Arc<BacktestStatus>) -> Self {
        Self::Status(value)
    }
}

impl From<TradingState> for BacktestUpdate {
    fn from(value: TradingState) -> Self {
        Self::TradingState(value)
    }
}

pub type BacktestTransmiter = broadcast::Sender<BacktestUpdate>;
pub type BacktestReceiver = broadcast::Receiver<BacktestUpdate>;

#[derive(Debug)]
struct BacktestStatusManager {
    status: Mutex<Arc<BacktestStatus>>,
    update_tx: BacktestTransmiter,
}

impl BacktestStatusManager {
    pub fn new(update_tx: BacktestTransmiter) -> Arc<Self> {
        let status = Mutex::new(Arc::new(BacktestStatus::NotInitiated));

        Arc::new(Self { status, update_tx })
    }

    fn lock_status(&self) -> MutexGuard<'_, Arc<BacktestStatus>> {
        self.status
            .lock()
            .expect("`BacktestStatusManager` mutex can't be poisoned")
    }

    pub fn snapshot(&self) -> Arc<BacktestStatus> {
        self.lock_status().clone()
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.update_tx.subscribe()
    }

    pub fn update(&self, new_status: BacktestStatus) {
        let new_status = Arc::new(new_status);

        let mut status_guard = self.lock_status();
        *status_guard = new_status.clone();
        drop(status_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_status.into());
    }
}

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

        return Err(BacktestError::Generic(
            "Backtest process was already consumed".to_string(),
        ));
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

        Err(BacktestError::Generic(
            "Backtest process was already consumed".to_string(),
        ))
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

pub struct BacktestConfig {
    buffer_size: usize,
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    tsl_step_size: BoundedPercentage,
    update_interval: Duration,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1800,
            max_running_qtd: 50,
            fee_perc: 0.1.try_into().expect("must be a valid `BoundedPercentage`"),
            tsl_step_size: BoundedPercentage::MIN,
            update_interval: Duration::days(1),
        }
    }
}

impl BacktestConfig {
    pub fn set_buffer_size(mut self, size: usize) -> Result<Self> {
        if size < 100 {
            return Err(BacktestError::Generic(
                "Buffer size must be at least 100".to_string(),
            ));
        }
        self.buffer_size = size;
        Ok(self)
    }

    pub fn set_max_running_qtd(mut self, max: usize) -> Result<Self> {
        if max == 0 {
            return Err(BacktestError::Generic(
                "Maximum running quantity must be at least 1".to_string(),
            ));
        }
        self.max_running_qtd = max;
        Ok(self)
    }

    pub fn set_fee_perc(mut self, fee_perc: BoundedPercentage) -> Self {
        self.fee_perc = fee_perc;
        self
    }

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn set_update_interval(mut self, hours: u32) -> Self {
        self.update_interval = Duration::hours(hours as i64);
        self
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
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                Ok(())
            }
            Operator::Raw {
                last_eval: _,
                raw_operator,
            } => {
                raw_operator
                    .set_trade_executor(trade_executor)
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

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
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

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
            return Err(BacktestError::Generic(
                "Start and end times must be rounded to seconds".to_string(),
            ));
        }

        // Validate duration is at least 1 day
        if end_time - start_time < chrono::Duration::days(1) {
            return Err(BacktestError::Generic(
                "Backtest duration must be at least 1 day".to_string(),
            ));
        }

        let max_ctx_window = operator.max_ctx_window_secs()?;

        if config.buffer_size < max_ctx_window {
            return Err(BacktestError::Generic(format!(
                "buffer size {} is incompatible with max ctx window {}",
                config.buffer_size, max_ctx_window
            )));
        }

        let price_history_state = PriceHistoryState::evaluate(&db)
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

        if !price_history_state
            .is_range_available(start_time, end_time)
            .map_err(|e| BacktestError::Generic(e.to_string()))?
        {
            return Err(BacktestError::Generic(format!(
                "range ({start_time} to {end_time}) is not available in price history ({price_history_state})"
            )));
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
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?
                .ok_or(BacktestError::Generic(format!(
                    "no entries before start_time"
                )))?;

            Arc::new(SimulatedTradeExecutor::new(
                self.config.max_running_qtd,
                self.config.fee_perc,
                self.config.tsl_step_size,
                self.start_time,
                start_time_entry.value,
                self.start_balance,
            ))
        };

        let mut operator = self.operator;

        operator
            .set_executor(trades_executor.clone())
            .map_err(|e| {
                BacktestError::Generic(format!(
                    "couldn't set the simulated trades manager {}",
                    e.to_string()
                ))
            })?;

        let max_ctx_window = operator.max_ctx_window_secs()?;

        let buffer_size = self.config.buffer_size;

        let get_buffers = |time_cursor: DateTime<Utc>| {
            let db = &self.db;
            async move {
                let buffer_end = time_cursor
                    .checked_add_signed(Duration::seconds(
                        buffer_size as i64 - max_ctx_window as i64,
                    ))
                    .ok_or(BacktestError::Generic(
                        "buffer date out of range".to_string(),
                    ))?;

                let locf_buffer = if max_ctx_window == 0 {
                    Vec::new()
                } else {
                    db.price_ticks
                        .compute_locf_entries_for_range(buffer_end, buffer_size)
                        .await
                        .map_err(|e| BacktestError::Generic(e.to_string()))?
                };

                let price_ticks = db
                    .price_history
                    .get_entries_between(time_cursor, buffer_end + Duration::seconds(1))
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

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

        let mut send_next_update_at = self.start_time + self.config.update_interval;

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
                            .map_err(|e| BacktestError::Generic(e.to_string()))?;

                        signal_operator
                            .process_signal(&signal)
                            .await
                            .map_err(|e| BacktestError::Generic(e.to_string()))?;
                    }
                }
                Operator::Raw {
                    last_eval,
                    raw_operator,
                } => {
                    let iteration_interval = raw_operator
                        .iteration_interval()
                        .map_err(|e| BacktestError::Generic(e.to_string()))?;

                    if time_cursor >= *last_eval + iteration_interval {
                        *last_eval = time_cursor;

                        let ctx_window_size = raw_operator
                            .context_window_secs()
                            .map_err(|e| BacktestError::Generic(e.to_string()))?;

                        let ctx_entries = if ctx_window_size == 0 {
                            &[]
                        } else {
                            &locf_buffer[locf_buffer_cursor_idx + 1 - ctx_window_size
                                ..=locf_buffer_cursor_idx]
                        };

                        raw_operator
                            .iterate(ctx_entries)
                            .await
                            .map_err(|e| BacktestError::Generic(e.to_string()))?;
                    }
                }
            }

            if time_cursor >= send_next_update_at {
                let trades_state = trades_executor
                    .trading_state()
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                // Ignore no-receivers errors
                let _ = self.update_tx.send(trades_state.into());

                send_next_update_at += self.config.update_interval;
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
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

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
                .map_err(|e| BacktestError::Generic(e.to_string()))?;
        }

        let final_state = trades_executor
            .trading_state()
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

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
