use std::{
    num::NonZeroU64,
    result,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures::future;
use tokio::{
    sync::broadcast::{
        self,
        error::{RecvError, TryRecvError},
    },
    time,
};
use uuid::Uuid;

use lnm_sdk::api_v3::{
    RestClient,
    models::{Leverage, PercentageCapped, Price, TradeSide, TradeSize, trade_util},
};

use crate::{
    db::Database,
    sync::{SyncReader, SyncReceiver, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, Never},
};

use super::{
    super::{
        core::{Stoploss, TradeExecutor, TradingState},
        error::TradeExecutorResult,
    },
    config::LiveTradeExecutorConfig,
};

pub(crate) mod error;
pub(in crate::trade) mod state;
pub(in crate::trade) mod update;

use error::{
    ExecutorActionError, ExecutorActionResult, ExecutorProcessFatalError,
    ExecutorProcessFatalResult, ExecutorProcessRecoverableError, LiveTradeExecutorError,
    LiveTradeExecutorResult,
};
use state::{
    LiveTradeExecutorState, LiveTradeExecutorStateManager, LiveTradeExecutorStatusNotReady,
    live_trading_session::{LiveTradingSession, TradingSessionTTL},
};
use update::{
    LiveTradeExecutorReceiver, LiveTradeExecutorTransmiter, LiveTradeExecutorUpdate,
    WrappedRestClient,
};

/// Live trade executor implementing the [`TradeExecutor`] trait for real-time trade execution on
/// an exchange. Manages trading sessions, validates operations, and maintains trade state.
pub struct LiveTradeExecutor {
    config: LiveTradeExecutorConfig,
    db: Arc<Database>,
    api: WrappedRestClient,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
}

impl LiveTradeExecutor {
    fn new(
        config: LiveTradeExecutorConfig,
        db: Arc<Database>,
        api: WrappedRestClient,
        update_tx: LiveTradeExecutorTransmiter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        handle: AbortOnDropHandle<()>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            db,
            api,
            update_tx,
            state_manager,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Creates a new [`LiveTradeExecutorReceiver`] for subscribing to trade executor updates
    /// including orders and closed trades.
    pub fn update_receiver(&self) -> LiveTradeExecutorReceiver {
        self.update_tx.subscribe()
    }

    pub(in crate::trade) async fn state_snapshot(&self) -> LiveTradeExecutorState {
        self.state_manager.snapshot().await
    }

    async fn get_estimated_market_price(&self) -> ExecutorActionResult<Price> {
        // Assuming that the db is up-to-date

        let (_, last_entry_price) = self
            .db
            .price_ticks
            .get_latest_entry()
            .await?
            .ok_or(ExecutorActionError::DbIsEmpty)?;

        let price =
            Price::round(last_entry_price).map_err(ExecutorActionError::InvalidMarketPrice)?;

        Ok(price)
    }

    async fn open_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> ExecutorActionResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let market_price = self.get_estimated_market_price().await?;

        let (stoploss_price, trade_tsl) = match stoploss {
            Some(stoploss) => {
                let (stoploss_price, tsl) = stoploss
                    .evaluate(
                        self.config.trailing_stoploss_step_size(),
                        side,
                        market_price,
                    )
                    .map_err(ExecutorActionError::StoplossEvaluation)?;
                (Some(stoploss_price), tsl)
            }
            None => (None, None),
        };

        let (_, margin, _, opening_fee, closing_fee_reserved) =
            trade_util::evaluate_open_trade_params(
                side,
                size,
                leverage,
                market_price,
                stoploss_price,
                takeprofit,
                self.config.trade_estimated_fee(),
            )
            .map_err(ExecutorActionError::InvalidTradeParams)?;

        let trading_session = locked_ready_state.trading_session();

        let balance_diff = margin.as_u64() + opening_fee + closing_fee_reserved;
        if balance_diff > trading_session.balance() {
            return Err(ExecutorActionError::BalanceTooLow);
        }

        let max_qtd = self.config.trade_max_running_qtd();
        if trading_session.running_map().len() == max_qtd {
            return Err(ExecutorActionError::MaxRunningTradesReached { max_qtd });
        }

        let trade = self
            .api
            .create_new_trade(side, size, leverage, stoploss_price, takeprofit)
            .await?;

        self.db
            .running_trades
            .add_running_trade(trade.id(), trade_tsl)
            .await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.register_running_trade(trade, trade_tsl, true)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn close_trades(&self, side: TradeSide) -> ExecutorActionResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        let mut to_close = Vec::new();

        for (trade, _) in new_trading_session.running_map().trades_desc() {
            if trade.side() == side {
                to_close.push(trade.id());
            }
        }

        // Process in batches of 1
        for chunk in to_close.chunks(1) {
            let close_futures = chunk
                .iter()
                .map(|&trade_id| self.api.close_trade(trade_id))
                .collect::<Vec<_>>();

            let closed_trades = future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()?;

            new_trading_session.close_trades(&closed_trades)?;

            let mut closed_ids = Vec::with_capacity(closed_trades.len());

            for closed_trade in closed_trades {
                closed_ids.push(closed_trade.id());

                // Ignore no-receiver errors
                let _ = self
                    .update_tx
                    .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
            }

            self.db
                .running_trades
                .remove_running_trades(closed_ids.as_slice())
                .await?;
        }

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`LiveTradeExecutor` mutex can't be poisoned")
            .take()
    }

    /// Shuts down the trade executor and optionally closes all running trades. This method can only
    /// be called once per executor instance.
    pub async fn shutdown(&self) -> LiveTradeExecutorResult<()> {
        let Some(handle) = self.try_consume_handle() else {
            return Err(LiveTradeExecutorError::ExecutorProcessAlreadyConsumed);
        };

        if handle.is_finished() {
            let status = self.state_manager.snapshot().await.status().clone();
            return Err(LiveTradeExecutorError::ExecutorProcessAlreadyTerminated(
                status,
            ));
        }

        self.state_manager
            .update_status_not_ready(LiveTradeExecutorStatusNotReady::ShutdownInitiated)
            .await;

        handle.abort();

        if !self.config.shutdown_clean_up_trades() {
            self.state_manager
                .update_status_not_ready(LiveTradeExecutorStatusNotReady::Shutdown)
                .await;
            return Ok(());
        }

        let (res, new_status) = if self.state_manager.has_registered_running_trades().await {
            // Perform clean up if there are running trades registered

            match futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades())
                .map_err(ExecutorProcessFatalError::FailedToCloseTradesOnShutdown)
            {
                Ok(_) => (Ok(()), LiveTradeExecutorStatusNotReady::Shutdown),
                Err(e) => {
                    let e_ref = Arc::new(e);

                    (
                        Err(LiveTradeExecutorError::ExecutorShutdownFailed(
                            e_ref.clone(),
                        )),
                        LiveTradeExecutorStatusNotReady::Terminated(e_ref),
                    )
                }
            }
        } else {
            (Ok(()), LiveTradeExecutorStatusNotReady::Shutdown)
        };

        self.state_manager.update_status_not_ready(new_status).await;

        res
    }
}

