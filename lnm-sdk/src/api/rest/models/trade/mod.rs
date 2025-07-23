use std::{fmt, result::Result};

use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ValidationError;
use super::{
    Leverage, Margin, Price, Quantity, error::FuturesTradeRequestValidationError, serde_util,
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

impl fmt::Display for TradeSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSide::Buy => "Buy".fmt(f),
            TradeSide::Sell => "Sell".fmt(f),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeSize {
    #[serde(rename = "quantity")]
    Quantity(Quantity),
    #[serde(rename = "margin")]
    Margin(Margin),
}

impl TradeSize {
    pub fn to_quantity_and_margin(
        &self,
        price: Price,
        leverage: Leverage,
    ) -> Result<(Quantity, Margin), ValidationError> {
        match self {
            TradeSize::Margin(margin) => {
                let quantity = Quantity::try_calculate(*margin, price, leverage)?;
                Ok((quantity, *margin))
            }
            TradeSize::Quantity(quantity) => {
                let margin = Margin::try_calculate(*quantity, price, leverage)?;
                Ok((*quantity, margin))
            }
        }
    }
}

impl From<Quantity> for TradeSize {
    fn from(quantity: Quantity) -> Self {
        TradeSize::Quantity(quantity)
    }
}

impl From<Margin> for TradeSize {
    fn from(margin: Margin) -> Self {
        TradeSize::Margin(margin)
    }
}

impl fmt::Display for TradeSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TradeSize::Quantity(quantity) => write!(f, "Quantity({})", quantity),
            TradeSize::Margin(margin) => write!(f, "Margin({})", margin),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecutionType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum TradeExecution {
    Market,
    Limit(Price),
}

impl TradeExecution {
    pub fn to_type(&self) -> TradeExecutionType {
        match self {
            TradeExecution::Market => TradeExecutionType::Market,
            TradeExecution::Limit(_) => TradeExecutionType::Limit,
        }
    }
}

impl From<Price> for TradeExecution {
    fn from(price: Price) -> Self {
        Self::Limit(price)
    }
}

pub fn estimate_liquidation_price(
    side: TradeSide,
    quantity: Quantity,
    entry_price: Price,
    leverage: Leverage,
) -> Price {
    // The `Margin::try_calculate` shouldn't be used here since 'ceil' is
    // used there to achive a `Margin` that would result in the same `Quantity`
    // input via `Quantity::try_calculate`. Said rounding would reduce the
    // corresponding liquidation contraint
    // Here, `floor` is used in order to *understate* the margin, resulting in
    // a more conservative liquidation price. As of May 4 2025, this approach
    // matches liquidation values obtained via the LNM platform.

    let quantity = quantity.into_f64();
    let price = entry_price.into_f64();
    let leverage = leverage.into_f64();

    let a = 1.0 / price;

    let floored_margin = (quantity * SATS_PER_BTC / price / leverage).floor();
    let b = floored_margin / SATS_PER_BTC / quantity;

    // May result in `f64::INFINITY`
    let liquidation_calc = match side {
        TradeSide::Buy => 1.0 / (a + b),
        TradeSide::Sell => 1.0 / (a - b).max(0.),
    };

    Price::clamp_from(liquidation_calc)
}

pub fn pl_estimate(
    side: TradeSide,
    quantity: Quantity,
    start_price: Price,
    end_price: Price,
) -> i64 {
    let start_price = start_price.into_f64();
    let end_price = end_price.into_f64();

    let inverse_price_delta = match side {
        TradeSide::Buy => SATS_PER_BTC / start_price - SATS_PER_BTC / end_price,
        TradeSide::Sell => SATS_PER_BTC / end_price - SATS_PER_BTC / start_price,
    };

    (quantity.into_f64() * inverse_price_delta).floor() as i64
}

pub fn price_from_pl(side: TradeSide, quantity: Quantity, start_price: Price, pl: i64) -> Price {
    let start_price = start_price.into_f64();
    let quantity = quantity.into_f64();

    let inverse_price_delta = (pl as f64) / quantity;

    let inverse_end_price = match side {
        TradeSide::Buy => (SATS_PER_BTC / start_price) - inverse_price_delta,
        TradeSide::Sell => (SATS_PER_BTC / start_price) + inverse_price_delta,
    };

    Price::clamp_from(SATS_PER_BTC / inverse_end_price)
}

#[derive(Serialize, Debug)]
pub struct FuturesTradeRequestBody {
    leverage: Leverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    side: TradeSide,

    #[serde(flatten)]
    size: TradeSize,

    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss {
                if stoploss >= price {
                    return Err(FuturesTradeRequestValidationError::StopLossHigherThanPrice);
                }
            }

            if let Some(takeprofit) = takeprofit {
                if takeprofit <= price {
                    return Err(FuturesTradeRequestValidationError::TakeProfitLowerThanPrice);
                }
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        Ok(FuturesTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            size,
            trade_type,
            price,
        })
    }
}

