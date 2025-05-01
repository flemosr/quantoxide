use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Margin, Price, Quantity, TradeSide,
};

use crate::db::DbContext;

use super::{
    TradesManager, TradesState,
    error::{Result, TradeError},
};

mod models;

use models::{SimulatedTradeClosed, SimulatedTradeRunning};

const SATS_PER_BTC: f64 = 100_000_000.;

enum Close {
    None,
    Side(TradeSide),
    All,
}

impl From<TradeSide> for Close {
    fn from(value: TradeSide) -> Self {
        Self::Side(value)
    }
}

enum RiskParams {
    Long {
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
    },
    Short {
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
    },
}

impl RiskParams {
    fn into_trade_params(self, market_price: Price) -> Result<(TradeSide, Price, Price)> {
        match self {
            Self::Long {
                stoploss_perc,
                takeprofit_perc,
            } => {
                let stoploss = market_price
                    .apply_discount(stoploss_perc)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                let takeprofit = market_price
                    .apply_gain(takeprofit_perc.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                Ok((TradeSide::Buy, stoploss, takeprofit))
            }
            RiskParams::Short {
                stoploss_perc,
                takeprofit_perc,
            } => {
                let stoploss = market_price
                    .apply_gain(stoploss_perc.into())
                    .map_err(|e| TradeError::Generic(e.to_string()))?;
                let takeprofit = market_price
                    .apply_discount(takeprofit_perc)
                    .map_err(|e| TradeError::Generic(e.to_string()))?;

                Ok((TradeSide::Sell, stoploss, takeprofit))
            }
        }
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    balance: i64,
    running: Vec<SimulatedTradeRunning>,
    running_long_qtd: usize,
    running_long_margin: Option<Margin>,
    running_short_qtd: usize,
    running_short_margin: Option<Margin>,
    running_pl: i64,
    running_fees_est: u64,
    closed: Vec<SimulatedTradeClosed>,
    closed_pl: i64,
    closed_fees: u64,
}

pub struct SimulatedTradesManager {
    db: Arc<DbContext>,
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<SimulatedTradesState>>,
}

impl SimulatedTradesManager {
    pub fn new(
        db: Arc<DbContext>,
        max_running_qtd: usize,
        fee_perc: BoundedPercentage,
        start_time: DateTime<Utc>,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState {
            time: start_time,
            balance: start_balance as i64,
            running: Vec::new(),
            running_long_qtd: 0,
            running_long_margin: None,
            running_short_qtd: 0,
            running_short_margin: None,
            running_pl: 0,
            running_fees_est: 0,
            closed: Vec::new(),
            closed_pl: 0,
            closed_fees: 0,
        };

        Self {
            db,
            max_running_qtd,
            fee_perc,
            start_time,
            start_balance,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    async fn update_state(
        &self,
        new_time: DateTime<Utc>,
        close: Close,
    ) -> Result<(MutexGuard<SimulatedTradesState>, Price)> {
        let mut state_guard = self.state.lock().await;

        if new_time <= state_guard.time {
            return Err(TradeError::Generic(format!(
                "tried to update state with new_time {new_time} but current time is {}",
                state_guard.time
            )));
        }

        let market_price = {
            let price_entry = self
                .db
                .price_history
                .get_latest_entry_at_or_before(new_time)
                .await
                .map_err(|e| TradeError::Generic(e.to_string()))?
                .ok_or(TradeError::Generic(format!(
                    "no price history entry was found with time at or before {}",
                    new_time
                )))?;
            Price::try_from(price_entry.value).map_err(|e| TradeError::Generic(e.to_string()))?
        };

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning,
                               close_time: DateTime<Utc>,
                               close_price: Price| {
            let closing_fee_reserved = trade.closing_fee_reserved as i64;
            let trade =
                SimulatedTradeClosed::from_running(trade, close_time, close_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee as i64;

            new_balance += trade.margin.into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee + trade.closing_fee;
            new_closed_trades.push(trade);
        };

        let previous_time = state_guard.time;
        let mut new_running_long_qtd: usize = 0;
        let mut new_running_long_margin: u64 = 0;
        let mut new_running_short_qtd: usize = 0;
        let mut new_running_short_margin: u64 = 0;
        let mut new_running_pl: i64 = 0;
        let mut new_running_fees_est: u64 = 0;
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit between
            // `current_time_guard` and `timestamp`.

            let (min, max) = match trade.side {
                TradeSide::Buy => (trade.stoploss.into_f64(), trade.takeprofit.into_f64()),
                TradeSide::Sell => (trade.takeprofit.into_f64(), trade.stoploss.into_f64()),
            };

            let boundary_entry_opt = self
                .db
                .price_history
                .get_first_entry_reaching_bounds(previous_time, new_time, min, max)
                .await
                .map_err(|e| TradeError::Generic(e.to_string()))?;

            if let Some(price_entry) = boundary_entry_opt {
                // Trade closed by `stoploss` or `takeprofit`

                let close_price = match trade.side {
                    TradeSide::Buy if price_entry.value <= min => trade.stoploss,
                    TradeSide::Buy if price_entry.value >= max => trade.takeprofit,
                    TradeSide::Sell if price_entry.value <= min => trade.takeprofit,
                    TradeSide::Sell if price_entry.value >= max => trade.stoploss,
                    _ => return Err(TradeError::Generic("invalid".to_string())),
                };

                close_trade(trade, price_entry.time, close_price);
            } else {
                // Trade still running

                let should_be_closed = match &close {
                    Close::Side(side) if *side == trade.side => true,
                    Close::All => true,
                    _ => false,
                };

                if should_be_closed {
                    close_trade(trade, new_time, market_price);
                } else {
                    match trade.side {
                        TradeSide::Buy => {
                            new_running_long_qtd += 1;
                            new_running_long_margin += trade.margin.into_u64();
                        }
                        TradeSide::Sell => {
                            new_running_short_qtd += 1;
                            new_running_short_margin += trade.margin.into_u64();
                        }
                    }
                    new_running_pl += trade.pl(market_price);
                    new_running_fees_est += trade.opening_fee + trade.closing_fee_reserved;
                    remaining_running_trades.push(trade);
                }
            }
        }

        state_guard.time = new_time;
        state_guard.balance = new_balance;

        state_guard.running = remaining_running_trades;
        state_guard.running_long_qtd = new_running_long_qtd;
        state_guard.running_long_margin = (new_running_long_margin > 0)
            .then(|| Margin::try_from(new_running_long_margin))
            .transpose()
            .map_err(|e| TradeError::Generic(e.to_string()))?;
        state_guard.running_short_qtd = new_running_short_qtd;
        state_guard.running_short_margin = (new_running_short_margin > 0)
            .then(|| Margin::try_from(new_running_short_margin))
            .transpose()
            .map_err(|e| TradeError::Generic(e.to_string()))?;
        state_guard.running_pl = new_running_pl;
        state_guard.running_fees_est = new_running_fees_est;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok((state_guard, market_price))
    }

    async fn create_running(
        &self,
        timestamp: DateTime<Utc>,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
        risk_params: RiskParams,
    ) -> Result<()> {
        let (mut state_guard, market_price) = self.update_state(timestamp, Close::None).await?;

        if state_guard.running.len() >= self.max_running_qtd {
            return Err(TradeError::Generic(format!(
                "received order but max qtd of running trades ({}) was reached",
                self.max_running_qtd
            )));
        }

        let quantity = {
            let balance_usd = state_guard.balance as f64 * market_price.into_f64() / SATS_PER_BTC;
            let quantity = balance_usd * balance_perc.into_f64() / 100.;
            Quantity::try_from(quantity.floor()).map_err(|e| TradeError::Generic(e.to_string()))?
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let trade = SimulatedTradeRunning::new(
            side,
            timestamp,
            market_price,
            stoploss,
            takeprofit,
            quantity,
            leverage,
            self.fee_perc,
        )?;

        state_guard.time = timestamp;
        state_guard.balance -=
            trade.margin.into_i64() - trade.opening_fee as i64 - trade.closing_fee_reserved as i64;
        state_guard.running.push(trade);

        Ok(())
    }
}

#[async_trait]
impl TradesManager for SimulatedTradesManager {
    async fn open_long(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(timestamp, balance_perc, leverage, risk_params)
            .await?;

        Ok(())
    }

    async fn open_short(
        &self,
        timestamp: DateTime<Utc>,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        self.create_running(timestamp, balance_perc, leverage, risk_params)
            .await?;

        Ok(())
    }

    async fn close_longs(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.update_state(timestamp, TradeSide::Buy.into()).await?;

        Ok(())
    }

    async fn close_shorts(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.update_state(timestamp, TradeSide::Sell.into()).await?;

        Ok(())
    }

    async fn close_all(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.update_state(timestamp, Close::All).await?;

        Ok(())
    }

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState> {
        let (state_guard, _) = self.update_state(timestamp, Close::None).await?;

        let trades_state = TradesState {
            start_time: self.start_time,
            start_balance: self.start_balance,
            current_time: state_guard.time,
            current_balance: state_guard.balance.max(0) as u64,
            running_long_qtd: state_guard.running_long_qtd,
            running_long_margin: state_guard.running_long_margin,
            running_short_qtd: state_guard.running_short_qtd,
            running_short_margin: state_guard.running_short_margin,
            running_pl: state_guard.running_pl,
            running_fees_est: state_guard.running_fees_est,
            closed_qtd: state_guard.closed.len(),
            closed_pl: state_guard.closed_pl,
            closed_fees: state_guard.closed_fees,
        };

        Ok(trades_state)
    }
}
