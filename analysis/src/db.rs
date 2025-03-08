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

    pub async fn get_price_history(&self) -> Result<Vec<PriceEntry>, sqlx::Error> {
        let pool = self.get_pool();
        sqlx::query_as::<_, PriceEntry>("SELECT * FROM price_history ORDER BY time DESC LIMIT 1000")
            .fetch_all(pool)
            .await
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

    pub async fn add_price_entry(&self, price_entry: &PriceEntryLNM) -> Result<bool, sqlx::Error> {
        let pool = self.get_pool();
        let query = "INSERT INTO price_history (time, value) 
                     VALUES ($1, $2) 
                     ON CONFLICT (time) 
                     DO NOTHING";

        let result = sqlx::query(query)
            .bind(price_entry.time())
            .bind(price_entry.value())
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
