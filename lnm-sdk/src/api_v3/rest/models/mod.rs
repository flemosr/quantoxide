use chrono::{
    DateTime, Utc,
    serde::{ts_milliseconds, ts_milliseconds_option},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::shared::models::trade::TradeSide;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
#[serde(rename_all = "lowercase")]
pub enum TradeExecutionType {
    Market,
    Limit,
}

// #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Copy)]
// #[serde(rename_all = "lowercase")]
// pub enum TradeSide {
//     Buy,
//     Sell,
// }

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
    // quantity: Quantity,
    // margin: Margin,
    // leverage: Leverage,
    // price: Price,
    // liquidation: Price,
    // #[serde(with = "serde_util::price_option")]
    // stoploss: Option<Price>,
    // #[serde(with = "serde_util::price_option")]
    // takeprofit: Option<Price>,
    // #[serde(with = "serde_util::price_option")]
    // exit_price: Option<Price>,
    // pl: i64,
    #[serde(with = "ts_milliseconds")]
    created_at: DateTime<Utc>,
    #[serde(with = "ts_milliseconds_option")]
    filled_at: Option<DateTime<Utc>>,
    #[serde(with = "ts_milliseconds_option")]
    closed_at: Option<DateTime<Utc>>,
    // #[serde(with = "serde_util::price_option")]
    // entry_price: Option<Price>,
    // entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_funding_fees: i64,
    client_id: String,
}
