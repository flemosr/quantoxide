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
    sync::{SyncReceiver, SyncState},
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

use state::{
    LiveTradeExecutorReadyStatus, LiveTradeExecutorState, LiveTradeExecutorStateManager,
    LiveTradeExecutorStateNotReady,
};
use update::{
    LiveTradeControllerReceiver, LiveTradeControllerUpdate, LiveTradeExecutorTransmiter,
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

    pub fn receiver(&self) -> LiveTradeControllerReceiver {
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
        let locked_ready_status = self.state_manager.try_lock_ready_status().await?;

        let market_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let quantity = Quantity::try_from_balance_perc(
            locked_ready_status.balance(),
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
                // Status needs to be recreated
                let new_state = LiveTradeExecutorStateNotReady::Failed(LiveError::Generic(
                    "api error".to_string(),
                ));
                self.state_manager
                    .update_from_locked_ready_status(locked_ready_status, new_state.into())
                    .await;

                return Err(e.into());
            }
        };

        self.db
            .running_trades
            .register_trade(trade.id(), trade_tsl)
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let mut new_status = locked_ready_status.to_owned();

        new_status.register_running_trade(self.tsl_step_size, trade, trade_tsl)?;

        let new_state = new_status.into();

        self.state_manager
            .update_from_locked_ready_status(locked_ready_status, new_state)
            .await;

        Ok(())
    }

    async fn close_trades(&self, side: TradeSide) -> Result<()> {
        let locked_ready_status = self.state_manager.try_lock_ready_status().await?;

        let mut new_status = locked_ready_status.to_owned();
        let mut to_close = Vec::new();

        for (id, (trade, _)) in locked_ready_status.running() {
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
                    let new_state = LiveTradeExecutorStateNotReady::Failed(LiveError::Generic(
                        "api error".to_string(),
                    ));
                    self.state_manager
                        .update_from_locked_ready_status(locked_ready_status, new_state.into())
                        .await;

                    return Err(e.into());
                }
            };

            new_status.close_trades(self.tsl_step_size, closed)?;
        }

        let new_state = new_status.into();

        self.state_manager
            .update_from_locked_ready_status(locked_ready_status, new_state)
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
        let locked_ready_status = self.state_manager.try_lock_ready_status().await?;

        let mut new_status = locked_ready_status.to_owned();

        let closed =
            match futures::try_join!(self.api.cancel_all_trades(), self.api.close_all_trades()) {
                Ok((_, closed)) => closed,
                Err(e) => {
                    // Status needs to be reevaluated
                    let new_state = LiveTradeExecutorStateNotReady::Failed(LiveError::Generic(
                        "api error".to_string(),
                    ));
                    self.state_manager
                        .update_from_locked_ready_status(locked_ready_status, new_state.into())
                        .await;

                    return Err(e.into());
                }
            };

        new_status.close_trades(self.tsl_step_size, closed)?;

        let new_state = new_status.into();

        self.state_manager
            .update_from_locked_ready_status(locked_ready_status, new_state)
            .await;

        Ok(())
    }

    async fn trading_state(&self) -> Result<TradingState> {
        let ready_status = self.state_manager.try_lock_ready_status().await?.to_owned();

        Ok(TradingState::from(&ready_status))
    }
}

pub struct LiveTradeManager {
    tsl_step_size: BoundedPercentage,
    db: Arc<DbContext>,
    api: WrappedApiContext,
    update_tx: LiveTradeExecutorTransmiter,
    state_manager: Arc<LiveTradeExecutorStateManager>,
    sync_rx: SyncReceiver,
}

impl LiveTradeManager {
    pub fn new(
        tsl_step_size: BoundedPercentage,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
    ) -> Self {
        let (update_tx, _) = broadcast::channel::<LiveTradeControllerUpdate>(100);

        let api = WrappedApiContext::new(api, update_tx.clone());

        let state_manager = LiveTradeExecutorStateManager::new(update_tx.clone());

        Self {
            tsl_step_size,
            db,
            api,
            update_tx,
            state_manager,
            sync_rx,
        }
    }

    pub fn update_receiver(&self) -> LiveTradeControllerReceiver {
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
            let handler = async || -> LiveResult<Never> {
                let mut sync_rx = sync_rx;
                loop {
                    match sync_rx.recv().await {
                        Ok(sync_state) => match sync_state.as_ref() {
                            SyncState::NotInitiated
                            | SyncState::Starting
                            | SyncState::InProgress(_)
                            | SyncState::WaitingForResync
                            | SyncState::Failed(_)
                            | SyncState::Restarting => {
                                let new_state =
                                    LiveTradeExecutorStateNotReady::WaitingForSync(sync_state);
                                state_manager.update(new_state.into()).await;
                            }
                            SyncState::Synced(_) => {
                                match state_manager.try_lock_ready_status().await {
                                    Ok(locked_ready_status) => {
                                        let mut tc_ready_status = locked_ready_status.to_owned();

                                        let new_state = match tc_ready_status
                                            .reevaluate(tsl_step_size, db.as_ref(), &api)
                                            .await
                                        {
                                            Ok(()) => tc_ready_status.into(),
                                            Err(e) => {
                                                // Recoverable error
                                                LiveTradeExecutorStateNotReady::Failed(e).into()
                                            }
                                        };

                                        state_manager
                                            .update_from_locked_ready_status(
                                                locked_ready_status,
                                                new_state,
                                            )
                                            .await;
                                    }
                                    Err(_) => {
                                        // Try to obtain `LiveTradeControllerStatus` via API
                                        let status = match LiveTradeExecutorReadyStatus::new(
                                            tsl_step_size,
                                            db.as_ref(),
                                            &api,
                                        )
                                        .await
                                        {
                                            Ok(status) => status,
                                            Err(e) => {
                                                // Recoverable error
                                                let new_state =
                                                    LiveTradeExecutorStateNotReady::Failed(e);
                                                state_manager.update(new_state.into()).await;
                                                continue;
                                            }
                                        };

                                        let new_state = status.into();
                                        state_manager.update(new_state).await;
                                    }
                                };
                            }
                            SyncState::ShutdownInitiated | SyncState::Shutdown => {
                                return Err(LiveError::Generic(
                                    "sync process was shutdown".to_string(),
                                ));
                            }
                        },
                        Err(e) => {
                            return Err(LiveError::Generic(format!("sync_rx error {e}")));
                        }
                    }
                }
            };

            let Err(e) = handler().await;

            let new_state = LiveTradeExecutorStateNotReady::NotViable(e);
            state_manager.update(new_state.into()).await;
        })
        .into()
    }

    pub async fn start(self) -> Result<Arc<LiveTradeExecutor>> {
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
