use chrono::Utc;
use std::sync::Arc;
use tokio::time;

use crate::{
    db::DbContext,
    sync::{SyncController, SyncState},
    util::CeilSec,
};

mod error;

use error::{Result, SignalError};

struct SignalProcess {
    config: SignalJobConfig,
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
}

impl SignalProcess {
    fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
    ) -> Self {
        Self {
            config,
            db,
            sync_controller,
        }
    }

    async fn run(&self) -> Result<()> {
        loop {
            time::sleep(self.config.eval_interval).await;

            let sync_state = self.sync_controller.state_snapshot().await;

            if *sync_state != SyncState::Synced {
                println!("\nNot synced. Skipping signal eval.");
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

#[derive(Clone, Debug)]
pub struct SignalJobConfig {
    eval_interval: time::Duration,
    restart_interval: time::Duration,
}

impl Default for SignalJobConfig {
    fn default() -> Self {
        Self {
            eval_interval: time::Duration::from_secs(60),
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
    process: SignalProcess,
    restart_interval: time::Duration,
}

impl SignalJob {
    pub fn new(
        config: SignalJobConfig,
        db: Arc<DbContext>,
        sync_controller: Arc<SyncController>,
    ) -> Self {
        let restart_interval = config.restart_interval;
        let process = SignalProcess::new(config, db, sync_controller);

        Self {
            process,
            restart_interval,
        }
    }

    async fn process_recovery_loop(self) -> Result<()> {
        loop {
            // self.state_manager.update(SignalState::Starting).await?;

            if let Err(e) = self.process.run().await {
                // self.state_manager
                //     .update(SignalState::Failed(e.to_string()))
                //     .await?
            }

            // self.state_manager.update(SignalState::Restarting).await?;
            time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) {
        let _ = tokio::spawn(self.process_recovery_loop());
    }
}
