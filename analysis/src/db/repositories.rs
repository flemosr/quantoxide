use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::api::rest::models::PriceEntryLNM;

use super::{
    error::Result,
    models::{PriceHistoryEntry, PriceHistoryEntryLOCF},
};

#[async_trait]
pub trait PriceHistoryRepository: Send + Sync {
    async fn get_earliest_entry_gap(&self) -> Result<Option<PriceHistoryEntry>>;

    async fn get_latest_entry(&self) -> Result<Option<PriceHistoryEntry>>;

    async fn get_earliest_entry(&self) -> Result<Option<PriceHistoryEntry>>;

    async fn get_earliest_entry_after(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceHistoryEntry>>;

    async fn add_entries(
        &self,
        entries: &Vec<PriceEntryLNM>,
        next_observed_time: Option<&DateTime<Utc>>,
    ) -> Result<()>;

    async fn eval_entries_locf(
        &self,
        time: &DateTime<Utc>,
        range_secs: usize,
    ) -> Result<Vec<PriceHistoryEntryLOCF>>;

    async fn update_entry_next(
        &self,
        entry_time: &DateTime<Utc>,
        next: &DateTime<Utc>,
    ) -> Result<bool>;
}
