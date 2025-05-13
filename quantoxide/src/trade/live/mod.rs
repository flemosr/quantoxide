use std::{result, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use lnm_sdk::api::rest::{
    RestApiContext,
    models::{
        BoundedPercentage, Leverage, LowerBoundedPercentage, Quantity, SATS_PER_BTC, Ticker,
        TradeExecution, TradeSide,
    },
};
use tokio::sync::Mutex;

use crate::trade::core::RiskParams;

use super::{
    core::{TradesManager, TradesState},
    error::Result,
};

pub mod error;

use error::LiveError;

struct LiveTradesState {
    last_trade_time: Option<DateTime<Utc>>,
}

pub struct LiveTradesManager {
    rest: Arc<RestApiContext>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<LiveTradesState>>,
}

impl LiveTradesManager {
    pub async fn new(rest: Arc<RestApiContext>) -> Result<Self> {
        rest.futures
            .cancel_all_trades()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        rest.futures
            .close_all_trades()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let user = rest
            .user
            .get_user()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let initial_state = LiveTradesState {
            last_trade_time: None,
        };

        Ok(Self {
            rest,
            start_time: Utc::now(),
            start_balance: user.balance(),
            state: Arc::new(Mutex::new(initial_state)),
        })
    }

    async fn get_current_balance(&self) -> Result<u64> {
        let user = self
            .rest
            .user
            .get_user()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        Ok(user.balance())
    }

    async fn get_ticker(&self) -> Result<Ticker> {
        let ticker = self
            .rest
            .futures
            .ticker()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        Ok(ticker)
    }
}

#[async_trait]
impl TradesManager for LiveTradesManager {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let ticker = self.get_ticker().await?;
        let balance = self.get_current_balance().await?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(ticker.ask_price())?;

        let quantity = {
            let balance_usd = balance as f64 * ticker.ask_price().into_f64() / SATS_PER_BTC;
            let quantity_target = balance_usd * balance_perc.into_f64() / 100.;
            if quantity_target < 1. {
                return Err(LiveError::Generic("balance is too low".to_string()))?;
            }

            Quantity::try_from(quantity_target.floor()).map_err(LiveError::QuantityValidation)?
        };

        self.rest
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
            .map_err(LiveError::RestApi)?;

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

        let ticker = self.get_ticker().await?;
        let balance = self.get_current_balance().await?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(ticker.bid_price())?;

        let quantity = {
            let balance_usd = balance as f64 * ticker.ask_price().into_f64() / SATS_PER_BTC;
            let quantity_target = balance_usd * balance_perc.into_f64() / 100.;
            if quantity_target < 1. {
                return Err(LiveError::Generic("balance is too low".to_string()))?;
            }

            Quantity::try_from(quantity_target.floor()).map_err(LiveError::QuantityValidation)?
        };

        self.rest
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
            .map_err(LiveError::RestApi)?;

        Ok(())
    }

    async fn close_longs(&self) -> Result<()> {
        let mut state_guard = self.state.lock().await;
        state_guard.last_trade_time = Some(Utc::now());

        let running = self
            .rest
            .futures
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
                    let rest = &self.rest;
                    async move { rest.futures.close_trade(trade.id()).await }
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
            .rest
            .futures
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
                    let rest = &self.rest;
                    async move { rest.futures.close_trade(trade.id()).await }
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

        self.rest
            .futures
            .cancel_all_trades()
            .await
            .map_err(LiveError::RestApi)?;

        self.rest
            .futures
            .close_all_trades()
            .await
            .map_err(LiveError::RestApi)?;

        Ok(())
    }

    async fn state(&self) -> Result<TradesState> {
        let state_guard = self.state.lock().await;

        let running_trades = self
            .rest
            .futures
            .get_trades_running(None, None, 1000.into())
            .await
            .map_err(LiveError::RestApi)?;

        let closed_trades = self
            .rest
            .futures
            .get_trades_closed(Some(&self.start_time), None, 1000.into())
            .await
            .map_err(LiveError::RestApi)?;

        let ticker = self.get_ticker().await?;
        let balance = self.get_current_balance().await?;

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

        let trades_state = TradesState::new(
            self.start_time,
            self.start_balance,
            Utc::now(),
            balance,
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
