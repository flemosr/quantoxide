use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::super::error::Result;

use super::models::PriceEntryLNM;

#[async_trait]
pub trait FuturesRepository: Send + Sync {
    async fn price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>>;
}
