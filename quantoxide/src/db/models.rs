use std::fmt;

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::util::DateTimeExt;

#[allow(dead_code)]
#[derive(Debug, FromRow)]
pub struct PriceEntryRow {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub created_at: DateTime<Utc>,
    pub next: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow, Clone)]
pub struct PriceEntryLOCF {
    pub time: DateTime<Utc>,
    pub value: f64,
    pub ma_5: Option<f64>,
    pub ma_60: Option<f64>,
    pub ma_300: Option<f64>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct PartialPriceEntryLOCF {
    pub time: DateTime<Utc>,
    pub value: f64,
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

#[derive(Debug, Clone, FromRow, PartialEq)]
pub(crate) struct RunningTrade {
    pub trade_id: Uuid,
    pub trailing_stoploss: Option<f64>,
    pub created_at: DateTime<Utc>,
}
