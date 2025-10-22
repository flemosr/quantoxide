use std::{num::NonZeroU64, result, sync::Arc};

use thiserror::Error;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

use lnm_sdk::api::rest::{
    error::RestApiError,
    models::error::{PriceValidationError, TradeValidationError},
};

use crate::{db::error::DbError, sync::SyncProcessFatalError};

use super::super::super::error::TradeCoreError;

#[derive(Error, Debug)]
pub enum LiveTradeExecutorError {
    #[error("[RestApi] {0}")]
    RestApi(#[from] RestApiError),

    #[error("Balance is too low error")]
    BalanceTooLow,

    #[error("Balance is too high error")]
    BalanceTooHigh,

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
    //
    #[error("[InvalidMarketPrice] {0}")]
    InvalidMarketPrice(PriceValidationError),

    #[error("Invalid trade params error {0}")]
    InvalidTradeParams(TradeValidationError),

    #[error("Max running trades ({max_qtd}) reached")]
    MaxRunningTradesReached { max_qtd: usize },

    #[error("Trade executor process already consumed")]
    TradeExecutorProcessAlreadyConsumed,

    #[error("Failed to close trades on shutdown: {0}")]
    FailedToCloseTradesOnShutdown(String),

    #[error("Margin amount {amount} exceeds maximum amount {max_amount} for trade")]
    MarginAmountExceedsMaxForTrade { amount: NonZeroU64, max_amount: u64 },

    #[error("Cash-in amount {amount} exceeds maximum cash-in {max_cash_in} for trade")]
    CashInAmountExceedsMaxForTrade {
        amount: NonZeroU64,
        max_cash_in: u64,
    },

    #[error("API credentials were not set")]
    ApiCredentialsNotSet,

    #[error("`Sync` process (dependency) was terminated with error: {0}")]
    SyncProcessTerminated(Arc<SyncProcessFatalError>), // Not recoverable

    #[error("`Sync` process (dependency) was shutdown")]
    SyncProcessShutdown,

    #[error("`Sync` `RecvError` error: {0}")]
    SyncRecv(RecvError),
}

pub type LiveTradeExecutorResult<T> = result::Result<T, LiveTradeExecutorError>;
