use std::fmt;

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

impl fmt::Display for PriceHistoryEntryLOCF {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "price_history_entry_locf:")?;
        writeln!(f, "  time: {}", self.time)?;
        writeln!(f, "  value: {:.2}", self.value)?;

        if let Some(ma_5) = self.ma_5 {
            writeln!(f, "  ma_5: {:.2}", ma_5)?;
        } else {
            writeln!(f, "  ma_5: null")?;
        }

        if let Some(ma_60) = self.ma_60 {
            writeln!(f, "  ma_60: {:.2}", ma_60)?;
        } else {
            writeln!(f, "  ma_60: null")?;
        }

        if let Some(ma_300) = self.ma_300 {
            writeln!(f, "  ma_300: {:.2}", ma_300)?;
        } else {
            writeln!(f, "  ma_300: null")?;
        }

        Ok(())
    }
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
