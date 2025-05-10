use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};

use crate::{
    db::DbContext,
    signal::{Signal, eval::SignalEvaluator},
    trade::{Operator, SimulatedTradesManager, TradesManager, TradesState},
    util::DateTimeExt,
};

pub mod error;

use error::{BacktestError, Result};

const BUFFER_SIZE_DEFAULT: usize = 1800;

#[derive(Debug, PartialEq)]
pub enum BacktestState {
    NotInitiated,
    Starting,
    Running(TradesState),
    Finished(TradesState),
    Failed(BacktestError),
    Aborted,
}

pub type BacktestTransmiter = broadcast::Sender<Arc<BacktestState>>;
pub type BacktestReceiver = broadcast::Receiver<Arc<BacktestState>>;

#[derive(Debug, Clone)]
struct BacktestStateManager {
    state: Arc<Mutex<Arc<BacktestState>>>,
    state_tx: BacktestTransmiter,
}

impl BacktestStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(BacktestState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<BacktestState>>(100);

        Self { state, state_tx }
    }

    pub async fn snapshot(&self) -> Arc<BacktestState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_tx.subscribe()
    }

    async fn try_send_state_update(&self, new_state: Arc<BacktestState>) -> Result<()> {
        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(BacktestError::TransmiterFailed)?;
        }

        Ok(())
    }

    pub async fn update(&self, new_state: BacktestState) -> Result<()> {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        if **state_guard == *new_state {
            return Ok(());
        }

        *state_guard = new_state.clone();
        drop(state_guard);

        self.try_send_state_update(new_state).await
    }
}

#[derive(Debug)]
pub struct BacktestController {
    state_manager: BacktestStateManager,
    handle: Arc<Mutex<Option<JoinHandle<Result<()>>>>>,
}

impl BacktestController {
    fn new(state_manager: BacktestStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle: Arc::new(Mutex::new(Some(handle))),
        }
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<BacktestState> {
        let state = self.state_manager.snapshot().await;

        match self.handle.lock().await.as_ref() {
            Some(handle) if handle.is_finished() => {
                // If the process has terminated but the state doesn't reflect that,
                // return a failure state
                match state.as_ref() {
                    BacktestState::Finished(_) | BacktestState::Failed(_) => state,
                    _ => Arc::new(BacktestState::Failed(BacktestError::Generic(
                        "Backtest terminated unexpectedly".to_string(),
                    ))),
                }
            }
            None => {
                return Arc::new(BacktestState::Failed(BacktestError::Generic(
                    "Backtest process was already consumed".to_string(),
                )));
            }
            _ => state,
        }
    }

    /// Consumes the task handle and waits for the backtest to complete.
    /// This method can only be called once per controller instance.
    /// Returns the final result of the backtest.
    pub async fn wait_for_completion(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            return handle.await.map_err(BacktestError::TaskJoin)?;
        }

        return Err(BacktestError::Generic(
            "Backtest process was already consumed".to_string(),
        ));
    }

    /// Aborts the backtest and consumes the task handle.
    /// This method can only be called once per controller instance.
    /// Returns the result of the aborted backtest.
    pub async fn abort(&self) -> Result<()> {
        let mut handle_guard = self.handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            if !handle.is_finished() {
                handle.abort();
                self.state_manager.update(BacktestState::Aborted).await?;
            }

            return handle.await.map_err(BacktestError::TaskJoin)?;
        }

        return Err(BacktestError::Generic(
            "Backtest process was already consumed".to_string(),
        ));
    }
}

pub struct BacktestConfig {
    buffer_size: usize,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            buffer_size: BUFFER_SIZE_DEFAULT,
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
}

pub struct Backtest {
    config: BacktestConfig,
    db: Arc<DbContext>,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    evaluator: Box<dyn SignalEvaluator>,
    operator: Box<dyn Operator>,
    state_manager: BacktestStateManager,
}

impl Backtest {
    pub async fn new(
        config: BacktestConfig,
        db: Arc<DbContext>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        evaluator: Box<dyn SignalEvaluator>,
        operator: Box<dyn Operator>,
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

        // TODO: multi-evaluator support

        // if evaluators.is_empty() {
        //     return Err(BacktestError::Generic(
        //         "At least one evaluator must be provided".to_string(),
        //     ));
        // }

        // let max_ctx_window = evaluators
        //     .iter()
        //     .map(|evaluator| evaluator.context_window_secs())
        //     .max()
        //     .expect("evaluators can't be empty");

        if config.buffer_size < evaluator.context_window_secs() {
            return Err(BacktestError::Generic(format!(
                "buffer size {} is incompatible with max ctx window {}",
                config.buffer_size,
                evaluator.context_window_secs()
            )));
        }

        let state_manager = BacktestStateManager::new();

        Ok(Self {
            config,
            db,
            start_time,
            end_time,
            evaluator,
            operator,
            state_manager,
        })
    }

