use std::env;

mod db;

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

    println!("Trying to init the db...");

    let pool = db::init(&postgres_db_url).await?;

    let price_history_entries = db::get_all_entries(&pool).await?;

    println!("price_history_entries: {:?}", price_history_entries);

    Ok(())
}
