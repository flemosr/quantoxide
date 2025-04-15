use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

pub mod error;
mod models;
mod postgres;
mod repositories;

use error::{DbError, Result};
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

        println!("Checking database connection...");
        let row = sqlx::query_scalar!("SELECT $1::bigint", 150)
            .fetch_one(&pool)
            .await
            .map_err(DbError::Query)?;

        assert_eq!(row, Some(150));
        println!("Database check successful");

        let pool = Arc::new(pool);
        let price_history = Box::new(PgPriceHistoryRepo::new(pool.clone()));
        let price_ticks = Box::new(PgPriceTicksRepo::new(pool));

        Ok(Arc::new(Self {
            price_history,
            price_ticks,
        }))
    }
}
