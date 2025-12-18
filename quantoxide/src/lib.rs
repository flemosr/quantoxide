#![doc = include_str!("../README.md")]

mod db;
mod shared;
pub mod signal;
pub mod sync;
pub mod trade;
pub mod tui;
mod util;

pub use db::Database;

pub mod error {
    pub use super::db::error::DbError;
    pub use super::shared::error::{
        LookbackPeriodValidationError, MinIterationIntervalValidationError,
    };
    pub use super::signal::{
        error::{SignalError, SignalValidationError},
        process::error::{
            SignalProcessError, SignalProcessFatalError, SignalProcessRecoverableError,
        },
    };
    pub use super::sync::{
        error::SyncError,
        process::{
            error::{SyncProcessFatalError, SyncProcessRecoverableError},
            real_time_collection_task::error::RealTimeCollectionError,
            sync_price_history_task::error::SyncPriceHistoryError,
        },
    };
    pub use super::trade::{
        backtest::error::BacktestError,
        error::{TradeCoreError, TradeExecutorError},
        live::{
            error::LiveError,
            executor::error::{
                ExecutorActionError, ExecutorProcessFatalError, ExecutorProcessRecoverableError,
                LiveTradeExecutorError,
            },
            process::error::{
                LiveProcessError, LiveProcessFatalError, LiveProcessRecoverableError,
            },
        },
    };
    pub use super::tui::error::TuiError;
    pub use super::util::PanicPayload;

    // Re-export selected `lnm-sdk::api_v3` errors for convenience
    pub use lnm_sdk::api_v3::error::{
        FuturesIsolatedTradeRequestValidationError, LeverageValidationError, MarginValidationError,
        PercentageCappedValidationError, PercentageValidationError, PriceValidationError,
        QuantityValidationError, RestApiError, RestApiV3Error, TradeValidationError,
    };

    /// Convenience general-purpose Result type alias.
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
}

pub mod models {
    pub use super::db::models::{OhlcCandleRow, PriceTickRow};
    pub use super::shared::{LookbackPeriod, MinIterationInterval};

    // Re-export selected `lnm-sdk::api_v3` models and utils for convenience
    pub use lnm_sdk::api_v3::models::{
        Leverage, Margin, Percentage, PercentageCapped, Price, Quantity, SATS_PER_BTC, Trade,
        TradeExecution, TradeExecutionType, TradeSide, TradeSize, TradeStatus, Uuid, trade_util,
    };
}

mod sealed {
    pub trait Sealed {}
}
