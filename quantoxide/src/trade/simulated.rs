use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Margin, Price, Quantity,
};

use crate::db::DbContext;

use super::{
    TradesManager, TradesState,
    error::{Result, TradeError},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TradeSide {
    Long,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulatedTradeRunning {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
}

impl SimulatedTradeRunning {
    fn pl(&self, current_price: Price) -> i64 {
        let price_diff = match self.side {
            TradeSide::Long => current_price.into_f64() - self.entry_price.into_f64(),
            TradeSide::Short => self.entry_price.into_f64() - current_price.into_f64(),
        };
        let pl = price_diff * self.margin.into_u64() as f64 * self.leverage.into_f64();
        pl as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SimulatedTradeClosed {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
}

impl SimulatedTradeClosed {
    fn from_running(
        running: SimulatedTradeRunning,
        close_time: DateTime<Utc>,
        close_price: Price,
    ) -> Self {
        SimulatedTradeClosed {
            side: running.side,
            entry_time: running.entry_time,
            entry_price: running.entry_price,
            stoploss: running.stoploss,
            takeprofit: running.takeprofit,
            margin: running.margin,
            leverage: running.leverage,
            close_time,
            close_price,
        }
    }

    fn pl(&self) -> i64 {
        let price_diff = match self.side {
            TradeSide::Long => self.close_price.into_f64() - self.entry_price.into_f64(),
            TradeSide::Short => self.entry_price.into_f64() - self.close_price.into_f64(),
        };
        let pl = price_diff * self.margin.into_u64() as f64 * self.leverage.into_f64();
        pl as i64
    }
}

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

                Ok((TradeSide::Long, stoploss, takeprofit))
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

                Ok((TradeSide::Short, stoploss, takeprofit))
            }
        }
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    balance: u64,
    running: Vec<SimulatedTradeRunning>,
    running_long_qtd: usize,
    running_long_margin: Option<Margin>,
    running_short_qtd: usize,
    running_short_margin: Option<Margin>,
    running_pl: i64,
    closed: Vec<SimulatedTradeClosed>,
    closed_pl: i64,
}

pub struct SimulatedTradesManager {
    db: Arc<DbContext>,
    max_running_qtd: usize,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<SimulatedTradesState>>,
}

impl SimulatedTradesManager {
    pub fn new(
        db: Arc<DbContext>,
        max_qtd_trades_running: usize,
        start_time: DateTime<Utc>,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState {
            time: start_time,
            balance: start_balance,
            running: Vec::new(),
            running_long_qtd: 0,
            running_long_margin: None,
            running_short_qtd: 0,
            running_short_margin: None,
            running_pl: 0,
            closed: Vec::new(),
            closed_pl: 0,
        };

        Self {
            db,
            max_running_qtd: max_qtd_trades_running,
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
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning,
                               close_time: DateTime<Utc>,
                               close_price: Price| {
            let closed_trade = SimulatedTradeClosed::from_running(trade, close_time, close_price);
            let closed_trade_pl = closed_trade.pl();

            new_balance += closed_trade.margin.into_u64() as i64 + closed_trade_pl;
            new_closed_pl += closed_trade_pl;
            new_closed_trades.push(closed_trade);
        };

        let previous_time = state_guard.time;
        let mut new_running_long_qtd: usize = 0;
        let mut new_running_long_margin: u64 = 0;
        let mut new_running_short_qtd: usize = 0;
        let mut new_running_short_margin: u64 = 0;
        let mut new_running_pl: i64 = 0;
        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit between
            // `current_time_guard` and `timestamp`.

            let (min, max) = match trade.side {
                TradeSide::Long => (trade.stoploss.into_f64(), trade.takeprofit.into_f64()),
                TradeSide::Short => (trade.takeprofit.into_f64(), trade.stoploss.into_f64()),
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
                    TradeSide::Long if price_entry.value <= min => trade.stoploss,
                    TradeSide::Long if price_entry.value >= max => trade.takeprofit,
                    TradeSide::Short if price_entry.value <= min => trade.takeprofit,
                    TradeSide::Short if price_entry.value >= max => trade.stoploss,
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
                        TradeSide::Long => {
                            new_running_long_qtd += 1;
                            new_running_long_margin += trade.margin.into_u64();
                        }
                        TradeSide::Short => {
                            new_running_short_qtd += 1;
                            new_running_short_margin += trade.margin.into_u64();
                        }
                    }
                    new_running_pl += trade.pl(market_price);
                    remaining_running_trades.push(trade);
                }
            }
        }

        state_guard.time = new_time;
        state_guard.balance = new_balance.max(0) as u64;

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

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;

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

        let margin = {
            let margin = state_guard.balance as f64 * balance_perc.into_f64() / 100.;
            let margin =
                Margin::try_from(margin.floor()).map_err(|e| TradeError::Generic(e.to_string()))?;
            let _ = Quantity::try_calculate(margin, market_price, leverage)
                .map_err(|e| TradeError::Generic(e.to_string()))?;
            margin
        };

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(market_price)?;

        let trade = SimulatedTradeRunning {
            side,
            entry_time: timestamp,
            entry_price: market_price,
            stoploss,
            takeprofit,
            margin,
            leverage,
        };

        state_guard.time = timestamp;
        state_guard.balance -= margin.into_u64();
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
        let _ = self.update_state(timestamp, TradeSide::Long.into()).await?;

        Ok(())
    }

    async fn close_shorts(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self
            .update_state(timestamp, TradeSide::Short.into())
            .await?;

        Ok(())
    }

    async fn close_all(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.update_state(timestamp, Close::All).await?;

        Ok(())
    }

    async fn state(&self, timestamp: DateTime<Utc>) -> Result<TradesState> {
        let (state_guard, _) = self.update_state(timestamp, Close::None).await?;

        let trades_state = TradesState::new(
            state_guard.time,
            state_guard.balance,
            state_guard.running_long_qtd,
            state_guard.running_short_margin,
            state_guard.running_short_qtd,
            state_guard.running_long_margin,
            state_guard.running_pl,
            state_guard.closed.len(),
            state_guard.closed_pl,
        );

        Ok(trades_state)
    }
}
