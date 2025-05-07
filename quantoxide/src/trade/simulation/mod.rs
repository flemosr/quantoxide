use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;
use tokio::sync::Mutex;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Price, Quantity, SATS_PER_BTC, TradeSide,
};

use super::{TradesManager, TradesState, error::Result};

pub mod error;
mod models;

use error::{Result as SimulationResult, SimulationError};
use models::{RiskParams, SimulatedTradeClosed, SimulatedTradeRunning};

enum Close {
    Side(TradeSide),
    All,
}

impl From<TradeSide> for Close {
    fn from(value: TradeSide) -> Self {
        Self::Side(value)
    }
}

struct SimulatedTradesState {
    time: DateTime<Utc>,
    market_price: f64,
    balance: i64,
    running: Vec<SimulatedTradeRunning>,
    closed: Vec<SimulatedTradeClosed>,
    closed_pl: i64,
    closed_fees: u64,
}

pub struct SimulatedTradesManager {
    max_running_qtd: usize,
    fee_perc: BoundedPercentage,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state: Arc<Mutex<SimulatedTradesState>>,
}

impl SimulatedTradesManager {
    pub fn new(
        max_running_qtd: usize,
        fee_perc: BoundedPercentage,
        start_time: DateTime<Utc>,
        market_price: f64,
        start_balance: u64,
    ) -> Self {
        let initial_state = SimulatedTradesState {
            time: start_time,
            market_price,
            balance: start_balance as i64,
            running: Vec::new(),
            closed: Vec::new(),
            closed_pl: 0,
            closed_fees: 0,
        };

        Self {
            max_running_qtd,
            fee_perc,
            start_time,
            start_balance,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    pub async fn tick_update(
        &self,
        time: DateTime<Utc>,
        market_price: f64,
    ) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if time <= state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: time,
                current_time: state_guard.time,
            });
        }

        // We can expect that, most of the time, tick updates won't result in
        // changes in our running trades. They will consist basically of a
        // market price update.

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning, close_price: Price| {
            let closing_fee_reserved = trade.closing_fee_reserved as i64;
            let trade = SimulatedTradeClosed::from_running(trade, time, close_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee as i64;

            new_balance += trade.margin.into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee + trade.closing_fee;
            new_closed_trades.push(trade);
        };

        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let (min, max) = match trade.side {
                TradeSide::Buy => (trade.stoploss, trade.takeprofit),
                TradeSide::Sell => (trade.takeprofit, trade.stoploss),
            };

            if market_price <= min.into_f64() {
                close_trade(trade, min);
            } else if market_price >= max.into_f64() {
                close_trade(trade, max);
            } else {
                remaining_running_trades.push(trade);
            }
        }

        state_guard.time = time;
        state_guard.market_price = market_price;
        state_guard.balance = new_balance;

        state_guard.running = remaining_running_trades;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn close_running(&self, timestamp: DateTime<Utc>, close: Close) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if timestamp < state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: timestamp,
                current_time: state_guard.time,
            });
        }

        let time = state_guard.time;
        let market_price = Price::round(state_guard.market_price)?;

        let mut new_balance = state_guard.balance as i64;
        let mut new_closed_pl = state_guard.closed_pl;
        let mut new_closed_fees = state_guard.closed_fees;
        let mut new_closed_trades = Vec::new();

        let mut close_trade = |trade: SimulatedTradeRunning| {
            let closing_fee_reserved = trade.closing_fee_reserved as i64;
            let trade =
                SimulatedTradeClosed::from_running(trade, time, market_price, self.fee_perc);
            let trade_pl = trade.pl();
            let closing_fee_diff = closing_fee_reserved - trade.closing_fee as i64;

            new_balance += trade.margin.into_i64() + trade_pl + closing_fee_diff;
            new_closed_pl += trade_pl;
            new_closed_fees += trade.opening_fee + trade.closing_fee;
            new_closed_trades.push(trade);
        };

        let mut remaining_running_trades = Vec::new();

        for trade in state_guard.running.drain(..) {
            // Check if price reached stoploss or takeprofit

            let should_be_closed = match &close {
                Close::Side(side) if *side == trade.side => true,
                Close::All => true,
                _ => false,
            };

            if should_be_closed {
                close_trade(trade);
            } else {
                remaining_running_trades.push(trade);
            }
        }

        state_guard.time = timestamp;
        state_guard.balance = new_balance;

        state_guard.running = remaining_running_trades;

        state_guard.closed.append(&mut new_closed_trades);
        state_guard.closed_pl = new_closed_pl;
        state_guard.closed_fees = new_closed_fees;

        Ok(())
    }

    async fn create_running(
        &self,
        timestamp: DateTime<Utc>,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
        risk_params: RiskParams,
    ) -> SimulationResult<()> {
        let mut state_guard = self.state.lock().await;

        if timestamp < state_guard.time {
            return Err(SimulationError::TimeSequenceViolation {
                new_time: timestamp,
                current_time: state_guard.time,
            });
        }

        if state_guard.running.len() >= self.max_running_qtd {
            return Err(SimulationError::MaxRunningTradesReached {
                max_qtd: self.max_running_qtd,
            });
        }

        let market_price = Price::round(state_guard.market_price)?;

        let quantity = {
            let balance_usd = state_guard.balance as f64 * market_price.into_f64() / SATS_PER_BTC;
            let quantity_target = balance_usd * balance_perc.into_f64() / 100.;
            Quantity::try_from(quantity_target.floor())?
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
            trade.margin.into_i64() + trade.opening_fee as i64 + trade.closing_fee_reserved as i64;

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
        let _ = self.close_running(timestamp, TradeSide::Buy.into()).await?;

        Ok(())
    }

    async fn close_shorts(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self
            .close_running(timestamp, TradeSide::Sell.into())
            .await?;

        Ok(())
    }

    async fn close_all(&self, timestamp: DateTime<Utc>) -> Result<()> {
        let _ = self.close_running(timestamp, Close::All).await?;

        Ok(())
    }

    async fn state(&self) -> Result<TradesState> {
        let state_guard = self.state.lock().await;

        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees_est: u64 = 0;

        // Use `Price::round_down` for long trades and `Price::round_up` for
        // short trades, in order to obtain more conservative prices. It is
        // expected that prices won't need to be rounded most of the time.

        for trade in state_guard.running.iter() {
            let market_price = match trade.side {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin += trade.margin.into_u64();

                    Price::round_down(state_guard.market_price).map_err(SimulationError::from)?
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin += trade.margin.into_u64();

                    Price::round_up(state_guard.market_price).map_err(SimulationError::from)?
                }
            };
            running_pl += trade.pl(market_price);
            running_fees_est += trade.opening_fee + trade.closing_fee_reserved;
        }

        let trades_state = TradesState {
            start_time: self.start_time,
            start_balance: self.start_balance,
            current_time: state_guard.time,
            current_balance: state_guard.balance.max(0) as u64,
            market_price: state_guard.market_price,
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            running_fees_est,
            closed_qtd: state_guard.closed.len(),
            closed_pl: state_guard.closed_pl,
            closed_fees: state_guard.closed_fees,
        };

        Ok(trades_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage};

    #[tokio::test]
    async fn test_simulated_trades_manager_long_profit() -> Result<()> {
        // Step 1: Create a new manager with market price as 99_000, balance of 1_000_000
        let start_time = Utc::now();
        let market_price = 99_000.0;
        let start_balance = 1_000_000;
        let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
        let max_running_qtd = 10;

        let manager = SimulatedTradesManager::new(
            max_running_qtd,
            fee_perc,
            start_time,
            market_price,
            start_balance,
        );

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, start_time);
        assert_eq!(state.current_balance, start_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 2: Update market price to 100_000
        let time = start_time + Duration::seconds(1);
        let market_price = 100_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, start_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 3: Open a long trade using 5% of balance
        let time = time + Duration::seconds(1);
        let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
        let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
        let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
        let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

        manager
            .open_long(time, stoploss_perc, takeprofit_perc, balance_perc, leverage)
            .await?;

        let state = manager.state().await?;
        let expected_balance = start_balance - state.running_long_margin - state.running_fees_est;

        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 1);
        assert!(
            state.running_long_margin > 0,
            "Long margin should be positive"
        );
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0); // No PL yet since price hasn't changed
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 4: Update price to 101_000
        let time = time + Duration::seconds(1);
        let market_price = 101_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 1);
        assert!(
            state.running_long_margin > 0,
            "Long margin should be positive"
        );
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert!(
            state.running_pl > 0,
            "Long position should be profitable after price increase"
        );
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 5: Close all running long trades
        let time = time + Duration::seconds(1);
        manager.close_longs(time).await?;

        let state = manager.state().await?;
        let expected_balance =
            (start_balance as i64 + state.closed_pl - state.closed_fees as i64) as u64;

        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 1);
        assert!(
            state.closed_pl > 0,
            "Should have positive PL after closing profitable long"
        );
        assert!(state.closed_fees > 0, "Should have paid trading fees");

        Ok(())
    }

    #[tokio::test]
    async fn test_simulated_trades_manager_long_loss() -> Result<()> {
        // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
        let start_time = Utc::now();
        let market_price = 100_000.0;
        let start_balance = 1_000_000;
        let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
        let max_running_qtd = 10;

        let manager = SimulatedTradesManager::new(
            max_running_qtd,
            fee_perc,
            start_time,
            market_price,
            start_balance,
        );

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, start_time);
        assert_eq!(state.current_balance, start_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 2: Open a long trade using 5% of balance
        let time = start_time + Duration::seconds(1);
        let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
        let takeprofit_perc = LowerBoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
        let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
        let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

        manager
            .open_long(time, stoploss_perc, takeprofit_perc, balance_perc, leverage)
            .await?;

        let state = manager.state().await?;
        let expected_balance = start_balance - state.running_long_margin - state.running_fees_est;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.running_long_qtd, 1);
        assert!(
            state.running_long_margin > 0,
            "Long margin should be positive"
        );
        assert_eq!(state.running_pl, 0);
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);

        // Step 3: Update price to 99_000 (1% drop)
        let time = time + Duration::seconds(1);
        let market_price = 99_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 1);
        assert!(
            state.running_long_margin > 0,
            "Long margin should be positive"
        );
        assert!(
            state.running_pl < 0,
            "Long position should be at a loss after price decrease"
        );
        assert_eq!(state.closed_qtd, 0);

        // Step 4: Update price to trigger stoploss (98_000, 2% drop from entry)
        let time = time + Duration::seconds(1);
        let market_price = 98_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        let expected_balance =
            (start_balance as i64 + state.closed_pl - state.closed_fees as i64) as u64;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0); // Trade should be closed by stoploss
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 1);
        assert!(
            state.closed_pl < 0,
            "Should have negative PL after hitting stoploss"
        );
        assert!(state.closed_fees > 0, "Should have paid trading fees");

        Ok(())
    }

    #[tokio::test]
    async fn test_simulated_trades_manager_short_profit() -> Result<()> {
        // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
        let start_time = Utc::now();
        let market_price = 100_000.0;
        let start_balance = 1_000_000;
        let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
        let max_running_qtd = 10;

        let manager = SimulatedTradesManager::new(
            max_running_qtd,
            fee_perc,
            start_time,
            market_price,
            start_balance,
        );

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, start_time);
        assert_eq!(state.current_balance, start_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 2: Open a short trade using 5% of balance
        let time = start_time + Duration::seconds(1);
        let stoploss_perc = BoundedPercentage::try_from(3.0).unwrap(); // 3% stoploss
        let takeprofit_perc = BoundedPercentage::try_from(4.0).unwrap(); // 4% takeprofit
        let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
        let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

        manager
            .open_short(time, stoploss_perc, takeprofit_perc, balance_perc, leverage)
            .await?;

        let state = manager.state().await?;
        let expected_balance = start_balance - state.running_short_margin - state.running_fees_est;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 1);
        assert!(
            state.running_short_margin > 0,
            "Short margin should be positive"
        );
        assert_eq!(state.running_pl, 0); // No PL yet since price hasn't changed
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 3: Update price to 98_000 (2% drop)
        let time = time + Duration::seconds(1);
        let market_price = 98_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_short_qtd, 1);
        assert!(
            state.running_short_margin > 0,
            "Short margin should be positive"
        );
        assert!(
            state.running_pl > 0,
            "Short position should be profitable after price decrease"
        );
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 4: Update price to trigger takeprofit (96_000, 4% drop from entry)
        let time = time + Duration::seconds(1);
        let market_price = 96_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        let expected_balance =
            (start_balance as i64 + state.closed_pl - state.closed_fees as i64) as u64;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_short_qtd, 0); // Trade should be closed by takeprofit
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 1);
        assert!(
            state.closed_pl > 0,
            "Should have positive PL after hitting takeprofit"
        );
        assert!(state.closed_fees > 0, "Should have paid trading fees");

        Ok(())
    }

    #[tokio::test]
    async fn test_simulated_trades_manager_short_loss() -> Result<()> {
        // Step 1: Create a new manager with market price as 100_000, balance of 1_000_000
        let start_time = Utc::now();
        let market_price = 100_000.0;
        let start_balance = 1_000_000;
        let fee_perc = BoundedPercentage::try_from(0.1).unwrap(); // 0.1% fee
        let max_running_qtd = 10;

        let manager = SimulatedTradesManager::new(
            max_running_qtd,
            fee_perc,
            start_time,
            market_price,
            start_balance,
        );

        let state = manager.state().await?;
        assert_eq!(state.start_time, start_time);
        assert_eq!(state.start_balance, start_balance);
        assert_eq!(state.current_time, start_time);
        assert_eq!(state.current_balance, start_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_long_qtd, 0);
        assert_eq!(state.running_long_margin, 0);
        assert_eq!(state.running_short_qtd, 0);
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 0);
        assert_eq!(state.closed_pl, 0);
        assert_eq!(state.closed_fees, 0);

        // Step 2: Open a short trade using 5% of balance
        let time = start_time + Duration::seconds(1);
        let stoploss_perc = BoundedPercentage::try_from(2.0).unwrap(); // 2% stoploss
        let takeprofit_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% takeprofit
        let balance_perc = BoundedPercentage::try_from(5.0).unwrap(); // 5% of balance
        let leverage = Leverage::try_from(1).unwrap(); // 1x leverage

        manager
            .open_short(time, stoploss_perc, takeprofit_perc, balance_perc, leverage)
            .await?;

        let state = manager.state().await?;
        let expected_balance = start_balance - state.running_short_margin - state.running_fees_est;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.running_short_qtd, 1);
        assert!(
            state.running_short_margin > 0,
            "Short margin should be positive"
        );
        assert_eq!(state.running_pl, 0);
        assert!(
            state.running_fees_est > 0,
            "Trading fees should be estimated"
        );
        assert_eq!(state.closed_qtd, 0);

        // Step 3: Update price to 101_000 (1% increase)
        let time = time + Duration::seconds(1);
        let market_price = 101_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_short_qtd, 1);
        assert!(
            state.running_short_margin > 0,
            "Short margin should be positive"
        );
        assert!(
            state.running_pl < 0,
            "Short position should be at a loss after price increase"
        );
        assert_eq!(state.closed_qtd, 0);

        // Step 4: Update price to trigger stoploss (102_000, 2% increase from entry)
        let time = time + Duration::seconds(1);
        let market_price = 102_000.0;
        manager.tick_update(time, market_price).await?;

        let state = manager.state().await?;
        let expected_balance =
            (start_balance as i64 + state.closed_pl - state.closed_fees as i64) as u64;

        assert_eq!(state.current_time, time);
        assert_eq!(state.current_balance, expected_balance);
        assert_eq!(state.market_price, market_price);
        assert_eq!(state.running_short_qtd, 0); // Trade should be closed by stoploss
        assert_eq!(state.running_short_margin, 0);
        assert_eq!(state.running_pl, 0);
        assert_eq!(state.running_fees_est, 0);
        assert_eq!(state.closed_qtd, 1);
        assert!(
            state.closed_pl < 0,
            "Should have negative PL after hitting stoploss"
        );
        assert!(state.closed_fees > 0, "Should have paid trading fees");

        Ok(())
    }
}
