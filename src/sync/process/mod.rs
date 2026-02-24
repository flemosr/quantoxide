use std::{pin::Pin, sync::Arc};

use chrono::Duration;
use futures::TryFutureExt;
use tokio::{
    sync::{broadcast, mpsc},
    time,
};

use lnm_sdk::{api_v2::WebSocketClient, api_v3::RestClient};

use crate::{
    db::{Database, models::PriceTickRow},
    util::{AbortOnDropHandle, Never},
};

use super::{
    config::{SyncConfig, SyncProcessConfig},
    engine::SyncModeInt,
    state::{SyncStatus, SyncStatusManager, SyncStatusNotSynced, SyncTransmitter},
};

pub(crate) mod error;
pub(crate) mod real_time_collection_task;
pub(crate) mod sync_funding_settlements_task;
pub(crate) mod sync_price_history_task;

use error::{Result, SyncProcessError, SyncProcessFatalError, SyncProcessRecoverableError};
use real_time_collection_task::RealTimeCollectionTask;
use sync_funding_settlements_task::{
    FundingSettlementsStateTransmitter, SyncFundingSettlementsTask,
    error::SyncFundingSettlementsError, funding_settlements_state::FundingSettlementsState,
};
use sync_price_history_task::{
    PriceHistoryStateTransmitter, SyncPriceHistoryTask, price_history_state::PriceHistoryState,
};

pub(super) struct SyncProcess {
    config: SyncProcessConfig,
    db: Arc<Database>,
    mode_int: SyncModeInt,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmitter,
}

impl SyncProcess {
    #[allow(clippy::too_many_arguments)]
    pub fn spawn(
        config: &SyncConfig,
        db: Arc<Database>,
        mode_int: SyncModeInt,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<SyncStatusManager>,
        update_tx: SyncTransmitter,
    ) -> AbortOnDropHandle<()> {
        let config = config.into();

        tokio::spawn(async move {
            let process = Self {
                config,
                db,
                mode_int,
                shutdown_tx,
                status_manager,
                update_tx,
            };

            process.recovery_loop().await
        })
        .into()
    }

