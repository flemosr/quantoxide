use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Duration;
use tokio::{sync::broadcast, time};

use lnm_sdk::api_v2::{ApiClientConfig, RestClient, WebSocketClient};

use crate::{
    db::Database,
    sync::config::{SyncConfig, SyncControllerConfig},
    tui::{TuiControllerShutdown, TuiError, error::Result as TuiResult},
    util::AbortOnDropHandle,
};

use super::{
    error::{Result, SyncError},
    process::{SyncProcess, error::SyncProcessFatalError},
    state::{SyncReader, SyncReceiver, SyncStatus, SyncStatusManager, SyncTransmiter, SyncUpdate},
};

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

        if handle.is_finished() {
            let status = self.status_manager.status_snapshot();
            return Err(SyncError::SyncAlreadyTerminated(status));
        }

        self.status_manager.update(SyncStatus::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SyncProcessFatalError::SendShutdownSignalFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SyncProcessFatalError::SyncProcessTaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout()) => {
                        handle.abort();
                        Err(SyncProcessFatalError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        if let Err(e) = shutdown_res {
            let e_ref = Arc::new(e);
            self.status_manager.update(e_ref.clone().into());

            return Err(SyncError::SyncShutdownFailed(e_ref));
        }

        self.status_manager.update(SyncStatus::Shutdown);
        Ok(())
    }
}

#[async_trait]
impl TuiControllerShutdown for SyncController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::SyncShutdownFailed)
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
    db: Arc<Database>,
    api_rest: Arc<RestClient>,
    api_ws: Arc<WebSocketClient>,
    mode: SyncMode,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmiter,
}

impl SyncEngine {
    pub(crate) fn with_api(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
        mode: SyncMode,
    ) -> Self {
        let (update_tx, _) = broadcast::channel::<SyncUpdate>(1000);

        let status_manager = SyncStatusManager::new(update_tx.clone());

        Self {
            config: config.into(),
            db,
            api_rest,
            api_ws,
            mode,
            status_manager,
            update_tx,
        }
    }

    pub fn new(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_domain: impl ToString,
        mode: SyncMode,
    ) -> Result<Self> {
        let config: SyncConfig = config.into();
        let api_config = ApiClientConfig::from(&config);
        let domain = api_domain.to_string();

        let api_rest =
            RestClient::new(&api_config, domain.clone()).map_err(SyncError::RestApiInit)?;
        let api_ws = WebSocketClient::new(&api_config, domain);

        Ok(SyncEngine::with_api(config, db, api_rest, api_ws, mode))
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

        let handle = SyncProcess::spawn(
            &self.config,
            self.db,
            self.api_rest,
            self.api_ws,
            self.mode,
            shutdown_tx.clone(),
            self.status_manager.clone(),
            self.update_tx,
        );

        SyncController::new(&self.config, handle, shutdown_tx, self.status_manager)
    }
}
