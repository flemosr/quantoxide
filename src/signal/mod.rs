mod config;
mod core;
mod engine;
pub(crate) mod error;
pub(crate) mod process;
mod state;

pub use config::LiveSignalConfig;
pub use core::{
    ConfiguredSignalEvaluator, Signal, SignalAction, SignalActionEvaluator, SignalEvaluator,
    SignalExtra, SignalName,
};
pub use engine::{LiveSignalController, LiveSignalEngine};
pub use state::{
    LiveSignalReader, LiveSignalReceiver, LiveSignalStatus, LiveSignalStatusNotRunning,
    LiveSignalUpdate,
};
