use std::{result, sync::Arc};

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
    sync::{SyncReceiver, SyncState, SyncUpdate},
    trade::live::executor::state::LiveTradeExecutorState,
    util::{AbortOnDropHandle, Never},
};

use super::{
    super::{
        core::{RiskParams, StoplossMode, TradeExecutor, TradeTrailingStoploss, TradingState},
        error::{Result, TradeError},
    },
    error::{LiveError, Result as LiveResult},
};

pub mod state;
pub mod update;

use state::{LiveTradeExecutorStateManager, LiveTradeExecutorStatusNotReady, LiveTradingSession};
use update::{
    LiveTradeExecutorReceiver, LiveTradeExecutorTransmiter, LiveTradeExecutorUpdate,
    WrappedApiContext,
};

pub struct LiveTradeExecutor {
    tsl_step_size: BoundedPercentage,
    db: Arc<DbContext>,
    api: WrappedApiContext,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    _handle: AbortOnDropHandle<()>,
}

impl LiveTradeExecutor {
    pub fn new(
        tsl_step_size: BoundedPercentage,
        db: Arc<DbContext>,
        api: WrappedApiContext,
        update_tx: LiveTradeExecutorTransmiter,
        state_manager: Arc<LiveTradeExecutorStateManager>,
        _handle: AbortOnDropHandle<()>,
    ) -> Arc<Self> {
        Arc::new(Self {
            tsl_step_size,
            db,
            api,
            update_tx,
            state_manager,
            _handle,
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
            .register_trade(trade.id(), trade_tsl)
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let mut new_trading_session = locked_ready_state.trading_session().to_owned();

        new_trading_session.register_running_trade(self.tsl_step_size, trade, trade_tsl)?;

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

            let closed = match future::join_all(close_futures)
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

            new_trading_session.close_trades(self.tsl_step_size, closed)?;
        }

        locked_ready_state
            .update_trading_session(new_trading_session)
            .await;

        Ok(())
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
        let trade_tsl = stoploss_mode.validate_trade_tsl(self.tsl_step_size, stoploss_perc)?;

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
        let trade_tsl = stoploss_mode.validate_trade_tsl(self.tsl_step_size, stoploss_perc)?;

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

        let closed =
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

        new_trading_session.close_trades(self.tsl_step_size, closed)?;

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
    tsl_step_size: BoundedPercentage,
    db: Arc<DbContext>,
    api: WrappedApiContext,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    sync_rx: SyncReceiver,
}

impl LiveTradeExecutorLauncher {
    pub fn new(
        tsl_step_size: BoundedPercentage,
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
            tsl_step_size,
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
        tsl_step_size: BoundedPercentage,
        db: Arc<DbContext>,
        api: WrappedApiContext,
        sync_rx: SyncReceiver,
        state_manager: Arc<LiveTradeExecutorStateManager>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let refresh_trading_session = async || {
                let Ok(locked_ready_state) = state_manager.try_lock_ready_state().await else {
                    // Create fresh trading session from API

                    match LiveTradingSession::new(tsl_step_size, db.as_ref(), &api).await {
                        Ok(new_trading_session) => {
                            state_manager.update_status_ready(new_trading_session).await;
                        }
                        Err(e) => {
                            state_manager
                                .update_status_not_ready(LiveTradeExecutorStatusNotReady::Failed(e))
                                .await;
                        }
                    };

                    return;
                };

                // Reevaluate existing status

                let mut new_trading_session = locked_ready_state.trading_session().to_owned();

                if let Err(e) = new_trading_session
                    .reevaluate(tsl_step_size, db.as_ref(), &api)
                    .await
                {
                    let new_status_not_ready = LiveTradeExecutorStatusNotReady::Failed(e);
                    locked_ready_state.update_status_not_ready(new_status_not_ready);
                    return;
                }

                locked_ready_state
                    .update_trading_session(new_trading_session)
                    .await;
            };

            let handler = async || -> LiveResult<Never> {
                let mut sync_rx = sync_rx;
                loop {
                    match sync_rx.recv().await {
                        Ok(sync_update) => match sync_update {
                            SyncUpdate::StateChange(sync_state) => match sync_state {
                                SyncState::NotSynced(sync_state_not_synced) => {
                                    let new_status_not_ready =
                                        LiveTradeExecutorStatusNotReady::WaitingForSync(
                                            sync_state_not_synced,
                                        );
                                    state_manager
                                        .update_status_not_ready(new_status_not_ready)
                                        .await;
                                }
                                SyncState::ShutdownInitiated | SyncState::Shutdown => {
                                    // Non-recoverable error
                                    return Err(LiveError::Generic(
                                        "sync process was shutdown".to_string(),
                                    ));
                                }
                                SyncState::Synced => refresh_trading_session().await,
                            },
                            SyncUpdate::PriceTick(_) => refresh_trading_session().await,
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
        let (_, _) = futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades())?;

        let _handle = Self::spawn_sync_processor(
            self.tsl_step_size,
            self.db.clone(),
            self.api.clone(),
            self.sync_rx,
            self.state_manager.clone(),
        );

        Ok(LiveTradeExecutor::new(
            self.tsl_step_size,
            self.db,
            self.api,
            self.update_tx,
            self.state_manager,
            _handle,
        ))
    }
}
