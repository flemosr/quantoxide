use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres, Transaction};

use lnm_sdk::models::PriceEntryLNM;

use crate::{indicators::IndicatorsEvaluator, util::DateTimeExt};

use super::super::{
    error::{DbError, Result},
    models::{PartialPriceHistoryEntryLOCF, PriceHistoryEntry},
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
             WHERE time >= $1 AND time < $2
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
        struct GapEntry {
            time: Option<DateTime<Utc>>,
            is_gap: Option<bool>,
        }

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
            let from = entry.time.ok_or(DbError::UnexpectedQueryResult(
                "Price history entry `time` must be `Some`".into(),
            ))?;
            if entry.is_gap.unwrap_or(false) {
                if let Some(to) = entries.get(i + 1) {
                    let to = to.time.ok_or(DbError::UnexpectedQueryResult(
                        "Price history entry `time` must be `Some`".into(),
                    ))?;
                    gaps.push((from, to))
                }
            }
        }

        Ok(gaps)
    }

    async fn add_entries(
        &self,
        entries: &[PriceEntryLNM],
        next_observed_time: Option<DateTime<Utc>>,
    ) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }

        if next_observed_time.map_or(false, |time| time <= entries.first().unwrap().time()) {
            return Err(DbError::NewEntriesInvalidNextObservedTime {
                next_observed_time: next_observed_time.expect("Not `None`"),
                first_entry_time: entries.first().unwrap().time(),
            });
        }

        if !entries.is_sorted_by(|a, b| a.time() >= b.time()) {
            return Err(DbError::NewEntriesNotSortedTimeDescending);
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

        // It can be assumed that `price_history_locf` is up-to-date in regards to the previously
        // added price entries.
        // A new entry-batch will potentially affect the values of all locf entries between its own
        // `min_locf_sec` and the `locf_sec` corresponding to the observed entry just AFTER it.
        // If there are no observed entries after the just-added entry-batch, the batch's
        // `max_locf_sec` will be the new locf upper bound.
        // A new batch added after the current max locf time, or before the current min locf time
        // may result in gaps in the `price_history_locf` history. Said gaps need to be filled by
        // carrying the corresponding locf value forward.

        let earliest_entry_time = entries.last().expect("not empty").time();
        let added_start_locf_sec = earliest_entry_time.ceil_sec();

        let prev_locf_sec = sqlx::query_scalar!(
            "SELECT max(time) FROM price_history_locf WHERE time <= $1",
            earliest_entry_time
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        // `prev_locf_sec` will be `None` only if `added_start_locf_sec` is the new min locf time
        let start_locf_sec = prev_locf_sec.unwrap_or(added_start_locf_sec);

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

        // Update indicators affected by the updated LOCF entries

        let start_indicator_sec =
            IndicatorsEvaluator::get_first_required_locf_entry(start_locf_sec);

        let end_indicator_sec = IndicatorsEvaluator::get_last_affected_locf_entry(end_locf_sec);

        let partial_locf_entries = sqlx::query_as!(
            PartialPriceHistoryEntryLOCF,
            r#"
                SELECT time, value
                FROM price_history_locf
                WHERE time >= $1 AND time <= $2 ORDER BY time ASC
            "#,
            start_indicator_sec,
            end_indicator_sec
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        let full_locf_entries =
            IndicatorsEvaluator::evaluate(partial_locf_entries, start_locf_sec)?;

        if !full_locf_entries.is_empty() {
            let times: Vec<_> = full_locf_entries.iter().map(|e| e.time).collect();
            let ma_5s: Vec<_> = full_locf_entries.iter().map(|e| e.ma_5).collect();
            let ma_60s: Vec<_> = full_locf_entries.iter().map(|e| e.ma_60).collect();
            let ma_300s: Vec<_> = full_locf_entries.iter().map(|e| e.ma_300).collect();

            sqlx::query!(
                r#"
                    UPDATE price_history_locf AS phl
                    SET
                        ma_5 = updates.ma_5,
                        ma_60 = updates.ma_60,
                        ma_300 = updates.ma_300
                    FROM (
                        SELECT *
                        FROM unnest($1::timestamptz[], $2::float8[], $3::float8[], $4::float8[])
                        AS t(time, ma_5, ma_60, ma_300)
                    ) AS updates
                    WHERE phl.time = updates.time
                "#,
                &times,
                &ma_5s as &[Option<f64>],
                &ma_60s as &[Option<f64>],
                &ma_300s as &[Option<f64>]
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;
        }

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }

    async fn update_entry_next(
        &self,
        entry_time: DateTime<Utc>,
        next: DateTime<Utc>,
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
