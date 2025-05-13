use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lnm_sdk::api::rest::{
    RestApiContext,
    models::{BoundedPercentage, Leverage, LowerBoundedPercentage, Quantity, Ticker, TradeSide},
};
use tokio::sync::Mutex;

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
    max_running_qtd: usize,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<LiveTradesState>>,
}

impl LiveTradesManager {
    pub async fn new(rest: Arc<RestApiContext>, max_running_qtd: usize) -> Result<Self> {
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
            max_running_qtd,
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

    async fn eval_trade_quantity(&self, balance_perc: BoundedPercentage) -> Result<Quantity> {
        todo!()
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
        todo!()
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        todo!()
    }

    async fn close_longs(&self) -> Result<()> {
        todo!()
    }

    async fn close_shorts(&self) -> Result<()> {
        todo!()
    }

    async fn close_all(&self) -> Result<()> {
        todo!()
    }

    async fn state(&self) -> Result<TradesState> {
        let state_guard = self.state.lock().await;

        let running_trades = self
            .rest
            .futures
            .get_trades_running(Some(&self.start_time), None, 1000.into())
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

        let closed_trades = self
            .rest
            .futures
            .get_trades_closed(Some(&self.start_time), None, 1000.into())
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?;

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
