use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::sync::OnceCell;

use crate::Result;

mod models;
pub mod price_history;

static DB_CONNECTION: OnceCell<Pool<Postgres>> = OnceCell::const_new();

pub async fn init(postgres_db_url: &str) -> Result<()> {
    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(postgres_db_url)
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

    DB_CONNECTION
        .set(pool)
        .expect("`db` must not be initialized");

    Ok(())
}

fn get_pool() -> &'static Pool<Postgres> {
    DB_CONNECTION.get().expect("`db` must be initialized")
}
