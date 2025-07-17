use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use tokio::sync::broadcast;

use lnm_sdk::api::rest::models::BoundedPercentage;

use crate::{
    db::DbContext,
    signal::core::{ConfiguredSignalEvaluator, Signal},
    sync::PriceHistoryState,
    tui::{Result as TuiResult, TuiControllerShutdown, TuiError},
    util::{AbortOnDropHandle, DateTimeExt},
};

use super::core::{Operator, TradeExecutor, TradingState, WrappedOperator};

pub mod error;

mod executor;

use error::{BacktestError, Result};
use executor::SimulatedTradeExecutor;

#[derive(Debug)]
pub enum BacktestState {
    NotInitiated,
    Starting,
    Running(TradingState),
    Finished(TradingState),
    Failed(BacktestError),
    Aborted,
}

pub type BacktestTransmiter = broadcast::Sender<Arc<BacktestState>>;
pub type BacktestReceiver = broadcast::Receiver<Arc<BacktestState>>;

#[derive(Debug)]
struct BacktestStateManager {
    state: Mutex<Arc<BacktestState>>,
    state_tx: BacktestTransmiter,
}

impl BacktestStateManager {
    pub fn new() -> Arc<Self> {
        let state = Mutex::new(Arc::new(BacktestState::NotInitiated));
        let (state_tx, _) = broadcast::channel::<Arc<BacktestState>>(100);

        Arc::new(Self { state, state_tx })
    }

    pub fn snapshot(&self) -> Arc<BacktestState> {
        self.state
            .lock()
            .expect("state lock can't be poisoned")
            .clone()
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_tx.subscribe()
    }

    pub fn update(&self, new_state: BacktestState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().expect("state lock can't be poisoned");
        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.state_tx.send(new_state);
    }
}

#[derive(Debug)]
pub struct BacktestController {
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    state_manager: Arc<BacktestStateManager>,
}

impl BacktestController {
    fn new(handle: AbortOnDropHandle<()>, state_manager: Arc<BacktestStateManager>) -> Arc<Self> {
        Arc::new(Self {
            handle: Mutex::new(Some(handle)),
            state_manager,
        })
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_manager.receiver()
    }

    pub fn state_snapshot(&self) -> Arc<BacktestState> {
        self.state_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        let mut handle_guard = self
            .handle
            .lock()
            .expect("`BacktestController` mutex can't be poisoned");
        handle_guard.take()
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
                self.state_manager.update(BacktestState::Aborted);
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
        self.abort()
            .await
            .map_err(|e| TuiError::Generic(e.to_string()))
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
            update_interval: Duration::milliseconds(300),
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

    pub fn set_update_interval(mut self, millis: u32) -> Self {
        self.update_interval = Duration::milliseconds(millis as i64);
        self
    }
}

pub struct BacktestEngine {
    config: BacktestConfig,
    db: Arc<DbContext>,
    evaluators: Vec<ConfiguredSignalEvaluator>,
    operator: WrappedOperator,
    start_time: DateTime<Utc>,
    start_balance: u64,
    end_time: DateTime<Utc>,
    state_manager: Arc<BacktestStateManager>,
}

impl BacktestEngine {
    pub async fn new(
        config: BacktestConfig,
        db: Arc<DbContext>,
        evaluators: Vec<ConfiguredSignalEvaluator>,
        operator: Box<dyn Operator>,
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

        let max_ctx_window = evaluators
            .iter()
            .map(|evaluator| evaluator.context_window_secs())
            .max()
            .ok_or(BacktestError::Generic(
                "At least one evaluator must be provided".to_string(),
            ))?;

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

        let state_manager = BacktestStateManager::new();

        Ok(Self {
            config,
            db,
            evaluators,
            operator: operator.into(),
            start_time,
            start_balance,
            end_time,
            state_manager,
        })
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_manager.receiver()
    }

