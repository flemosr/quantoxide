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

use lnm_sdk::rest::v3::{
    RestClient,
    models::{
        ClientId, CrossLeverage, Leverage, PercentageCapped, Price, TradeSide, TradeSize,
        trade_util,
    },
};

use crate::{
    db::Database,
    sync::{SyncReader, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, Never},
};

use super::{
    super::{
        core::{
            CrossOrderRequest, CrossPositionCore, IsolatedOrderRequest, Stoploss, TradeExecutor,
            TradingState,
        },
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
    live_trading_session::LiveTradingSession,
};
use update::{
    LiveTradeExecutorReceiver, LiveTradeExecutorTransmitter, LiveTradeExecutorUpdate,
    WrappedRestClient,
};

/// Live trade executor implementing the [`TradeExecutor`] trait for real-time trade execution on
/// an exchange. Manages trading sessions, validates operations, and maintains trade state.
pub struct LiveTradeExecutor {
    config: LiveTradeExecutorConfig,
    db: Arc<Database>,
    api: WrappedRestClient,
    account_id: Uuid,
    update_tx: LiveTradeExecutorTransmitter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
}

impl LiveTradeExecutor {
    fn new(
        config: LiveTradeExecutorConfig,
        db: Arc<Database>,
        api: WrappedRestClient,
        account_id: Uuid,
        update_tx: LiveTradeExecutorTransmitter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        handle: AbortOnDropHandle<()>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config,
            db,
            api,
            account_id,
            update_tx,
            state_manager,
            handle: Mutex::new(Some(handle)),
        })
    }

    /// Creates a new [`LiveTradeExecutorReceiver`] for subscribing to trade executor updates
    /// including executor actions, status changes, trading state, and closed trades.
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

    async fn execute_isolated_order(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Stoploss>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    ) -> ExecutorActionResult<Uuid> {
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
            .isolated_order(side, size, leverage, stoploss_price, takeprofit, client_id)
            .await?;

        let trade_id = trade.id();

        self.db
            .running_trades
            .add_running_trade(self.account_id, trade_id, trade_tsl)
            .await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.register_running_trade(trade, trade_tsl, true)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(trade_id)
    }

    async fn close_trades(&self, side: TradeSide) -> ExecutorActionResult<Vec<Uuid>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        let mut to_close = Vec::new();

        for (trade, _) in new_trading_session.running_map().trades_desc() {
            if trade.side() == side {
                to_close.push(trade.id());
            }
        }

        let mut all_closed_ids = Vec::with_capacity(to_close.len());

        // Process in batches of 3
        for chunk in to_close.chunks(3) {
            let close_futures = chunk
                .iter()
                .map(|&trade_id| self.api.isolated_order_close(trade_id))
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
                .remove_running_trades(self.account_id, closed_ids.as_slice())
                .await?;

            all_closed_ids.extend(closed_ids);
        }

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(all_closed_ids)
    }

    async fn clean_up_all_api_trades(api: &WrappedRestClient) -> ExecutorActionResult<()> {
        let (_, _, _, _) = futures::try_join!(
            api.isolated_order_cancel_all(),
            api.isolated_order_close_all(),
            api.cross_cancel_all_orders(),
            api.cross_order_close_position()
        )?;

        Ok(())
    }

    async fn clean_up_all_trades(&self) -> ExecutorActionResult<()> {
        Self::clean_up_all_api_trades(&self.api).await
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

        if !self.config.shutdown_clean_up_trades()
            || !self.state_manager.has_registered_running_trades().await
        {
            self.state_manager
                .update_status_not_ready(LiveTradeExecutorStatusNotReady::Shutdown)
                .await;
            return Ok(());
        }

        let (res, new_status) = match self
            .clean_up_all_trades()
            .await
            .map_err(ExecutorProcessFatalError::FailedToCloseTradesOnShutdown)
        {
            Ok(()) => (Ok(()), LiveTradeExecutorStatusNotReady::Shutdown),
            Err(e) => {
                let e_ref = Arc::new(e);

                (
                    Err(LiveTradeExecutorError::ExecutorShutdownFailed(
                        e_ref.clone(),
                    )),
                    LiveTradeExecutorStatusNotReady::Terminated(e_ref),
                )
            }
        };

        self.state_manager.update_status_not_ready(new_status).await;

        res
    }
}

#[async_trait]
impl TradeExecutor for LiveTradeExecutor {
    async fn isolated_order(&self, request: IsolatedOrderRequest) -> TradeExecutorResult<Uuid> {
        let (side, size, leverage, stoploss, takeprofit, client_id) =
            request.into_isolated_order_parts();

        Ok(self
            .execute_isolated_order(side, size, leverage, stoploss, takeprofit, client_id)
            .await?)
    }