    async fn run(self) -> Result<()> {
        self.state_manager.update(BacktestState::Starting).await?;

        let trades_manager = {
            let start_time_entry = self
                .db
                .price_history
                .get_latest_entry_at_or_before(self.start_time)
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?
                .ok_or(BacktestError::Generic(format!(
                    "no entries before start_time"
                )))?;

            // TODO: Use config
            Arc::new(SimulatedTradesManager::new(
                50,
                0.1.try_into().unwrap(),
                self.start_time,
                start_time_entry.value,
                1_000_000,
            ))
        };

        let mut operator = self.operator;

        operator
            .set_trades_manager(trades_manager.clone())
            .map_err(|e| {
                BacktestError::Generic(format!(
                    "couldn't set the simulated trades manager {}",
                    e.to_string()
                ))
            })?;

        let ctx_window_size = self.evaluator.context_window_secs() as usize;
        let buffer_size = self.config.buffer_size;

        let get_buffers = |time_cursor: DateTime<Utc>, ctx_window_size: usize| {
            let db = &self.db;
            async move {
                let locf_buffer_last_time = time_cursor
                    .checked_add_signed(Duration::seconds(
                        buffer_size as i64 - ctx_window_size as i64,
                    ))
                    .ok_or(BacktestError::Generic(
                        "buffer date out of range".to_string(),
                    ))?;

                let locf_buffer = db
                    .price_history
                    .eval_entries_locf(&locf_buffer_last_time, buffer_size)
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                let locf_buffer_cursor_idx = ctx_window_size - 1;

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
        ) = get_buffers(time_cursor, ctx_window_size).await?;

        {
            let trades_state = trades_manager
                .state()
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?;

            self.state_manager
                .update(BacktestState::Running(trades_state))
                .await?;
        }

        loop {
            if time_cursor >= self.end_time {
                trades_manager
                    .as_ref()
                    .close_all()
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;
                break;
            }

            let ctx_entries =
                &locf_buffer[locf_buffer_cursor_idx + 1 - ctx_window_size..=locf_buffer_cursor_idx];

            let signal = Signal::try_evaluate(&self.evaluator, ctx_entries)
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?;

            operator
                .consume_signal(signal)
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?;

            time_cursor = time_cursor + Duration::seconds(1);

            if locf_buffer_cursor_idx < locf_buffer.len() - 1 {
                locf_buffer_cursor_idx += 1;
            } else {
                // Reached the end of the current buffer

                let trades_state = trades_manager
                    .state()
                    .await
                    .map_err(|e| BacktestError::Generic(e.to_string()))?;

                self.state_manager
                    .update(BacktestState::Running(trades_state))
                    .await?;

                (
                    locf_buffer,
                    locf_buffer_cursor_idx,
                    price_ticks,
                    price_ticks_cursor_idx,
                ) = get_buffers(time_cursor, ctx_window_size).await?;
            }

            // Update `SimulatedTradesManager` with all the price ticks with time lte
            // the new `time_cursor`.
            while let Some(next_price_tick) = price_ticks.get(price_ticks_cursor_idx) {
                if next_price_tick.time <= time_cursor {
                    trades_manager
                        .tick_update(next_price_tick.time, next_price_tick.value)
                        .await
                        .map_err(|e| BacktestError::Generic(e.to_string()))?;

                    price_ticks_cursor_idx += 1;
                } else {
                    break;
                }
            }
        }

        trades_manager
            .close_all()
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

        let final_state = trades_manager
            .state()
            .await
            .map_err(|e| BacktestError::Generic(e.to_string()))?;

        self.state_manager
            .update(BacktestState::Finished(final_state))
            .await?;

        Ok(())
    }

    pub fn start(self) -> Result<Arc<BacktestController>> {
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(async move {
            let state_manager = self.state_manager.clone();
            if let Err(e) = self.run().await {
                return state_manager.update(BacktestState::Failed(e)).await;
            }
            Ok(())
        });

        let backtest_controller = BacktestController::new(state_manager, handle);

        Ok(Arc::new(backtest_controller))
    }
}
