use chrono::Utc;
use std::sync::Arc;

use crate::{
    db::DbContext,
    sync::{SyncController, SyncState},
    util::CeilSec,
};

pub struct SignalJob {
    db: Arc<DbContext>,
    sync_controller: Arc<SyncController>,
}

impl SignalJob {
    pub fn new(db: Arc<DbContext>, sync_controller: Arc<SyncController>) -> Self {
        SignalJob {
            db,
            sync_controller,
        }
    }

    pub fn start(self) {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                let sync_state = self
                    .sync_controller
                    .state()
                    .await
                    .expect("must fetch sync state");

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
                    .expect("db should work");
                let curr_locf = entries.first().expect("not empty");

                println!("\n{curr_locf}");
            }
        });
    }
}