    async fn run(self) -> Result<TradingState> {
        self.state_manager.update(BacktestState::Starting);

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
            .set_trade_executor(trades_executor.clone())
            .map_err(|e| {
                BacktestError::Generic(format!(
                    "couldn't set the simulated trades manager {}",
                    e.to_string()
                ))
            })?;

        let max_ctx_window = self
            .evaluators
            .iter()
            .map(|evaluator| evaluator.context_window_secs())
            .max()
            .expect("evaluators can't be empty");

        let buffer_size = self.config.buffer_size;

        let get_buffers = |time_cursor: DateTime<Utc>| {
            let db = &self.db;
            async move {
                let locf_buffer_last_time = time_cursor
                    .checked_add_signed(Duration::seconds(
                        buffer_size as i64 - max_ctx_window as i64,
                    ))
                    .ok_or(BacktestError::Generic(
                        "buffer date out of range".to_string(),
                    ))?;

                let locf_buffer = db
                    .price_ticks
                    .compute_locf_entries_for_range(locf_buffer_last_time, buffer_size)
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                let locf_buffer_cursor_idx = max_ctx_window - 1;

                if locf_buffer.len() != buffer_size
                    || locf_buffer[locf_buffer_cursor_idx].time != time_cursor
                {
                    return Err(BacktestError::Generic(
                        "unexpected `eval_entries_locf` result".to_string(),
                    ));
                }

                let price_ticks = db
                    .price_history
                    .get_entries_between(time_cursor, locf_buffer_last_time)
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                Ok::<_, BacktestError>((locf_buffer, locf_buffer_cursor_idx, price_ticks, 0))
            }
        };

        let mut time_cursor = self.start_time;

        let (
            mut locf_buffer,
            mut locf_buffer_cursor_idx,
            mut price_ticks,
            mut price_ticks_cursor_idx,
        ) = get_buffers(time_cursor).await?;

        let mut last_state_update = {
            let trades_state = trades_executor
                .trading_state()
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?;

            self.state_manager
                .update(BacktestState::Running(trades_state));

            Utc::now()
        };

        loop {
            if time_cursor >= self.end_time {
                break;
            }

            for evaluator in &self.evaluators {
                let ctx_window_size = evaluator.context_window_secs();

                let ctx_entries = &locf_buffer
                    [locf_buffer_cursor_idx + 1 - ctx_window_size..=locf_buffer_cursor_idx];

                let signal = Signal::try_evaluate(evaluator, ctx_entries)
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                operator
                    .process_signal(&signal)
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;
            }

            time_cursor = time_cursor + Duration::seconds(1);

            if locf_buffer_cursor_idx < locf_buffer.len() - 1 {
                locf_buffer_cursor_idx += 1;
            } else {
                // Reached the end of the current buffer

                // Update the state not more than once every `update_interval`

                if Utc::now().signed_duration_since(last_state_update) > self.config.update_interval
                {
                    let trades_state = trades_executor
                        .trading_state()
                        .await
                        .map_err(|e| BacktestError::Generic(e.to_string()))?;

                    self.state_manager
                        .update(BacktestState::Running(trades_state));

                    last_state_update = Utc::now();
                }

                // Update buffers

                (
                    locf_buffer,
                    locf_buffer_cursor_idx,
                    price_ticks,
                    price_ticks_cursor_idx,
                ) = get_buffers(time_cursor).await?;
            }

            // Update `SimulatedTradeExecutor` with all the price ticks with time lte
            // the new `time_cursor`.
            while let Some(next_price_tick) = price_ticks.get(price_ticks_cursor_idx) {
                if next_price_tick.time <= time_cursor {
                    trades_executor
                        .tick_update(next_price_tick.time, next_price_tick.value)
                        .await
                        .map_err(|e| BacktestError::Generic(e.to_string()))?;

                    price_ticks_cursor_idx += 1;
                } else {
                    break;
                }
            }
        }

        trades_executor
            .close_all()
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

        let final_state = trades_executor
            .trading_state()
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

        Ok(final_state)
    }

    pub fn start(self) -> Arc<BacktestController> {
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(async move {
            let state_manager = self.state_manager.clone();

            let final_backtest_state = match self.run().await {
                Ok(final_trade_state) => BacktestState::Finished(final_trade_state),
                Err(e) => BacktestState::Failed(e),
            };

            state_manager.update(final_backtest_state);
        })
        .into();

        BacktestController::new(handle, state_manager)
    }
}
