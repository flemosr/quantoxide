use analysis::{
    api::ApiContext,
    db::DbContext,
    error::Result,
    sync::{Sync, SyncState},
};

mod env;

use env::{
    LNM_API_COOLDOWN_SEC, LNM_API_DOMAIN, LNM_API_ERROR_COOLDOWN_SEC, LNM_API_ERROR_MAX_TRIALS,
    LNM_PRICE_HISTORY_BATCH_ENTRIES, POSTGRES_DB_URL, SYNC_HISTORY_REACH_WEEKS,
};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    let db = DbContext::new(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Init `api`...");

    let api = ApiContext::new(LNM_API_DOMAIN.to_string());

    println!("`api` is ready. Starting `sync`...");

    let sync = Sync::new(
        *LNM_API_COOLDOWN_SEC,
        *LNM_API_ERROR_COOLDOWN_SEC,
        *LNM_API_ERROR_MAX_TRIALS,
        *LNM_PRICE_HISTORY_BATCH_ENTRIES,
        *SYNC_HISTORY_REACH_WEEKS,
        db.clone(),
        api.clone(),
    );

    let mut sync_rx = sync.receiver();
    tokio::spawn(async move {
        while let Ok(res) = sync_rx.recv().await {
            match res {
                SyncState::NotInitiated => {
                    println!("SyncState::NotInitiated");
                }
                SyncState::InProgress(price_history_state) => {
                    println!("SyncState::InProgress");
                    println!("{price_history_state}");
                }
                SyncState::Synced => {
                    println!("SyncState::Synced");
                }
                SyncState::Failed => {
                    println!("SyncState::Failed");
                    break;
                }
            }
        }
        println!("Sync Receiver closed");
    });

    sync.start().await
}
