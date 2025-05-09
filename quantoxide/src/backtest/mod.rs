use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};

use crate::{
    db::DbContext,
    signal::eval::SignalEvaluator,
    trade::{Operator, SimulatedTradesManager, TradesState},
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
    evaluators: Vec<Box<dyn SignalEvaluator>>,
    operator: Box<dyn Operator>,
    state_manager: BacktestStateManager,
}

impl Backtest {
    pub async fn new(
        config: BacktestConfig,
        db: Arc<DbContext>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        evaluators: Vec<Box<dyn SignalEvaluator>>,
        mut operator: Box<dyn Operator>,
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

        if evaluators.is_empty() {
            return Err(BacktestError::Generic(
                "At least one evaluator must be provided".to_string(),
            ));
        }

        let max_ctx_window = evaluators
            .iter()
            .map(|evaluator| evaluator.context_window_secs())
            .max()
            .expect("evaluators can't be empty");

        if config.buffer_size < max_ctx_window {
            return Err(BacktestError::Generic(format!(
                "buffer size {} is incompatible with max ctx window {}",
                config.buffer_size, max_ctx_window
            )));
        }

        let trades_manager = {
            let start_time_entry = db
                .price_history
                .get_latest_entry_at_or_before(start_time)
                .await
                .map_err(|e| BacktestError::Generic(e.to_string()))?
                .ok_or(BacktestError::Generic(format!(
                    "no entries before start_time"
                )))?;

            SimulatedTradesManager::new(
                50,
                0.1.try_into().unwrap(),
                start_time,
                start_time_entry.value,
                1_000_000,
            )
        };

        operator.set_trades_manager(Box::new(trades_manager));

        let state_manager = BacktestStateManager::new();

        Ok(Self {
            config,
            db,
            start_time,
            end_time,
            evaluators,
            operator,
            state_manager,
        })
    }

    async fn run(self) -> Result<()> {
        self.state_manager.update(BacktestState::Starting).await?;
        // let trades_manager = self.operator.trades_manager();

        loop {
            todo!()
        }

        // self.state_manager.update(BacktestState::Finished).await?;
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
