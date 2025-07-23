use std::{num::NonZeroU64, sync::Arc};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, Margin, Price, Quantity, SATS_PER_BTC, Trade, TradeClosed,
    TradeExecutionType, TradeRunning, TradeSide, TradeSize, trade_util,
};

use super::{
    super::super::core::TradeExt,
    error::{Result, SimulatedTradeExecutorError},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeRunning {
    id: Uuid,
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    price: Price,
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
        size: TradeSize,
        leverage: Leverage,
        entry_time: DateTime<Utc>,
        entry_price: Price,
        stoploss: Price,
        takeprofit: Price,
        fee_perc: BoundedPercentage,
    ) -> Result<Arc<Self>> {
        let (quantity, margin) = size.to_quantity_and_margin(entry_price, leverage)?;

        let liquidation =
            trade_util::estimate_liquidation_price(side, quantity, entry_price, leverage);

        match side {
            TradeSide::Buy => {
                if stoploss < liquidation {
                    return Err(SimulatedTradeExecutorError::StoplossBelowLiquidationLong {
                        stoploss,
                        liquidation,
                    });
                }
                if stoploss >= entry_price {
                    return Err(SimulatedTradeExecutorError::StoplossAboveEntryForLong {
                        stoploss,
                        entry_price,
                    });
                }
                if takeprofit <= entry_price {
                    return Err(SimulatedTradeExecutorError::TakeprofitBelowEntryForLong {
                        takeprofit,
                        entry_price,
                    });
                }
            }
            TradeSide::Sell => {
                if stoploss > liquidation {
                    return Err(SimulatedTradeExecutorError::StoplossAboveLiquidationShort {
                        stoploss,
                        liquidation,
                    });
                }
                if stoploss <= entry_price {
                    return Err(SimulatedTradeExecutorError::StoplossBelowEntryForShort {
                        stoploss,
                        entry_price,
                    });
                }
                if takeprofit >= entry_price {
                    return Err(SimulatedTradeExecutorError::TakeprofitAboveEntryForShort {
                        takeprofit,
                        entry_price,
                    });
                }
            }
        };

        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let opening_fee = (fee_calc * quantity.into_f64() / entry_price.into_f64()).floor() as u64;
        let closing_fee_reserved =
            (fee_calc * quantity.into_f64() / liquidation.into_f64()).floor() as u64;

        Ok(Arc::new(Self {
            id: Uuid::new_v4(),
            side,
            entry_time,
            entry_price,
            price: entry_price,
            stoploss,
            takeprofit,
            margin,
            quantity,
            leverage,
            liquidation,
            opening_fee,
            closing_fee_reserved,
        }))
    }

    pub fn with_new_stoploss(&self, new_stoploss: Price) -> Result<Arc<Self>> {
        match self.side {
            TradeSide::Buy => {
                if new_stoploss < self.liquidation {
                    return Err(SimulatedTradeExecutorError::StoplossBelowLiquidationLong {
                        stoploss: new_stoploss,
                        liquidation: self.liquidation,
                    });
                }
                if new_stoploss >= self.takeprofit {
                    return Err(SimulatedTradeExecutorError::Generic(format!(
                        "For long position, stoploss ({}) must be below takeprofit ({})",
                        new_stoploss, self.takeprofit
                    )));
                }
            }
            TradeSide::Sell => {
                if new_stoploss > self.liquidation {
                    return Err(SimulatedTradeExecutorError::StoplossAboveLiquidationShort {
                        stoploss: new_stoploss,
                        liquidation: self.liquidation,
                    });
                }
                if new_stoploss <= self.takeprofit {
                    return Err(SimulatedTradeExecutorError::Generic(format!(
                        "For short position, stoploss ({}) must be above takeprofit ({})",
                        new_stoploss, self.takeprofit
                    )));
                }
            }
        }

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            stoploss: new_stoploss,
            takeprofit: self.takeprofit,
            margin: self.margin,
            quantity: self.quantity,
            leverage: self.leverage,
            liquidation: self.liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
        }))
    }

    pub fn with_added_margin(&self, amount: NonZeroU64) -> Result<Arc<Self>> {
        let new_margin = self.margin() + amount.into();
        let new_leverage = Leverage::try_calculate(self.quantity(), new_margin, self.price())
            .map_err(|e| {
                SimulatedTradeExecutorError::Generic(format!(
                    "added margin would result in invalid leverage: {e}"
                ))
            })?;
        let new_liquidation = trade_util::estimate_liquidation_price(
            self.side,
            self.quantity,
            self.price,
            new_leverage,
        );

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            stoploss: self.stoploss,
            takeprofit: self.takeprofit,
            margin: new_margin,
            quantity: self.quantity,
            leverage: new_leverage,
            liquidation: new_liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
        }))
    }

    pub fn with_cash_in(&self, market_price: Price, amount: NonZeroU64) -> Result<Arc<Self>> {
        let amount = amount.get() as u64;
        let current_pl = self.pl_estimate(market_price);

        let (new_price, remaining_amount) = if current_pl > 0 {
            if amount < current_pl as u64 {
                // PL should be partially cashed-in. Calculate price that would
                // correspond to the PL that will be extracted.
                let new_price = trade_util::price_from_pl(
                    self.side(),
                    self.quantity(),
                    self.entry_price().expect("must have `entry_price`"),
                    amount as i64,
                );
                (new_price, 0)
            } else {
                // Whole PL should be cashed-in. Adjust trade price to market price
                (market_price, amount - current_pl as u64)
            }
        } else {
            // No PL to be cashed-in. Trade price shouldn't change
            (self.price, amount)
        };

        let new_margin = if remaining_amount == 0 {
            // Only PL will be cashed-in. Margin shouldn't change
            self.margin()
        } else {
            Margin::try_from(self.margin().into_u64().saturating_sub(remaining_amount)).map_err(
                |e| {
                    SimulatedTradeExecutorError::Generic(format!(
                        "cash-in would result in invalid margin: {e}"
                    ))
                },
            )?
        };

        let new_leverage = Leverage::try_calculate(self.quantity(), new_margin, new_price)
            .map_err(|e| {
                SimulatedTradeExecutorError::Generic(format!(
                    "cash-in would result in invalid leverage: {e}"
                ))
            })?;
        let new_liquidation = trade_util::estimate_liquidation_price(
            self.side,
            self.quantity,
            new_price,
            new_leverage,
        );

        // TODO: Make stoploss optional with this case in mind
        let new_stoploss = match self.side {
            TradeSide::Buy => new_liquidation.max(self.stoploss),
            TradeSide::Sell => new_liquidation.min(self.stoploss),
        };

        Ok(Arc::new(Self {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: new_price,
            stoploss: new_stoploss,
            takeprofit: self.takeprofit,
            margin: new_margin,
            quantity: self.quantity,
            leverage: new_leverage,
            liquidation: new_liquidation,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
        }))
    }

    #[cfg(test)]
    fn closing_fee_est(&self, fee_perc: BoundedPercentage, close_price: Price) -> u64 {
        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;

        (fee_calc * self.quantity.into_f64() / close_price.into_f64()).floor() as u64
    }

    #[cfg(test)]
    fn net_pl_est(&self, fee_perc: BoundedPercentage, current_price: Price) -> i64 {
        let pl = self.pl_estimate(current_price);
        pl - self.opening_fee as i64 - self.closing_fee_est(fee_perc, current_price) as i64
    }

    pub fn to_closed(
        &self,
        close_time: DateTime<Utc>,
        close_price: Price,
        fee_perc: BoundedPercentage,
    ) -> Arc<SimulatedTradeClosed> {
        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let closing_fee =
            (fee_calc * self.quantity.into_f64() / close_price.into_f64()).floor() as u64;

        Arc::new(SimulatedTradeClosed {
            id: self.id,
            side: self.side,
            entry_time: self.entry_time,
            entry_price: self.entry_price,
            price: self.price,
            liquidation: self.liquidation,
            stoploss: self.stoploss,
            takeprofit: self.takeprofit,
            margin: self.margin,
            quantity: self.quantity,
            leverage: self.leverage,
            close_time,
            close_price,
            opening_fee: self.opening_fee,
            closing_fee_reserved: self.closing_fee_reserved,
            closing_fee,
        })
    }
}

