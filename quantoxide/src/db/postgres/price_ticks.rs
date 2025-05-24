use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, SubsecRound, Utc};
use sqlx::{Pool, Postgres};

use lnm_sdk::api::websocket::models::PriceTickLNM;

use crate::db::models::PriceTick;

use super::super::{
    error::{DbError, Result},
    models::PriceHistoryEntryLOCF,
    repositories::PriceTicksRepository,
};

pub struct PgPriceTicksRepo {
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

struct UnionPriceEntry {
    time: Option<DateTime<Utc>>,
    price: Option<f64>,
}

#[async_trait]
impl PriceTicksRepository for PgPriceTicksRepo {
    async fn add_tick(&self, tick: &PriceTickLNM) -> Result<Option<PriceTick>> {
        let price_tick = sqlx::query_as!(
            PriceTick,
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
        let latest_entry_opt = sqlx::query_as!(
            UnionPriceEntry,
            r#"
                WITH latest_tick AS (
                    SELECT time, last_price as price
                    FROM price_ticks
                    ORDER BY time DESC
                    LIMIT 1
                ),
                latest_history AS (
                    SELECT time, value as price
                    FROM price_history
                    ORDER BY time DESC
                    LIMIT 1
                )
                SELECT time, price
                FROM (
                    SELECT time, price FROM latest_tick
                    UNION ALL
                    SELECT time, price FROM latest_history
                ) combined
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        let latest_entry_opt = latest_entry_opt.and_then(|entry| match (entry.time, entry.price) {
            (Some(time), Some(price)) => Some((time, price)),
            _ => None,
        });

        Ok(latest_entry_opt)
    }

    async fn eval_entries_locf(
        &self,
        time: &DateTime<Utc>,
        range_secs: usize,
    ) -> Result<Vec<PriceHistoryEntryLOCF>> {
        let locf_sec = time.trunc_subsecs(0);
        let min_locf_sec = locf_sec - Duration::seconds(range_secs as i64 - 1);

        let entries_locf = sqlx::query_as!(
            PriceHistoryEntryLOCF,
            "SELECT * FROM price_history_locf WHERE time >= $1 ORDER BY time ASC LIMIT $2",
            min_locf_sec,
            range_secs as i32
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        if entries_locf.len() == range_secs {
            return Ok(entries_locf);
        }

        // `locf_sec` is not present in the current historical data.
        // indicators will have to be estimated from historical prices

        const MAX_MA_PERIOD_SECS: i64 = 300;
        let start_ma_sec = min_locf_sec - Duration::seconds(MAX_MA_PERIOD_SECS - 1);
        let end_ma_sec = locf_sec;

        // At least one price entry must exist before `min_locf_sec` in order to
        // compute locf values and indicators.
        let is_time_valid = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM price_history WHERE time <= $1)",
            min_locf_sec
        )
        .fetch_one(self.pool())
        .await
        .map_err(DbError::Query)?
        .unwrap_or(false);

        if !is_time_valid {
            return Err(DbError::Generic(format!(
                "The is not price entry with time lte {min_locf_sec}"
            )));
        }

        let entries_locf = sqlx::query_as!(
            PriceHistoryEntryLOCF,
            r#"
                WITH price_data AS (
                    SELECT
                        s.time,
                        CASE
                            WHEN pt.last_price IS NOT NULL AND ph.value IS NOT NULL THEN
                                CASE
                                    WHEN ph_time < pt_time THEN pt.last_price
                                    ELSE ph.value
                                END
                            ELSE COALESCE(pt.last_price, ph.value)
                        END AS value
                    FROM generate_series($1, $2, '1 second'::interval) AS s(time)
                    LEFT JOIN LATERAL (
                        SELECT time AS ph_time, value
                        FROM price_history
                        WHERE time <= s.time
                        ORDER BY time DESC
                        LIMIT 1
                    ) ph ON true
                    LEFT JOIN LATERAL (
                        SELECT time AS pt_time, last_price
                        FROM price_ticks
                        WHERE time <= s.time
                        ORDER BY time DESC
                        LIMIT 1
                    ) pt ON true
                    ORDER BY time ASC
                ),
                eval_indicators AS (
                    SELECT
                        time,
                        value,
                        AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 4 PRECEDING AND CURRENT ROW) AS ma_5,
                        AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 59 PRECEDING AND CURRENT ROW) AS ma_60,
                        AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 299 PRECEDING AND CURRENT ROW) AS ma_300
                    FROM price_data
                )
                SELECT
                    time as "time!",
                    value as "value!",
                    ma_5 as "ma_5",
                    ma_60 as "ma_60",
                    ma_300 as "ma_300"
                FROM eval_indicators
                WHERE eval_indicators.time >= $3
            "#,
            start_ma_sec,
            end_ma_sec,
            min_locf_sec
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(entries_locf)
    }
}
