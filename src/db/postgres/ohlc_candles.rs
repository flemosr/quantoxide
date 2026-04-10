use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Timelike, Utc};
use sqlx::{Pool, Postgres, Transaction};

use lnm_sdk::api_v3::models::OhlcCandle;

use crate::shared::OhlcResolution;

use super::super::{
    CANDLE_STABLE_AGE,
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

        // Validate the last candle's time (also handles single candles)
        if period_start.second() != 0 || period_start.nanosecond() != 0 {
            return Err(DbError::NewDbCandlesTimesNotRoundedToMinute);
        }

        let mut tx = self.start_transaction().await?;

        // Clear the gap flag on the candle immediately after the period (the `to` boundary that
        // came with the download range). This is unrelated to the batch below — the gap marker
        // lives on an existing stable row whose values we're not touching.
        if let Some(before_candle_time) = before_candle_time {
            sqlx::query!(
                "UPDATE ohlc_candles SET gap = false WHERE time = $1",
                before_candle_time
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;
        }

        // Gap-marker placement: if the candle immediately before the batch (`period_start - 1min`)
        // is not stable, flag the batch's oldest candle with `gap=true`. This deliberately checks
        // `stable = true`, not just existence — an unstable predecessor (e.g. leftover from a
        // previous sync session's tail) must trigger a gap so that `get_gaps` picks it up and the
        // unstable region gets re-fetched. This is the only mechanism that detects stale unstable
        // tails in live mode, which does not run `flag_missing_candles`.
        let before_period_time = period_start - Duration::minutes(1);
        let before_period_candle_exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM ohlc_candles WHERE time = $1 AND stable = true)",
            before_period_time
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?
        .unwrap_or(false);

        let mut times = Vec::with_capacity(new_candles.len());
        let mut opens = Vec::with_capacity(new_candles.len());
        let mut highs = Vec::with_capacity(new_candles.len());
        let mut lows = Vec::with_capacity(new_candles.len());
        let mut closes = Vec::with_capacity(new_candles.len());
        let mut volumes = Vec::with_capacity(new_candles.len());

        for candle in new_candles {
            times.push(candle.time());
            opens.push(candle.open().as_f64());
            highs.push(candle.high().as_f64());
            lows.push(candle.low().as_f64());
            closes.push(candle.close().as_f64());
            volumes.push(candle.volume() as i64);
        }

        let mut gaps: Vec<bool> = vec![false; new_candles.len()];
        gaps[new_candles.len() - 1] = !before_period_candle_exists;

        let stable_cutoff = Utc::now() - CANDLE_STABLE_AGE;
        let stables: Vec<bool> = new_candles
            .iter()
            .map(|c| c.time() <= stable_cutoff)
            .collect();

        // Batch upsert all candles. The WHERE clause on DO UPDATE prevents the updated_at
        // trigger from firing when no values actually changed.
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
                    WHERE ohlc_candles.open != EXCLUDED.open
                       OR ohlc_candles.high != EXCLUDED.high
                       OR ohlc_candles.low != EXCLUDED.low
                       OR ohlc_candles.close != EXCLUDED.close
                       OR ohlc_candles.volume != EXCLUDED.volume
                       OR ohlc_candles.gap != EXCLUDED.gap
                       OR ohlc_candles.stable != EXCLUDED.stable
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
                SELECT time, open, high, low, close, volume, created_at, updated_at, stable
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

    // Candles are aggregated using:
    // - First open price in the period
    // - Maximum high price
    // - Minimum low price
    // - Last close price in the period
    // - Sum of volumes
    // - `stable` is true only if all constituent candles are stable AND the candle's time period
    //   has fully elapsed (bucket_time + resolution <= to)
    async fn get_candles_consolidated(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        resolution: OhlcResolution,
    ) -> Result<Vec<OhlcCandleRow>> {
        if matches!(resolution, OhlcResolution::OneMinute) {
            return self.get_candles(from, to).await;
        }

        let rows = match resolution {
            OhlcResolution::OneMinute => unreachable!("handled above"),

            // Sub-hourly: bucket by minutes using epoch-based calculation
            OhlcResolution::ThreeMinutes
            | OhlcResolution::FiveMinutes
            | OhlcResolution::TenMinutes
            | OhlcResolution::FifteenMinutes
            | OhlcResolution::ThirtyMinutes
            | OhlcResolution::FortyFiveMinutes => {
                let resolution_seconds = resolution.as_seconds() as i64;
                sqlx::query_as!(
                    OhlcCandleRow,
                    r#"
                        WITH bucketed AS (
                            SELECT
                                to_timestamp(FLOOR(EXTRACT(EPOCH FROM time)::BIGINT / $3) * $3) AS bucket_time,
                                open, high, low, close, volume, created_at, updated_at, stable, time
                            FROM ohlc_candles
                            WHERE time >= $1 AND time <= $2
                        )
                        SELECT
                            bucket_time AS "time!",
                            (array_agg(open ORDER BY time ASC))[1] AS "open!",
                            MAX(high) AS "high!",
                            MIN(low) AS "low!",
                            (array_agg(close ORDER BY time DESC))[1] AS "close!",
                            SUM(volume)::BIGINT AS "volume!",
                            MIN(created_at) AS "created_at!",
                            MAX(updated_at) AS "updated_at!",
                            BOOL_AND(stable) AND (bucket_time + $3 * INTERVAL '1 second') <= $2 AS "stable!"
                        FROM bucketed
                        GROUP BY bucket_time
                        ORDER BY bucket_time ASC
                    "#,
                    from,
                    to,
                    resolution_seconds
                )
                .fetch_all(self.pool())
                .await
                .map_err(DbError::Query)?
            }

            // Hourly resolutions: bucket by hours using epoch-based calculation
            OhlcResolution::OneHour
            | OhlcResolution::TwoHours
            | OhlcResolution::ThreeHours
            | OhlcResolution::FourHours => {
                let resolution_seconds = resolution.as_seconds() as i64;
                sqlx::query_as!(
                    OhlcCandleRow,
                    r#"
                        WITH bucketed AS (
                            SELECT
                                to_timestamp(FLOOR(EXTRACT(EPOCH FROM time)::BIGINT / $3) * $3) AS bucket_time,
                                open, high, low, close, volume, created_at, updated_at, stable, time
                            FROM ohlc_candles
                            WHERE time >= $1 AND time <= $2
                        )
                        SELECT
                            bucket_time AS "time!",
                            (array_agg(open ORDER BY time ASC))[1] AS "open!",
                            MAX(high) AS "high!",
                            MIN(low) AS "low!",
                            (array_agg(close ORDER BY time DESC))[1] AS "close!",
                            SUM(volume)::BIGINT AS "volume!",
                            MIN(created_at) AS "created_at!",
                            MAX(updated_at) AS "updated_at!",
                            BOOL_AND(stable) AND (bucket_time + $3 * INTERVAL '1 second') <= $2 AS "stable!"
                        FROM bucketed
                        GROUP BY bucket_time
                        ORDER BY bucket_time ASC
                    "#,
                    from,
                    to,
                    resolution_seconds
                )
                .fetch_all(self.pool())
                .await
                .map_err(DbError::Query)?
            }

            // Daily: bucket by day
            OhlcResolution::OneDay => sqlx::query_as!(
                OhlcCandleRow,
                r#"
                    WITH bucketed AS (
                        SELECT
                            date_trunc('day', time) AS bucket_time,
                            open, high, low, close, volume, created_at, updated_at, stable, time
                        FROM ohlc_candles
                        WHERE time >= $1 AND time <= $2
                    )
                    SELECT
                        bucket_time AS "time!",
                        (array_agg(open ORDER BY time ASC))[1] AS "open!",
                        MAX(high) AS "high!",
                        MIN(low) AS "low!",
                        (array_agg(close ORDER BY time DESC))[1] AS "close!",
                        SUM(volume)::BIGINT AS "volume!",
                        MIN(created_at) AS "created_at!",
                        MAX(updated_at) AS "updated_at!",
                        BOOL_AND(stable) AND (bucket_time + INTERVAL '1 day') <= $2 AS "stable!"
                    FROM bucketed
                    GROUP BY bucket_time
                    ORDER BY bucket_time ASC
                "#,
                from,
                to
            )
            .fetch_all(self.pool())
            .await
            .map_err(DbError::Query)?,
        };

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

    // Identifies and flags gaps in OHLC candle data within a specified time range.
    //
    // This function detects missing candles (gaps longer than 1 minute) and marks surrounding
    // candles as unstable to create a safety margin around unreliable data.
    //
    // # Algorithm Overview
    //
    // 1. **Gap Detection**: Finds all candles where the next candle is more than 1 minute away
    //    - Only examines candles after `cutoff_time` (now - range)
    //    - Ignores candles already flagged as unstable or gaps
    //    - Returns the timestamp immediately before each gap (`gap_after_time`)
    //
    // 2. **Unstable Marking**: For each gap, marks the 5 nearest candles before and after as unstable
    //    - Creates a safety buffer around gaps since time-based calculations are unreliable
    //    - Uses LATERAL joins to find the actual 5 closest candles (not 5th position)
    //    - Handles missing candles correctly (doesn't assume continuous time series)
    //
    // 3. **Gap Flagging**: Marks the 6th candle after each gap with `gap = true`
    //    - Provides a clear indicator of where normal data resumes
    //    - The first 5 candles after the gap are marked unstable, the 6th is marked as a gap
    //
    // # Performance Optimizations
    //
    // - Uses `cutoff_time` to limit table scans to recent data only
    // - Batches all gaps into 2 queries instead of 3N queries (where N = number of gaps)
    // - Leverages LATERAL joins for efficient nearest-neighbor searches
    // - Processes all operations within a single transaction
    //
    // # Example
    //
    // Given candles at times: [9:57, 9:58, 9:59, 10:00, 10:01, 10:02, 10:05, 10:06, 10:07, 10:08, 10:09, 10:10, ...]
    // - Gap detected: 10:02 → 10:05 (more than 1 minute)
    // - `gap_after_time` = 10:02
    // - Unstable: 9:58, 9:59, 10:00, 10:01, 10:02 (5 before) and 10:05, 10:06, 10:07, 10:08, 10:09 (5 after)
    // - Gap flag: 10:10 (6th candle after 10:02)
    async fn flag_missing_candles(&self, range: Duration) -> Result<()> {
        let mut tx = self.start_transaction().await?;

        let cutoff_time = Utc::now() - range;

        // Find all stable candles within the specified time range where the next candle is more
        // than 1 minute away and it is marked as 'stable' and not marked as a 'gap' (unflagged gaps).
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
                WHERE c1.time >= $1
                AND c1.stable = true
                AND c2.time > c1.time + INTERVAL '1 minute'
                AND c2.stable = true
                AND c2.gap = false
                ORDER BY c1.time ASC
            "#,
            cutoff_time
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        if !gap_after_times.is_empty() {
            // Batch process: Mark unstable candles around gaps
            // For each gap_after_time, find the 5 nearest candles before and after
            sqlx::query!(
                r#"
                    WITH gap_times AS (
                        SELECT unnest($2::timestamptz[]) as gap_time
                    ),
                    unstable_times AS (
                        -- Get 5 candles before each gap
                        SELECT DISTINCT time
                        FROM gap_times gt
                        CROSS JOIN LATERAL (
                            SELECT time
                            FROM ohlc_candles
                            WHERE time >= $1 AND time <= gt.gap_time
                            ORDER BY time DESC
                            LIMIT 5
                        ) before_gap
                        UNION
                        -- Get 5 candles after each gap
                        SELECT DISTINCT time
                        FROM gap_times gt
                        CROSS JOIN LATERAL (
                            SELECT time
                            FROM ohlc_candles
                            WHERE time >= $1 AND time > gt.gap_time
                            ORDER BY time ASC
                            LIMIT 5
                        ) after_gap
                    )
                    UPDATE ohlc_candles
                    SET stable = false
                    WHERE time IN (SELECT time FROM unstable_times)
                "#,
                cutoff_time,
                &gap_after_times
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            // Batch process: Mark 6th candles after gaps
            sqlx::query!(
                r#"
                    WITH gap_times AS (
                        SELECT unnest($2::timestamptz[]) as gap_time
                    ),
                    sixth_candles AS (
                        SELECT DISTINCT time
                        FROM gap_times gt
                        CROSS JOIN LATERAL (
                            SELECT time
                            FROM ohlc_candles
                            WHERE time >= $1 AND time > gt.gap_time
                            ORDER BY time ASC
                            LIMIT 1 OFFSET 5
                        ) sixth
                    )
                    UPDATE ohlc_candles
                    SET gap = true
                    WHERE time IN (SELECT time FROM sixth_candles)
                "#,
                cutoff_time,
                &gap_after_times
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;
        }

        // Flag unstable regions that lack gap markers. Find stable candles whose immediate
        // predecessor is unstable and not already gap-flagged. This ensures regions made unstable
        // by CANDLE_STABLE_AGE (or the buffer marking above) are picked up by the gap-fill
        // machinery.
        sqlx::query!(
            r#"
                UPDATE ohlc_candles
                SET gap = true
                WHERE time IN (
                    SELECT c_stable.time
                    FROM ohlc_candles c_stable
                    INNER JOIN LATERAL (
                        SELECT stable
                        FROM ohlc_candles
                        WHERE time < c_stable.time
                        ORDER BY time DESC
                        LIMIT 1
                    ) c_prev ON true
                    WHERE c_stable.time >= $1
                    AND c_stable.stable = true
                    AND c_stable.gap = false
                    AND c_prev.stable = false
                )
            "#,
            cutoff_time
        )
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }
}
