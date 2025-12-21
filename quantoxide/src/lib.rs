#![doc = include_str!("../README.md")]

mod db;
mod shared;
/// Exports [`SignalActionEvaluator`] and other types related to signal evaluation.
///
/// [`SignalActionEvaluator`]: crate::signal::SignalActionEvaluator
pub mod signal;
/// Exports [`SyncEngine`] and other types related to price data synchronization.
///
/// [`SyncEngine`]: crate::sync::SyncEngine
pub mod sync;
/// Exports [`BacktestEngine`], [`LiveTradeEngine`], [`LiveTradeExecutor`], and other types related
/// to trading execution.
///
/// [`BacktestEngine`]: crate::trade::BacktestEngine
/// [`LiveTradeEngine`]: crate::trade::LiveTradeEngine
/// [`LiveTradeExecutor`]: crate::trade::LiveTradeExecutor
pub mod trade;
/// Exports [`SyncTui`], [`BacktestTui`], [`LiveTui`], and other types related to Terminal User
/// Interfaces (TUIs).
///
/// [`SyncTui`]: crate::tui::SyncTui
/// [`BacktestTui`]: crate::tui::BacktestTui
/// [`LiveTui`]: crate::tui::LiveTui
pub mod tui;
mod util;

pub use db::Database;

/// Error types returned by `quantoxide`.
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

/// Exports database models, shared configuration types, and selected `lnm-sdk` models.
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
