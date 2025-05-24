use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, SubsecRound, Utc};
use sqlx::{Pool, Postgres, Transaction};

use lnm_sdk::api::rest::models::PriceEntryLNM;

use crate::util::DateTimeExt;

use super::super::{
    error::{DbError, Result},
    models::{PriceHistoryEntry, PriceHistoryEntryLOCF},
    repositories::PriceHistoryRepository,
};

pub struct PgPriceHistoryRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgPriceHistoryRepo {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<Postgres> {
        self.pool.as_ref()
    }

    async fn start_transaction(&self) -> Result<Transaction<'static, Postgres>> {
        self.pool.begin().await.map_err(DbError::TransactionBegin)
    }
}

struct GapEntry {
    time: Option<DateTime<Utc>>,
    is_gap: Option<bool>,
}

#[async_trait]
impl PriceHistoryRepository for PgPriceHistoryRepo {
    async fn get_earliest_entry_gap(&self) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history WHERE next IS NULL ORDER BY time ASC LIMIT 1"
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_latest_entry(&self) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history ORDER BY time DESC LIMIT 1"
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_earliest_entry(&self) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history ORDER BY time ASC LIMIT 1"
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_latest_entry_at_or_before(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history WHERE time <= $1 ORDER BY time DESC LIMIT 1",
            time
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_earliest_entry_after(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history WHERE time > $1 ORDER BY time ASC LIMIT 1",
            time
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_first_entry_reaching_bounds(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        min: f64,
        max: f64,
    ) -> Result<Option<PriceHistoryEntry>> {
        sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history
             WHERE time >= $1 AND time <= $2
             AND (value <= $3 OR value >= $4)
             ORDER BY time ASC
             LIMIT 1",
            start,
            end,
            min,
            max
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)
    }

    async fn get_entries_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<PriceHistoryEntry>> {
        let entries = sqlx::query_as!(
            PriceHistoryEntry,
            "SELECT * FROM price_history
             WHERE time >= $1 AND time <= $2
             ORDER BY time ASC",
            start,
            end,
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(entries)
    }

    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
        let entries = sqlx::query_as!(
            GapEntry,
            r#"
                WITH gap_entries AS (
                    SELECT time, next FROM price_history WHERE next IS NULL
                ),
                next_entries AS (
                    SELECT ph.time, ph.next
                    FROM price_history ph
                    JOIN gap_entries ge ON ph.time = (
                        SELECT MIN(time)
                        FROM price_history
                        WHERE time > ge.time
                    )
                )
                SELECT time, next IS NULL as "is_gap" FROM gap_entries
                UNION ALL
                SELECT time, next IS NULL as "is_gap" FROM next_entries
                ORDER BY time ASC
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        let mut gaps = Vec::new();

        for (i, entry) in entries.iter().enumerate() {
            let from = entry
                .time
                .ok_or(DbError::Generic("unexpected query result".into()))?;
            if entry.is_gap.unwrap_or(false) {
                if let Some(to) = entries.get(i + 1) {
                    let to = to
                        .time
                        .ok_or(DbError::Generic("unexpected query result".into()))?;
                    gaps.push((from, to))
                }
            }
        }

        Ok(gaps)
    }

    async fn add_entries(
        &self,
        entries: &[PriceEntryLNM],
        next_observed_time: Option<&DateTime<Utc>>,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        let mut tx = self.start_transaction().await?;

        let mut next_entry_time = next_observed_time;

        for entry in entries {
            sqlx::query!(
                r#"
                    INSERT INTO price_history (time, value, next)
                    VALUES ($1, $2, $3)
                "#,
                entry.time(),
                entry.value().into_f64(),
                next_entry_time
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            next_entry_time = Some(entry.time());
        }

        // We can assume that `price_history_locf` is up-to-date in regards to the previously
        // added price entries.
        // A new entry-batch will potentially affect the values of all locf entries between its own
        // `min_locf_sec` and the `locf_sec` corresponding to the observed entry just AFTER it.
        // If there are no observed entries after the just-added entry-batch, the batch's
        // `max_locf_sec` will be the new locf upper bound.
        // A new batch added after the current max locf time, or before the current min locf time
        // may result in gaps in the `price_history_locf` history. Said gaps need to be filled by
        // carrying the corresponding locf value forward.

        let earliest_entry_time = entries.last().expect("not empty").time();
        let start_locf_sec = earliest_entry_time.ceil_sec();

        let prev_locf_sec = sqlx::query_scalar!(
            "SELECT max(time) FROM price_history_locf WHERE time <= $1",
            earliest_entry_time
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?;
        // `prev_locf_sec` will be `None` only when `added_locf_sec` is the new min locf time
        let start_locf_sec = prev_locf_sec.unwrap_or(start_locf_sec);

        let latest_batch_time = entries.first().expect("not empty").time();
        let latest_ob_time_after_batch = next_observed_time.unwrap_or(latest_batch_time);
        let end_locf_sec = latest_ob_time_after_batch.ceil_sec();

        sqlx::query!(
            r#"
                INSERT INTO price_history_locf (time, value)
                SELECT s.time, t.value
                FROM generate_series($1, $2, '1 second'::interval) AS s(time)
                LEFT JOIN LATERAL (
                    SELECT value
                    FROM price_history
                    WHERE time <= s.time
                    ORDER BY time DESC
                    LIMIT 1
                ) t ON true
                ON CONFLICT (time)
                DO UPDATE SET value = EXCLUDED.value
            "#,
            start_locf_sec,
            end_locf_sec
        )
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        // Moving averages from start_locf_sec until end_locf_sec + max period
        // secs could be affected.

        const MAX_MA_PERIOD_SECS: i64 = 300;
        let start_ma_sec = start_locf_sec - Duration::seconds(MAX_MA_PERIOD_SECS - 1);
        let end_ma_sec = end_locf_sec + Duration::seconds(MAX_MA_PERIOD_SECS - 1);

        sqlx::query!(
            r#"
                WITH price_data AS (
                    SELECT time, value, ROW_NUMBER() OVER (ORDER BY time) AS rn
                    FROM price_history_locf
                    WHERE time >= $1 AND time <= $2
                    ORDER BY time ASC
                ),
                eval_indicators AS (
                    SELECT
                        time,
                        CASE
                            WHEN rn >= 5
                            THEN AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 4 PRECEDING AND CURRENT ROW)
                            ELSE NULL
                        END AS ma_5,
                        CASE
                            WHEN rn >= 60
                            THEN AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 59 PRECEDING AND CURRENT ROW)
                            ELSE NULL
                        END AS ma_60,
                        CASE
                            WHEN rn >= 300
                            THEN AVG(value) OVER (ORDER BY time ASC ROWS BETWEEN 299 PRECEDING AND CURRENT ROW)
                            ELSE NULL
                        END AS ma_300
                    FROM price_data
                )
                UPDATE price_history_locf
                SET
                    ma_5 = eval_indicators.ma_5,
                    ma_60 = eval_indicators.ma_60,
                    ma_300 = eval_indicators.ma_300
                FROM eval_indicators
                WHERE
                    eval_indicators.time >= $3
                    AND price_history_locf.time = eval_indicators.time
            "#,
            start_ma_sec,
            end_ma_sec,
            start_locf_sec
        )
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }

    async fn update_entry_next(
        &self,
        entry_time: &DateTime<Utc>,
        next: &DateTime<Utc>,
    ) -> Result<bool> {
        let result = sqlx::query!(
            r#"
                UPDATE price_history
                SET next = $1
                WHERE time = $2 AND next IS NULL
            "#,
            next,
            entry_time
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(result.rows_affected() > 0)
    }
}