#[async_trait]
impl TradeExecutor for LiveTradeExecutor {
    async fn open_long(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> TradeExecutorResult<()> {
        self.open_trade(TradeSide::Buy, size, leverage, stoploss, takeprofit)
            .await?;
        Ok(())
    }

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
    ) -> TradeExecutorResult<()> {
        self.open_trade(TradeSide::Sell, size, leverage, stoploss, takeprofit)
            .await?;
        Ok(())
    }

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        let Some((current_trade, _)) = trading_session.running_map().get_by_id(trade_id) else {
            return Err(ExecutorActionError::TradeNotRegistered { trade_id })?;
        };

        let max_amount = current_trade.est_max_additional_margin();
        if amount.get() > max_amount {
            return Err(ExecutorActionError::MarginAmountExceedsMaxForTrade {
                amount,
                max_amount,
            })?;
        }

        let balance = trading_session.balance();
        if amount.get() >= balance {
            return Err(ExecutorActionError::BalanceTooLow)?;
        }

        let updated_trade = self.api.add_margin(trade_id, amount).await?;

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> TradeExecutorResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        let Some((current_trade, _)) = trading_session.running_map().get_by_id(trade_id) else {
            return Err(ExecutorActionError::TradeNotRegistered { trade_id })?;
        };

        let market_price = self.get_estimated_market_price().await?;

        let max_cash_in = current_trade.est_max_cash_in(market_price);
        if amount.get() > max_cash_in {
            return Err(ExecutorActionError::CashInAmountExceedsMaxForTrade {
                amount,
                max_cash_in,
            })?;
        }

