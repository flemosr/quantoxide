use std::fmt;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::util::DateTimeExt;

#[derive(Debug, Clone)]
pub(crate) struct RunningTrade {
    pub trade_id: Uuid,
    pub trailing_stoploss: Option<f64>,
}

/// Database row representing a single OHLC (Open, High, Low, Close) candlestick.
///
/// Contains aggregated price and volume data for a specific time period, along with metadata
/// indicating when the row was created and updated, and whether the candle is stable (finalized).
#[derive(Debug, Clone)]
pub struct OhlcCandleRow {
    pub time: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
            stable: true,
        }
    }

    /// Returns a formatted string representation of the candle data for display purposes.
    pub fn as_data_str(&self) -> String {
        let time_str = self.time.format_local_millis();
        let created_at_str = self.created_at.format_local_millis();
        let updated_at_str = self.updated_at.format_local_millis();
        let open_str = format!("{:.1}", self.open);
        let high_str = format!("{:.1}", self.high);
        let low_str = format!("{:.1}", self.low);
        let close_str = format!("{:.1}", self.close);

        format!(
            "time: {time_str}\n\
             open: {open_str}\n\
             high: {high_str}\n\
             low: {low_str}\n\
             close: {close_str}\n\
             volume: {}\n\
             stable: {}\n\
             created_at: {created_at_str}\n\
             updated_at: {updated_at_str}",
            self.volume, self.stable
        )
    }
}

impl fmt::Display for OhlcCandleRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OHLC Candle Row:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}

/// Database row representing a single price tick observation.
///
/// Contains the last traded price at a specific point in time, along with metadata indicating when
/// the observation was recorded.
#[derive(Debug, Clone)]
pub struct PriceTickRow {
    pub time: DateTime<Utc>,
    pub last_price: f64,
    pub created_at: DateTime<Utc>,
}

impl PriceTickRow {
    /// Returns a formatted string representation of the price tick data for display purposes.
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

/// Database row representing a funding settlement.
///
/// Contains the fixing price and funding rate at a specific settlement time.
#[derive(Debug, Clone)]
pub struct FundingSettlementRow {
    pub id: Uuid,
    pub time: DateTime<Utc>,
    pub fixing_price: f64,
    pub funding_rate: f64,
    pub created_at: DateTime<Utc>,
}

impl FundingSettlementRow {
    /// Returns a formatted string representation of the funding settlement data for display
    /// purposes.
    pub fn as_data_str(&self) -> String {
        let time_str = self.time.format_local_millis();
        let created_at_str = self.created_at.format_local_millis();
        let fixing_price_str = format!("{:.1}", self.fixing_price);

        format!(
            "id: {}\n\
             time: {time_str}\n\
             fixing_price: {fixing_price_str}\n\
             funding_rate: {:.6}\n\
             created_at: {created_at_str}",
            self.id, self.funding_rate
        )
    }
}

impl fmt::Display for FundingSettlementRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Funding Settlement Row:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
