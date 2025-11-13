use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api_v3::rest::models::error::FuturesIsolatedTradeRequestValidationError,
    shared::models::{
        leverage::Leverage,
        margin::Margin,
        price::Price,
        quantity::Quantity,
        serde_util,
        trade::{TradeExecution, TradeExecutionType, TradeSide, TradeSize},
    },
};

#[derive(Serialize, Debug)]
pub(in crate::api_v3) struct FuturesIsolatedTradeRequestBody {
    leverage: Leverage,
    side: TradeSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    client_id: Option<String>,
    #[serde(flatten)]
    size: TradeSize,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesIsolatedTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        client_id: Option<String>,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesIsolatedTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss {
                if stoploss >= price {
                    return Err(
                        FuturesIsolatedTradeRequestValidationError::StopLossHigherThanPrice,
                    );
                }
            }

            if let Some(takeprofit) = takeprofit {
                if takeprofit <= price {
                    return Err(
                        FuturesIsolatedTradeRequestValidationError::TakeProfitLowerThanPrice,
                    );
                }
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        if client_id
            .as_ref()
            .map_or(false, |client_id| client_id.len() > 64)
        {
            return Err(FuturesIsolatedTradeRequestValidationError::ClientIdTooLong);
        }

        Ok(FuturesIsolatedTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            client_id,
            size,
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
    #[serde(with = "serde_util::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    #[serde(with = "ts_milliseconds")]
    created_at: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    filled_at: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_at: Option<DateTime<Utc>>,
    #[serde(with = "serde_util::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_funding_fees: i64,
    client_id: String,
}