        let updated_trade = self.api.cash_in(trade_id, amount).await?;

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn close_trade(&self, trade_id: Uuid) -> TradeExecutorResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        if !trading_session.running_map().contains(&trade_id) {
            return Err(ExecutorActionError::TradeNotRegistered { trade_id })?;
        };

        let closed_trade = self.api.close_trade(trade_id).await?;

        self.db
            .running_trades
            .remove_running_trades(&[closed_trade.id()])
            .await
            .map_err(ExecutorActionError::Db)?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.close_trade(&closed_trade)?;

        // Ignore no-receiver errors
        let _ = self
            .update_tx
            .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn close_longs(&self) -> TradeExecutorResult<()> {
        self.close_trades(TradeSide::Buy).await?;
        Ok(())
    }

    async fn close_shorts(&self) -> TradeExecutorResult<()> {
        self.close_trades(TradeSide::Sell).await?;
        Ok(())
    }

    async fn close_all(&self) -> TradeExecutorResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        let (_, closed_trades) =
            futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades())?;

        new_trading_session.close_trades(&closed_trades)?;

        for closed_trade in closed_trades {
            // Ignore no-receiver errors
            let _ = self
                .update_tx
                .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
        }

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn trading_state(&self) -> TradeExecutorResult<TradingState> {
        let trading_session = self
            .state_manager
            .try_lock_ready_state()
            .await?
            .trading_session()
            .to_owned();

        Ok(TradingState::from(trading_session))
    }
}

/// Launcher for initializing and starting a live trade executor. Validates configuration and API
/// credentials before launching the executor process.
pub struct LiveTradeExecutorLauncher {
    config: LiveTradeExecutorConfig,
    db: Arc<Database>,
    api_rest: WrappedRestClient,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    sync_rx: SyncReceiver,
}

impl LiveTradeExecutorLauncher {
    /// Creates a new launcher for the live trade executor. Validates that API credentials are set
    /// and that the sync engine has an active live feed.
    pub fn new(
        config: impl Into<LiveTradeExecutorConfig>,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        sync_reader: Arc<dyn SyncReader>,
    ) -> LiveTradeExecutorResult<Self> {
        if !api_rest.has_credentials {
            return Err(LiveTradeExecutorError::ApiCredentialsNotSet);
        }

        let sync_mode = sync_reader.mode();
        if !sync_mode.live_feed_active() {
            return Err(LiveTradeExecutorError::SyncEngineLiveFeedInactive(
                sync_mode,
            ));
        }

        let (update_tx, _) = broadcast::channel::<LiveTradeExecutorUpdate>(1_000);

        let api_rest = WrappedRestClient::new(api_rest, update_tx.clone());

        let state_manager = LiveTradeExecutorStateManager::new(update_tx.clone());

        Ok(Self {
            config: config.into(),
            db,
            api_rest,
            update_tx,
            state_manager,
            sync_rx: sync_reader.update_receiver(),
        })
    }

