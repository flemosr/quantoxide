use std::{num::NonZeroU64, result, sync::Arc};

use chrono::Duration;
use thiserror::Error;
use uuid::Uuid;

use lnm_sdk::error::{PriceValidationError, RestApiError, TradeValidationError};

use crate::{db::error::DbError, sync::process::error::SyncProcessFatalError};

use super::{
    super::super::error::TradeCoreError,
    state::{LiveTradeExecutorStatus, TradingSessionRefreshOffset},
};

#[derive(Error, Debug)]
pub enum ExecutorActionError {
    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("[Db] {0}")]
    Db(#[from] DbError),

    #[error("Db is empty error")]
    DbIsEmpty,

    #[error("Stoploss evaluation error")]
    StoplossEvaluation(TradeCoreError),

    #[error("New Trade {trade_id} is not running")]
    NewTradeNotRunning { trade_id: Uuid },

    #[error("Trade {trade_id} is already registered")]
    TradeAlreadyRegistered { trade_id: Uuid },

    #[error("Price Trigger update error")]
    PriceTriggerUpdate(TradeCoreError),

    #[error("Updated trades {trade_ids:?} not running")]
    UpdatedTradesNotRunning { trade_ids: Vec<Uuid> },

    #[error("Trade {trade_id} is not closed")]
    TradeNotClosed { trade_id: Uuid },

    #[error("Trade {trade_id} is not registered")]
    TradeNotRegistered { trade_id: Uuid },

    #[error("Live trade executor is not ready")]
    ExecutorNotReady,

    #[error("[InvalidMarketPrice] {0}")]
    InvalidMarketPrice(PriceValidationError),

    #[error("Invalid trade params error {0}")]
    InvalidTradeParams(TradeValidationError),

    #[error("Max running trades ({max_qtd}) reached")]
    MaxRunningTradesReached { max_qtd: usize },

    #[error("Margin amount {amount} exceeds maximum amount {max_amount} for trade")]
    MarginAmountExceedsMaxForTrade { amount: NonZeroU64, max_amount: u64 },

    #[error("Cash-in amount {amount} exceeds maximum cash-in {max_cash_in} for trade")]
    CashInAmountExceedsMaxForTrade {
        amount: NonZeroU64,
        max_cash_in: u64,
    },

    #[error(
        "`TradingSessionRefreshOffset` must be at least {}. Value: {value}",
        TradingSessionRefreshOffset::MIN
    )]
    InvalidTradingSessionRefreshOffset { value: Duration },
}

pub type ExecutorActionResult<T> = result::Result<T, ExecutorActionError>;

#[derive(Error, Debug)]
pub enum ExecutorProcessRecoverableError {
    #[error("Live Trade Session evaluation error {0}")]
    LiveTradeSessionEvaluation(ExecutorActionError),

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },
}

#[derive(Error, Debug)]
pub enum ExecutorProcessFatalError {
    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>),

    #[error("Failed to close trades on shutdown: {0}")]
    FailedToCloseTradesOnShutdown(ExecutorActionError),

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown,

    #[error("`SyncRecvClosed` error")]
    SyncRecvClosed,
}

pub type ExecutorProcessFatalResult<T> = result::Result<T, ExecutorProcessFatalError>;

#[derive(Error, Debug)]
pub enum LiveTradeExecutorError {
    #[error("Launch clean up error {0}")]
    LaunchCleanUp(ExecutorActionError),

    #[error("API credentials were not set")]
    ApiCredentialsNotSet,

    #[error("Executor process already consumed")]
    ExecutorProcessAlreadyConsumed,

    #[error("Executor process already terminated error, status: {0}")]
    ExecutorProcessAlreadyTerminated(LiveTradeExecutorStatus),

    #[error("Executor shutdown procedure failed: {0}")]
    ExecutorShutdownFailed(Arc<ExecutorProcessFatalError>),
}

pub type LiveTradeExecutorResult<T> = result::Result<T, LiveTradeExecutorError>;
