use chrono::Utc;
use std::sync::Arc;
use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
    time,
};

use crate::{
    db::DbContext,
    sync::{SyncController, SyncState},
    util::CeilSec,
};

mod error;
pub mod eval;

use error::{Result, SignalError};

#[derive(Debug, PartialEq, Eq)]
pub enum SignalJobState {
    NotInitiated,
    Starting,
    Running,
    WaitingForSync,
    Failed(SignalError),
    Restarting,
}

pub type SignalJobTransmiter = broadcast::Sender<Arc<SignalJobState>>;
pub type SignalJobReceiver = broadcast::Receiver<Arc<SignalJobState>>;

#[derive(Clone)]
struct SignalJobStateManager {
    state: Arc<Mutex<Arc<SignalJobState>>>,
    state_tx: SignalJobTransmiter,
}

impl SignalJobStateManager {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(Arc::new(SignalJobState::NotInitiated)));
        let (state_tx, _) = broadcast::channel::<Arc<SignalJobState>>(100);

        Self { state, state_tx }
    }

    pub async fn state_snapshopt(&self) -> Arc<SignalJobState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> SignalJobReceiver {
        self.state_tx.subscribe()
    }

    async fn try_send_state_update(&self, new_state: Arc<SignalJobState>) -> Result<()> {
        if self.state_tx.receiver_count() > 0 {
            self.state_tx
                .send(new_state)
                .map_err(SignalError::SignalTransmiterFailed)?;
        }

        Ok(())
    }

    pub async fn update(&self, new_state: SignalJobState) -> Result<()> {
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

struct SignalProcess {
    config: SignalJobConfig,
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
    state_manager: SignalJobStateManager,
}

impl SignalProcess {
    fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
        state_manager: SignalJobStateManager,
    ) -> Self {
        Self {
            config,
            db,
            sync_controller,
            state_manager,
        }
    }

    async fn run(&self) -> Result<()> {
        loop {
            time::sleep(self.config.eval_interval).await;

            let sync_state = self.sync_controller.state_snapshot().await;

            if *sync_state == SyncState::Synced {
                self.state_manager.update(SignalJobState::Running).await?;
            } else {
                self.state_manager
                    .update(SignalJobState::WaitingForSync)
                    .await?;

                continue;
            }

            let now = Utc::now().ceil_sec();
            let entries = self
                .db
                .price_history
                .eval_entries_locf(&now, 10)
                .await
                .map_err(|_| SignalError::Generic("db error".to_string()))?;
            let curr_locf = entries
                .first()
                .ok_or(SignalError::Generic("db inconsistency error".to_string()))?;

            println!("\n{curr_locf}");
        }
    }
}

pub struct SignalJobController {
    state_manager: SignalJobStateManager,
    handle: JoinHandle<Result<()>>,
}

impl SignalJobController {
    fn new(state_manager: SignalJobStateManager, handle: JoinHandle<Result<()>>) -> Self {
        Self {
            state_manager,
            handle,
        }
    }

    pub fn receiver(&self) -> SignalJobReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<SignalJobState> {
        self.state_manager.state_snapshopt().await
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

#[derive(Clone, Debug)]
pub struct SignalJobConfig {
    eval_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for SignalJobConfig {
    fn default() -> Self {
        Self {
            eval_interval: time::Duration::from_secs(1),
            restart_interval: time::Duration::from_secs(10),
        }
    }
}

impl SignalJobConfig {
    pub fn set_eval_interval(mut self, secs: u64) -> Self {
        self.eval_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }
}

pub struct SignalJob {
    state_manager: SignalJobStateManager,
    process: SignalProcess,
    restart_interval: time::Duration,
}

impl SignalJob {
    pub fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
    ) -> Self {
        let state_manager = SignalJobStateManager::new();
        let restart_interval = config.restart_interval;
        let process = SignalProcess::new(config, db, sync_controller, state_manager.clone());

        Self {
            state_manager,
            process,
            restart_interval,
        }
    }

    async fn process_recovery_loop(self) -> Result<()> {
        loop {
            self.state_manager.update(SignalJobState::Starting).await?;

            if let Err(e) = self.process.run().await {
                self.state_manager.update(SignalJobState::Failed(e)).await?
            }

            self.state_manager
                .update(SignalJobState::Restarting)
                .await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) -> Result<Arc<SignalJobController>> {
        let state_manager = self.state_manager.clone();
        let handle = tokio::spawn(self.process_recovery_loop());

        let signal_controller = SignalJobController::new(state_manager, handle);

        Ok(Arc::new(signal_controller))
    }
}
