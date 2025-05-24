use std::{result, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, SATS_PER_BTC, Ticker,
        Trade, TradeExecution, TradeSide,
    },
    websocket::models::PriceTickLNM,
};
use tokio::sync::Mutex;

use crate::{
    sync::{SyncController, SyncState},
    trade::core::RiskParams,
};

use super::{
    super::{
        core::{TradeManager, TradeManagerState},
        error::Result,
    },
    error::{LiveTradeError, Result as LiveTradeResult},
};

fn calculate_quantity(
    balance: u64,
    market_price: Price,
    balance_perc: BoundedPercentage,
) -> Result<Quantity> {
    let balance_usd = balance as f64 * market_price.into_f64() / SATS_PER_BTC;
    let quantity_target = balance_usd * balance_perc.into_f64() / 100.;

    if quantity_target < 1. {
        return Err(LiveTradeError::Generic("balance is too low".to_string()))?;
    }

    Ok(Quantity::try_from(quantity_target.floor()).map_err(LiveTradeError::QuantityValidation)?)
}

pub enum LiveTradeManagerStatus {
    WaitingForSync(Arc<SyncState>),
    Ready,
    NotViable(Arc<SyncState>),
}

impl From<Arc<SyncState>> for LiveTradeManagerStatus {
    fn from(value: Arc<SyncState>) -> Self {
        match value.as_ref() {
            SyncState::NotInitiated
            | SyncState::Starting
            | SyncState::InProgress(_)
            | SyncState::Failed(_)
            | SyncState::Restarting => LiveTradeManagerStatus::WaitingForSync(value),
            SyncState::Synced => LiveTradeManagerStatus::Ready,
            SyncState::ShutdownInitiated | SyncState::Shutdown => {
                LiveTradeManagerStatus::NotViable(value)
            }
        }
    }
}

struct LiveTradeManagerState {
    last_trade_time: Option<DateTime<Utc>>,
    last_tick: Option<PriceTickLNM>,
}

pub struct LiveTradeManager {
    api: Arc<ApiContext>,
    sync_controller: Arc<SyncController>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<LiveTradeManagerState>>,
}

impl LiveTradeManager {
    pub async fn new(api: Arc<ApiContext>, sync_controller: Arc<SyncController>) -> Result<Self> {
        let (_, _, user) = futures::try_join!(
            api.rest().futures().cancel_all_trades(),
            api.rest().futures().close_all_trades(),
            api.rest().user().get_user()
        )
        .map_err(LiveTradeError::RestApi)?;

        let state = Arc::new(Mutex::new(LiveTradeManagerState {
            last_trade_time: None,
            last_tick: None,
        }));

        Ok(Self {
            api,
            sync_controller,
            start_time: Utc::now(),
            start_balance: user.balance(),
            state,
        })
    }

    pub async fn status(&self) -> LiveTradeManagerStatus {
        self.sync_controller.state_snapshot().await.into()
    }

    async fn check_if_ready(&self) -> LiveTradeResult<()> {
        match self.status().await {
            LiveTradeManagerStatus::WaitingForSync(sync_state) => {
                Err(LiveTradeError::ManagerNotReady(sync_state))
            }
            LiveTradeManagerStatus::Ready => Ok(()),
            LiveTradeManagerStatus::NotViable(sync_state) => {
                Err(LiveTradeError::ManagerNotViable(sync_state))
            }
        }
    }

    async fn get_ticker_and_balance(&self) -> Result<(Ticker, u64)> {
        let (ticker, user) = futures::try_join!(
            self.api.rest().futures().ticker(),
            self.api.rest().user().get_user()
        )
        .map_err(LiveTradeError::RestApi)?;

        Ok((ticker, user.balance()))
    }
}

#[async_trait]
impl TradeManager for LiveTradeManager {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let (ticker, balance) = self.get_ticker_and_balance().await?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(ticker.ask_price())?;

        let quantity = calculate_quantity(balance, ticker.ask_price(), balance_perc)?;

        self.api
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
            .map_err(LiveTradeError::RestApi)?;

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
        state_guard.last_trade_time = Some(Utc::now());

        let (ticker, balance) = self.get_ticker_and_balance().await?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(ticker.bid_price())?;

        let quantity = calculate_quantity(balance, ticker.bid_price(), balance_perc)?;

        self.api
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
            .map_err(LiveTradeError::RestApi)?;

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
            .map_err(LiveTradeError::RestApi)?;

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
                .map_err(LiveTradeError::RestApi)?;
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
            .map_err(LiveTradeError::RestApi)?;

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
                .map_err(LiveTradeError::RestApi)?;
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
        .map_err(LiveTradeError::RestApi)?;

        Ok(())
    }

    async fn state(&self) -> Result<TradeManagerState> {
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
        .map_err(LiveTradeError::RestApi)?;

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

        let trades_state = TradeManagerState::new(
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
