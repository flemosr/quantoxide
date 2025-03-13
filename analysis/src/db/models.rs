use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct PriceEntry {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub created_at: DateTime<Utc>,
    pub next: Option<DateTime<Utc>>,
}
