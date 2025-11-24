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
    async fn add_candles(&self, candles: &[OhlcCandle]) -> Result<()> {
        if candles.is_empty() {
            return Ok(());
        }

        // Validate: times must be rounded to the minute and continuous (1 minute apart, descending)
        for window in candles.windows(2) {
            // Check if current candle time is rounded
            if window[0].time().second() != 0 || window[0].time().nanosecond() != 0 {
                return Err(DbError::NewCandlesTimesNotRoundedToMinute);
            }

            // Check continuity: next candle should be exactly 1 minute before current
            let expected_prev_time = window[0].time() - Duration::minutes(1);
            if window[1].time() != expected_prev_time {
                return Err(DbError::NewCandlesNotContinuous);
            }
        }

        let period_start = candles.last().expect("not empty").time();
        let period_end = candles.first().expect("not empty").time();

        // Check the last candle's time is rounded. Not checked when iterating over
        // `candles.windows(2)`. Also handles single candles.
        if period_start.second() != 0 || period_start.nanosecond() != 0 {
            return Err(DbError::NewCandlesTimesNotRoundedToMinute);
        }

        let mut tx = self.start_transaction().await?;

        // If a candle exists at period_end + 1 minute, set its gap to false (gap being filled)
        let next_candle_time = period_end + Duration::minutes(1);
        sqlx::query!(
            "UPDATE ohlc_candles SET gap = false WHERE time = $1",
            next_candle_time
        )
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        // Check if a candle exists at period_start - 1 minute (to determine if earliest candle has a gap)
        let prev_candle_time = period_start - Duration::minutes(1);
        let prev_candle_exists = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM ohlc_candles WHERE time = $1)",
            prev_candle_time
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(DbError::Query)?
        .unwrap_or(false);

        // Prepare batch insert data
        let times: Vec<DateTime<Utc>> = candles.iter().map(|c| c.time()).collect();
        let opens: Vec<f64> = candles.iter().map(|c| c.open().into_f64()).collect();
        let highs: Vec<f64> = candles.iter().map(|c| c.high().into_f64()).collect();
        let lows: Vec<f64> = candles.iter().map(|c| c.low().into_f64()).collect();
        let closes: Vec<f64> = candles.iter().map(|c| c.close().into_f64()).collect();
        let volumes: Vec<i64> = candles.iter().map(|c| c.volume() as i64).collect();

        // All candles have gap=false except the last one (earliest time) which has a gap if no previous candle
        let mut gaps: Vec<bool> = vec![false; candles.len()];
        if !prev_candle_exists {
            gaps[candles.len() - 1] = true;
        }

        // Batch insert all candles
        sqlx::query!(
            r#"
                INSERT INTO ohlc_candles (time, open, high, low, close, volume, gap)
                SELECT * FROM unnest($1::timestamptz[], $2::float8[], $3::float8[], $4::float8[], $5::float8[], $6::bigint[], $7::bool[])
            "#,
            &times,
            &opens,
            &highs,
            &lows,
            &closes,
            &volumes,
            &gaps
        )
        .execute(&mut *tx)
        .await
        .map_err(DbError::Query)?;

        tx.commit().await.map_err(DbError::TransactionCommit)?;

        Ok(())
    }

    async fn get_earliest_candle(&self) -> Result<Option<OhlcCandleRow>> {
        let row = sqlx::query_as!(
            OhlcCandleRow,
            r#"
                SELECT time, open, high, low, close, volume, created_at, gap
                FROM ohlc_candles
                ORDER BY time ASC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(row)
    }

    async fn get_latest_candle(&self) -> Result<Option<OhlcCandleRow>> {
        let row = sqlx::query_as!(
            OhlcCandleRow,
            r#"
                SELECT time, open, high, low, close, volume, created_at, gap
                FROM ohlc_candles
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(row)
    }

    async fn get_gaps(&self) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
        // Find all candles with gap=true, excluding the earliest one (db bound)
        // For each, also get the latest candle before it
        let gaps = sqlx::query!(
            r#"
                SELECT
                    (
                        SELECT time FROM ohlc_candles
                        WHERE time < gap_candle.time
                        ORDER BY time DESC
                        LIMIT 1
                    ) as "from_time!",
                    gap_candle.time as "gap_time!"
                FROM ohlc_candles gap_candle
                WHERE gap_candle.gap = true
                AND gap_candle.time > (SELECT MIN(time) FROM ohlc_candles)
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
}
