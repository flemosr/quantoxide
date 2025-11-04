use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::{indicators::IndicatorsEvaluator, util::DateTimeExt};

pub(crate) mod error;
pub(crate) mod models;
mod postgres;
mod repositories;

use error::{DbError, Result};
use models::PartialPriceHistoryEntryLOCF;
use postgres::{
    price_history::PgPriceHistoryRepo, price_ticks::PgPriceTicksRepo,
    running_trades::PgRunningTradesRepo,
};
use repositories::{PriceHistoryRepository, PriceTicksRepository, RunningTradesRepository};

pub struct DbContext {
    pub(crate) price_history: Box<dyn PriceHistoryRepository>,
    pub(crate) price_ticks: Box<dyn PriceTicksRepository>,
    pub(crate) running_trades: Box<dyn RunningTradesRepository>,
}

impl DbContext {
    pub async fn new(postgres_db_url: &str) -> Result<Arc<Self>> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(postgres_db_url)
            .await
            .map_err(DbError::Connection)?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(DbError::Migration)?;

        Self::initialize_locf_table(&pool).await?;

        let pool = Arc::new(pool);
        let price_history = Box::new(PgPriceHistoryRepo::new(pool.clone()));
        let price_ticks = Box::new(PgPriceTicksRepo::new(pool.clone()));
        let running_trades = Box::new(PgRunningTradesRepo::new(pool.clone()));

        Ok(Arc::new(Self {
            price_history,
            price_ticks,
            running_trades,
        }))
    }

    const INIT_LOCF_BATCH_SIZE: i64 = 100_000;

    // TODO: Improve progress feedback to consumers
    async fn initialize_locf_table(pool: &PgPool) -> Result<()> {
        struct PriceHistoryRange {
            min_time: Option<DateTime<Utc>>,
            max_time: Option<DateTime<Utc>>,
        }

        let time_range = sqlx::query_as!(
            PriceHistoryRange,
            "SELECT MIN(time) as min_time, MAX(time) as max_time FROM price_history"
        )
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

        let (min_time, max_time) = match (time_range.min_time, time_range.max_time) {
            (Some(min), Some(max)) => (min, max),
            _ => return Ok(()),
        };

        let start_locf_sec = min_time.ceil_sec();
        let end_locf_sec = max_time.ceil_sec();

        struct LocfHealthCheck {
            start_exists: Option<bool>,
            cur_max_time: Option<DateTime<Utc>>,
        }

        let locf_table_check = sqlx::query_as!(
            LocfHealthCheck,
            r#"
                SELECT
                    EXISTS(SELECT 1 FROM price_history_locf WHERE time = $1) as start_exists,
                    MAX(time) as cur_max_time FROM price_history_locf
            "#,
            start_locf_sec
        )
        .fetch_one(pool)
        .await
        .map_err(DbError::Query)?;

        let mut batch_start = match (locf_table_check.start_exists, locf_table_check.cur_max_time) {
            (Some(start_exists), Some(cur_max_locf_time)) => {
                if !start_exists || cur_max_locf_time > end_locf_sec {
                    // Assume table is corrupted

                    sqlx::query!("DELETE FROM price_history_locf")
                        .execute(pool)
                        .await
                        .map_err(DbError::Query)?;

                    start_locf_sec
                } else if cur_max_locf_time == end_locf_sec {
                    return Ok(());
                } else {
                    // cur_max_locf_time < end_locf_sec

                    cur_max_locf_time + chrono::Duration::seconds(1)
                }
            }
            _ => start_locf_sec,
        };

        while batch_start <= end_locf_sec {
            let batch_end =
                (batch_start + Duration::seconds(Self::INIT_LOCF_BATCH_SIZE - 1)).min(end_locf_sec);

            let mut tx = pool.begin().await.map_err(DbError::TransactionBegin)?;

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
                "#,
                batch_start,
                batch_end
            )
            .execute(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            let start_indicator_sec =
                IndicatorsEvaluator::get_first_required_locf_entry(batch_start);

            let partial_locf_entries = sqlx::query_as!(
                PartialPriceHistoryEntryLOCF,
                r#"
                    SELECT time, value
                    FROM price_history_locf
                    WHERE time >= $1 AND time <= $2 ORDER BY time ASC
                "#,
                start_indicator_sec,
                batch_end
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(DbError::Query)?;

            let full_locf_entries =
                IndicatorsEvaluator::evaluate(partial_locf_entries, batch_start)?;

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

            batch_start = batch_end + chrono::Duration::seconds(1);
        }

        Ok(())
    }
}
