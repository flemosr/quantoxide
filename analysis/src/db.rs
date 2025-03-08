use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::sync::OnceCell;

use crate::api::PriceEntry;

mod models;

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

    pub async fn get_all_entries(&self) -> Result<Vec<models::PriceHistoryEntry>, sqlx::Error> {
        let pool = self.get_pool();
        sqlx::query_as::<_, models::PriceHistoryEntry>(
            "SELECT id, timestamp, value FROM price_history ORDER BY timestamp DESC",
        )
        .fetch_all(pool)
        .await
    }

    pub async fn add_price_entry(&self, price_entry: &PriceEntry) -> Result<bool, sqlx::Error> {
        let pool = self.get_pool();
        let query = "INSERT INTO price_history (timestamp, value) 
                     VALUES ($1, $2) 
                     ON CONFLICT (timestamp) 
                     DO NOTHING";

        let result = sqlx::query(query)
            .bind(price_entry.time())
            .bind(price_entry.value())
            .execute(pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }
}
