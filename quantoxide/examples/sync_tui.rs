//! Example demonstrating how to run the sync process using its TUI abstraction.

use std::env;

use dotenv::dotenv;

use quantoxide::{
    Database,
    sync::{SyncConfig, SyncEngine, SyncMode},
    tui::{SyncTui, TuiConfig, TuiLogger},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let pg_url = env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    println!("Launching `SyncTui`...");

    let sync_tui = SyncTui::launch(TuiConfig::default(), None).await?;

    sync_tui.log("Initializing database...".into()).await?;

    let db = Database::new(&pg_url).await?;

    sync_tui
        .log("Database ready. Initializing `SyncEngine`...".into())
        .await?;

    let config = SyncConfig::default();
    // How far back to fetch price history data can be configured with:
    // let config = config.with_price_history_reach(180.try_into()?); // 180 days

    let sync_engine = SyncEngine::new(config, db, domain, SyncMode::Backfill)?;

    sync_tui
        .log("Initialization OK. Coupling `SyncEngine`...".into())
        .await?;

    sync_tui.couple(sync_engine)?;

    let final_status = sync_tui.until_stopped().await;
    println!("`SyncTui` status: {final_status}");

    Ok(())
}
