pub(crate) mod backtest;
mod core;
pub(crate) mod error;
pub(crate) mod live;

pub use backtest::{
    config::BacktestConfig,
    engine::{BacktestController, BacktestEngine},
    state::{BacktestReceiver, BacktestStatus, BacktestUpdate},
};
pub use core::{
    ClosedTradeHistory, RawOperator, RunningTradesMap, SignalOperator, Stoploss, TradeClosed,
    TradeExecutor, TradeRunning, TradeTrailingStoploss, TradingState,
};
pub use live::{
    config::{LiveConfig, LiveTradeExecutorConfig},
    engine::{LiveController, LiveEngine},
    executor::{
        LiveTradeExecutor, LiveTradeExecutorLauncher,
        state::{
            LiveTradeExecutorStatus, LiveTradeExecutorStatusNotReady,
            live_trading_session::TradingSessionRefreshOffset,
        },
        update::{
            LiveTradeExecutorReceiver, LiveTradeExecutorUpdate, LiveTradeExecutorUpdateOrder,
        },
    },
    state::{LiveReader, LiveReceiver, LiveStatus, LiveUpdate},
};
