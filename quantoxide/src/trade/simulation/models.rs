use chrono::{DateTime, Utc};

use lnm_sdk::api::rest::models::{
    BoundedPercentage, Leverage, LowerBoundedPercentage, Margin, Price, Quantity, TradeSide,
};

use super::super::error::{Result, TradeError};

use super::SATS_PER_BTC;

pub enum RiskParams {
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
    pub fn into_trade_params(self, market_price: Price) -> Result<(TradeSide, Price, Price)> {
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
                    return Err(TradeError::Generic(format!(
                        "Stoploss {} can't be bellow the liquitation price {} for long positions",
                        stoploss.into_f64(),
                        liquitation.into_f64()
                    )));
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

    fn get_lnm_fee() -> BoundedPercentage {
        BoundedPercentage::try_from(0.1).unwrap()
    }

    #[test]
    fn test_long_liquidation_calculation() {
        // Create a long trade with known parameters
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(85_000.0).unwrap(),
            Price::try_from(95_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.liquitation.into_f64(), 81_819.0);
    }

    #[test]
    fn test_short_liquidation_calculation() {
        // Create a short trade with known parameters
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(95_000.0).unwrap(),
            Price::try_from(85_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.liquitation.into_f64(), 99_999.0);
    }
    #[test]
    fn test_long_stoploss_validation() {
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();
        // From test_long_liquidation_calculation, we know liquidation is at 81,819.0

        // Test: Stoploss must be below entry price for long positions
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(95_000.0).unwrap(), // Invalid: above entry price
            Price::try_from(100_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Stoploss cannot be equal to entry price
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
            Price::try_from(100_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Stoploss cannot be below liquidation price
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(81_000.0).unwrap(), // Invalid: below liquidation
            Price::try_from(100_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Valid long stoploss (between liquidation and entry)
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(85_000.0).unwrap(), // Valid: above liquidation, below entry
            Price::try_from(100_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_long_takeprofit_validation() {
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();
        let valid_stoploss = Price::try_from(85_000.0).unwrap();

        // Test: Takeprofit must be above entry price for long positions
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(85_000.0).unwrap(), // Invalid: below entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Takeprofit cannot be equal to entry price
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Valid long takeprofit
        let result = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(95_000.0).unwrap(), // Valid: above entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_short_stoploss_validation() {
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();
        // From test_short_liquidation_calculation, we know liquidation is at 99,999.0

        // Test: Stoploss must be above entry price for short positions
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(85_000.0).unwrap(), // Invalid: below entry
            Price::try_from(80_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Stoploss cannot be equal to entry price
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
            Price::try_from(85_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Stoploss cannot be above liquidation price
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(100_500.0).unwrap(), // Invalid: above liquidation
            Price::try_from(85_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Valid short stoploss (between entry and liquidation)
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(95_000.0).unwrap(), // Valid: above entry, below liquidation
            Price::try_from(85_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_short_takeprofit_validation() {
        let entry_price = Price::try_from(90_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(10.0).unwrap();
        // Using valid stoploss that's below liquidation price
        let valid_stoploss = Price::try_from(95_000.0).unwrap();

        // Test: Takeprofit must be below entry price for short positions
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(95_000.0).unwrap(), // Invalid: above entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Takeprofit cannot be equal to entry price
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(90_000.0).unwrap(), // Invalid: equal to entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_err());

        // Test: Valid short takeprofit
        let result = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            valid_stoploss,
            Price::try_from(85_000.0).unwrap(), // Valid: below entry
            quantity,
            leverage,
            get_lnm_fee(),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_running_long_pl_calculation() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(45_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let expected_pl = 1818;

        assert_eq!(trade.pl(current_price), expected_pl);
        assert_eq!(
            trade.net_pl(current_price),
            expected_pl - trade.opening_fee as i64 - trade.closing_fee_reserved as i64
        );
    }

    #[test]
    fn test_running_long_pl_loss() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(45_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(42_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let expected_pl = -2223;

        assert_eq!(trade.pl(current_price), expected_pl);
        assert_eq!(
            trade.net_pl(current_price),
            expected_pl - trade.opening_fee as i64 - trade.closing_fee_reserved as i64
        );
    }

    #[test]
    fn test_running_short_pl_calculation() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(45_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(55_000.0).unwrap(),
            Price::try_from(45_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let expected_pl = 2222;

        assert_eq!(trade.pl(current_price), expected_pl);
        assert_eq!(
            trade.net_pl(current_price),
            expected_pl - trade.opening_fee as i64 - trade.closing_fee_reserved as i64
        );
    }

    #[test]
    fn test_running_short_pl_loss() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(60_000.0).unwrap(),
            Price::try_from(45_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let expected_pl = -1819;

        assert_eq!(trade.pl(current_price), expected_pl);
        assert_eq!(
            trade.net_pl(current_price),
            expected_pl - trade.opening_fee as i64 - trade.closing_fee_reserved as i64
        );
    }

    #[test]
    fn test_closed_long_pl_calculation() {
        // Create a closed long trade
        let entry_price = Price::try_from(50_000.0).unwrap();
        let close_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let running_trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(45_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let closed_trade = SimulatedTradeClosed::from_running(
            running_trade.clone(),
            Utc::now(),
            close_price,
            get_lnm_fee(),
        );

        let expected_pl = 1818;

        assert_eq!(closed_trade.pl(), expected_pl);
        assert_eq!(closed_trade.pl(), running_trade.pl(close_price));
        assert_eq!(
            closed_trade.net_pl(),
            expected_pl - closed_trade.opening_fee as i64 - closed_trade.closing_fee as i64
        );
    }

    #[test]
    fn test_closed_short_pl_calculation() {
        // Create a closed short trade
        let entry_price = Price::try_from(50_000.0).unwrap();
        let close_price = Price::try_from(45_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let running_trade = SimulatedTradeRunning::new(
            TradeSide::Sell,
            Utc::now(),
            entry_price,
            Price::try_from(55_000.0).unwrap(),
            Price::try_from(45_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        let closed_trade = SimulatedTradeClosed::from_running(
            running_trade.clone(),
            Utc::now(),
            close_price,
            get_lnm_fee(),
        );

        let expected_pl = 2222;

        assert_eq!(closed_trade.pl(), expected_pl);
        assert_eq!(closed_trade.pl(), running_trade.pl(close_price));
        assert_eq!(
            closed_trade.net_pl(),
            expected_pl - closed_trade.opening_fee as i64 - closed_trade.closing_fee as i64
        );
    }

    #[test]
    fn test_edge_case_min_quantity() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(1).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(45_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.pl(current_price), 181);
    }

    #[test]
    fn test_edge_case_max_quantity() {
        // Test with maximum quantity (500_000)
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(50_500.0).unwrap();
        let quantity = Quantity::try_from(500_000).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(49_000.0).unwrap(),
            Price::try_from(55_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.pl(current_price), 9900990);
    }

    #[test]
    fn test_edge_case_min_leverage() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(1.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(45_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        // Leverage doesn't directly affect PL calculation, but it's important
        // for testing that our trade construction works with min leverage
        // PL should be the same as other tests with same price movement

        assert_eq!(trade.pl(current_price), 1818);
    }

    #[test]
    fn test_edge_case_max_leverage() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = Price::try_from(55_000.0).unwrap();
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(100.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(49_800.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        // Again, leverage doesn't directly affect PL calculation

        assert_eq!(trade.pl(current_price), 1818);
    }

    #[test]
    fn test_edge_case_small_prices() {
        let entry_price = Price::try_from(1.5).unwrap();
        let current_price = Price::try_from(2.0).unwrap();
        let quantity = Quantity::try_from(1).unwrap();
        let leverage = Leverage::try_from(1.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(1.0).unwrap(),
            Price::try_from(2.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.pl(current_price), 16_666_666);
    }

    #[test]
    fn test_no_price_movement() {
        let entry_price = Price::try_from(50_000.0).unwrap();
        let current_price = entry_price;
        let quantity = Quantity::try_from(10).unwrap();
        let leverage = Leverage::try_from(5.0).unwrap();

        let trade = SimulatedTradeRunning::new(
            TradeSide::Buy,
            Utc::now(),
            entry_price,
            Price::try_from(45_000.0).unwrap(),
            Price::try_from(60_000.0).unwrap(),
            quantity,
            leverage,
            get_lnm_fee(),
        )
        .unwrap();

        assert_eq!(trade.pl(current_price), 0);
        assert_eq!(trade.net_pl(current_price), -44);
    }
}
