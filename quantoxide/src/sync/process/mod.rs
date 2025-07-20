use std::{pin::Pin, sync::Arc};

use chrono::Duration;
use futures::TryFutureExt;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};

use lnm_sdk::api::ApiContext;

use crate::{
    db::{DbContext, models::PriceTick},
    util::{AbortOnDropHandle, Never},
};

use super::{
    engine::{SyncConfig, SyncMode},
    error::{Result, SyncError},
    state::{SyncStatus, SyncStatusManager, SyncStatusNotSynced, SyncTransmiter},
};

mod real_time_collection_task;
mod sync_price_history_task;

use real_time_collection_task::RealTimeCollectionTask;
use sync_price_history_task::{PriceHistoryStateTransmiter, SyncPriceHistoryTask};

pub use real_time_collection_task::RealTimeCollectionError;
pub use sync_price_history_task::{PriceHistoryState, SyncPriceHistoryError};

#[derive(Clone)]
struct SyncProcessConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    restart_interval: time::Duration,
}

impl From<&SyncConfig> for SyncProcessConfig {
    fn from(value: &SyncConfig) -> Self {
        Self {
            api_cooldown: value.api_cooldown(),
            api_error_cooldown: value.api_error_cooldown(),
            api_error_max_trials: value.api_error_max_trials(),
            api_history_batch_size: value.api_history_batch_size(),
            sync_history_reach: value.sync_history_reach(),
            re_sync_history_interval: value.re_sync_history_interval(),
            restart_interval: value.restart_interval(),
        }
    }
}

pub struct SyncProcess {
    config: SyncProcessConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    mode: SyncMode,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmiter,
}

impl SyncProcess {
    pub fn new(
        config: &SyncConfig,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        mode: SyncMode,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<SyncStatusManager>,
        update_tx: SyncTransmiter,
    ) -> Self {
        Self {
            config: config.into(),
            db,
            api,
            mode,
            shutdown_tx,
            status_manager,
            update_tx,
        }
    }

    async fn run_price_history_task_backfill(
        &self,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(
            &self.config,
            self.db.clone(),
            self.api.clone(),
            history_state_tx,
        )
        .backfill()
        .await
        .map_err(SyncError::SyncPriceHistory)
    }

    async fn run_price_history_task_live(
        &self,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
        range: Duration,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(
            &self.config,
            self.db.clone(),
            self.api.clone(),
            history_state_tx,
        )
        .live(range)
        .await
        .map_err(SyncError::SyncPriceHistory)
    }

    /// Clean up is not needed since the task is terminated when
    /// `history_state_tx` is dropped.
    pub fn spawn_history_state_update_handler(
        &self,
        mut history_state_rx: mpsc::Receiver<PriceHistoryState>,
    ) {
        let update_tx = self.update_tx.clone();
        tokio::spawn(async move {
            while let Some(new_history_state) = history_state_rx.recv().await {
                // Ignore no-receivers errors
                let _ = update_tx.send(new_history_state.into());
            }
        });
    }

    fn spawn_real_time_collection_task(
        &self,
        price_tick_tx: broadcast::Sender<PriceTick>,
    ) -> AbortOnDropHandle<Result<()>> {
        let task = RealTimeCollectionTask::new(
            self.db.clone(),
            self.api.clone(),
            self.shutdown_tx.clone(),
            price_tick_tx,
        );

        tokio::spawn(task.run().map_err(SyncError::RealTimeCollection)).into()
    }

    async fn run_backfill(&self) -> Result<Never> {
        loop {
            let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

            self.spawn_history_state_update_handler(history_state_rx);

            self.run_price_history_task_backfill(Some(history_state_tx))
                .await?;

            self.status_manager
                .update(SyncStatusNotSynced::WaitingForResync.into());

            time::sleep(self.config.re_sync_history_interval).await;
        }
    }

    async fn run_live(&self, range: Duration) -> Result<Never> {
        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTick>(100);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(price_tick_tx.clone());

        // Ensure the database contains all entries from the last `range` duration
        // (e.g., if range is 1 hour, we need all data from the past hour available).

        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        self.spawn_history_state_update_handler(history_state_rx);

        self.run_price_history_task_live(Some(history_state_tx), range)
            .await?;

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncError::TaskJoin)??;

            return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
        }

        // Sync achieved

        self.status_manager.update(SyncStatus::Synced);

        let mut price_tick_rx = price_tick_tx.subscribe();

        loop {
            tokio::select! {
                rt_res = &mut real_time_collection_handle => {
                    rt_res.map_err(SyncError::TaskJoin)??;
                    return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
                }
                tick_res = price_tick_rx.recv() => {
                    let tick = tick_res.map_err(|e| SyncError::Generic(e.to_string()))?;
                    let _ = self.update_tx.send(tick.into());
                }
            }
        }
    }

    async fn run_full(&self) -> Result<Never> {
        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        self.spawn_history_state_update_handler(history_state_rx);

        self.run_price_history_task_backfill(Some(history_state_tx))
            .await?;

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTick>(100);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(price_tick_tx.clone());

        // Additional price history backfill to ensure overlap with real-time data

        self.run_price_history_task_backfill(None).await?;

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncError::TaskJoin)??;

            return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
        }

        // Sync achieved

        self.status_manager.update(SyncStatus::Synced);

        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_re_sync_timer = || Box::pin(time::sleep(self.config.re_sync_history_interval));
        let mut re_sync_timer = new_re_sync_timer();

        loop {
            tokio::select! {
                rt_res = &mut real_time_collection_handle => {
                    rt_res.map_err(SyncError::TaskJoin)??;
                    return Err(SyncError::UnexpectedRealTimeCollectionShutdown);
                }
                tick_res = price_tick_rx.recv() => {
                    match tick_res {
                        Ok(tick) => {
                            let _ = self.update_tx.send(tick.into());
                        },
                        Err(e) => return Err(SyncError::Generic(e.to_string()))
                    }
                }
                _ = &mut re_sync_timer => {
                    // Ensure the price history db remains relatively up-to-date
                    self.run_price_history_task_backfill(None).await?;
                    re_sync_timer = new_re_sync_timer();
                }
            }
        }
    }

    fn run_mode(&self) -> Pin<Box<dyn Future<Output = Result<Never>> + Send + '_>> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        match &self.mode {
            SyncMode::Backfill => Box::pin(self.run_backfill()),
            SyncMode::Live { range } => Box::pin(self.run_live(*range)),
            SyncMode::Full => Box::pin(self.run_full()),
        }
    }

    pub fn spawn_recovery_loop(self) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            loop {
                self.status_manager.update(SyncStatusNotSynced::Starting.into());

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    run_res = self.run_mode() => {
                        let Err(sync_error) = run_res;
                        self.status_manager.update(SyncStatusNotSynced::Failed(sync_error).into());
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(SyncStatusNotSynced::Failed(SyncError::ShutdownRecv(e)).into());
                        }
                        return;
                    }
                };

                self.status_manager.update(SyncStatusNotSynced::Restarting.into());

                // Handle shutdown signals while waiting for `restart_interval`

                let mut shutdown_rx = self.shutdown_tx.subscribe();
                tokio::select! {
                    _ = time::sleep(self.config.restart_interval) => {
                        // Continue with the restart loop
                    }
                    shutdown_res = shutdown_rx.recv() => {
                        if let Err(e) = shutdown_res {
                            self.status_manager.update(SyncStatusNotSynced::Failed(SyncError::ShutdownRecv(e)).into());
                        }
                        return;
                    }
                }
            }
        }).into()
    }
}
