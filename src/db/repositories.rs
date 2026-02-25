use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use lnm_sdk::{
    api_v2::models::PriceTick,
    api_v3::models::{FundingSettlement, OhlcCandle},
};

use crate::{shared::OhlcResolution, trade::TradeTrailingStoploss};

use super::{
    error::Result,
    models::{FundingSettlementRow, OhlcCandleRow, PriceTickRow},
};

#[async_trait]
pub(crate) trait PriceTicksRepository: Send + Sync {
    /// Adds multiple price ticks to the database in a single batch operation.
    /// Uses INSERT ON CONFLICT DO NOTHING to avoid duplicate entries.
    ///
    /// Returns only the ticks that were successfully inserted (new entries).
    async fn add_ticks(&self, ticks: &[PriceTick]) -> Result<Vec<PriceTickRow>>;

    async fn get_latest_entry(&self) -> Result<Option<(DateTime<Utc>, f64)>>;

    async fn get_price_range_from(
        &self,
        start: DateTime<Utc>,
    ) -> Result<Option<(f64, f64, DateTime<Utc>, f64)>>;

    async fn remove_ticks(&self, before: DateTime<Utc>) -> Result<()>;
}

#[async_trait]
pub(crate) trait RunningTradesRepository: Send + Sync {
    async fn add_running_trade(
        &self,
        trade_id: Uuid,
        trailing_stoploss: Option<TradeTrailingStoploss>,
    ) -> Result<()>;

    async fn get_running_trades_map(&self) -> Result<HashMap<Uuid, Option<TradeTrailingStoploss>>>;

    async fn remove_running_trades(&self, trade_ids: &[Uuid]) -> Result<()>;
}

#[async_trait]
pub(crate) trait OhlcCandlesRepository: Send + Sync {
    /// Adds OHLC candles to the database, distinguishing between stable and unstable candles.
    async fn add_candles(
        &self,
        before_candle_time: Option<DateTime<Utc>>,
        new_candles: &[OhlcCandle],
    ) -> Result<()>;

    async fn get_candles(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<OhlcCandleRow>>;

    /// Fetches OHLC candles consolidated to the specified resolution.
    async fn get_candles_consolidated(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        resolution: OhlcResolution,
    ) -> Result<Vec<OhlcCandleRow>>;

    async fn remove_gap_flag(&self, time: DateTime<Utc>) -> Result<()>;

    async fn get_earliest_stable_candle_time(&self) -> Result<Option<DateTime<Utc>>>;

    async fn get_latest_stable_candle_time(&self) -> Result<Option<DateTime<Utc>>>;

    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>>;

    /// Finds unflagged gaps in the candle history and marks surrounding candles as unstable
    /// so they can be re-fetched from the API.
    async fn flag_missing_candles(&self, range: Duration) -> Result<()>;
}

#[async_trait]
pub(crate) trait FundingSettlementsRepository: Send + Sync {
    /// Adds multiple funding settlements to the database. Idempotent.
    async fn add_settlements(&self, settlements: &[FundingSettlement]) -> Result<()>;

    /// Retrieves funding settlements within the specified time range, ordered by time ASC.
    async fn get_settlements(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<FundingSettlementRow>>;

    /// Returns the earliest settlement time in the database.
    async fn get_earliest_settlement_time(&self) -> Result<Option<DateTime<Utc>>>;

    /// Returns the latest settlement time in the database.
    async fn get_latest_settlement_time(&self) -> Result<Option<DateTime<Utc>>>;

    /// Returns the times of missing settlements on the funding settlement grid between the given
    /// bounds, ordered by time ASC. Handles all three LNM funding settlement grid phases
    /// transitions internally:
    /// + Phase A ({08} UTC, 24h)
    /// + Phase B ({04, 12, 20} UTC, 8h)
    /// + Phase C ({00, 08, 16} UTC, 8h)
    async fn get_missing_settlement_times(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>>;
}
