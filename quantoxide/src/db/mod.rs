use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::indicators::IndicatorsEvaluator;
use crate::util::DateTimeExt;

pub mod error;
pub mod models;
mod postgres;
mod repositories;

use error::{DbError, Result};
use models::PartialPriceHistoryEntryLOCF;
use postgres::{price_history::PgPriceHistoryRepo, price_ticks::PgPriceTicksRepo};
use repositories::{PriceHistoryRepository, PriceTicksRepository};

pub struct DbContext {
    pub price_history: Box<dyn PriceHistoryRepository>,
    pub price_ticks: Box<dyn PriceTicksRepository>,
}

impl DbContext {
    pub async fn new(postgres_db_url: &str) -> Result<Arc<Self>> {
        println!("Connecting to database...");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(postgres_db_url)
            .await
            .map_err(DbError::Connection)?;

        println!("Successfully connected to the database");

        println!("Running migrations...");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(DbError::Migration)?;

        println!("Migrations completed successfully");

        println!("Initializing `price_history_locf` table...");

        Self::initialize_locf_table(&pool).await?;

        println!("Database ready");

        let pool = Arc::new(pool);
        let price_history = Box::new(PgPriceHistoryRepo::new(pool.clone()));
        let price_ticks = Box::new(PgPriceTicksRepo::new(pool));

        Ok(Arc::new(Self {
            price_history,
            price_ticks,
        }))
    }

    const INIT_LOCF_BATCH_SIZE: i64 = 100_000;

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
            _ => {
                println!(
                    "No price history data found. Skipping 'price_history_locf' initialization"
                );
                return Ok(());
            }
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

                    println!("`price_history_locf` table needs to be recreated");

                    println!("Deleting previous `price_history_locf` entries...",);

                    sqlx::query!("DELETE FROM price_history_locf")
                        .execute(pool)
                        .await
                        .map_err(DbError::Query)?;

                    println!(
                        "Deleted previous `price_history_locf` entries. Recreating from {start_locf_sec}"
                    );

                    start_locf_sec
                } else if cur_max_locf_time == end_locf_sec {
                    println!(
                        "`price_history_locf` table appears to be up-to-date. Skipping initialization"
                    );
                    return Ok(());
                } else {
                    // cur_max_locf_time < end_locf_sec

                    println!(
                        "`price_history_locf` initialization is in progress. Restarting from {cur_max_locf_time}"
                    );

                    cur_max_locf_time + chrono::Duration::seconds(1)
                }
            }
            _ => {
                println!(
                    "`price_history_locf` table needs to be initialized. Starting from {start_locf_sec}"
                );
                start_locf_sec
            }
        };

        while batch_start <= end_locf_sec {
            let batch_end =
                (batch_start + Duration::seconds(Self::INIT_LOCF_BATCH_SIZE - 1)).min(end_locf_sec);

            println!("Processing locf entries batch: {batch_start} to {batch_end}");

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

            let (start_indicator_sec, _) =
                IndicatorsEvaluator::get_indicator_calculation_range(batch_start, batch_end)
                    .map_err(|e| DbError::Generic(e.to_string()))?;

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
                IndicatorsEvaluator::evaluate(partial_locf_entries, batch_start)
                    .map_err(|e| DbError::Generic(e.to_string()))?;

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

        println!("`price_history_locf` table initialization completed successfully");

        Ok(())
    }
}
