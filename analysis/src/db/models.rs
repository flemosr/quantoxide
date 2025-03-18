use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct PriceEntry {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub created_at: DateTime<Utc>,
    pub next: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
pub struct PriceEntryLOCF {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub ma_5: Option<f64>,
    pub ma_60: Option<f64>,
    pub ma_300: Option<f64>,
}
