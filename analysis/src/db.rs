use error::Result;
use sqlx::{postgres::PgPoolOptions, Pool, Postgres};
use tokio::sync::OnceCell;

pub mod error;
mod models;
pub mod price_history;

use error::DbError;

static DB_CONNECTION: OnceCell<Pool<Postgres>> = OnceCell::const_new();

pub async fn init(postgres_db_url: &str) -> Result<()> {
    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(postgres_db_url)
        .await
        .map_err(|e| DbError::Connection(e))?;

    println!("Successfully connected to the database");

    println!("Running migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| DbError::Migration(e))?;

    println!("Migrations completed successfully");

    println!("Checking database connection...");
    let row: (i64,) = sqlx::query_as("SELECT $1")
        .bind(150_i64)
        .fetch_one(&pool)
        .await
        .map_err(|e| DbError::Query(e))?;

    assert_eq!(row.0, 150);
    println!("Database check successful");

    DB_CONNECTION
        .set(pool)
        .map_err(|_| DbError::Init("`db` must not be initialized"))?;

    Ok(())
}

fn get_pool() -> Result<&'static Pool<Postgres>> {
    let pool = DB_CONNECTION
        .get()
        .ok_or_else(|| DbError::Init("`db` must be initialized"))?;

    Ok(&pool)
}
