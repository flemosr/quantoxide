use std::fmt;

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::util::DateTimeExt;

#[derive(Debug, Clone, FromRow, PartialEq)]
pub(crate) struct RunningTrade {
    pub trade_id: Uuid,
    pub trailing_stoploss: Option<f64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub struct OhlcCandleRow {
    pub time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub gap: bool,
    pub stable: bool,
}

impl OhlcCandleRow {
    #[cfg(test)]
    pub(crate) fn new_simple(time: DateTime<Utc>, price: f64, volume: i64) -> Self {
        Self {
            time,
            open: price,
            high: price,
            low: price,
            close: price,
            volume,
            created_at: time,
            updated_at: time,
            gap: false,
            stable: true,
        }
    }
}

#[derive(Debug, Clone, FromRow, PartialEq)]
pub struct PriceTickRow {
    pub time: DateTime<Utc>,
    pub last_price: f64,
    pub created_at: DateTime<Utc>,
}

impl PriceTickRow {
    pub fn as_data_str(&self) -> String {
        let time_str = self.time.format_local_millis();
        let created_at_str = self.created_at.format_local_millis();
        let price_str = format!("{:.1}", self.last_price);

        format!("time: {time_str}\nprice: {price_str}\ncreated_at: {created_at_str}")
    }
}

impl fmt::Display for PriceTickRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Price Tick Row:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