    /// Creates a new [`LiveTradeExecutorReceiver`] for subscribing to trade executor updates.
    pub fn update_receiver(&self) -> LiveTradeExecutorReceiver {
        self.update_tx.subscribe()
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_sync_processor(
        startup_recover_trades: bool,
        trade_tsl_step_size: PercentageCapped,
        session_refresh_offset: TradingSessionTTL,
        trading_session_refresh_interval: time::Duration,
        db: Arc<Database>,
        api: WrappedRestClient,
        update_tx: LiveTradeExecutorTransmiter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        sync_rx: SyncReceiver,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let refresh_trading_session = async || {
                let locked_state = state_manager.lock_state().await;

                let current_trading_session = match locked_state.trading_session().cloned() {
                    Some(old_trading_session) if !old_trading_session.is_expired() => {
                        let mut restored_trading_session = old_trading_session;

                        match restored_trading_session
                            .reevaluate(db.as_ref(), &api)
                            .await
                            .map_err(ExecutorProcessRecoverableError::LiveTradeSessionEvaluation)
                        {
                            Ok(closed_trades) => {
                                for closed_trade in closed_trades.into_iter() {
                                    // Ignore no-receiver errors
                                    let _ = update_tx
                                        .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
                                }
                            }
                            Err(e) => {
                                let new_status_not_ready =
                                    LiveTradeExecutorStatusNotReady::Failed(Arc::new(e));
                                locked_state.update_status_not_ready(new_status_not_ready);
                                return;
                            }
                        }

                        restored_trading_session
                    }
                    prev_session => {
                        match LiveTradingSession::new(
                            startup_recover_trades,
                            trade_tsl_step_size,
                            session_refresh_offset,
                            db.as_ref(),
                            &api,
                            prev_session,
                        )
                        .await
                        .map_err(ExecutorProcessRecoverableError::LiveTradeSessionEvaluation)
                        {
                            Ok(new_trading_session) => new_trading_session,
                            Err(e) => {
                                locked_state.update_status_not_ready(
                                    LiveTradeExecutorStatusNotReady::Failed(Arc::new(e)),
                                );
                                return;
                            }
                        }
                    }
                };

                locked_state.update_status_ready(current_trading_session);
            };

            let handler = async || -> ExecutorProcessFatalResult<Never> {
                let mut sync_rx = sync_rx;
                let mut should_refresh = false;
                let new_refresh_timer = || Box::pin(time::sleep(trading_session_refresh_interval));
                let mut refresh_timer = new_refresh_timer();

                loop {
                    tokio::select! {
                        recv_result = sync_rx.recv() => {
                            match recv_result {
                                Ok(sync_update) => match sync_update {
                                    SyncUpdate::Status(sync_status) => match sync_status {
                                        SyncStatus::NotSynced(sync_status_not_synced) => {
                                            let new_status_not_ready =
                                                LiveTradeExecutorStatusNotReady::WaitingForSync(
                                                    sync_status_not_synced,
                                                );
                                            state_manager
                                                .update_status_not_ready(new_status_not_ready)
                                                .await;
                                        }
                                        SyncStatus::Terminated(err) => {
                                            return Err(ExecutorProcessFatalError::SyncProcessTerminated(
                                                err,
                                            ));
                                        }
                                        SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                            return Err(ExecutorProcessFatalError::SyncProcessShutdown);
                                        }
                                        SyncStatus::Synced => should_refresh = true,
                                    },
                                    SyncUpdate::PriceTick(_) => should_refresh = true,
                                    SyncUpdate::PriceHistoryState(_) => {}
                                },
                                Err(RecvError::Lagged(skipped)) => {
                                    state_manager
                                        .update_status_not_ready(LiveTradeExecutorStatusNotReady::Failed(
                                            Arc::new(ExecutorProcessRecoverableError::SyncRecvLagged {
                                                skipped,
                                            }),
                                        ))
                                        .await;

                                    // Drain all remaining messages to catch up to current state
                                    loop {
                                        match sync_rx.try_recv() {
                                            Ok(_) | Err(TryRecvError::Lagged(_)) => continue,
                                            Err(TryRecvError::Empty) => break,
                                            Err(TryRecvError::Closed) => {
                                                return Err(ExecutorProcessFatalError::SyncRecvClosed);
                                            }
                                        }
                                    }
                                }
                                Err(RecvError::Closed) => {
                                    return Err(ExecutorProcessFatalError::SyncRecvClosed);
                                }
                            }
                        }
                        _ = &mut refresh_timer => {
                            if should_refresh {
                                should_refresh = false;
                                refresh_trading_session().await;
                            }
                            refresh_timer = new_refresh_timer();
                        }
                    }
                }
            };

            let Err(e) = handler().await;

            let new_status_not_ready = LiveTradeExecutorStatusNotReady::Terminated(Arc::new(e));
            state_manager
                .update_status_not_ready(new_status_not_ready)
                .await;
        })
        .into()
    }

    /// Launches the live trade executor after optionally cleaning up existing trades. Returns a
    /// running executor instance.
    pub async fn launch(self) -> LiveTradeExecutorResult<Arc<LiveTradeExecutor>> {
        if self.config.startup_clean_up_trades() {
            let (_, _) = futures::try_join!(
                self.api_rest.cancel_all_trades(),
                self.api_rest.close_all_trades()
            )
            .map_err(LiveTradeExecutorError::LaunchCleanUp)?;
        }

        let handle = Self::spawn_sync_processor(
            self.config.startup_recover_trades(),
            self.config.trailing_stoploss_step_size(),
            self.config.trading_session_ttl(),
            self.config.trading_session_refresh_interval(),
            self.db.clone(),
            self.api_rest.clone(),
            self.update_tx.clone(),
            self.state_manager.clone(),
            self.sync_rx,
        );

        Ok(LiveTradeExecutor::new(
            self.config,
            self.db,
            self.api_rest,
            self.update_tx,
            self.state_manager,
            handle,
        ))
    }
}
