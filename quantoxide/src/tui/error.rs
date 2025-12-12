use std::{io, result};

use thiserror::Error;
use tokio::{
    sync::{broadcast::error::RecvError, mpsc::error::SendError},
    task::JoinError,
};

use crate::{
    sync::error::SyncError,
    trade::{error::TradeCoreError, live::error::LiveError},
};

use super::{TuiStatus, backtest::BacktestUiMessage, live::LiveUiMessage, sync::SyncUiMessage};

#[derive(Error, Debug)]
pub enum TuiError {
    #[error("TUI not running error: {0}")]
    TuiNotRunning(TuiStatus),

    #[error("Terminal setup error: {0}")]
    TerminalSetup(io::Error),

    #[error("Terminal restore error: {0}")]
    TerminalRestore(io::Error),

    #[error("Terminal event read error, {0}")]
    TerminalEventRead(io::Error),

    #[error("Draw failed, terminal already restored")]
    DrawTerminalAlreadyRestored,

    #[error("Draw failed error: {0}")]
    DrawFailed(io::Error),

    #[error("Open log file error: {0}")]
    LogFileOpen(io::Error),

    #[error("Write to log file error: {0}")]
    LogFileWrite(io::Error),

    #[error("Failed to send TUI process shutdown request error: {0}")]
    SendShutdownFailed(SendError<()>),

    #[error("TUI already shutdown error")]
    TuiAlreadyShutdown,

    #[error("TUI crashed without status update error")]
    TuiCrashedWithoutStatusUpdate,

    #[error("Failed to send shutdown completed signal error: {0}")]
    SendShutdownCompletedFailed(String),

    #[error("TaskJoin error {0}")]
    TaskJoin(JoinError),

    #[error("TUI shutdown timeout error")]
    ShutdownTimeout,

    #[error("TUI shutdown failed: {0}")]
    ShutdownFailed(String),

    #[error("Sync TUI send failed: {0}")]
    SyncTuiSendFailed(Box<SendError<SyncUiMessage>>),

    #[error("Sync recv error: {0}")]
    SyncRecv(RecvError),

    #[error("Sync engine already coupled")]
    SyncEngineAlreadyCoupled,

    #[error("Sync shutdown failed: {0}")]
    SyncShutdownFailed(SyncError),

    #[error("Live TUI send failed: {0}")]
    LiveTuiSendFailed(Box<SendError<LiveUiMessage>>),

    #[error("Live handle closed trade failed: {0}")]
    LiveHandleClosedTradeFailed(TradeCoreError),

    #[error("Live recv error: {0}")]
    LiveRecv(RecvError),

    #[error("Live trade engine already coupled")]
    LiveTradeEngineAlreadyCoupled,

    #[error("Live trade engine start failed: {0}")]
    LiveTradeEngineStartFailed(LiveError),

    #[error("Live shutdown failed: {0}")]
    LiveShutdownFailed(LiveError),

    #[error("Backtest TUI send failed: {0}")]
    BacktestTuiSendFailed(Box<SendError<BacktestUiMessage>>),

    #[error("Backtest recv error: {0}")]
    BacktestRecv(RecvError),

    #[error("Backtest engine already coupled")]
    BacktestEngineAlreadyCoupled,
}

pub(crate) type Result<T> = result::Result<T, TuiError>;
