pub const SATS_PER_BTC: f64 = 100_000_000.;

pub mod error;
mod leverage;
mod margin;
mod price;
mod price_history;
mod quantity;
mod ticker;
mod trade;
mod user;
mod utils;

pub use leverage::Leverage;
pub use margin::Margin;
pub use price::{BoundedPercentage, LowerBoundedPercentage, Price};
pub use price_history::PriceEntryLNM;
pub use quantity::Quantity;
pub use ticker::Ticker;
pub use trade::{
    FuturesTradeRequestBody, FuturesUpdateTradeRequestBody, LnmTrade, NestedTradesResponse, Trade,
    TradeClosed, TradeExecution, TradeExecutionType, TradeRunning, TradeSide, TradeSize,
    TradeStatus, TradeUpdateType, estimate_liquidation_price, pl_estimate, price_from_pl,
};
pub use user::{User, UserRole};