pub enum TradeStatus {
    Open,
    Running,
    Closed,
}

impl TradeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeStatus::Open => "open",
            TradeStatus::Running => "running",
            TradeStatus::Closed => "closed",
        }
    }

    pub fn to_string(&self) -> String {
        self.as_str().to_string()
    }
}

pub trait Trade: Send + Sync + fmt::Debug + 'static {
    fn id(&self) -> Uuid;
    fn trade_type(&self) -> TradeExecutionType;
    fn side(&self) -> TradeSide;
    fn opening_fee(&self) -> u64;
    fn closing_fee(&self) -> u64;
    fn maintenance_margin(&self) -> i64;
    fn quantity(&self) -> Quantity;
    fn margin(&self) -> Margin;
    fn leverage(&self) -> Leverage;
    fn price(&self) -> Price;
    fn liquidation(&self) -> Price;
    fn stoploss(&self) -> Option<Price>;
    fn takeprofit(&self) -> Option<Price>;
    fn exit_price(&self) -> Option<Price>;
    fn creation_ts(&self) -> DateTime<Utc>;
    fn market_filled_ts(&self) -> Option<DateTime<Utc>>;
    fn closed_ts(&self) -> Option<DateTime<Utc>>;
    fn entry_price(&self) -> Option<Price>;
    fn entry_margin(&self) -> Option<Margin>;
    fn open(&self) -> bool;
    fn running(&self) -> bool;
    fn canceled(&self) -> bool;
    fn closed(&self) -> bool;
}

pub trait TradeRunning: Trade {
    fn pl_estimate(&self, market_price: Price) -> i64;
}

pub trait TradeClosed: Trade {
    fn pl(&self) -> i64;
}

#[derive(Deserialize, Debug, Clone)]
pub struct LnmTrade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: i64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    #[serde(with = "utils::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "utils::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "utils::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_ts: Option<DateTime<Utc>>,
    #[serde(with = "utils::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: i64,
}

impl LnmTrade {
    pub fn uid(&self) -> Uuid {
        self.uid
    }

    pub fn pl(&self) -> i64 {
        self.pl
    }

    pub fn sum_carry_fees(&self) -> i64 {
        self.sum_carry_fees
    }
}

impl Trade for LnmTrade {
    fn id(&self) -> Uuid {
        self.id
    }

    fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
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
        self.maintenance_margin
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
        self.stoploss
    }

    fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    fn exit_price(&self) -> Option<Price> {
        self.exit_price
    }

    fn creation_ts(&self) -> DateTime<Utc> {
        self.creation_ts
    }

    fn market_filled_ts(&self) -> Option<DateTime<Utc>> {
        self.market_filled_ts
    }

    fn closed_ts(&self) -> Option<DateTime<Utc>> {
        self.closed_ts
    }

    fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    fn open(&self) -> bool {
        self.open
    }

    fn running(&self) -> bool {
        self.running
    }

    fn canceled(&self) -> bool {
        self.canceled
    }

    fn closed(&self) -> bool {
        self.closed
    }
}

impl TradeRunning for LnmTrade {
    fn pl_estimate(&self, market_price: Price) -> i64 {
        pl_estimate(self.side(), self.quantity(), self.price(), market_price)
    }
}

