use std::env;

use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), sqlx::Error> {
    let lnm_api_key = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    let lnm_api_secret = env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    let lnm_api_passphrase =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");
    let postgres_db_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");

    println!("LNM_API_KEY: {lnm_api_key}");
    println!("LNM_API_SECRET: {lnm_api_secret}");
    println!("LNM_API_PASSPHRASE: {lnm_api_passphrase}");
    println!("POSTGRES_DB_URL: {postgres_db_url}");

    println!("Trying to connect to the db...");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&postgres_db_url)
        .await?;

    let row: (i64,) = sqlx::query_as("SELECT $1")
        .bind(150_i64)
        .fetch_one(&pool)
        .await?;

    assert_eq!(row.0, 150);

    println!("Connected successfully");

    Ok(())
}
