use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::{api_v2::models::PriceTick, api_v3::models::OhlcCandle};

use crate::{db::models::OhlcCandleRow, trade::TradeTrailingStoploss};

use super::{
    error::Result,
    models::{PriceEntryLOCF, PriceTickRow},
};

#[async_trait]
pub(crate) trait PriceTicksRepository: Send + Sync {
    /// Adds a new price tick to the database.
    /// Uses INSERT ON CONFLICT DO NOTHING to avoid duplicate entries for the same timestamp.
    ///
    /// Parameters:
    ///   - `tick`: The price tick data to insert
    ///
    /// Returns:
    ///   - `Ok(Some(PriceTick))` if the tick was successfully inserted (new entry)
    ///   - `Ok(None)` if the tick already existed in the database (no insertion occurred)
    ///   - `Err` on database errors
    async fn add_tick(&self, tick: &PriceTick) -> Result<Option<PriceTickRow>>;

    async fn get_latest_entry(&self) -> Result<Option<(DateTime<Utc>, f64)>>;

    async fn get_price_range_from(
        &self,
        start: DateTime<Utc>,
    ) -> Result<Option<(f64, f64, DateTime<Utc>, f64)>>;

    /// Computes Last Observation Carried Forward (LOCF) price entries with technical indicators
    /// for a specified time range ending at the given timestamp.
    ///
    /// This method attempts to retrieve pre-computed LOCF entries from the `price_history_locf`
    /// table first. If the required entries are not available (e.g., for real-time data or gaps
    /// in historical data), it dynamically computes them by:
    /// 1. Fetching raw price data from both `price_history` and `price_ticks` tables
    /// 2. Applying LOCF logic to fill gaps in the time series
    /// 3. Computing technical indicators using `IndicatorsEvaluator`
    ///
    /// # Arguments
    ///
    /// * `time` - The end timestamp for the range (will be truncated to seconds)
    /// * `range_secs` - The number of seconds to include in the range (working backwards from `time`)
    ///
    /// # Returns
    ///
    /// Returns a `Vec<PriceHistoryEntryLOCF>` containing exactly `range_secs` entries if successful,
    /// ordered chronologically from earliest to latest.
    ///
    /// # Errors
    ///
    /// Returns `DbError` if:
    /// - No price data exists at or before the start of the requested range
    /// - Database queries fail
    /// - Indicator calculation fails due to data inconsistencies
    async fn compute_locf_entries_for_range(
        &self,
        time: DateTime<Utc>,
        range_secs: usize,
    ) -> Result<Vec<PriceEntryLOCF>>;

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

    async fn remove_gap_flag(&self, time: DateTime<Utc>) -> Result<()>;

    async fn get_earliest_stable_candle(&self) -> Result<Option<OhlcCandleRow>>;

    async fn get_latest_stable_candle(&self) -> Result<Option<OhlcCandleRow>>;

    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>>;

    /// Finds unflagged gaps in the candle history and marks surrounding candles as unstable
    /// so they can be re-fetched from the API.
    async fn flag_missing_candles(&self) -> Result<()>;
}