    async fn recovery_loop(self) {
        self.status_manager
            .update(SyncStatusNotSynced::Starting.into());

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
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

            self.status_manager
                .update(SyncStatusNotSynced::Restarting.into());
        }
    }

    fn run_mode(&self) -> Pin<Box<dyn Future<Output = Result<Never>> + Send + '_>> {
        match &self.mode_int {
            SyncModeInt::Backfill { api_rest } => Box::pin(self.run_backfill(api_rest)),
            SyncModeInt::LiveNoLookback { api_ws } => Box::pin(self.run_live_no_lookback(api_ws)),
            SyncModeInt::LiveWithLookback {
                api_rest,
                api_ws,
                lookback,
            } => Box::pin(self.run_live_with_lookback(api_rest, api_ws, lookback.as_duration())),
            SyncModeInt::Full { api_rest, api_ws } => Box::pin(self.run_full(api_rest, api_ws)),
        }
    }

    async fn run_backfill(&self, api_rest: &Arc<RestClient>) -> Result<Never> {
        let mut flag_gaps_range = self.config.price_history_flag_gap_range();
        let mut flag_missing_range = self.config.funding_settlement_flag_missing_range();

        loop {
            self.status_manager
                .update(SyncStatusNotSynced::InProgress.into());

            // Backfill funding settlements

            let (funding_state_tx, funding_state_rx) =
                mpsc::channel::<FundingSettlementsState>(100);

            self.spawn_funding_state_update_handler(funding_state_rx);

            let _ = self
                .run_funding_settlements_task_backfill(
                    api_rest.clone(),
                    Some(funding_state_tx),
                    flag_missing_range,
                )
                .await?;

            // Backfill full historical price data

            let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

            self.spawn_history_state_update_handler(history_state_rx);

            self.run_price_history_task_backfill(
                api_rest.clone(),
                Some(history_state_tx),
                flag_gaps_range,
            )
            .await?;

            // Skip expensive gap-detection on subsequent re-sync cycles; interior gaps are
            // only scanned on the first pass after process (re)start.
            flag_gaps_range = None;
            flag_missing_range = None;

            self.status_manager
                .update(SyncStatusNotSynced::WaitingForResync.into());

            time::sleep(self.config.price_history_re_sync_interval()).await;
        }
    }

    async fn run_live_no_lookback(&self, api_ws: &Arc<WebSocketClient>) -> Result<Never> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        api_ws.reset().await;

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTickRow>(1_000);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(api_ws.clone(), price_tick_tx.clone());

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

            return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
        }

        // Handle updates and re-syncs

        let mut is_synced = false;
        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_tick_interval_timer =
            || Box::pin(time::sleep(self.config.live_price_tick_max_interval()));
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
                    if !is_synced {
                        self.status_manager.update(SyncStatus::Synced);
                        is_synced = true;
                    }

                    let _ = self.update_tx.send(tick.into());
                }
                _ = &mut tick_interval_timer => {
                    // Maximum interval between Price Ticks was exceeded
                    return Err(SyncProcessRecoverableError::MaxPriceTickIntevalExceeded(
                        self.config.live_price_tick_max_interval(),
                    )
                    .into());
                }
            }
        }
    }

    async fn run_live_with_lookback(
        &self,
        api_rest: &Arc<RestClient>,
        api_ws: &Arc<WebSocketClient>,
        lookback: Duration,
    ) -> Result<Never> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        api_ws.reset().await;

        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        self.spawn_history_state_update_handler(history_state_rx);

        self.run_price_history_task_live(api_rest.clone(), Some(history_state_tx), lookback)
            .await?;

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTickRow>(10_000);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(api_ws.clone(), price_tick_tx.clone());

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

            return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
        }

        // Handle updates and re-syncs

        let mut is_synced = false;
        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_re_sync_timer =
            || Box::pin(time::sleep(self.config.price_history_re_sync_interval()));
        let mut re_sync_timer = new_re_sync_timer();

        let new_tick_interval_timer =
            || Box::pin(time::sleep(self.config.live_price_tick_max_interval()));
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
                    if !is_synced {
                        self.status_manager.update(SyncStatus::Synced);
                        is_synced = true;
                    }

                    let _ = self.update_tx.send(tick.into());
                }
                _ = &mut re_sync_timer => {
                    // Ensure the OHLC candles DB remains up-to-date
                    self.run_price_history_task_live(api_rest.clone(), None, lookback).await?;
                    re_sync_timer = new_re_sync_timer();
                }
                _ = &mut tick_interval_timer => {
                    // Maximum interval between Price Ticks was exceeded
                    return Err(SyncProcessRecoverableError::MaxPriceTickIntevalExceeded(
                        self.config.live_price_tick_max_interval(),
                    )
                    .into());
                }
            }
        }
    }

    async fn run_full(
        &self,
        api_rest: &Arc<RestClient>,
        api_ws: &Arc<WebSocketClient>,
    ) -> Result<Never> {
        self.status_manager
            .update(SyncStatusNotSynced::InProgress.into());

        api_ws.reset().await;

        // Backfill funding settlements

        let (funding_state_tx, funding_state_rx) = mpsc::channel::<FundingSettlementsState>(100);

        self.spawn_funding_state_update_handler(funding_state_rx);

        let _ = self
            .run_funding_settlements_task_backfill(
                api_rest.clone(),
                Some(funding_state_tx),
                self.config.funding_settlement_flag_missing_range(),
            )
            .await?;

        // Backfill full historical price data

        let (history_state_tx, history_state_rx) = mpsc::channel::<PriceHistoryState>(100);

        self.spawn_history_state_update_handler(history_state_rx);

        self.run_price_history_task_backfill(
            api_rest.clone(),
            Some(history_state_tx),
            self.config.price_history_flag_gap_range(),
        )
        .await?;

        // Start to collect real-time data

        let (price_tick_tx, _) = broadcast::channel::<PriceTickRow>(10_000);

        let mut real_time_collection_handle =
            self.spawn_real_time_collection_task(api_ws.clone(), price_tick_tx.clone());

        if real_time_collection_handle.is_finished() {
            real_time_collection_handle
                .await
                .map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

            return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
        }

        // Handle updates and re-syncs

        let mut is_synced = false;
        let mut price_tick_rx = price_tick_tx.subscribe();

        let new_re_sync_timer =
            || Box::pin(time::sleep(self.config.price_history_re_sync_interval()));
        let mut re_sync_timer = new_re_sync_timer();

        let new_tick_interval_timer =
            || Box::pin(time::sleep(self.config.live_price_tick_max_interval()));
        let mut tick_interval_timer = new_tick_interval_timer();

        let retry_interval = self.config.funding_settlement_retry_interval();
        let new_funding_timer = |synced: bool| -> Pin<Box<time::Sleep>> {
            if synced {
                SyncFundingSettlementsTask::next_funding_timer()
            } else {
                Box::pin(time::sleep(retry_interval))
            }
        };
        let mut funding_timer = new_funding_timer(true);

        loop {
            tokio::select! {
                rt_res = &mut real_time_collection_handle => {
                    rt_res.map_err(SyncProcessRecoverableError::RealTimeCollectionTaskJoin)??;

                    return Err(SyncProcessRecoverableError::UnexpectedRealTimeCollectionShutdown.into());
                }
                tick_res = price_tick_rx.recv() => {
                    tick_interval_timer = new_tick_interval_timer();

                    let tick = tick_res.map_err(SyncProcessRecoverableError::PriceTickRecv)?;
                    if !is_synced {
                        self.status_manager.update(SyncStatus::Synced);
                        is_synced = true;
                    }

                    let _ = self.update_tx.send(tick.into());
                }
                _ = &mut re_sync_timer => {
                    // Ensure the OHLC candles DB remains up-to-date
                    self.run_price_history_task_backfill(api_rest.clone(), None, None).await?;
                    re_sync_timer = new_re_sync_timer();
                }
                _ = &mut funding_timer => {
                    let synced = self.run_funding_settlements_task_backfill(api_rest.clone(), None, None).await?;
                    funding_timer = new_funding_timer(synced);
                }
                _ = &mut tick_interval_timer => {
                    // Maximum interval between Price Ticks was exceeded
                    return Err(SyncProcessRecoverableError::MaxPriceTickIntevalExceeded(
                        self.config.live_price_tick_max_interval(),
                    )
                    .into());
                }
            }
        }
    }

    async fn run_price_history_task_backfill(
        &self,
        api_rest: Arc<RestClient>,
        history_state_tx: Option<PriceHistoryStateTransmitter>,
        flag_gaps_range: Option<Duration>,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(&self.config, self.db.clone(), api_rest, history_state_tx)
            .backfill(flag_gaps_range)
            .await
            .map_err(|e| SyncProcessRecoverableError::SyncPriceHistory(e).into())
    }

    async fn run_price_history_task_live(
        &self,
        api_rest: Arc<RestClient>,
        history_state_tx: Option<PriceHistoryStateTransmitter>,
        lookback: Duration,
    ) -> Result<()> {
        SyncPriceHistoryTask::new(&self.config, self.db.clone(), api_rest, history_state_tx)
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

    async fn run_funding_settlements_task_backfill(
        &self,
        api_rest: Arc<RestClient>,
        funding_state_tx: Option<FundingSettlementsStateTransmitter>,
        flag_missing_range: Option<Duration>,
    ) -> Result<bool> {
        SyncFundingSettlementsTask::new(&self.config, self.db.clone(), api_rest, funding_state_tx)
            .backfill(flag_missing_range)
            .await
            .map_err(Self::map_funding_settlements_error)
    }

    fn map_funding_settlements_error(e: SyncFundingSettlementsError) -> SyncProcessError {
        match e {
            SyncFundingSettlementsError::InvalidSettlementTime { time } => {
                SyncProcessFatalError::InvalidFundingSettlementTime { time }.into()
            }
            SyncFundingSettlementsError::UnreachableMissingSettlement { time, reach } => {
                SyncProcessFatalError::UnreachableMissingFundingSettlement { time, reach }.into()
            }
            other => SyncProcessRecoverableError::SyncFundingSettlements(other).into(),
        }
    }

    fn spawn_funding_state_update_handler(
        &self,
        mut funding_state_rx: mpsc::Receiver<FundingSettlementsState>,
    ) {
        let update_tx = self.update_tx.clone();
        tokio::spawn(async move {
            while let Some(new_state) = funding_state_rx.recv().await {
                let _ = update_tx.send(new_state.into());
            }
        });
    }

    fn spawn_real_time_collection_task(
        &self,
        api_ws: Arc<WebSocketClient>,
        price_tick_tx: broadcast::Sender<PriceTickRow>,
    ) -> AbortOnDropHandle<Result<()>> {
        let task = RealTimeCollectionTask::new(
            self.db.clone(),
            api_ws,
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