impl Trade for SimulatedTradeRunning {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        TradeExecutionType::Market
    }

    fn side(&self) -> TradeSide {
        self.side
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn closing_fee(&self) -> u64 {
        0
    }

    fn maintenance_margin(&self) -> i64 {
        self.opening_fee as i64 + self.closing_fee_reserved as i64
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

    fn price(&self) -> Price {
        self.price
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn stoploss(&self) -> Option<Price> {
        Some(self.stoploss)
    }

    fn takeprofit(&self) -> Option<Price> {
        Some(self.takeprofit)
    }

    fn exit_price(&self) -> Option<Price> {
        None
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        None
    }

    fn entry_price(&self) -> Option<Price> {
        Some(self.entry_price)
    }

    fn entry_margin(&self) -> Option<Margin> {
        Some(self.margin)
    }

    fn open(&self) -> bool {
        false
    }

    fn running(&self) -> bool {
        true
    }

    fn canceled(&self) -> bool {
        false
    }

    fn closed(&self) -> bool {
        false
    }
}

impl TradeRunning for SimulatedTradeRunning {
    fn pl_estimate(&self, market_price: Price) -> i64 {
        trade_util::pl_estimate(self.side(), self.quantity(), self.price(), market_price)
    }
}

impl TradeExt for SimulatedTradeRunning {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeClosed {
    id: Uuid,
    side: TradeSide,
    entry_time: DateTime<Utc>,
    entry_price: Price,
    price: Price,
    liquidation: Price,
    stoploss: Price,
    takeprofit: Price,
    margin: Margin,
    quantity: Quantity,
    leverage: Leverage,
    close_time: DateTime<Utc>,
    close_price: Price,
    opening_fee: u64,
    closing_fee_reserved: u64,
    closing_fee: u64,
}

impl SimulatedTradeClosed {
    #[cfg(test)]
    fn net_pl(&self) -> i64 {
        let pl = self.pl();
        pl - self.opening_fee as i64 - self.closing_fee as i64
    }
}

impl Trade for SimulatedTradeClosed {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        TradeExecutionType::Market
    }

    fn side(&self) -> TradeSide {
        self.side
    }

    fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    fn maintenance_margin(&self) -> i64 {
        self.opening_fee as i64 + self.closing_fee_reserved as i64
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

    fn price(&self) -> Price {
        self.price
    }

    fn liquidation(&self) -> Price {
        self.liquidation
    }

    fn stoploss(&self) -> Option<Price> {
        Some(self.stoploss)
    }

    fn takeprofit(&self) -> Option<Price> {
        Some(self.takeprofit)
    }

    fn exit_price(&self) -> Option<Price> {
        Some(self.close_price)
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.entry_time
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.entry_time)
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        Some(self.close_time)
    }

    fn entry_price(&self) -> Option<Price> {
        Some(self.entry_price)
    }

    fn entry_margin(&self) -> Option<Margin> {
        Some(self.margin)
    }

    fn open(&self) -> bool {
        false
    }

    fn running(&self) -> bool {
        false
    }

    fn canceled(&self) -> bool {
        false
    }

    fn closed(&self) -> bool {
        true
    }
}

impl TradeClosed for SimulatedTradeClosed {
    fn pl(&self) -> i64 {
        trade_util::pl_estimate(self.side(), self.quantity(), self.price(), self.close_price)
    }
}

#[cfg(test)]
mod tests;
