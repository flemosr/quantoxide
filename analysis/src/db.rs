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

    pub async fn add_price_entry(
        &self,
        price_entry: &PriceEntryLNM,
        next: Option<&DateTime<Utc>>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.get_pool().begin().await?;
        let query = r#"
            INSERT INTO price_history (time, value, next) 
            VALUES ($1, $2, $3) 
            ON CONFLICT (time) 
            DO NOTHING
        "#;

        let result = sqlx::query(query)
            .bind(price_entry.time())
            .bind(price_entry.value())
            .bind(next)
            .execute(&mut *tx)
            .await?;

        if result.rows_affected() == 0 {
            return Ok(false);
        }

        // We can assume that `price_history_locf` is up-to-date in regards to the previously
        // added price entries.
        // A new entry will potentially affect the values of all locf entries between its own
        // `locf_sec` and the `locf_sec` corresponding to the observed entry just AFTER it.
        // If there are no observed entries after the just-added entry, only the value of the locf
        // entry corresponding to `locf_sec` will potentially be affected.
        // A new entry added after the max locf time, or before the min locf time may result in
        // gaps in the `price_history_locf` history. Said gaps need to be filled carrying the
        // corresponding locf value onwards or backwards.

        let added_entry_time = price_entry.time();
        let added_locf_sec = Self::get_locf_sec(added_entry_time);

        let prev_locf_sec: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT max(time) FROM price_history_locf WHERE time <= $1")
                .bind(added_entry_time)
                .fetch_one(&mut *tx)
                .await?;
        // `prev_locf_sec` will be `None` only when `added_locf_sec` is the new min locf time
        let start_locf_sec = prev_locf_sec.unwrap_or(added_locf_sec);

        let next_observed_entry_time: Option<DateTime<Utc>> =
            sqlx::query_scalar("SELECT min(time) FROM price_history WHERE time > $1")
                .bind(added_entry_time)
                .fetch_one(&mut *tx)
                .await?;
        let end_locf_sec = if let Some(next_observed_entry_time) = next_observed_entry_time {
            Self::get_locf_sec(&next_observed_entry_time)
        } else {
            added_locf_sec
        };

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

        tx.commit().await?;

        Ok(true)
    }

    pub async fn update_price_entry_next(
        &self,
        price_entry: &PriceEntryLNM,
        next: &DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let pool = self.get_pool();
        let query = "UPDATE price_history 
                     SET next = $1 
                     WHERE time = $2 AND value = $3 AND next IS NULL";

        let result = sqlx::query(query)
            .bind(next)
            .bind(price_entry.time())
            .bind(price_entry.value())
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
