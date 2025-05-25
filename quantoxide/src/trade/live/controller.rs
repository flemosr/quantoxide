use std::{result, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use tokio::{
    sync::{Mutex, broadcast},
    task::JoinHandle,
};

use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Price, Quantity,
        SATS_PER_BTC, Trade, TradeExecution, TradeSide,
    },
};

use crate::{
    db::DbContext,
    sync::{SyncController, SyncState},
    trade::core::RiskParams,
};

use super::{
    super::{
        core::{TradeController, TradeControllerState},
        error::Result,
    },
    error::{LiveError, Result as LiveTradeResult},
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

pub enum LiveTradeManagerStatus {
    WaitingForSync(Arc<SyncState>),
    Ready,
    NotViable(LiveError),
}

impl From<Arc<SyncState>> for LiveTradeManagerStatus {
    fn from(value: Arc<SyncState>) -> Self {
        match value.as_ref() {
            SyncState::NotInitiated
            | SyncState::Starting
            | SyncState::InProgress(_)
            | SyncState::Failed(_)
            | SyncState::Restarting => LiveTradeManagerStatus::WaitingForSync(value),
            SyncState::Synced(_) => LiveTradeManagerStatus::Ready,
            SyncState::ShutdownInitiated | SyncState::Shutdown => {
                LiveTradeManagerStatus::NotViable(LiveError::Generic(
                    "sync process was shutdown".to_string(),
                ))
            }
        }
    }
}

struct LiveTradeManagerState {
    status: LiveTradeManagerStatus,
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    running: Vec<LnmTrade>,
    closed: Vec<LnmTrade>,
    closed_pl: i64,
    closed_fees: u64,
}

pub struct LiveTradeManager {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    sync_controller: Arc<SyncController>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<LiveTradeManagerState>>,
    handle: JoinHandle<()>,
}

impl LiveTradeManager {
    fn monitor_running_trades(
        mut sync_rx: broadcast::Receiver<Arc<SyncState>>,
        state: Arc<Mutex<LiveTradeManagerState>>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                let res = sync_rx.recv().await;
                let mut state_guard = state.lock().await;

                let new_status = match res {
                    Ok(sync_state) => {
                        let new_status = LiveTradeManagerStatus::from(sync_state);
                        if matches!(new_status, LiveTradeManagerStatus::Ready) {
                            // TODO: Trigger proper state update
                        }
                        new_status
                    }
                    Err(e) => LiveTradeManagerStatus::NotViable(LiveError::Generic(e.to_string())),
                };

                state_guard.status = new_status;

                if matches!(state_guard.status, LiveTradeManagerStatus::NotViable(_)) {
                    return;
                }
            }
        })
    }

    pub async fn new(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_controller: Arc<SyncController>,
    ) -> Result<Self> {
        let start_time = Utc::now();

        let (_, _, user) = futures::try_join!(
            api.rest().futures().cancel_all_trades(),
            api.rest().futures().close_all_trades(),
            api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        let start_balance = user.balance();

        let initial_sync_state = sync_controller.state_snapshot().await;

        let state = Arc::new(Mutex::new(LiveTradeManagerState {
            status: LiveTradeManagerStatus::from(initial_sync_state),
            last_trade_time: None,
            balance: start_balance,
            running: Vec::new(),
            closed: Vec::new(),
            closed_pl: 0,
            closed_fees: 0,
        }));

        let handle =
            LiveTradeManager::monitor_running_trades(sync_controller.receiver(), state.clone());

        Ok(Self {
            db,
            api,
            sync_controller,
            start_time,
            start_balance,
            state,
            handle,
        })
    }

    pub async fn status(&self) -> LiveTradeManagerStatus {
        self.sync_controller.state_snapshot().await.into()
    }

    async fn check_if_ready(&self) -> LiveTradeResult<()> {
        match self.status().await {
            LiveTradeManagerStatus::WaitingForSync(sync_state) => {
                Err(LiveError::ManagerNotReady(sync_state))
            }
            LiveTradeManagerStatus::Ready => Ok(()),
            LiveTradeManagerStatus::NotViable(err) => Err(err),
        }
    }

    async fn get_estimated_market_price(&self) -> Result<Price> {
        self.check_if_ready().await?;

        // We can assume that the db is up-to-date

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
}

#[async_trait]
impl TradeController for LiveTradeManager {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let est_price = self.get_estimated_market_price().await?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(est_price)?;

        let quantity = calculate_quantity(state_guard.balance, est_price, balance_perc)?;

        let trade = self
            .api
            .rest()
            .futures()
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)?;

        state_guard.last_trade_time = Some(Utc::now());

        let new_balance = state_guard.balance as i64
            - trade.margin().into_i64()
            - trade.maintenance_margin() as i64;
        state_guard.balance = new_balance.min(0) as u64;

        state_guard.running.push(trade);

        Ok(())
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;

        let est_price = self.get_estimated_market_price().await?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(est_price)?;

        let quantity = calculate_quantity(state_guard.balance, est_price, balance_perc)?;

        let trade = self
            .api
            .rest()
            .futures()
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)?;

        state_guard.last_trade_time = Some(Utc::now());

        let new_balance = state_guard.balance as i64
            - trade.margin().into_i64()
            - trade.maintenance_margin() as i64;
        state_guard.balance = new_balance.min(0) as u64;

        state_guard.running.push(trade);

        Ok(())
    }

    async fn close_longs(&self) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let running = self
            .api
            .rest()
            .futures()
            .get_trades_running(None, None, 1000.into())
            .await
            .map_err(LiveError::RestApi)?;

        let long_trades = running
            .into_iter()
            .filter(|trade| trade.side() == TradeSide::Buy)
            .collect::<Vec<_>>();

        // Process in batches of 5
        for chunk in long_trades.chunks(5) {
            let close_futures = chunk
                .iter()
                .map(|trade| {
                    let rest_futures = self.api.rest().futures();
                    async move { rest_futures.close_trade(trade.id()).await }
                })
                .collect::<Vec<_>>();

            future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)?;
        }

        Ok(())
    }

    async fn close_shorts(&self) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let running = self
            .api
            .rest()
            .futures()
            .get_trades_running(None, None, 1000.into())
            .await
            .map_err(LiveError::RestApi)?;

        let short_trades = running
            .into_iter()
            .filter(|trade| trade.side() == TradeSide::Sell)
            .collect::<Vec<_>>();

        // Process in batches of 5
        for chunk in short_trades.chunks(5) {
            let close_futures = chunk
                .iter()
                .map(|trade| {
                    let rest_futures = self.api.rest().futures();
                    async move { rest_futures.close_trade(trade.id()).await }
                })
                .collect::<Vec<_>>();

            future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)?;
        }

        Ok(())
    }

    async fn close_all(&self) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let (_, _) = futures::try_join!(
            self.api.rest().futures().cancel_all_trades(),
            self.api.rest().futures().close_all_trades(),
        )
        .map_err(LiveError::RestApi)?;

        Ok(())
    }

    async fn state(&self) -> Result<TradeControllerState> {
        let state_guard = self.state.lock().await;

        let (running_trades, closed_trades, ticker, user) = futures::try_join!(
            self.api
                .rest()
                .futures()
                .get_trades_running(None, None, 1000.into()),
            self.api
                .rest()
                .futures()
                .get_trades_closed(Some(&self.start_time), None, 1000.into()),
            self.api.rest().futures().ticker(),
            self.api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;
        let mut running_maintenance_margin: u64 = 0;

        for trade in running_trades.iter() {
            match trade.side() {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin += trade.margin().into_u64();
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin += trade.margin().into_u64();
                }
            };

            running_pl += trade.pl();
            running_fees += trade.opening_fee();
            running_maintenance_margin += trade.maintenance_margin();
        }

        let mut closed_pl: i64 = 0;
        let mut closed_fees: u64 = 0;

        for trade in closed_trades.iter() {
            closed_pl += trade.pl();
            closed_fees += trade.opening_fee() + trade.closing_fee();
        }

        let trades_state = TradeControllerState::new(
            self.start_time,
            self.start_balance,
            Utc::now(),
            user.balance(),
            ticker.last_price().into_f64(),
            state_guard.last_trade_time,
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            running_fees,
            running_maintenance_margin,
            closed_trades.len(),
            closed_pl,
            closed_fees,
        );

        Ok(trades_state)
    }
}
