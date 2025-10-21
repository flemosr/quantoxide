use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Duration;
use tokio::{sync::broadcast, time};

use lnm_sdk::api::ApiContext;

use crate::{
    db::DbContext,
    trade::live::LiveConfig,
    tui::{Result as TuiResult, TuiControllerShutdown, TuiError},
    util::AbortOnDropHandle,
};

use super::{
    error::{Result, SyncError},
    process::SyncProcess,
    state::{SyncReader, SyncReceiver, SyncStatus, SyncStatusManager, SyncTransmiter, SyncUpdate},
};

#[derive(Debug)]
struct SyncControllerConfig {
    shutdown_timeout: time::Duration,
}

impl From<&SyncConfig> for SyncControllerConfig {
    fn from(value: &SyncConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

#[derive(Debug)]
pub struct SyncController {
    config: SyncControllerConfig,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<SyncStatusManager>,
}

impl SyncController {
    fn new(
        config: &SyncConfig,
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<SyncStatusManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            status_manager,
        })
    }

    pub fn reader(&self) -> Arc<dyn SyncReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> SyncReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> SyncStatus {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`SyncController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the sync process and consumes the
    /// task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(SyncError::SyncAlreadyShutdown);
        };

        self.status_manager.update(SyncStatus::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SyncError::SendShutdownFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SyncError::TaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout) => {
                        handle.abort();
                        Err(SyncError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        self.status_manager.update(SyncStatus::Shutdown);

        shutdown_res
    }
}

#[async_trait]
impl TuiControllerShutdown for SyncController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::SyncShutdownFailed)
    }
}

#[derive(Clone, Debug)]
pub struct SyncConfig {
    api_cooldown: time::Duration,
    api_error_cooldown: time::Duration,
    api_error_max_trials: u32,
    api_history_batch_size: usize,
    sync_history_reach: Duration,
    re_sync_history_interval: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            api_cooldown: time::Duration::from_secs(2),
            api_error_cooldown: time::Duration::from_secs(10),
            api_error_max_trials: 3,
            api_history_batch_size: 1000,
            sync_history_reach: Duration::hours(24 * 7 * 4),
            re_sync_history_interval: time::Duration::from_secs(300),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl SyncConfig {
    pub fn api_cooldown(&self) -> time::Duration {
        self.api_cooldown
    }

    pub fn api_error_cooldown(&self) -> time::Duration {
        self.api_error_cooldown
    }

    pub fn api_error_max_trials(&self) -> u32 {
        self.api_error_max_trials
    }

    pub fn api_history_batch_size(&self) -> usize {
        self.api_history_batch_size
    }

    pub fn sync_history_reach(&self) -> Duration {
        self.sync_history_reach
    }

    pub fn re_sync_history_interval(&self) -> time::Duration {
        self.re_sync_history_interval
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn set_api_cooldown(mut self, secs: u64) -> Self {
        self.api_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_cooldown(mut self, secs: u64) -> Self {
        self.api_error_cooldown = time::Duration::from_secs(secs);
        self
    }

    pub fn set_api_error_max_trials(mut self, max_trials: u32) -> Self {
        self.api_error_max_trials = max_trials;
        self
    }

    pub fn set_api_history_batch_size(mut self, size: usize) -> Self {
        self.api_history_batch_size = size;
        self
    }

    pub fn set_sync_history_reach(mut self, hours: u64) -> Self {
        self.sync_history_reach = Duration::hours(hours as i64);
        self
    }

    pub fn set_re_sync_history_interval(mut self, secs: u64) -> Self {
        self.re_sync_history_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_restart_interval(mut self, secs: u64) -> Self {
        self.restart_interval = time::Duration::from_secs(secs);
        self
    }

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = time::Duration::from_secs(secs);
        self
    }
}

impl From<&LiveConfig> for SyncConfig {
    fn from(value: &LiveConfig) -> Self {
        SyncConfig {
            api_cooldown: value.api_cooldown(),
            api_error_cooldown: value.api_error_cooldown(),
            api_error_max_trials: value.api_error_max_trials(),
            api_history_batch_size: value.api_history_batch_size(),
            sync_history_reach: value.sync_history_reach(),
            re_sync_history_interval: value.re_sync_history_interval(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

#[derive(Debug)]
pub enum SyncMode {
    Backfill,
    Live { range: Duration },
    Full,
}

pub struct SyncEngine {
    config: SyncConfig,
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    mode: SyncMode,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmiter,
}

impl SyncEngine {
    pub fn new(
        config: impl Into<SyncConfig>,
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        mode: SyncMode,
    ) -> Self {
        let (update_tx, _) = broadcast::channel::<SyncUpdate>(100);

        let status_manager = SyncStatusManager::new(update_tx.clone());

        Self {
            config: config.into(),
            db,
            api,
            mode,
            status_manager,
            update_tx,
        }
    }

    pub fn reader(&self) -> Arc<dyn SyncReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> SyncReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> SyncStatus {
        self.status_manager.status_snapshot()
    }

    pub fn start(self) -> Arc<SyncController> {
        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let handle = SyncProcess::new(
            &self.config,
            self.db,
            self.api,
            self.mode,
            shutdown_tx.clone(),
            self.status_manager.clone(),
            self.update_tx,
        )
        .spawn_recovery_loop();

        SyncController::new(&self.config, handle, shutdown_tx, self.status_manager)
    }
}
