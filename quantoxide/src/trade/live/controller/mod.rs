use std::{result, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use tokio::task::JoinHandle;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, SATS_PER_BTC, Trade,
        TradeExecution, TradeSide,
    },
};

use crate::{
    db::DbContext,
    sync::{SyncReceiver, SyncState},
    trade::core::RiskParams,
    util::Never,
};

use super::{
    super::{
        core::{TradeController, TradeControllerState},
        error::Result,
    },
    error::{LiveError, Result as LiveResult},
};

pub mod state;

use state::{
    LiveTradeControllerReceiver, LiveTradeControllerState, LiveTradeControllerStateManager,
    LiveTradeControllerStatus,
};

fn calculate_quantity(
    balance: u64,
    market_price: Price,
    balance_perc: BoundedPercentage,
) -> Result<Quantity> {
    let balance_usd = balance as f64 * market_price.into_f64() / SATS_PER_BTC;
    let quantity_target = balance_usd * balance_perc.into_f64() / 100.;

    if quantity_target < 1. {
        return Err(LiveError::Generic("balance is too low".to_string()))?;
    }

    Ok(Quantity::try_from(quantity_target.floor()).map_err(LiveError::QuantityValidation)?)
}

pub struct LiveTradeController {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state_manager: Arc<LiveTradeControllerStateManager>,
    handle: JoinHandle<()>,
}

impl LiveTradeController {
    fn spawn_sync_processor(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
        state_manager: Arc<LiveTradeControllerStateManager>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let handler = async || -> LiveResult<Never> {
                let mut sync_rx = sync_rx;
                loop {
                    match sync_rx.recv().await {
                        Ok(sync_state) => match sync_state.as_ref() {
                            SyncState::NotInitiated
                            | SyncState::Starting
                            | SyncState::InProgress(_)
                            | SyncState::Failed(_)
                            | SyncState::Restarting => {
                                let new_state =
                                    LiveTradeControllerState::WaitingForSync(sync_state);
                                state_manager.update(new_state).await;
                            }
                            SyncState::Synced(_) => {
                                match state_manager.try_lock_status().await {
                                    Ok(locked_status) => {
                                        let mut status = locked_status.to_owned();

                                        let status_changed = match status
                                            .reevaluate(db.as_ref(), api.as_ref())
                                            .await
                                        {
                                            Ok(status_changed) => status_changed,
                                            Err(e) => {
                                                // Recoverable error
                                                let new_state = LiveTradeControllerState::Failed(e);
                                                state_manager.update(new_state).await;
                                                continue;
                                            }
                                        };

                                        if status_changed {
                                            state_manager
                                                .update_status(locked_status, status)
                                                .await;
                                        }
                                    }
                                    Err(_) => {
                                        // Try to obtain `LiveTradeControllerStatus` via API
                                        let status = match LiveTradeControllerStatus::new(
                                            db.as_ref(),
                                            api.as_ref(),
                                        )
                                        .await
                                        {
                                            Ok(status) => status,
                                            Err(e) => {
                                                // Recoverable error
                                                let new_state = LiveTradeControllerState::Failed(e);
                                                state_manager.update(new_state).await;
                                                continue;
                                            }
                                        };

                                        let new_state = LiveTradeControllerState::Ready(status);
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

            let new_state = LiveTradeControllerState::NotViable(e);
            state_manager.update(new_state).await;
        })
    }

    pub async fn new(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
    ) -> Result<Arc<Self>> {
        let start_time = Utc::now();

        let (_, _) = futures::try_join!(
            api.rest.futures.cancel_all_trades(),
            api.rest.futures.close_all_trades(),
        )
        .map_err(LiveError::RestApi)?;

        let user = api.rest.user.get_user().await.map_err(LiveError::RestApi)?;

        let state_manager = LiveTradeControllerStateManager::new();

        let handle =
            Self::spawn_sync_processor(db.clone(), api.clone(), sync_rx, state_manager.clone());

        Ok(Arc::new(Self {
            db,
            api,
            start_time,
            start_balance: user.balance(),
            state_manager,
            handle,
        }))
    }

    pub fn receiver(&self) -> LiveTradeControllerReceiver {
        self.state_manager.receiver()
    }

    pub async fn state_snapshot(&self) -> Arc<LiveTradeControllerState> {
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
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        let est_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(est_price)?;

        let quantity = calculate_quantity(locked_status.balance(), est_price, balance_perc)?;

        let trade = match self
            .api
            .rest
            .futures
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)
        {
            Ok(trade) => trade,
            Err(e) => {
                // Status needs to be recreated
                let new_state =
                    LiveTradeControllerState::Failed(LiveError::Generic("api error".to_string()));
                self.state_manager.update(new_state).await;

                return Err(e.into());
            }
        };

        let mut new_status = locked_status.to_owned();

        new_status.register_running_trade(trade)?;

        self.state_manager
            .update_status(locked_status, new_status)
            .await;

        Ok(())
    }

    async fn close_trades(&self, side: TradeSide) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        let mut new_status = locked_status.to_owned();
        let mut to_close = Vec::new();

        for (id, trade) in locked_status.running() {
            if trade.side() == side {
                to_close.push(id.clone());
            }
        }

        // Process in batches of 5
        for chunk in to_close.chunks(5) {
            let close_futures = chunk
                .iter()
                .map(|&trade_id| self.api.rest.futures.close_trade(trade_id))
                .collect::<Vec<_>>();

            let closed = match future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)
            {
                Ok(closed) => closed,
                Err(e) => {
                    // Status needs to be recreated
                    let new_state = LiveTradeControllerState::Failed(LiveError::Generic(
                        "api error".to_string(),
                    ));
                    self.state_manager.update(new_state).await;

                    return Err(e.into());
                }
            };

            new_status.close_trades(closed)?;
        }

        self.state_manager
            .update_status(locked_status, new_status)
            .await;

        Ok(())
    }
}

#[async_trait]
impl TradeController for LiveTradeController {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.open_trade(risk_params, balance_perc, leverage).await
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.open_trade(risk_params, balance_perc, leverage).await
    }

    async fn close_longs(&self) -> Result<()> {
        self.close_trades(TradeSide::Buy).await
    }

    async fn close_shorts(&self) -> Result<()> {
        self.close_trades(TradeSide::Sell).await
    }

    async fn close_all(&self) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        let mut new_status = locked_status.to_owned();

        let closed = match futures::try_join!(
            self.api.rest.futures.cancel_all_trades(),
            self.api.rest.futures.close_all_trades(),
        )
        .map_err(LiveError::RestApi)
        {
            Ok((_, closed)) => closed,
            Err(e) => {
                // Status needs to be reevaluated
                let new_state =
                    LiveTradeControllerState::Failed(LiveError::Generic("api error".to_string()));
                self.state_manager.update(new_state).await;

                return Err(e.into());
            }
        };

        new_status.close_trades(closed)?;

        self.state_manager
            .update_status(locked_status, new_status)
            .await;

        Ok(())
    }

