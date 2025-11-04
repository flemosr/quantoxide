pub mod db;
pub(crate) mod indicators;
pub mod signal;
pub mod sync;
pub mod trade; // WIP
pub mod tui; // TODO
pub(crate) mod util;

pub mod error {
    pub use super::db::error::DbError;
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
    pub use super::util::PanicPayload;
}

pub mod models {
    pub use super::db::models::{PriceHistoryEntry, PriceHistoryEntryLOCF, PriceTick};
}
