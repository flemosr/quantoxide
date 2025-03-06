use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct PriceHistoryEntry {
    pub id: i32,
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub created_at: DateTime<Utc>,
}
