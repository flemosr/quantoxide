pub const SATS_PER_BTC: f64 = 100_000_000.;

pub mod error;
mod leverage;
mod margin;
mod price;
mod price_history;
mod quantity;
mod serde_util;
mod ticker;
mod trade;
mod user;

pub use leverage::Leverage;
pub use margin::Margin;
pub use price::{BoundedPercentage, LowerBoundedPercentage, Price};
pub use price_history::PriceEntryLNM;
pub use quantity::Quantity;
pub use ticker::Ticker;
pub use trade::{
    FuturesTradeRequestBody, FuturesUpdateTradeRequestBody, LnmTrade, NestedTradesResponse, Trade,
    TradeClosed, TradeExecution, TradeExecutionType, TradeRunning, TradeSide, TradeSize,
    TradeStatus, TradeUpdateType, util as trade_util,
};
pub use user::{User, UserRole};
