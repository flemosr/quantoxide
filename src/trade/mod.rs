pub(crate) mod backtest;
mod core;
pub(crate) mod error;
pub(crate) mod live;

pub use backtest::{
    config::{BacktestConfig, MIN_BUFFER_SIZE},
    parallel::{controller::BacktestParallelController, engine::BacktestParallelEngine},
    single::{controller::BacktestController, engine::BacktestEngine},
    state::{
        BacktestParallelReceiver, BacktestParallelUpdate, BacktestReceiver, BacktestStatus,
        BacktestUpdate,
    },
};
pub use core::{
    ClosedTradeHistory, CrossExposure, CrossExposureRunning, CrossPositionCore, CrossQuantity,
    DynRunningTradesMap, Raw, RawOperator, RunningTradesMap, SignalOperator, Stoploss, TradeClosed,
    TradeCore, TradeExecutor, TradeReference, TradeRunning, TradeTrailingStoploss, TradingState,
};
pub use error::{CrossExposureValidationError, CrossQuantityValidationError};
pub use live::{
    config::{LiveTradeConfig, LiveTradeExecutorConfig},
    engine::{LiveTradeController, LiveTradeEngine},
    executor::{
        LiveTradeExecutor, LiveTradeExecutorLauncher,
        state::{LiveTradeExecutorStatus, LiveTradeExecutorStatusNotReady},
        update::{
            LiveTradeExecutorReceiver, LiveTradeExecutorUpdate, LiveTradeExecutorUpdateOrder,
        },
    },
    state::{LiveTradeReader, LiveTradeReceiver, LiveTradeStatus, LiveTradeUpdate},
};
