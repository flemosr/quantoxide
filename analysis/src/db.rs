use chrono::{DateTime, Duration, SubsecRound, Utc};
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::sync::OnceCell;

use crate::api::PriceEntryLNM;

mod models;

use models::PriceEntry;

pub static DB: Database = Database::new();

pub struct Database(OnceCell<Pool<Postgres>>);

impl Database {
    const fn new() -> Self {
        Self(OnceCell::const_new())
    }

    pub async fn init(&self, database_url: &str) -> Result<(), sqlx::Error> {
        println!("Connecting to database...");
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        println!("Successfully connected to the database");

        println!("Running migrations...");
        sqlx::migrate!("./migrations").run(&pool).await?;

        println!("Migrations completed successfully");

        println!("Checking database connection...");
        let row: (i64,) = sqlx::query_as("SELECT $1")
            .bind(150_i64)
            .fetch_one(&pool)
            .await?;

        assert_eq!(row.0, 150);
        println!("Database check successful");

        self.0.set(pool).expect("DB must not be initialized");

        Ok(())
    }

    fn get_pool(&self) -> &Pool<Postgres> {
        self.0.get().expect("DB must be initialized")
    }

    pub async fn get_earliest_price_entry_gap(&self) -> Result<Option<PriceEntry>, sqlx::Error> {
        let pool = self.get_pool();
        match sqlx::query_as::<_, PriceEntry>(
            "SELECT * FROM price_history WHERE next IS NULL ORDER BY time ASC LIMIT 1",
        )
        .fetch_one(pool)
        .await
        {
            Ok(price_entry) => Ok(Some(price_entry)),
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(None),
                _ => Err(e),
            },
        }
    }

    pub async fn get_latest_price_entry(&self) -> Result<Option<PriceEntry>, sqlx::Error> {
        let pool = self.get_pool();
        match sqlx::query_as::<_, PriceEntry>(
            "SELECT * FROM price_history ORDER BY time DESC LIMIT 1",
        )
        .fetch_one(pool)
        .await
        {
            Ok(price_entry) => Ok(Some(price_entry)),
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(None),
                _ => Err(e),
            },
        }
    }

    pub async fn get_earliest_price_entry(&self) -> Result<Option<PriceEntry>, sqlx::Error> {
        let pool = self.get_pool();
        match sqlx::query_as::<_, PriceEntry>(
            "SELECT * FROM price_history ORDER BY time ASC LIMIT 1",
        )
        .fetch_one(pool)
        .await
        {
            Ok(price_entry) => Ok(Some(price_entry)),
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(None),
                _ => Err(e),
            },
        }
    }

    pub async fn get_first_price_entry_after(
        &self,
        time: DateTime<Utc>,
    ) -> Result<Option<PriceEntry>, sqlx::Error> {
        let pool = self.get_pool();
        match sqlx::query_as::<_, PriceEntry>(
            "SELECT * FROM price_history WHERE time > $1 ORDER BY time ASC LIMIT 1",
        )
        .bind(time)
        .fetch_one(pool)
        .await
        {
            Ok(price_entry) => Ok(Some(price_entry)),
            Err(e) => match e {
                sqlx::Error::RowNotFound => Ok(None),
                _ => Err(e),
            },
        }
    }

    fn get_locf_sec(time: &DateTime<Utc>) -> DateTime<Utc> {
        let trunc_time_sec = time.trunc_subsecs(0);
        if trunc_time_sec == *time {
            trunc_time_sec
        } else {
            trunc_time_sec + Duration::seconds(1)
        }
    }

    pub async fn add_price_entries(
        &self,
        price_entries: &Vec<PriceEntryLNM>,
        next_observed_time: Option<&DateTime<Utc>>,
    ) -> Result<(), sqlx::Error> {
        if price_entries.is_empty() {
            return Ok(());
        }

        let mut tx = self.get_pool().begin().await?;

        let mut next_entry_time = next_observed_time;

        for price_entry in price_entries {
            let query = r#"
                INSERT INTO price_history (time, value, next)
                VALUES ($1, $2, $3)
            "#;
            sqlx::query(query)
                .bind(price_entry.time())
                .bind(price_entry.value())
                .bind(next_entry_time)
                .execute(&mut *tx)
                .await?;

            next_entry_time = Some(price_entry.time());
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

        let earliest_entry_time = price_entries.last().expect("not empty").time();
        let start_locf_sec = Self::get_locf_sec(&earliest_entry_time);

        let prev_locf_sec: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT max(time) FROM price_history_locf WHERE time <= $1")
                .bind(earliest_entry_time)
                .fetch_one(&mut *tx)
                .await?;
        // `prev_locf_sec` will be `None` only when `added_locf_sec` is the new min locf time
        let start_locf_sec = prev_locf_sec.unwrap_or(start_locf_sec);

        let latest_batch_time = price_entries.first().expect("not empty").time();
        let latest_ob_time_after_batch = next_observed_time.unwrap_or(latest_batch_time);
        let end_locf_sec = Self::get_locf_sec(&latest_ob_time_after_batch);

        let query = r#"
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
        "#;
        sqlx::query(query)
            .bind(start_locf_sec)
            .bind(end_locf_sec)
            .execute(&mut *tx)
            .await?;

        // Moving averages from start_locf_sec until end_locf_sec + max period
        // secs could be affected.

        const MAX_MA_PERIOD_SECS: i64 = 300;
        let start_ma_sec = start_locf_sec - Duration::seconds(MAX_MA_PERIOD_SECS - 1);
        let end_ma_sec = end_locf_sec + Duration::seconds(MAX_MA_PERIOD_SECS - 1);

        let query = r#"
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
                AND price_history_locf.time = eval_indicators.time;
        "#;
        sqlx::query(&query)
            .bind(start_ma_sec)
            .bind(end_ma_sec)
            .bind(start_locf_sec)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        return Ok(());
    }

    pub async fn update_price_entry_next(
        &self,
        price_entry_time: &DateTime<Utc>,
        next: &DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let pool = self.get_pool();

        let query = r#"
            UPDATE price_history 
            SET next = $1 
            WHERE time = $2 AND next IS NULL
        "#;
        let result = sqlx::query(query)
            .bind(next)
            .bind(price_entry_time)
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
