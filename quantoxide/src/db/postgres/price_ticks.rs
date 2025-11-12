use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, SubsecRound, Utc};
use sqlx::{Pool, Postgres};

use lnm_sdk::models::PriceTickLNM;

use crate::indicators::IndicatorsEvaluator;

use super::super::{
    error::{DbError, Result},
    models::{PartialPriceHistoryEntryLOCF, PriceHistoryEntryLOCF, PriceTickRow},
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
    async fn add_tick(&self, tick: &PriceTickLNM) -> Result<Option<PriceTickRow>> {
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
        struct UnionPriceEntry {
            time: Option<DateTime<Utc>>,
            price: Option<f64>,
        }

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

    async fn get_price_range_from(
        &self,
        start: DateTime<Utc>,
    ) -> Result<Option<(f64, f64, DateTime<Utc>, f64)>> {
        struct PriceEntry {
            time: Option<DateTime<Utc>>,
            price: Option<f64>,
        }

        impl PriceEntry {
            fn time(&self) -> Result<DateTime<Utc>> {
                self.time.ok_or(DbError::UnexpectedQueryResult(
                    "Combined price entry `time` can't be `None`".into(),
                ))
            }

            fn price(&self) -> Result<f64> {
                self.price.ok_or(DbError::UnexpectedQueryResult(
                    "Combined price entry `price` can't be `None`".into(),
                ))
            }
        }

        let combined_entries = sqlx::query_as!(
            PriceEntry,
            r#"
                SELECT time, last_price as price
                FROM price_ticks
                WHERE time >= $1
                UNION ALL
                SELECT time, value as price
                FROM price_history
                WHERE time >= $1
                ORDER BY time
            "#,
            start
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        if combined_entries.is_empty() {
            return Ok(None);
        }

        let mut min_price = combined_entries[0].price()?;
        let mut max_price = combined_entries[0].price()?;

        for entry in combined_entries.iter().skip(1) {
            let entry_price = entry.price()?;

            if entry_price < min_price {
                min_price = entry_price;
            }
            if entry_price > max_price {
                max_price = entry_price;
            }
        }

        let last_entry = combined_entries.last().unwrap();
        let latest_time = last_entry.time()?;
        let latest_price = last_entry.price()?;

        Ok(Some((min_price, max_price, latest_time, latest_price)))
    }

    async fn compute_locf_entries_for_range(
        &self,
        time: DateTime<Utc>,
        range_secs: usize,
    ) -> Result<Vec<PriceHistoryEntryLOCF>> {
        if range_secs == 0 {
            return Ok(Vec::new());
        }

        let end_locf_sec = time.trunc_subsecs(0);
        let start_locf_sec = end_locf_sec - Duration::seconds(range_secs as i64 - 1);

        let entries_locf = sqlx::query_as!(
            PriceHistoryEntryLOCF,
            "SELECT * FROM price_history_locf WHERE time >= $1 ORDER BY time ASC LIMIT $2",
            start_locf_sec,
            range_secs as i32
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        if entries_locf.len() == range_secs {
            return Ok(entries_locf);
        }

        // Range is not present in the current historical data. Indicators will have to be
        // calculated from historical prices.

        // At least one price entry must exist before `start_locf_sec` in order to compute locf
        // values and indicators.
        let is_time_valid = sqlx::query_scalar!(
            "SELECT EXISTS (SELECT 1 FROM price_history WHERE time <= $1)",
            start_locf_sec
        )
        .fetch_one(self.pool())
        .await
        .map_err(DbError::Query)?
        .unwrap_or(false);

        if !is_time_valid {
            return Err(DbError::InvalidLocfRange { start_locf_sec });
        }

        let start_indicator_sec =
            IndicatorsEvaluator::get_first_required_locf_entry(start_locf_sec);

        struct CoalescedEntry {
            pub time: Option<DateTime<Utc>>,
            pub value: Option<f64>,
        }

        let coalesced_entries = sqlx::query_as!(
            CoalescedEntry,
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
                )
                SELECT time, value
                FROM price_data
            "#,
            start_indicator_sec,
            end_locf_sec
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        let partial_locf_entries = coalesced_entries
            .into_iter()
            .map(|coalesced| PartialPriceHistoryEntryLOCF {
                time: coalesced
                    .time
                    .expect("generate_series() can't produce NULL timestamps"),
                value: coalesced
                    .value
                    .expect("COALESCE can't return NULL due to validation and LOCF logic"),
            })
            .collect();

        let full_locf_entries =
            IndicatorsEvaluator::evaluate(partial_locf_entries, start_locf_sec)?;

        Ok(full_locf_entries)
    }

    async fn remove_ticks(&self, before: DateTime<Utc>) -> Result<()> {
        sqlx::query!("DELETE FROM price_ticks WHERE time <= $1", before)
            .execute(self.pool())
            .await
            .map_err(DbError::Query)?;

        Ok(())
    }
}
