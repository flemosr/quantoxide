use std::{
    result,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures::future;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, Trade, TradeSide,
        error::QuantityValidationError,
    },
};
use tokio::sync::broadcast;

use crate::{
    db::DbContext,
    sync::{SyncReceiver, SyncStatus, SyncUpdate},
    util::{AbortOnDropHandle, Never},
};

use super::{
    super::{
        core::{RiskParams, StoplossMode, TradeExecutor, TradeTrailingStoploss, TradingState},
        error::{Result, TradeError},
    },
    LiveConfig,
    error::{LiveError, Result as LiveResult},
};

pub mod state;
pub mod update;

use state::{
    LiveTradeExecutorState, LiveTradeExecutorStateManager, LiveTradeExecutorStatusNotReady,
    LiveTradingSession,
};
use update::{
    LiveTradeExecutorReceiver, LiveTradeExecutorTransmiter, LiveTradeExecutorUpdate,
    WrappedApiContext,
};

pub struct LiveTradeExecutorConfig {
    tsl_step_size: BoundedPercentage,
    clean_up_trades_on_startup: bool,
    recover_trades_on_startup: bool,
    clean_up_trades_on_shutdown: bool,
}

impl LiveTradeExecutorConfig {
    pub fn trailing_stoploss_step_size(&self) -> BoundedPercentage {
        self.tsl_step_size
    }

    pub fn clean_up_trades_on_startup(&self) -> bool {
        self.clean_up_trades_on_startup
    }

    pub fn recover_trades_on_startup(&self) -> bool {
        self.recover_trades_on_startup
    }

    pub fn clean_up_trades_on_shutdown(&self) -> bool {
        self.clean_up_trades_on_shutdown
    }

    pub fn set_trailing_stoploss_step_size(mut self, tsl_step_size: BoundedPercentage) -> Self {
        self.tsl_step_size = tsl_step_size;
        self
    }

    pub fn set_clean_up_trades_on_startup(mut self, clean_up_trades_on_startup: bool) -> Self {
        self.clean_up_trades_on_startup = clean_up_trades_on_startup;
        self
    }

    pub fn set_recover_trades_on_startup(mut self, recover_trades_on_startup: bool) -> Self {
        self.recover_trades_on_startup = recover_trades_on_startup;
        self
    }

    pub fn set_clean_up_trades_on_shutdown(mut self, clean_up_trades_on_shutdown: bool) -> Self {
        self.clean_up_trades_on_shutdown = clean_up_trades_on_shutdown;
        self
    }
}

impl Default for LiveTradeExecutorConfig {
    fn default() -> Self {
        Self {
            tsl_step_size: BoundedPercentage::MIN,
            clean_up_trades_on_startup: true,
            recover_trades_on_startup: false,
            clean_up_trades_on_shutdown: true,
        }
    }
}

impl From<&LiveConfig> for LiveTradeExecutorConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            tsl_step_size: value.tsl_step_size,
            clean_up_trades_on_startup: value.clean_up_trades_on_startup,
            recover_trades_on_startup: value.recover_trades_on_startup,
            clean_up_trades_on_shutdown: value.clean_up_trades_on_shutdown,
        }
    }
}

pub struct LiveTradeExecutor {
    config: LiveTradeExecutorConfig,
    db: Arc<DbContext>,
    api: WrappedApiContext,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
}

