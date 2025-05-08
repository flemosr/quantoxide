use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};

use crate::{db::DbContext, signal::eval::SignalEvaluator, trade::TradesState, util::DateTimeExt};

pub mod error;

use error::{BacktestError, Result};

const BUFFER_SIZE_DEFAULT: usize = 1800;

#[derive(Debug, PartialEq)]
pub enum BacktestState {
    NotInitiated,
    Starting,
    Running(TradesState),
    Failed(BacktestError),
}

pub type BacktestTransmiter = broadcast::Sender<Arc<BacktestState>>;
pub type BacktestReceiver = broadcast::Receiver<Arc<BacktestState>>;

#[derive(Clone)]
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

    pub async fn state_snapshopt(&self) -> Arc<BacktestState> {
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

pub struct BacktestController {
    state_manager: BacktestStateManager,
    handle: JoinHandle<Result<()>>,
}

impl BacktestController {
    fn new(state_manager: BacktestStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle,
        }
    }

    pub fn receiver(&self) -> BacktestReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<BacktestState> {
        self.state_manager.state_snapshopt().await
    }

    pub fn abort(&self) {
        self.handle.abort();
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
    state_manager: BacktestStateManager,
}

impl Backtest {
    pub fn new(
        config: BacktestConfig,
        db: Arc<DbContext>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        evaluators: Vec<Box<dyn SignalEvaluator>>,
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

        let state_manager = BacktestStateManager::new();

        Ok(Self {
            config,
            db,
            start_time,
            end_time,
            evaluators,
            state_manager,
        })
    }

    async fn run(self) -> Result<()> {
        self.state_manager.update(BacktestState::Starting).await?;

        loop {
            todo!()
        }
    }

    pub fn start(self) -> Result<Arc<BacktestController>> {
        let state_manager = self.state_manager.clone();

        let handle = tokio::spawn(self.run());

        let backtest_controller = BacktestController::new(state_manager, handle);

        Ok(Arc::new(backtest_controller))
    }
}
