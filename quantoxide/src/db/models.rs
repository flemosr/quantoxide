use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct PriceHistoryEntry {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub created_at: DateTime<Utc>,
    pub next: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow, Clone)]
pub struct PriceHistoryEntryLOCF {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub ma_5: Option<f64>,
    pub ma_60: Option<f64>,
    pub ma_300: Option<f64>,
}

#[derive(Debug, FromRow, Clone)]
pub struct PartialPriceHistoryEntryLOCF {
    pub time: DateTime<Utc>,
    pub value: f64,
}

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct PriceTick {
    pub time: DateTime<Utc>,
    pub last_price: f64,
    pub created_at: DateTime<Utc>,
}
