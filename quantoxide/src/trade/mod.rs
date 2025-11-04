mod core;
pub(crate) mod error;

pub mod backtest;
pub mod live;

pub use core::{
    ClosedTradeHistory, RawOperator, RunningTradesMap, SignalOperator, Stoploss, TradeExecutor,
    TradeExt, TradeTrailingStoploss, TradingState,
};
