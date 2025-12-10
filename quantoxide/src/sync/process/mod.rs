use std::{future, pin::Pin, sync::Arc};

use futures::TryFutureExt;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};

use lnm_sdk::{api_v2::WebSocketClient, api_v3::RestClient};

use crate::{
    db::{Database, models::PriceTickRow},
    shared::LookbackPeriod,
    util::{AbortOnDropHandle, Never},
};

use super::{
    config::{SyncConfig, SyncProcessConfig},
    engine::SyncMode,
    state::{SyncStatus, SyncStatusManager, SyncStatusNotSynced, SyncTransmiter},
};

pub(crate) mod error;
pub(crate) mod real_time_collection_task;
pub(crate) mod sync_price_history_task;

use error::{Result, SyncProcessError, SyncProcessFatalError, SyncProcessRecoverableError};
use real_time_collection_task::RealTimeCollectionTask;
use sync_price_history_task::{
    PriceHistoryStateTransmiter, SyncPriceHistoryTask, price_history_state::PriceHistoryState,
};

pub(super) struct SyncProcess {
    config: SyncProcessConfig,
    db: Arc<Database>,
    api_rest: Arc<RestClient>,
    api_ws: Arc<WebSocketClient>,
    mode: SyncMode,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmiter,
}

impl SyncProcess {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        config: &SyncConfig,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
        mode: SyncMode,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<SyncStatusManager>,
        update_tx: SyncTransmiter,
    ) -> AbortOnDropHandle<()> {
        let config = config.into();

        tokio::spawn(async move {
            let process = Self {
                config,
                db,
                api_rest,
                api_ws,
                mode,
                shutdown_tx,
                status_manager,
                update_tx,
            };

            process.recovery_loop().await
        })
        .into()
    }

    async fn recovery_loop(self) {
        loop {
            self.status_manager
                .update(SyncStatusNotSynced::Starting.into());

            self.api_ws.reset().await;

            let mut shutdown_rx = self.shutdown_tx.subscribe();

            let sync_process_error = tokio::select! {
                Err(sync_error) = self.run_mode() => sync_error,
                shutdown_res = shutdown_rx.recv() => {
                    let Err(e) = shutdown_res else {
                        // Shutdown signal received
                        return;
                    };

                    SyncProcessFatalError::ShutdownSignalRecv(e).into()
                }
            };

            match sync_process_error {
                SyncProcessError::Fatal(err) => {
                    self.status_manager.update(err.into());
                    return;
                }
                SyncProcessError::Recoverable(err) => {
                    self.status_manager.update(err.into());
                }
            }

            self.status_manager
                .update(SyncStatusNotSynced::Restarting.into());

            // Handle shutdown signals while waiting for `restart_interval`

            tokio::select! {
                _ = time::sleep(self.config.restart_interval()) => {} // Loop restarts
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        let status = SyncProcessFatalError::ShutdownSignalRecv(e).into();
                        self.status_manager.update(status);
                    }
                    return;
                }
            }
        }
    }

    fn run_mode(&self) -> Pin<Box<dyn Future<Output = Result<Never>> + Send + '_>> {
        match &self.mode {
            SyncMode::Backfill => Box::pin(self.run_backfill()),
            SyncMode::Live(range) => Box::pin(self.run_live(*range)),
            SyncMode::Full => Box::pin(self.run_full()),
        }
    }

    async fn run_backfill(&self) -> Result<Never> {
        loop {
            self.status_manager
                .update(SyncStatusNotSynced::InProgress.into());

            // Backfill full historical price data

            let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

            self.spawn_history_state_update_handler(history_state_rx);

            self.run_price_history_task_backfill(Some(history_state_tx))
                .await?;

            self.status_manager
                .update(SyncStatusNotSynced::WaitingForResync.into());

            time::sleep(self.config.re_sync_history_interval()).await;
        }
    }

    async fn run_live(&self, lookback: Option<LookbackPeriod>) -> Result<Never> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        if let Some(ref range) = lookback {
            let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

            self.spawn_history_state_update_handler(history_state_rx);

            self.run_price_history_task_live(Some(history_state_tx), *range)
                .await?;
        }

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTickRow>(1000);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(price_tick_tx.clone());

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

            return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
        }

        self.status_manager.update(SyncStatus::Synced);

        // Handle updates and re-syncs

        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_re_sync_timer = || Box::pin(time::sleep(self.config.re_sync_history_interval()));
        let mut re_sync_timer: Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            if lookback.is_some() {
                new_re_sync_timer()
            } else {
                Box::pin(future::pending::<()>())
            };
        let new_tick_interval_timer = || Box::pin(time::sleep(self.config.max_tick_interval()));
        let mut tick_interval_timer = new_tick_interval_timer();

        loop {
            tokio::select! {
                rt_res = &mut real_time_collection_handle => {
                    rt_res.map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

                    return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
                }
                tick_res = price_tick_rx.recv() => {
                    tick_interval_timer = new_tick_interval_timer();
                    let tick = tick_res.map_err(SyncProcessRecoverableError::PriceTickRecv)?;
                    let _ = self.update_tx.send(tick.into());
                }
                _ = &mut re_sync_timer => {
                    // Ensure the OHLC candles DB remains up-to-date
                    let range = lookback.expect("must be `Some` from `re_sync_timer` definition");
                    self.run_price_history_task_live(None, range).await?;
                    re_sync_timer = new_re_sync_timer();
                }
                _ = &mut tick_interval_timer => {
                    // Maximum interval between Price Ticks was exceeded
                    return Err(SyncProcessRecoverableError::MaxPriceTickIntevalExceeded(
                        self.config.max_tick_interval(),
                    )
                    .into());
                }
            }
        }
    }

    async fn run_full(&self) -> Result<Never> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        // Backfill full historical price data

        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        self.spawn_history_state_update_handler(history_state_rx);

        self.run_price_history_task_backfill(Some(history_state_tx))
            .await?;

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTickRow>(1000);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(price_tick_tx.clone());

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

            return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
        }

        self.status_manager.update(SyncStatus::Synced);

        // Handle updates and re-syncs

        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_re_sync_timer = || Box::pin(time::sleep(self.config.re_sync_history_interval()));
        let mut re_sync_timer = new_re_sync_timer();
        let new_tick_interval_timer = || Box::pin(time::sleep(self.config.max_tick_interval()));
        let mut tick_interval_timer = new_tick_interval_timer();

        loop {
            tokio::select! {
                rt_res = &mut real_time_collection_handle => {
                    rt_res.map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

                    return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
                }
                tick_res = price_tick_rx.recv() => {
                    match tick_res {
                        Ok(tick) => {
                            tick_interval_timer = new_tick_interval_timer();
                            let _ = self.update_tx.send(tick.into());
                        },
                        Err(e) => return Err(SyncProcessRecoverableError::PriceTickRecv(e).into())
                    }
                }
                _ = &mut re_sync_timer => {
                    // Ensure the OHLC candles DB remains up-to-date
                    self.run_price_history_task_backfill(None).await?;
                    re_sync_timer = new_re_sync_timer();
                }
                _ = &mut tick_interval_timer => {
                    // Maximum interval between Price Ticks was exceeded
                    return Err(SyncProcessRecoverableError::MaxPriceTickIntevalExceeded(
                        self.config.max_tick_interval(),
                    )
                    .into());
                }
            }
        }
    }

    async fn run_price_history_task_backfill(
        &self,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(
            &self.config,
            self.db.clone(),
            self.api_rest.clone(),
            history_state_tx,
        )
        .backfill()
        .await
        .map_err(|e| SyncProcessRecoverableError::SyncPriceHistory(e).into())
    }

    async fn run_price_history_task_live(
        &self,
        history_state_tx: Option<PriceHistoryStateTransmiter>,
        lookback: LookbackPeriod,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(
            &self.config,
            self.db.clone(),
            self.api_rest.clone(),
            history_state_tx,
        )
        .live(lookback)
        .await
        .map_err(|e| SyncProcessRecoverableError::SyncPriceHistory(e).into())
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
        price_tick_tx: broadcast::Sender<PriceTickRow>,
    ) -> AbortOnDropHandle<Result<()>> {
        let task = RealTimeCollectionTask::new(
            self.db.clone(),
            self.api_ws.clone(),
            self.shutdown_tx.clone(),
            price_tick_tx,
        );

        tokio::spawn(
            task.run()
                .map_err(|e| SyncProcessRecoverableError::RealTimeCollection(e).into()),
        )
        .into()
    }
}
