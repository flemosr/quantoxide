use sqlx::{postgres::PgPoolOptions, Pool, Postgres};

mod models;

pub type DbPool = Pool<Postgres>;

pub async fn init(database_url: &str) -> Result<DbPool, sqlx::Error> {
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

    Ok(pool)
}

pub async fn get_all_entries(pool: &DbPool) -> Result<Vec<models::PriceHistoryEntry>, sqlx::Error> {
    sqlx::query_as::<_, models::PriceHistoryEntry>(
        "SELECT id, timestamp, value FROM price_history ORDER BY timestamp DESC",
    )
    .fetch_all(pool)
    .await
}
