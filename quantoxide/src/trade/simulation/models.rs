use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, Margin, Price, Quantity, TradeSide};

use super::super::error::{Result, TradeError};

use super::SATS_PER_BTC;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeRunning {
    pub side: TradeSide,
    pub entry_time: DateTime<Utc>,
    pub entry_price: Price,
    pub stoploss: Price,
    pub takeprofit: Price,
    pub margin: Margin,
    pub quantity: Quantity,
    pub leverage: Leverage,
    pub liquitation: Price,
    pub opening_fee: u64,
    pub closing_fee_reserved: u64,
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
        let margin = Margin::try_calculate(quantity, entry_price, leverage)
            .map_err(|e| TradeError::Generic(format!("Invalid margin calculation: {}", e)))?;

        let margin_btc = margin.into_f64() / SATS_PER_BTC; // From sats to BTC

        let liquitation = match side {
            TradeSide::Buy => {
                let liquitation = {
                    let value =
                        1.0 / (1.0 / entry_price.into_f64() + margin_btc / quantity.into_f64());
                    Price::round(value).map_err(|e| TradeError::Generic(e.to_string()))?
                };

                if stoploss < liquitation {
                    return Err(TradeError::Generic(
                        "Stoploss can't be bellow the liquitation price for long positions"
                            .to_string(),
                    ));
                }
                if stoploss >= entry_price {
                    return Err(TradeError::Generic(
                        "Stoploss must be below entry price for long positions".to_string(),
                    ));
                }
                if takeprofit <= entry_price {
                    return Err(TradeError::Generic(
                        "Takeprofit must be above entry price for long positions".to_string(),
                    ));
                }

                liquitation
            }
            TradeSide::Sell => {
                let liquitation = {
                    let value =
                        1.0 / (1.0 / entry_price.into_f64() - margin_btc / quantity.into_f64());
                    Price::round(value).map_err(|e| TradeError::Generic(e.to_string()))?
                };

                if stoploss > liquitation {
                    return Err(TradeError::Generic(
                        "Stoploss can't be above the liquitation price for short positions"
                            .to_string(),
                    ));
                }
                if stoploss <= entry_price {
                    return Err(TradeError::Generic(
                        "Stoploss must be above entry price for short positions".to_string(),
                    ));
                }
                if takeprofit >= entry_price {
                    return Err(TradeError::Generic(
                        "Takeprofit must be below entry price for short positions".to_string(),
                    ));
                }

                liquitation
            }
        };

        let fee_calc = SATS_PER_BTC * fee_perc.into_f64() / 100.;
        let opening_fee = (fee_calc * quantity.into_f64() / entry_price.into_f64()).floor() as u64;
        let closing_fee_reserved =
            (fee_calc * quantity.into_f64() / liquitation.into_f64()).floor() as u64;

        Ok(Self {
            side,
            entry_time,
            entry_price,
            stoploss,
            takeprofit,
            margin,
            quantity,
            leverage,
            liquitation,
            opening_fee,
            closing_fee_reserved,
        })
    }

    pub fn pl(&self, current_price: Price) -> i64 {
        let entry_price = self.entry_price.into_f64();
        let current_price = current_price.into_f64();

        let inverse_price_delta = match self.side {
            TradeSide::Buy => SATS_PER_BTC / entry_price - SATS_PER_BTC / current_price,
            TradeSide::Sell => SATS_PER_BTC / current_price - SATS_PER_BTC / entry_price,
        };

        (self.quantity.into_f64() * inverse_price_delta).floor() as i64
    }

    pub fn net_pl(&self, current_price: Price) -> i64 {
        let pl = self.pl(current_price);
        pl - self.opening_fee as i64 - self.closing_fee_reserved as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatedTradeClosed {
    pub side: TradeSide,
    pub entry_time: DateTime<Utc>,
    pub entry_price: Price,
    pub stoploss: Price,
    pub takeprofit: Price,
    pub margin: Margin,
    pub quantity: Quantity,
    pub leverage: Leverage,
    pub close_time: DateTime<Utc>,
    pub close_price: Price,
    pub opening_fee: u64,
    pub closing_fee: u64,
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

    pub fn pl(&self) -> i64 {
        let entry_price = self.entry_price.into_f64();
        let close_price = self.close_price.into_f64();

        let inverse_price_delta = match self.side {
            TradeSide::Buy => SATS_PER_BTC / entry_price - SATS_PER_BTC / close_price,
            TradeSide::Sell => SATS_PER_BTC / close_price - SATS_PER_BTC / entry_price,
        };

        (self.quantity.into_f64() * inverse_price_delta).floor() as i64
    }

    pub fn net_pl(&self) -> i64 {
        let pl = self.pl();
        pl - self.opening_fee as i64 - self.closing_fee as i64
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    #[test]
    fn test_long_pl_calculation() {
        let lnm_estimated_fee = BoundedPercentage::try_from(0.1).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(90_000).unwrap(),
            Price::try_from(110_000).unwrap(),
            Quantity::try_from(500).unwrap(),
            Leverage::try_from(1).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 50_000.0);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(90_000).unwrap(),
            Price::try_from(110_000).unwrap(),
            Quantity::try_from(1_000).unwrap(),
            Leverage::try_from(2).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 66_666.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(90_000).unwrap(),
            Price::try_from(110_000).unwrap(),
            Quantity::try_from(1_500).unwrap(),
            Leverage::try_from(3).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 75_000.0);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(90_000).unwrap(),
            Price::try_from(110_000).unwrap(),
            Quantity::try_from(2_500).unwrap(),
            Leverage::try_from(5).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 83_333.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(99_000).unwrap(),
            Price::try_from(101_000).unwrap(),
            Quantity::try_from(40_000).unwrap(),
            Leverage::try_from(80).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 98_765.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(101_000).unwrap(),
            Price::try_from(99_000).unwrap(),
            Quantity::try_from(40_000).unwrap(),
            Leverage::try_from(80).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 500_000);
        assert_eq!(trade.liquitation.into_f64(), 101_266.0);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(99_000).unwrap(),
            Price::try_from(101_000).unwrap(),
            Quantity::try_from(50).unwrap(),
            Leverage::try_from(5).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 10_000);
        assert_eq!(trade.liquitation.into_f64(), 83_333.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(99_000).unwrap(),
            Price::try_from(101_000).unwrap(),
            Quantity::try_from(50).unwrap(),
            Leverage::try_from(5).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 10_000);
        assert_eq!(trade.liquitation.into_f64(), 83_333.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(100_000).unwrap(),
            Price::try_from(99_000).unwrap(),
            Price::try_from(101_000).unwrap(),
            Quantity::try_from(1).unwrap(),
            Leverage::try_from(5).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.margin.into_u64(), 200);
        assert_eq!(trade.liquitation.into_f64(), 83_333.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(95_000).unwrap(),
            Price::try_from(94_500).unwrap(),
            Price::try_from(95_500).unwrap(),
            Quantity::try_from(5).unwrap(),
            Leverage::try_from(100).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();

        assert_eq!(trade.liquitation.into_f64(), 94_070.5);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(96332.5).unwrap(),
            Price::try_from(90000).unwrap(),
            Price::try_from(110000).unwrap(),
            Quantity::try_from(337).unwrap(),
            Leverage::try_from(7).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();
        let closed_trade = SimulatedTradeClosed::from_running(
            trade,
            Utc::now(),
            Price::try_from(96330).unwrap(),
            lnm_estimated_fee,
        );

        assert_eq!(closed_trade.pl(), -10);
        assert_eq!(closed_trade.net_pl(), -708);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(96550.5).unwrap(),
            Price::try_from(90000).unwrap(),
            Price::try_from(110000).unwrap(),
            Quantity::try_from(1).unwrap(),
            Leverage::try_from(1).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();
        let closed_trade = SimulatedTradeClosed::from_running(
            trade,
            Utc::now(),
            Price::try_from(96508).unwrap(),
            lnm_estimated_fee,
        );

        assert_eq!(closed_trade.pl(), -1);
        assert_eq!(closed_trade.net_pl(), -3);

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            Price::try_from(94027.5).unwrap(),
            Price::try_from(90000).unwrap(),
            Price::try_from(110000).unwrap(),
            Quantity::try_from(1).unwrap(),
            Leverage::try_from(1).unwrap(),
            lnm_estimated_fee,
        )
        .unwrap();
        let closed_trade = SimulatedTradeClosed::from_running(
            trade,
            Utc::now(),
            Price::try_from(94176.5).unwrap(),
            lnm_estimated_fee,
        );

        assert_eq!(closed_trade.pl(), 1);
        assert_eq!(closed_trade.net_pl(), -1);
    }
}
