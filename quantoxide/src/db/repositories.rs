use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use lnm_sdk::api::{
    rest::models::{BoundedPercentage, PriceEntryLNM},
    websocket::models::PriceTickLNM,
};

use crate::trade::core::TradeTrailingStoploss;

use super::{
    error::Result,
    models::{PriceHistoryEntry, PriceHistoryEntryLOCF, PriceTick},
};

#[async_trait]
pub trait PriceHistoryRepository: Send + Sync {
    /// Retrieves the earliest price history entry that has no successor.
    /// This represents a gap in the continuity of price history data.
    ///
    /// Returns:
    ///   - `Ok(Some(entry))` if a gap exists
    ///   - `Ok(None)` if no gaps are found
    ///   - `Err` on database errors
    async fn get_earliest_entry_gap(&self) -> Result<Option<PriceHistoryEntry>>;

    /// Retrieves the most recent price history entry.
    ///
    /// Returns:
    ///   - `Ok(Some(entry))` with the latest entry
    ///   - `Ok(None)` if no price history exists
    ///   - `Err` on database errors
    async fn get_latest_entry(&self) -> Result<Option<PriceHistoryEntry>>;

    /// Retrieves the oldest price history entry.
    ///
    /// Returns:
    ///   - `Ok(Some(entry))` with the earliest entry
    ///   - `Ok(None)` if no price history exists
    ///   - `Err` on database errors
    async fn get_earliest_entry(&self) -> Result<Option<PriceHistoryEntry>>;

    /// Retrieves the latest price history entry at or before the specified time.
    ///
    /// Parameters:
    ///   - `time`: The timestamp to find entries before
    ///
    /// Returns:
    ///   - `Ok(Some(entry))` with the latest entry at or before the given time
    ///   - `Ok(None)` if no entries exist  at or before the specified time
    ///   - `Err` on database errors
    async fn get_latest_entry_at_or_before(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceHistoryEntry>>;

    /// Retrieves the first price history entry that occurs after the specified time.
    ///
    /// Parameters:
    ///   - `time`: The timestamp to find entries after
    ///
    /// Returns:
    ///   - `Ok(Some(entry))` with the next entry after the given time
    ///   - `Ok(None)` if no entries exist after the specified time
    ///   - `Err` on database errors
    async fn get_earliest_entry_after(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceHistoryEntry>>;

    async fn get_first_entry_reaching_bounds(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        min: f64,
        max: f64,
    ) -> Result<Option<PriceHistoryEntry>>;

    /// Retrieves price history entries within the specified time range (inclusive).
    ///
    /// Parameters:
    ///   - `start`: The lower bound timestamp (inclusive)
    ///   - `end`: The upper bound timestamp (inclusive)
    ///
    /// Returns:
    ///   - `Ok(Vec<PriceHistoryEntry>)` containing entries ordered by time ascending
    ///   - `Err` on database errors
    async fn get_entries_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PriceHistoryEntry>>;

    /// Retrieves gaps in price history data from the database.
    ///
    /// This method finds time periods where price history data is missing by:
    /// 1. Identifying entries that have `None` in their 'next' field (indicating where data continuity breaks)
    /// 2. Finding the next available entry after each gap
    /// 3. Combining this information to return a list of gaps as (start_time, end_time) tuples
    ///
    /// # Returns
    /// - `Result<Vec<(DateTime<Utc>, DateTime<Utc>)>>`: A vector of tuples where each tuple represents
    ///   a gap in price history data. The first element (`from`) is the timestamp of the entry at the start
    ///   of the gap (where `next` is `None`), and the second element (`to`) is the timestamp of the next
    ///   entry after it (earliest entry with `time` > `from`).
    ///
    /// # Errors
    /// - Returns a database error if the query fails
    /// - Returns a generic error if the query returns unexpected results
    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>>;

    /// Adds multiple price entries to the history and updates related data structures.
    /// This includes:
    /// - Inserting entries into the price history table
    /// - Updating last-observed-carry-forward (LOCF) values
    /// - Recalculating moving averages
    ///
    /// Parameters:
    ///   - `entries`: Vector of price entries to add (newest first)
    ///   - `next_observed_time`: Time of the next price observation after these entries
    ///
    /// Returns:
    ///   - `Ok(())` on success
    ///   - `Err` on database or transaction errors
    async fn add_entries(
        &self,
        entries: &[PriceEntryLNM],
        next_observed_time: Option<DateTime<Utc>>,
    ) -> Result<()>;

    /// Updates the "next" pointer for a price history entry.
    /// Used to establish continuity between entries in the price history.
    ///
    /// Parameters:
    ///   - `entry_time`: Time of the entry to update
    ///   - `next`: Time of the entry that follows
    ///
    /// Returns:
    ///   - `Ok(true)` if an entry was updated
    ///   - `Ok(false)` if no entry was found or already had a next pointer
    ///   - `Err` on database errors
    async fn update_entry_next(
        &self,
        entry_time: DateTime<Utc>,
        next: DateTime<Utc>,
    ) -> Result<bool>;
}

#[async_trait]
pub trait PriceTicksRepository: Send + Sync {
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
    async fn add_tick(&self, tick: &PriceTickLNM) -> Result<Option<PriceTick>>;

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
    ) -> Result<Vec<PriceHistoryEntryLOCF>>;

    async fn remove_ticks(&self, before: DateTime<Utc>) -> Result<()>;
}
