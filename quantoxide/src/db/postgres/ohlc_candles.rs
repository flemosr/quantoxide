use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use sqlx::{Pool, Postgres, Transaction};

use lnm_sdk::api_v3::models::OhlcCandle;

use super::super::{
    error::{DbError, Result},
    models::OhlcCandleRow,
    repositories::OhlcCandlesRepository,
};

pub(crate) struct PgOhlcCandlesRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgOhlcCandlesRepo {
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
impl OhlcCandlesRepository for PgOhlcCandlesRepo {
    async fn add_candles(
        &self,
        before_candle_time: Option<DateTime<Utc>>,
        new_candles: &[OhlcCandle],
    ) -> Result<()> {
        if new_candles.is_empty() {
            return Ok(());
        }

        for window in new_candles.windows(2) {
            let [current, next] = window else {
                unreachable!()
            };

            if current.time().second() != 0 || current.time().nanosecond() != 0 {
                return Err(DbError::NewDbCandlesTimesNotRoundedToMinute);
            }

            if next.time() >= current.time() {
                return Err(DbError::NewDbCandlesNotOrderedByTimeDesc {
                    inconsistency_at: next.time(),
                });
            }
        }

        let period_start = new_candles.last().expect("not empty").time();
        let period_end = new_candles.first().expect("not empty").time();

        // Validate the last candle's time (also handles single candles)
        if period_start.second() != 0 || period_start.nanosecond() != 0 {
            return Err(DbError::NewDbCandlesTimesNotRoundedToMinute);
        }

        let mut tx = self.start_transaction().await?;

        let conflicting_stable = sqlx::query_scalar!(
                "SELECT time FROM ohlc_candles WHERE time >= $1 AND time <= $2 AND stable = true LIMIT 1",
                period_start,
                period_end
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(DbError::Query)?;

        if let Some(conflicting_time) = conflicting_stable {
            return Err(DbError::AttemptedToUpdateStableCandle {
                time: conflicting_time,
            });
        }

        if let Some(before_candle_time) = before_candle_time {
            sqlx::query!(
                "UPDATE ohlc_candles SET gap = false WHERE time = $1",
                before_candle_time
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;
        }

        let mut times = Vec::with_capacity(new_candles.len());
        let mut opens = Vec::with_capacity(new_candles.len());
        let mut highs = Vec::with_capacity(new_candles.len());
        let mut lows = Vec::with_capacity(new_candles.len());
        let mut closes = Vec::with_capacity(new_candles.len());
        let mut volumes = Vec::with_capacity(new_candles.len());

        for candle in new_candles {
            times.push(candle.time());
            opens.push(candle.open().into_f64());
            highs.push(candle.high().into_f64());
            lows.push(candle.low().into_f64());
            closes.push(candle.close().into_f64());
            volumes.push(candle.volume() as i64);
        }

        let before_period_time = period_start - Duration::minutes(1);
        let before_period_candle_exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM ohlc_candles WHERE time = $1 AND stable = true)",
            before_period_time
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?
        .unwrap_or(false);

        let mut gaps: Vec<bool> = vec![false; new_candles.len()];
        gaps[new_candles.len() - 1] = !before_period_candle_exists;

        // The presence of `before_candle_time` indicates that the latest candle of the period is
        // not the candle corresponding to the current minute, and therefore can be considered
        // stable.
        let mut stables: Vec<bool> = vec![true; new_candles.len()];
        stables[0] = before_candle_time.is_some();

        // Batch insert all candles, overwriting provisional candles if any

        sqlx::query!(
                r#"
                    INSERT INTO ohlc_candles (time, open, high, low, close, volume, gap, stable)
                    SELECT * FROM unnest($1::timestamptz[], $2::float8[], $3::float8[], $4::float8[], $5::float8[], $6::bigint[], $7::bool[], $8::bool[])
                    ON CONFLICT (time) DO UPDATE
                    SET open = EXCLUDED.open,
                        high = EXCLUDED.high,
                        low = EXCLUDED.low,
                        close = EXCLUDED.close,
                        volume = EXCLUDED.volume,
                        gap = EXCLUDED.gap,
                        stable = EXCLUDED.stable
                "#,
                &times,
                &opens,
                &highs,
                &lows,
                &closes,
                &volumes,
                &gaps,
                &stables
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }

    async fn get_candles(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<OhlcCandleRow>> {
        let rows = sqlx::query_as!(
            OhlcCandleRow,
            r#"
                SELECT time, open, high, low, close, volume, created_at, updated_at, gap, stable
                FROM ohlc_candles
                WHERE time >= $1 AND time <= $2
                ORDER BY time ASC
            "#,
            from,
            to
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(rows)
    }

    async fn remove_gap_flag(&self, time: DateTime<Utc>) -> Result<()> {
        sqlx::query!("UPDATE ohlc_candles SET gap = false WHERE time = $1", time)
            .execute(self.pool())
            .await
            .map_err(DbError::Query)?;
        Ok(())
    }

    async fn get_earliest_stable_candle_time(&self) -> Result<Option<DateTime<Utc>>> {
        struct OhlcCandlePartial {
            pub time: DateTime<Utc>,
        }

        let candle = sqlx::query_as!(
            OhlcCandlePartial,
            r#"
                SELECT time
                FROM ohlc_candles
                WHERE stable = true
                ORDER BY time ASC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(candle.map(|c| c.time))
    }

    async fn get_latest_stable_candle_time(&self) -> Result<Option<DateTime<Utc>>> {
        struct OhlcCandlePartial {
            pub time: DateTime<Utc>,
        }

        let candle = sqlx::query_as!(
            OhlcCandlePartial,
            r#"
                SELECT time
                FROM ohlc_candles
                WHERE stable = true
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(candle.map(|c| c.time))
    }

    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
        // Find all stable candles with gap=true, excluding the earliest one (db bound)
        // For each, also get the latest stable candle before it
        let gaps = sqlx::query!(
            r#"
                SELECT
                    (
                        SELECT time FROM ohlc_candles
                        WHERE time < gap_candle.time AND stable = true
                        ORDER BY time DESC
                        LIMIT 1
                    ) as "from_time!",
                    gap_candle.time as "gap_time!"
                FROM ohlc_candles gap_candle
                WHERE gap_candle.gap = true
                AND gap_candle.stable = true
                AND EXISTS (
                    SELECT 1 FROM ohlc_candles
                    WHERE time < gap_candle.time AND stable = true
                )
                ORDER BY gap_candle.time ASC
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?
        .into_iter()
        .map(|row| (row.from_time, row.gap_time))
        .collect();

        Ok(gaps)
    }

    async fn flag_missing_candles(&self) -> Result<()> {
        let mut tx = self.start_transaction().await?;

        // Find all stable candles where the next candle is more than 1 minute away and it is marked
        // as 'stable' and not marked as a 'gap' (unflagged gaps)
        let gap_after_times = sqlx::query_scalar!(
            r#"
                SELECT c1.time
                FROM ohlc_candles c1
                INNER JOIN LATERAL (
                    SELECT time, gap, stable
                    FROM ohlc_candles
                    WHERE time > c1.time
                    ORDER BY time ASC
                    LIMIT 1
                ) c2 ON true
                WHERE c1.stable = true
                AND c2.time > c1.time + INTERVAL '1 minute'
                AND c2.stable = true
                AND c2.gap = false
                ORDER BY c1.time ASC
            "#
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        if gap_after_times.is_empty() {
            return Ok(());
        }

        for gap_after_time in gap_after_times {
            // Mark the 5 candles before and after `gap_after_time` as unstable
            // Time ranges shouldn't be relied upon directly since there's a gap
            sqlx::query!(
                r#"
                    UPDATE ohlc_candles
                    SET stable = false
                    WHERE time IN (
                        SELECT time FROM ohlc_candles
                        WHERE time <= $1
                        ORDER BY time DESC
                        LIMIT 5
                    )
                    OR time IN (
                        SELECT time FROM ohlc_candles
                        WHERE time > $1
                        ORDER BY time ASC
                        LIMIT 5
                    )
                "#,
                gap_after_time
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            // Mark the 6th candle after `gap_after_time` as a gap
            let sixth_candle = sqlx::query_scalar!(
                r#"
                    SELECT time FROM ohlc_candles
                    WHERE time > $1
                    ORDER BY time ASC
                    LIMIT 1 OFFSET 5
                "#,
                gap_after_time
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            if let Some(sixth_candle_time) = sixth_candle {
                sqlx::query!(
                    "UPDATE ohlc_candles SET gap = true WHERE time = $1",
                    sixth_candle_time
                )
                .execute(&mut *tx)
                .await
                .map_err(DbError::Query)?;
            }
        }

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }
}
