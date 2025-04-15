use analysis::{
    api::ApiContext,
    db::DbContext,
    error::Result,
    signal::{SignalJob, SignalJobConfig},
    sync::{Sync, SyncConfig, SyncState},
};

mod env;

use env::{
    LNM_API_COOLDOWN_SEC, LNM_API_DOMAIN, LNM_API_ERROR_COOLDOWN_SEC, LNM_API_ERROR_MAX_TRIALS,
    LNM_PRICE_HISTORY_BATCH_SIZE, POSTGRES_DB_URL, RESTART_SYNC_INTERVAL_SEC,
    RE_SYNC_HISTORY_INTERVAL_SEC, SYNC_HISTORY_REACH_HOURS,
};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Trying to init `db`...");

    let db = DbContext::new(&POSTGRES_DB_URL).await?;

    println!("`db` is ready. Init `api`...");

    let api = ApiContext::new(LNM_API_DOMAIN.to_string());

    println!("`api` is ready. Starting `sync`...");

    let config = SyncConfig::default()
        .set_api_cooldown(*LNM_API_COOLDOWN_SEC)
        .set_api_error_cooldown(*LNM_API_ERROR_COOLDOWN_SEC)
        .set_api_error_max_trials(*LNM_API_ERROR_MAX_TRIALS)
        .set_api_history_batch_size(*LNM_PRICE_HISTORY_BATCH_SIZE)
        .set_sync_history_reach(*SYNC_HISTORY_REACH_HOURS)
        .set_re_sync_history_interval(*RE_SYNC_HISTORY_INTERVAL_SEC)
        .set_restart_interval(*RESTART_SYNC_INTERVAL_SEC);

    let sync = Sync::new(config, db.clone(), api);

    let sync_controller = sync.start()?;

    // let mut sync_rx = sync_controller.receiver();

    println!("`sync` started");

    while let Ok(res) = sync_controller.receiver().recv().await {
        match res.as_ref() {
            SyncState::NotInitiated => {
                println!("SyncState::NotInitiated");
            }
            SyncState::Starting => {
                println!("SyncState::Starting");
            }
            SyncState::InProgress(price_history_state) => {
                println!("SyncState::InProgress");
                println!("{price_history_state}");
            }
            SyncState::Synced => {
                println!("SyncState::Synced");
                break;
            }
            SyncState::Restarting => {
                println!("SyncState::Restarting");
            }
            SyncState::Failed(err) => {
                println!("SyncState::Failed with error {err}");
            }
        }
    }

    println!("sync state achieved. Starting `signal` job...");

    let config = SignalJobConfig::default();
    let signal_job = SignalJob::new(config, db, sync_controller.clone());

    signal_job.start();

    // Wait for termination signal
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for event");

    sync_controller.abort();

    Ok(())
}
