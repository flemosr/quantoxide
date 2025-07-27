use std::{
    num::NonZeroU64,
    result,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures::future;
use tokio::sync::broadcast;
use uuid::Uuid;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Trade, TradeRunning, TradeSide,
        TradeSize, trade_util,
    },
};

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
    estimated_fee_perc: BoundedPercentage,
    max_running_qtd: usize,
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

    pub fn estimated_fee_perc(&self) -> BoundedPercentage {
        self.estimated_fee_perc
    }

    pub fn max_running_qtd(&self) -> usize {
        self.max_running_qtd
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

    pub fn set_estimated_fee_perc(mut self, estimated_fee_perc: BoundedPercentage) -> Self {
        self.estimated_fee_perc = estimated_fee_perc;
        self
    }

    pub fn set_max_running_qtd(mut self, max_running_qtd: usize) -> Self {
        self.max_running_qtd = max_running_qtd;
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
            estimated_fee_perc: BoundedPercentage::try_from(0.1)
                .expect("must be valid `BoundedPercentage`"),
            max_running_qtd: 50,
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
            estimated_fee_perc: value.estimated_fee_perc,
            max_running_qtd: value.max_running_qtd,
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
        size: TradeSize,
        leverage: Leverage,
        risk_params: RiskParams,
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let market_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let (_, margin, _, opening_fee, closing_fee_reserved) =
            trade_util::evaluate_open_trade_params(
                side,
                size,
                leverage,
                market_price,
                stoploss,
                takeprofit,
                self.config.estimated_fee_perc,
            )
            .map_err(TradeError::TradeValidation)?;

        let trading_session = locked_ready_state.trading_session();

        let balance_diff = margin.into_u64() + opening_fee + closing_fee_reserved;
        if balance_diff > trading_session.balance() {
            return Err(TradeError::BalanceTooLow);
        }

        let max_running_qtd = self.config.max_running_qtd();
        if trading_session.running().len() == max_running_qtd {
            return Err(TradeError::Generic(format!(
                "can't open trade, max_running_qtd {max_running_qtd} already reached"
            )));
        }

        let trade = match self
            .api
            .create_new_trade(side, size, leverage, stoploss, takeprofit)
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

        new_trading_session.register_running_trade(trade, trade_tsl, true)?;

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
                .await
                .map_err(|e| LiveError::Generic(e.to_string()))?;
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
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<(BoundedPercentage, StoplossMode)>,
        takeprofit: Option<LowerBoundedPercentage>,
    ) -> Result<()> {
        let (stoploss_perc, trade_tsl) = match stoploss {
            Some((stoploss_perc, stoploss_mode)) => {
                let trade_tsl =
                    stoploss_mode.validate_trade_tsl(self.config.tsl_step_size, stoploss_perc)?;
                (Some(stoploss_perc), trade_tsl)
            }
            None => (None, None),
        };

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc: takeprofit,
        };

        self.open_trade(size, leverage, risk_params, trade_tsl)
            .await
    }

    async fn open_short(
        &self,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<(BoundedPercentage, StoplossMode)>,
        takeprofit: Option<BoundedPercentage>,
    ) -> Result<()> {
        let (stoploss_perc, trade_tsl) = match stoploss {
            Some((stoploss_perc, stoploss_mode)) => {
                let trade_tsl =
                    stoploss_mode.validate_trade_tsl(self.config.tsl_step_size, stoploss_perc)?;
                (Some(stoploss_perc), trade_tsl)
            }
            None => (None, None),
        };

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc: takeprofit,
        };

        self.open_trade(size, leverage, risk_params, trade_tsl)
            .await
    }

    async fn add_margin(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        if trading_session.running().get(&trade_id).is_none() {}

        let Some((current_trade, _)) = trading_session.running().get(&trade_id) else {
            return Err(TradeError::Generic(format!(
                "trade {trade_id} is not running"
            )));
        };

        let max_additional_margin = current_trade.est_max_additional_margin();
        if amount.get() > max_additional_margin {
            return Err(TradeError::Generic(format!(
                "amount {amount} exceeds max_additional_margin {max_additional_margin} for trade {trade_id}"
            )));
        }

        let balance = trading_session.balance();
        if amount.get() >= balance {
            return Err(TradeError::Generic(format!(
                "amount {amount} exceeds current balance {balance}"
            )));
        }

        let updated_trade = match self.api.add_margin(trade_id, amount).await {
            Ok(trade) => trade,
            Err(e) => {
                let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(
                    LiveError::Generic("api error".to_string()),
                );
                locked_ready_state.update_status_not_ready(new_status_not_ready);

                return Err(e.into());
            }
        };

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn cash_in(&self, trade_id: Uuid, amount: NonZeroU64) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        let Some((current_trade, _)) = trading_session.running().get(&trade_id) else {
            return Err(TradeError::Generic(format!(
                "trade {trade_id} is not running"
            )));
        };

        let market_price = self.get_estimated_market_price().await?;

        let max_cash_in = current_trade.est_max_cash_in(market_price);
        if amount.get() > max_cash_in {
            return Err(TradeError::Generic(format!(
                "amount {amount} exceeds max_cash_in {max_cash_in} for trade {trade_id}"
            )));
        }

        let updated_trade = match self.api.cash_in(trade_id, amount).await {
            Ok(trade) => trade,
            Err(e) => {
                let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(
                    LiveError::Generic("api error".to_string()),
                );
                locked_ready_state.update_status_not_ready(new_status_not_ready);

                return Err(e.into());
            }
        };

        let mut new_trading_session = trading_session.to_owned();

        new_trading_session.update_running_trade(updated_trade)?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
    }

    async fn close_trade(&self, trade_id: Uuid) -> Result<()> {
        let locked_ready_state = self.state_manager.try_lock_ready_state().await?;

        let trading_session = locked_ready_state.trading_session();

        if trading_session.running().get(&trade_id).is_none() {
            return Err(TradeError::Generic(format!(
                "trade {trade_id} is not running"
            )));
        };

        let close_trade = match self.api.close_trade(trade_id).await {
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
            .remove_running_trades(&[close_trade.id()])
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.close_trades(&vec![close_trade])?;

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
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
