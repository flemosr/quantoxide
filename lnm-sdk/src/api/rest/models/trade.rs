use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{Leverage, Margin, Price, Quantity, error::FuturesTradeRequestValidationError};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TradeType {
    #[serde(rename = "m")]
    Market,
    #[serde(rename = "l")]
    Limit,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TradeSide {
    #[serde(rename = "b")]
    Buy,
    #[serde(rename = "s")]
    Sell,
}

#[derive(Serialize)]
pub struct FuturesTradeRequestBody {
    leverage: Leverage,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    side: TradeSide,

    #[serde(skip_serializing_if = "Option::is_none")]
    quantity: Option<Quantity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    margin: Option<Margin>,

    #[serde(rename = "type")]
    trade_type: TradeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        quantity: Option<Quantity>,
        margin: Option<Margin>,
        trade_type: TradeType,
        price: Option<Price>,
    ) -> Result<Self, FuturesTradeRequestValidationError> {
        match (quantity, margin) {
            (None, None) => {
                return Err(FuturesTradeRequestValidationError::MissingQuantityAndMargin);
            }
            (Some(_), Some(_)) => {
                return Err(FuturesTradeRequestValidationError::BothQuantityAndMarginProvided);
            }
            _ => {}
        }

        match (&trade_type, price) {
            (TradeType::Market, Some(_)) => {
                return Err(FuturesTradeRequestValidationError::PriceSetForMarketOrder);
            }
            (TradeType::Limit, None) => {
                return Err(FuturesTradeRequestValidationError::MissingPriceForLimitOrder);
            }
            _ => {}
        }

        match (margin, price) {
            (Some(margin), Some(price)) => {
                let _ = Quantity::try_calculate(margin, price, leverage)?;
            }
            _ => {}
        };

        if let Some(price_val) = price {
            if let Some(stoploss_val) = stoploss {
                if stoploss_val >= price_val {
                    return Err(FuturesTradeRequestValidationError::StopLossHigherThanPrice);
                }
            }

            if let Some(takeprofit_val) = takeprofit {
                if takeprofit_val <= price_val {
                    return Err(FuturesTradeRequestValidationError::TakeProfitLowerThanPrice);
                }
            }
        }

        Ok(FuturesTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            quantity,
            margin,
            trade_type,
            price,
        })
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Trade {
    id: Uuid,
    uid: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: u64,
    quantity: u64,
    margin: u64,
    leverage: f64,
    price: f64,
    liquidation: f64,
    stoploss: f64,
    takeprofit: f64,
    exit_price: Option<f64>,
    pl: u64,
    #[serde(with = "ts_milliseconds")]
    creation_ts: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    market_filled_ts: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_ts: Option<DateTime<Utc>>,
    entry_price: Option<f64>,
    entry_margin: Option<u64>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_carry_fees: u64,
}