impl TradeClosed for LnmTrade {
    fn pl(&self) -> i64 {
        self.pl
    }
}

#[derive(Deserialize)]
pub struct NestedTradesResponse {
    pub trades: Vec<LnmTrade>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum TradeUpdateType {
    Stoploss,
    Takeprofit,
}

impl TradeUpdateType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeUpdateType::Stoploss => "stoploss",
            TradeUpdateType::Takeprofit => "takeprofit",
        }
    }
}

#[derive(Serialize, Debug)]
pub struct FuturesUpdateTradeRequestBody {
    id: Uuid,
    #[serde(rename = "type")]
    update_type: TradeUpdateType,
    value: Price,
}

impl FuturesUpdateTradeRequestBody {
    pub fn new(id: Uuid, update_type: TradeUpdateType, value: Price) -> Self {
        Self {
            id,
            update_type,
            value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_liquidation_price() {
        // Test case 1: Buy side with min leverage

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1_000).unwrap();
        let entry_price = Price::try_from(110_000).unwrap();
        let leverage = Leverage::MIN;

        let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
        let expected_liquidation_price = Price::try_from(55_000).unwrap();

        assert_eq!(liquidation_price, expected_liquidation_price);

        // Test case 2: Buy side with max leverage

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1_000).unwrap();
        let entry_price = Price::try_from(110_000).unwrap();
        let leverage = Leverage::MAX;

        let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
        let expected_liquidation_price = Price::try_from(108_911).unwrap();

        assert_eq!(liquidation_price, expected_liquidation_price);

        // Test case 3: Sell side with min leverage

        let side = TradeSide::Sell;
        let quantity = Quantity::try_from(1_000).unwrap();
        let entry_price = Price::try_from(110_000).unwrap();
        let leverage = Leverage::MIN;

        let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
        let expected_liquidation_price = Price::MAX;

        assert_eq!(liquidation_price, expected_liquidation_price);

        // Test case 4: Sell side with max leverage

        let side = TradeSide::Sell;
        let quantity = Quantity::try_from(1_000).unwrap();
        let entry_price = Price::try_from(110_000).unwrap();
        let leverage = Leverage::MAX;

        let liquidation_price = estimate_liquidation_price(side, quantity, entry_price, leverage);
        let expected_liquidation_price = Price::try_from(111_111).unwrap();

        assert_eq!(liquidation_price, expected_liquidation_price);
    }

    #[test]
    fn test_pl_estimate_and_price_from_pl() {
        // Test case 1: Buy side with profit

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1_000).unwrap();
        let start_price = Price::try_from(110_000).unwrap();
        let end_price = Price::try_from(120_000).unwrap();

        let pl = pl_estimate(side, quantity, start_price, end_price);
        let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

        assert_eq!(pl, 75_757);
        assert_eq!(calculated_end_price, end_price);

        // Test case 2: Buy side with loss

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1_000).unwrap();
        let start_price = Price::try_from(110_000).unwrap();
        let end_price = Price::try_from(105_000).unwrap();

        let pl = pl_estimate(side, quantity, start_price, end_price);
        let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

        assert_eq!(pl, -43_291);
        assert_eq!(calculated_end_price, end_price);

        // Test case 3: Sell side with profit

        let side = TradeSide::Sell;
        let quantity = Quantity::try_from(1_000).unwrap();
        let start_price = Price::try_from(110_000).unwrap();
        let end_price = Price::try_from(90_000).unwrap();

        let pl = pl_estimate(side, quantity, start_price, end_price);
        let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

        assert_eq!(pl, 202_020);
        assert_eq!(calculated_end_price, end_price);

        // Test case 4: Sell side with loss

        let side = TradeSide::Sell;
        let quantity = Quantity::try_from(1_000).unwrap();
        let start_price = Price::try_from(110_000).unwrap();
        let end_price = Price::try_from(115_000).unwrap();

        let pl = pl_estimate(side, quantity, start_price, end_price);
        let calculated_end_price = price_from_pl(side, quantity, start_price, pl);

        assert_eq!(pl, -39_526);
        assert_eq!(calculated_end_price, end_price);
    }
}
