mod config;
mod core;
mod engine;
pub(crate) mod error;
pub(crate) mod process;
mod state;

pub use config::LiveSignalConfig;
pub use core::{Signal, SignalEvaluator};
pub use engine::{LiveSignalController, LiveSignalEngine};
pub use state::{
    LiveSignalReader, LiveSignalReceiver, LiveSignalStatus, LiveSignalStatusNotRunning,
    LiveSignalUpdate,
};

// Internal re-exports
pub(crate) use core::WrappedSignalEvaluator;
