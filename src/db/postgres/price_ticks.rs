use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres};

use lnm_sdk::api_v2::models::PriceTick;

use super::super::{
    error::{DbError, Result},
    models::PriceTickRow,
    repositories::PriceTicksRepository,
};

pub(crate) struct PgPriceTicksRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgPriceTicksRepo {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<Postgres> {
        self.pool.as_ref()
    }
}

#[async_trait]
impl PriceTicksRepository for PgPriceTicksRepo {
    async fn add_ticks(&self, ticks: &[PriceTick]) -> Result<Vec<PriceTickRow>> {
        if ticks.is_empty() {
            return Ok(Vec::new());
        }

        let mut times = Vec::with_capacity(ticks.len());
        let mut prices = Vec::with_capacity(ticks.len());

        for tick in ticks {
            times.push(tick.time());
            prices.push(tick.last_price());
        }

        let inserted = sqlx::query_as!(
            PriceTickRow,
            r#"
                INSERT INTO price_ticks (time, last_price)
                SELECT * FROM unnest($1::timestamptz[], $2::float8[])
                ON CONFLICT (time) DO NOTHING
                RETURNING time, last_price, created_at
            "#,
            &times,
            &prices,
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(inserted)
    }

    async fn get_latest_entry(&self) -> Result<Option<(DateTime<Utc>, f64)>> {
        struct PriceEntry {
            pub time: DateTime<Utc>,
            pub price: f64,
        }

        let last_tick_opt = sqlx::query_as!(
            PriceEntry,
            r#"
                SELECT time, last_price as price
                FROM price_ticks
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        let last_candle_opt = sqlx::query_as!(
            PriceEntry,
            r#"
                SELECT time, close as price
                FROM ohlc_candles
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        // Prefer candle over tick when times are equal, since candles are minute-floored.
        let latest_entry = [last_tick_opt, last_candle_opt]
            .into_iter()
            .flatten()
            .max_by_key(|entry| entry.time)
            .map(|entry| (entry.time, entry.price));

        Ok(latest_entry)
    }

    async fn get_price_range_from(
        &self,
        start: DateTime<Utc>,
    ) -> Result<Option<(f64, f64, DateTime<Utc>, f64)>> {
        struct PriceEntry {
            pub time: DateTime<Utc>,
            pub price: f64,
        }

        let tick_entries = sqlx::query_as!(
            PriceEntry,
            r#"
                SELECT time, last_price as price
                FROM price_ticks
                WHERE time >= $1
                ORDER BY time ASC
            "#,
            start
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        struct OhlcCandlePartial {
            pub time: DateTime<Utc>,
            pub high: f64,
            pub low: f64,
            pub close: f64,
        }

        let candle_entries = sqlx::query_as!(
            OhlcCandlePartial,
            r#"
                SELECT time, high, low, close
                FROM ohlc_candles
                WHERE time >= $1
                ORDER BY time ASC
            "#,
            start
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        if tick_entries.is_empty() && candle_entries.is_empty() {
            return Ok(None);
        }

        let mut min_price = f64::INFINITY;
        let mut max_price = f64::NEG_INFINITY;

        for entry in &tick_entries {
            let entry_price = entry.price;
            if entry_price < min_price {
                min_price = entry_price;
            }
            if entry_price > max_price {
                max_price = entry_price;
            }
        }

        for candle in &candle_entries {
            if candle.low < min_price {
                min_price = candle.low;
            }
            if candle.high > max_price {
                max_price = candle.high;
            }
        }

        let last_tick_opt = tick_entries.into_iter().last();

        let last_candle_opt = candle_entries.into_iter().last().map(|c| PriceEntry {
            time: c.time,
            price: c.close,
        });

        // Prefer candle over tick when times are equal, since candles are minute-floored.
        let latest_entry = [last_tick_opt, last_candle_opt]
            .into_iter()
            .flatten()
            .max_by_key(|entry| entry.time)
            .expect("at least one entry exists");

        Ok(Some((
            min_price,
            max_price,
            latest_entry.time,
            latest_entry.price,
        )))
    }

    async fn remove_ticks(&self, before: DateTime<Utc>) -> Result<()> {
        sqlx::query!("DELETE FROM price_ticks WHERE time <= $1", before)
            .execute(self.pool())
            .await
            .map_err(DbError::Query)?;

        Ok(())
    }
}
