use std::{num::NonZeroU64, result, sync::Arc};

use chrono::Duration;
use thiserror::Error;
use uuid::Uuid;

use lnm_sdk::api_v3::error::{PriceValidationError, RestApiError, TradeValidationError};

use crate::{
    db::error::DbError,
    sync::{SyncMode, process::error::SyncProcessFatalError},
};

use super::{
    super::super::error::TradeCoreError,
    state::{
        LiveTradeExecutorStatus, LiveTradeExecutorStatusNotReady,
        live_trading_session::TradingSessionTTL,
    },
};

#[derive(Error, Debug)]
#[non_exhaustive]
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

    #[error("Closed history update error")]
    ClosedHistoryUpdate(TradeCoreError),

    #[error("Updated trades {trade_ids:?} not running")]
    UpdatedTradesNotRunning { trade_ids: Vec<Uuid> },

    #[error("Trade {trade_id} is not closed")]
    TradeNotClosed { trade_id: Uuid },

    #[error("Trade {trade_id} is not registered")]
    TradeNotRegistered { trade_id: Uuid },

    #[error("Live trade executor is not ready. No session.")]
    ExecutorNotReadyNoSession,

    #[error("Live trade executor is not ready: {0}")]
    ExecutorNotReady(LiveTradeExecutorStatusNotReady),

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
        "`TradingSessionTTL` must be at least {}. Value: {value}",
        TradingSessionTTL::MIN
    )]
    InvalidTradingSessionTTL { value: Duration },

    #[error("Unexpected closed trade returned by the server. Id: {trade_id}")]
    UnexpectedClosedTrade { trade_id: Uuid },

    #[error("Closed trade not confirmed by the server. Id: {trade_id}")]
    ClosedTradeNotConfirmed { trade_id: Uuid },
}

pub(super) type ExecutorActionResult<T> = result::Result<T, ExecutorActionError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ExecutorProcessRecoverableError {
    #[error("Live Trade Session evaluation error {0}")]
    LiveTradeSessionEvaluation(ExecutorActionError),

    #[error("`SyncRecvLagged` error, skipped: {skipped}")]
    SyncRecvLagged { skipped: u64 },
}

#[derive(Error, Debug)]
#[non_exhaustive]
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

pub(super) type ExecutorProcessFatalResult<T> = result::Result<T, ExecutorProcessFatalError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum LiveTradeExecutorError {
    #[error("REST API client initialization error: {0}")]
    RestApiInit(RestApiError),

    #[error("Sync engine live price feed is not active. Mode: {0}")]
    SyncEngineLiveFeedInactive(SyncMode),

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

pub(super) type LiveTradeExecutorResult<T> = result::Result<T, LiveTradeExecutorError>;
