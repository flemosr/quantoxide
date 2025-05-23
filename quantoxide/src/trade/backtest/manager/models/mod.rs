use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, Margin, Price, Quantity, SATS_PER_BTC, TradeSide,
    estimate_liquidation_price, estimate_pl,
};

use super::{
    super::super::core::TradeGetters,
    error::{Result, SimulatedTradeManagerError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeRunning {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    quantity: Quantity,
    leverage: Leverage,
    liquidation: Price,
    opening_fee: u64,
    closing_fee_reserved: u64,
}

impl SimulatedTradeRunning {
    pub fn new(
        side: TradeSide,
        entry_time: DateTime<Utc>,
        entry_price: Price,
        stoploss: Price,
        takeprofit: Price,
        quantity: Quantity,
        leverage: Leverage,
        fee_perc: BoundedPercentage,
    ) -> Result<Self> {
        let liquidation = estimate_liquidation_price(side, quantity, entry_price, leverage);

        match side {
            TradeSide::Buy => {
                if stoploss < liquidation {
                    return Err(SimulatedTradeManagerError::StoplossBelowLiquidationLong {
                        stoploss,
                        liquidation,
                    });
                }
                if stoploss >= entry_price {
                    return Err(SimulatedTradeManagerError::StoplossAboveEntryForLong {
                        stoploss,
                        entry_price,
                    });
                }
                if takeprofit <= entry_price {
                    return Err(SimulatedTradeManagerError::TakeprofitBelowEntryForLong {
                        takeprofit,
                        entry_price,
                    });
                }
            }
            TradeSide::Sell => {
                if stoploss > liquidation {
                    return Err(SimulatedTradeManagerError::StoplossAboveLiquidationShort {
                        stoploss,
                        liquidation,
                    });
                }
                if stoploss <= entry_price {
                    return Err(SimulatedTradeManagerError::StoplossBelowEntryForShort {
                        stoploss,
                        entry_price,
                    });
                }
                if takeprofit >= entry_price {
                    return Err(SimulatedTradeManagerError::TakeprofitAboveEntryForShort {
                        takeprofit,
                        entry_price,
                    });
                }
            }
        };

        let margin = Margin::try_calculate(quantity, entry_price, leverage)?;

        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let opening_fee = (fee_calc * quantity.into_f64() / entry_price.into_f64()).floor() as u64;
        let closing_fee_reserved =
            (fee_calc * quantity.into_f64() / liquidation.into_f64()).floor() as u64;

        Ok(Self {
            side,
            entry_time,
            entry_price,
            stoploss,
            takeprofit,
            margin,
            quantity,
            leverage,
            liquidation,
            opening_fee,
            closing_fee_reserved,
        })
    }

    pub fn closing_fee_reserved(&self) -> u64 {
        self.closing_fee_reserved
    }

    pub fn pl(&self, current_price: Price) -> i64 {
        estimate_pl(self.side, self.quantity, self.entry_price, current_price)
    }

    pub fn closing_fee_est(&self, fee_perc: BoundedPercentage, close_price: Price) -> u64 {
        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;

        (fee_calc * self.quantity.into_f64() / close_price.into_f64()).floor() as u64
    }

    pub fn net_pl_est(&self, fee_perc: BoundedPercentage, current_price: Price) -> i64 {
        let pl = self.pl(current_price);
        pl - self.opening_fee as i64 - self.closing_fee_est(fee_perc, current_price) as i64
    }
}

impl TradeGetters for SimulatedTradeRunning {
    fn side(&self) -> TradeSide {
        self.side
    }

    fn price(&self) -> Price {
        self.entry_price
    }

    fn stoploss(&self) -> Option<Price> {
        Some(self.stoploss)
    }

    fn takeprofit(&self) -> Option<Price> {
        Some(self.takeprofit)
    }

    fn quantity(&self) -> Quantity {
        self.quantity
    }

    fn margin(&self) -> Margin {
        self.margin
    }

    fn leverage(&self) -> Leverage {
        self.leverage
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn maintenance_margin(&self) -> u64 {
        self.opening_fee + self.closing_fee_reserved
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeClosed {
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    quantity: Quantity,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
    opening_fee: u64,
    closing_fee: u64,
}

impl SimulatedTradeClosed {
    pub fn from_running(
        running: SimulatedTradeRunning,
        close_time: DateTime<Utc>,
        close_price: Price,
        fee_perc: BoundedPercentage,
    ) -> Self {
        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let closing_fee =
            (fee_calc * running.quantity.into_f64() / close_price.into_f64()).floor() as u64;

        SimulatedTradeClosed {
            side: running.side,
            entry_time: running.entry_time,
            entry_price: running.entry_price,
            stoploss: running.stoploss,
            takeprofit: running.takeprofit,
            margin: running.margin,
            quantity: running.quantity,
            leverage: running.leverage,
            close_time,
            close_price,
            opening_fee: running.opening_fee,
            closing_fee,
        }
    }

    pub fn side(&self) -> TradeSide {
        self.side
    }

    pub fn entry_time(&self) -> DateTime<Utc> {
        self.entry_time
    }

    pub fn entry_price(&self) -> Price {
        self.entry_price
    }

    pub fn stoploss(&self) -> Price {
        self.stoploss
    }

    pub fn takeprofit(&self) -> Price {
        self.takeprofit
    }

    pub fn margin(&self) -> Margin {
        self.margin
    }

    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    pub fn close_time(&self) -> DateTime<Utc> {
        self.close_time
    }

    pub fn close_price(&self) -> Price {
        self.close_price
    }

    pub fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    pub fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    pub fn pl(&self) -> i64 {
        estimate_pl(self.side, self.quantity, self.entry_price, self.close_price)
    }

    pub fn net_pl(&self) -> i64 {
        let pl = self.pl();
        pl - self.opening_fee as i64 - self.closing_fee as i64
    }
}

#[cfg(test)]
mod tests;
