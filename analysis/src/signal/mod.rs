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
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
}

impl SignalProcess {
    fn new(db: Arc<DbContext>, sync_controller: Arc<SyncController>) -> Self {
        Self {
            db,
            sync_controller,
        }
    }

    async fn run(&self) -> Result<()> {
        loop {
            time::sleep(time::Duration::from_millis(500)).await;
            let sync_state = self
                .sync_controller
                .state()
                .await
                .map_err(|_| SignalError::Generic("couldn't get sync state".to_string()))?;

            if sync_state != SyncState::Synced {
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

pub struct SignalJob {
    process: SignalProcess,
}

impl SignalJob {
    pub fn new(db: Arc<DbContext>, sync_controller: Arc<SyncController>) -> Self {
        let process = SignalProcess::new(db, sync_controller);

        Self { process }
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
            // time::sleep(self.restart_interval).await;
        }
    }

    pub fn start(self) {
        let _ = tokio::spawn(self.process_recovery_loop());
    }
}