    async fn state(&self) -> Result<TradeControllerState> {
        let status = {
            let locked = self.state_manager.try_lock_status().await?;
            locked.to_owned()
        };

        let market_price = self.get_estimated_market_price().await?;

        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_long_quantity: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_short_quantity: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;

        for trade in status.running().values() {
            match trade.side() {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_long_quantity += trade.quantity().into_u64();
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin +=
                        trade.margin().into_u64() + trade.maintenance_margin().max(0) as u64;
                    running_short_quantity += trade.quantity().into_u64();
                }
            };

            running_pl += trade.estimate_pl(market_price);
            running_fees += trade.opening_fee();
        }

        let mut closed_pl: i64 = 0;
        let mut closed_fees: u64 = 0;

        for trade in status.closed().iter() {
            closed_pl += trade.pl();
            closed_fees += trade.opening_fee() + trade.closing_fee();
        }

        let trades_state = TradeControllerState::new(
            self.start_time,
            self.start_balance,
            Utc::now(),
            status.balance(),
            market_price.into_f64(),
            status.last_trade_time(),
            running_long_qtd,
            running_long_margin,
            running_long_quantity,
            running_short_qtd,
            running_short_margin,
            running_short_quantity,
            running_pl,
            running_fees,
            status.closed().len(),
            closed_pl,
            closed_fees,
        );

        Ok(trades_state)
    }
}

impl Drop for LiveTradeController {
    fn drop(&mut self) {
        self.handle.abort();
    }
}
