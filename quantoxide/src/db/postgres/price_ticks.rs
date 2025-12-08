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
    async fn add_tick(&self, tick: &PriceTick) -> Result<Option<PriceTickRow>> {
        let price_tick = sqlx::query_as!(
            PriceTickRow,
            r#"
                INSERT INTO price_ticks (time, last_price)
                VALUES ($1, $2)
                ON CONFLICT (time) DO NOTHING
                RETURNING time, last_price, created_at
            "#,
            tick.time(),
            tick.last_price(),
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(price_tick)
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
        let entries = sqlx::query_as!(
            PriceTickRow,
            r#"
                SELECT time, last_price, created_at
                FROM price_ticks
                WHERE time >= $1
                ORDER BY time
            "#,
            start
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        if entries.is_empty() {
            return Ok(None);
        }

        let mut min_price = entries[0].last_price;
        let mut max_price = entries[0].last_price;

        for entry in entries.iter().skip(1) {
            let entry_price = entry.last_price;

            if entry_price < min_price {
                min_price = entry_price;
            }
            if entry_price > max_price {
                max_price = entry_price;
            }
        }

        let last_entry = entries.last().expect("not `None`");
        let latest_time = last_entry.time;
        let latest_price = last_entry.last_price;

        Ok(Some((min_price, max_price, latest_time, latest_price)))
    }

    async fn remove_ticks(&self, before: DateTime<Utc>) -> Result<()> {
        sqlx::query!("DELETE FROM price_ticks WHERE time <= $1", before)
            .execute(self.pool())
            .await
            .map_err(DbError::Query)?;

        Ok(())
    }
}
