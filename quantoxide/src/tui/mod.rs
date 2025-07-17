mod backtest;
mod config;
mod core;
mod error;
mod live;
mod status;
mod sync;
mod terminal;
mod view;

pub(crate) use error::Result;

pub use backtest::BacktestTui;
pub use config::TuiConfig;
pub use core::TuiControllerShutdown;
pub use error::TuiError;
pub use live::LiveTui;
pub use status::{TuiStatus, TuiStatusStopped};
pub use sync::SyncTui;