    async fn isolated_trade_add_margin(
        &self,
        trade_id: Uuid,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<()> {
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

        let updated_trade = self.api.isolated_trade_add_margin(trade_id, amount).await?;

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn isolated_trade_cash_in(
        &self,
        trade_id: Uuid,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<()> {
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

        let updated_trade = self.api.isolated_trade_cash_in(trade_id, amount).await?;

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn isolated_order_close(&self, trade_id: Uuid) -> TradeExecutorResult<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        if !trading_session.running_map().contains(&trade_id) {
            return Err(ExecutorActionError::TradeNotRegistered { trade_id })?;
        };

        let closed_trade = self.api.isolated_order_close(trade_id).await?;

        self.db
            .running_trades
            .remove_running_trades(self.account_id, &[closed_trade.id()])
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

    async fn isolated_order_close_longs(&self) -> TradeExecutorResult<Vec<Uuid>> {
        Ok(self.close_trades(TradeSide::Buy).await?)
    }

    async fn isolated_order_close_shorts(&self) -> TradeExecutorResult<Vec<Uuid>> {
        Ok(self.close_trades(TradeSide::Sell).await?)
    }

    async fn isolated_order_close_all(&self) -> TradeExecutorResult<Vec<Uuid>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        let (_, closed_trades) = futures::try_join!(
            self.api.isolated_order_cancel_all(),
            self.api.isolated_order_close_all()
        )?;

        new_trading_session.close_trades(&closed_trades)?;

        let mut closed_ids = Vec::with_capacity(closed_trades.len());

        for closed_trade in closed_trades {
            closed_ids.push(closed_trade.id());

            // Ignore no-receiver errors
            let _ = self
                .update_tx
                .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
        }

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(closed_ids)
    }

    async fn cross_deposit(
        &self,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;
        let trading_session = locked_ready_state.trading_session();

        if amount.get() > trading_session.balance() {
            return Err(ExecutorActionError::BalanceTooLow)?;
        }

        let cross_position_raw = self.api.cross_deposit(amount).await?;

        let mut new_trading_session = trading_session.to_owned();
        new_trading_session.apply_cross_deposit(amount, cross_position_raw)?;
        let cross_position = new_trading_session.cross_position();

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(cross_position)
    }

    async fn cross_withdraw(
        &self,
        amount: NonZeroU64,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let cross_position_raw = self.api.cross_withdraw(amount).await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        new_trading_session.apply_cross_withdraw(amount, cross_position_raw)?;
        let cross_position = new_trading_session.cross_position();

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(cross_position)
    }

    async fn cross_set_leverage(
        &self,
        leverage: CrossLeverage,
    ) -> TradeExecutorResult<Arc<dyn CrossPositionCore>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let cross_position_raw = self.api.cross_set_leverage(leverage).await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        new_trading_session.replace_cross_position(cross_position_raw)?;
        let cross_position = new_trading_session.cross_position();

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(cross_position)
    }

    async fn cross_order(&self, request: CrossOrderRequest) -> TradeExecutorResult<Uuid> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;
        let (side, quantity, client_id) = request.into_cross_order_parts();

        let cross_order = self.api.cross_order(side, quantity, client_id).await?;
        if !cross_order.filled() {
            return Err(ExecutorActionError::CrossOrderNotFilled {
                order_id: cross_order.id(),
            }
            .into());
        }
        let cross_position_raw = self.api.cross_get_position().await?;
        let order_id = cross_order.id();

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        new_trading_session.register_cross_order(cross_position_raw, &cross_order)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(order_id)
    }

    async fn cross_order_close_position(&self) -> TradeExecutorResult<Option<Uuid>> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let latest_cross_position = self.api.cross_get_position().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        new_trading_session.replace_cross_position(latest_cross_position)?;

        if !new_trading_session.cross_position_is_running() {
            locked_ready_state
                .update_trading_session(new_trading_session)
                .await;

            return Ok(None);
        }

        let cross_order = self.api.cross_order_close_position().await?;
        if !cross_order.filled() {
            return Err(ExecutorActionError::CrossOrderNotFilled {
                order_id: cross_order.id(),
            }
            .into());
        }
        let cross_position_raw = self.api.cross_get_position().await?;
        let order_id = cross_order.id();

        new_trading_session.register_cross_order(cross_position_raw, &cross_order)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(Some(order_id))
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
    update_tx: LiveTradeExecutorTransmitter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    sync_reader: Arc<dyn SyncReader>,
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
            sync_reader,
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
        trading_session_refresh_interval: time::Duration,
        db: Arc<Database>,
        api: WrappedRestClient,
        account_id: Uuid,
        update_tx: LiveTradeExecutorTransmitter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        sync_reader: Arc<dyn SyncReader>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let refresh_trading_session = async || {
                // Hold the state lock across the entire REST + DB cycle so that mutating executor
                // Mutating executor actions cannot interleave with the rebuild's reads.
                let locked_state = state_manager.lock_state().await;
                let prev_session = locked_state.trading_session().cloned();

                let result = match prev_session {
                    Some(old_trading_session) if !old_trading_session.is_expired() => {
                        let mut restored_trading_session = old_trading_session;

                        match restored_trading_session
                            .reevaluate(db.as_ref(), &api)
                            .await
                            .map_err(ExecutorProcessRecoverableError::LiveTradeSessionEvaluation)
                        {
                            Ok(closed_trades) => Ok((restored_trading_session, closed_trades)),
                            Err(e) => Err(e),
                        }
                    }
                    prev_session => {
                        match LiveTradingSession::new(
                            startup_recover_trades,
                            trade_tsl_step_size,
                            db.as_ref(),
                            &api,
                            account_id,
                            prev_session,
                        )
                        .await
                        .map_err(ExecutorProcessRecoverableError::LiveTradeSessionEvaluation)
                        {
                            Ok(new_trading_session) => Ok((new_trading_session, Vec::new())),
                            Err(e) => Err(e),
                        }
                    }
                };

                match result {
                    Ok((trading_session, closed_trades)) => {
                        locked_state.update_status_ready(trading_session);

                        for closed_trade in closed_trades {
                            // Ignore no-receiver errors
                            let _ = update_tx
                                .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
                        }
                    }
                    Err(e) => {
                        locked_state.update_status_not_ready(
                            LiveTradeExecutorStatusNotReady::Failed(Arc::new(e)),
                        );
                    }
                }
            };

            let mut sync_rx = sync_reader.update_receiver();

            let mut handler = async || -> ExecutorProcessFatalResult<Never> {
                let mut is_synced = matches!(sync_reader.status_snapshot(), SyncStatus::Synced);
                let mut should_refresh = is_synced;
                let new_refresh_timer = || Box::pin(time::sleep(trading_session_refresh_interval));
                let mut refresh_timer = new_refresh_timer();

                loop {
                    tokio::select! {
                        recv_result = sync_rx.recv() => {
                            match recv_result {
                                Ok(sync_update) => match sync_update {
                                    SyncUpdate::Status(sync_status) => match sync_status {
                                        SyncStatus::NotSynced(sync_status_not_synced) => {
                                            is_synced = false;
                                            should_refresh = false;

                                            let new_status_not_ready =
                                                LiveTradeExecutorStatusNotReady::WaitingForSync(
                                                    sync_status_not_synced,
                                                );
                                            state_manager
                                                .update_status_not_ready(new_status_not_ready)
                                                .await;
                                        }
                                        SyncStatus::Terminated(err) => {
                                            return Err(
                                                ExecutorProcessFatalError::SyncProcessTerminated(
                                                    err,
                                                ),
                                            );
                                        }
                                        SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                            return Err(
                                                ExecutorProcessFatalError::SyncProcessShutdown,
                                            );
                                        }
                                        SyncStatus::Backfilled => {}
                                        SyncStatus::Synced => {
                                            is_synced = true;
                                            should_refresh = true;
                                        }
                                    },
                                    SyncUpdate::PriceTick(_) | SyncUpdate::PriceHistoryState(_) => {
                                        should_refresh = is_synced;
                                    }
                                    SyncUpdate::FundingSettlementsState(_) => {}
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
            LiveTradeExecutor::clean_up_all_api_trades(&self.api_rest)
                .await
                .map_err(LiveTradeExecutorError::LaunchCleanUp)?;
        }

        let account_id = self
            .api_rest
            .get_user()
            .await
            .map_err(LiveTradeExecutorError::LaunchAccountResolution)?
            .id();

        let handle = Self::spawn_sync_processor(
            self.config.startup_recover_trades(),
            self.config.trailing_stoploss_step_size(),
            self.config.trading_session_refresh_interval(),
            self.db.clone(),
            self.api_rest.clone(),
            account_id,
            self.update_tx.clone(),
            self.state_manager.clone(),
            self.sync_reader,
        );

        Ok(LiveTradeExecutor::new(
            self.config,
            self.db,
            self.api_rest,
            account_id,
            self.update_tx,
            self.state_manager,
            handle,
        ))
    }
}