impl LiveTradeExecutor {
    fn new(
        config: LiveTradeExecutorConfig,
        db: Arc<DbContext>,
        api: WrappedApiContext,
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

    pub fn update_receiver(&self) -> LiveTradeExecutorReceiver {
        self.update_tx.subscribe()
    }

    pub async fn state_snapshot(&self) -> LiveTradeExecutorState {
        self.state_manager.snapshot().await
    }

    async fn get_estimated_market_price(&self) -> Result<Price> {
        // Assuming that the db is up-to-date

        let (_, last_entry_price) = self
            .db
            .price_ticks
            .get_latest_entry()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        let price =
            Price::round(last_entry_price).map_err(|e| LiveError::Generic(e.to_string()))?;

        Ok(price)
    }

    async fn open_trade(
        &self,
        risk_params: RiskParams,
        trade_tsl: Option<TradeTrailingStoploss>,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let market_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let quantity = Quantity::try_from_balance_perc(
            locked_ready_state.trading_session().balance(),
            market_price,
            balance_perc,
        )
        .map_err(|e| match e {
            QuantityValidationError::TooLow => TradeError::BalanceTooLow,
            QuantityValidationError::TooHigh => TradeError::BalanceTooHigh,
        })?;

        let trade = match self
            .api
            .create_new_trade(side, quantity, leverage, stoploss, takeprofit)
            .await
        {
            Ok(trade) => trade,
            Err(e) => {
                let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(
                    LiveError::Generic("api error".to_string()),
                );
                locked_ready_state.update_status_not_ready(new_status_not_ready);

                return Err(e.into());
            }
        };

        self.db
            .running_trades
            .add_running_trade(trade.id(), trade_tsl.clone())
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.register_running_trade(self.config.tsl_step_size, trade, trade_tsl)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn close_trades(&self, side: TradeSide) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();
        let mut to_close = Vec::new();

        for (id, (trade, _)) in new_trading_session.running() {
            if trade.side() == side {
                to_close.push(id.clone());
            }
        }

        // Process in batches of 1
        for chunk in to_close.chunks(1) {
            let close_futures = chunk
                .iter()
                .map(|&trade_id| self.api.close_trade(trade_id))
                .collect::<Vec<_>>();

            let closed_trades = match future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
            {
                Ok(closed) => closed,
                Err(e) => {
                    // Status needs to be recreated
                    let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(
                        LiveError::Generic("api error".to_string()),
                    );
                    locked_ready_state.update_status_not_ready(new_status_not_ready);

                    return Err(e.into());
                }
            };

            new_trading_session.close_trades(self.config.tsl_step_size, &closed_trades)?;

            for closed_trade in closed_trades {
                // Ignore no-receiver errors
                let _ = self
                    .update_tx
                    .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
            }
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

    pub async fn shutdown(&self) -> Result<()> {
        let Some(handle) = self.try_consume_handle() else {
            return Err(TradeError::Generic(
                "`LiveTradeExecutor` was already shutdown".to_string(),
            ));
        };

        self.state_manager
            .update_status_not_ready(LiveTradeExecutorStatusNotReady::ShutdownInitiated)
            .await;

        handle.abort();

        if !self.config.clean_up_trades_on_shutdown() {
            self.state_manager
                .update_status_not_ready(LiveTradeExecutorStatusNotReady::Shutdown)
                .await;
            return Ok(());
        }

        let (res, new_status) = if self.state_manager.has_registered_running_trades().await {
            // Perform clean up if there are running trades registered

            match futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades()) {
                Ok(_) => (Ok(()), LiveTradeExecutorStatusNotReady::Shutdown),
                Err(e) => (
                    Err(TradeError::Generic(format!(
                        "couldn't cancel and close trades on shutdown {:?}",
                        e
                    ))),
                    LiveTradeExecutorStatusNotReady::NotViable(e),
                ),
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
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let trade_tsl =
            stoploss_mode.validate_trade_tsl(self.config.tsl_step_size, stoploss_perc)?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.open_trade(risk_params, trade_tsl, balance_perc, leverage)
            .await
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        stoploss_mode: StoplossMode,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let trade_tsl =
            stoploss_mode.validate_trade_tsl(self.config.tsl_step_size, stoploss_perc)?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.open_trade(risk_params, trade_tsl, balance_perc, leverage)
            .await
    }

    async fn close_longs(&self) -> Result<()> {
        self.close_trades(TradeSide::Buy).await
    }

    async fn close_shorts(&self) -> Result<()> {
        self.close_trades(TradeSide::Sell).await
    }

    async fn close_all(&self) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        let closed_trades =
            match futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades()) {
                Ok((_, closed)) => closed,
                Err(e) => {
                    // Status needs to be reevaluated
                    let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(
                        LiveError::Generic("api error".to_string()),
                    );
                    locked_ready_state.update_status_not_ready(new_status_not_ready);

                    return Err(e.into());
                }
            };

        new_trading_session.close_trades(self.config.tsl_step_size, &closed_trades)?;

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

    async fn trading_state(&self) -> Result<TradingState> {
        let trading_session = self
            .state_manager
            .try_lock_ready_state()
            .await?
            .trading_session()
            .to_owned();

        Ok(TradingState::from(trading_session))
    }
}

pub struct LiveTradeExecutorLauncher {
    config: LiveTradeExecutorConfig,
    db: Arc<DbContext>,
    api: WrappedApiContext,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    sync_rx: SyncReceiver,
}

impl LiveTradeExecutorLauncher {
    pub fn new(
        config: impl Into<LiveTradeExecutorConfig>,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
    ) -> LiveResult<Self> {
        if !api.rest.has_credentials {
            return Err(LiveError::Generic(
                "`LiveTradeExecutorLauncher`'s `api` must have credentials".to_string(),
            ));
        }

        let (update_tx, _) = broadcast::channel::<LiveTradeExecutorUpdate>(100);

        let api = WrappedApiContext::new(api, update_tx.clone());

        let state_manager = LiveTradeExecutorStateManager::new(update_tx.clone());

        Ok(Self {
            config: config.into(),
            db,
            api,
            update_tx,
            state_manager,
            sync_rx,
        })
    }

    pub fn update_receiver(&self) -> LiveTradeExecutorReceiver {
        self.update_tx.subscribe()
    }

    fn spawn_sync_processor(
        recover_trades_on_startup: bool,
        tsl_step_size: BoundedPercentage,
        db: Arc<DbContext>,
        api: WrappedApiContext,
        update_tx: LiveTradeExecutorTransmiter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        sync_rx: SyncReceiver,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let refresh_trading_session = async || {
                let locked_state = state_manager.lock_state().await;

                let current_trading_session = if let Some(old_trading_session) =
                    locked_state.trading_session()
                {
                    let mut restored_trading_session = old_trading_session.clone();

                    match restored_trading_session.reevaluate(db.as_ref(), &api).await {
                        Ok(closed_trades) => {
                            for closed_trade in closed_trades.into_iter() {
                                // Ignore no-receiver errors
                                let _ = update_tx
                                    .send(LiveTradeExecutorUpdate::ClosedTrade(closed_trade));
                            }
                        }
                        Err(e) => {
                            let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(e);
                            locked_state.update_status_not_ready(new_status_not_ready);
                            return;
                        }
                    }

                    restored_trading_session
                } else {
                    match LiveTradingSession::new(
                        recover_trades_on_startup,
                        tsl_step_size,
                        db.as_ref(),
                        &api,
                    )
                    .await
                    {
                        Ok(new_trading_session) => new_trading_session,
                        Err(e) => {
                            locked_state.update_status_not_ready(
                                LiveTradeExecutorStatusNotReady::Failed(e),
                            );
                            return;
                        }
                    }
                };

                locked_state.update_status_ready(current_trading_session);
            };

            let handler = async || -> LiveResult<Never> {
                let mut sync_rx = sync_rx;
                loop {
                    match sync_rx.recv().await {
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
                                SyncStatus::ShutdownInitiated | SyncStatus::Shutdown => {
                                    // Non-recoverable error
                                    return Err(LiveError::Generic(
                                        "sync process was shutdown".to_string(),
                                    ));
                                }
                                SyncStatus::Synced => refresh_trading_session().await,
                            },
                            SyncUpdate::PriceTick(_) => refresh_trading_session().await,
                            SyncUpdate::PriceHistoryState(_) => {}
                        },
                        Err(e) => {
                            return Err(LiveError::Generic(format!("sync_rx error {e}")));
                        }
                    }
                }
            };

            let Err(e) = handler().await;

            let new_status_not_ready = LiveTradeExecutorStatusNotReady::NotViable(e);
            state_manager
                .update_status_not_ready(new_status_not_ready)
                .await;
        })
        .into()
    }

    pub async fn launch(self) -> Result<Arc<LiveTradeExecutor>> {
        if self.config.clean_up_trades_on_startup() {
            let (_, _) =
                futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades())?;
        }

        let handle = Self::spawn_sync_processor(
            self.config.recover_trades_on_startup(),
            self.config.trailing_stoploss_step_size(),
            self.db.clone(),
            self.api.clone(),
            self.update_tx.clone(),
            self.state_manager.clone(),
            self.sync_rx,
        );

        Ok(LiveTradeExecutor::new(
            self.config,
            self.db,
            self.api,
            self.update_tx,
            self.state_manager,
            handle,
        ))
    }
}
