use std::sync::{Arc, Mutex};

use tokio::{sync::broadcast, time};

use crate::{
    db::DbContext, signal::process::LiveSignalProcess, sync::SyncReader, trade::live::LiveConfig,
    util::AbortOnDropHandle,
};

use super::{
    core::ConfiguredSignalEvaluator,
    error::{Result, SignalError},
    state::{
        LiveSignalReader, LiveSignalReceiver, LiveSignalStatus, LiveSignalStatusManager,
        LiveSignalTransmiter, LiveSignalUpdate,
    },
};

#[derive(Debug)]
struct LiveSignalControllerConfig {
    shutdown_timeout: time::Duration,
}

impl From<&LiveSignalConfig> for LiveSignalControllerConfig {
    fn from(value: &LiveSignalConfig) -> Self {
        Self {
            shutdown_timeout: value.shutdown_timeout,
        }
    }
}

#[derive(Debug)]
pub struct LiveSignalController {
    config: LiveSignalControllerConfig,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<LiveSignalStatusManager>,
}

impl LiveSignalController {
    fn new(
        config: &LiveSignalConfig,
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<LiveSignalStatusManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            status_manager,
        })
    }

    pub fn reader(&self) -> Arc<dyn LiveSignalReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> LiveSignalReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> LiveSignalStatus {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`LiveSignalController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the live signal process and consumes
    /// the task handle. If a clean shutdown fails, the process is aborted.
    /// This method can only be called once per controller instance.
    /// Returns an error if the process had to be aborted, or if it the handle
    /// was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(SignalError::LiveSignalAlreadyShutdown);
        };

        self.status_manager
            .update(LiveSignalStatus::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SignalError::SendShutdownSignalFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SignalError::LiveSignalProcessTaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout) => {
                        handle.abort();
                        Err(SignalError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        self.status_manager.update(LiveSignalStatus::Shutdown);

        shutdown_res
    }
}

#[derive(Clone, Debug)]
pub struct LiveSignalConfig {
    sync_update_timeout: time::Duration,
    restart_interval: time::Duration,
    shutdown_timeout: time::Duration,
}

impl Default for LiveSignalConfig {
    fn default() -> Self {
        Self {
            sync_update_timeout: time::Duration::from_secs(5),
            restart_interval: time::Duration::from_secs(10),
            shutdown_timeout: time::Duration::from_secs(6),
        }
    }
}

impl LiveSignalConfig {
    pub fn sync_update_timeout(&self) -> time::Duration {
        self.sync_update_timeout
    }

    pub fn restart_interval(&self) -> time::Duration {
        self.restart_interval
    }

    pub fn shutdown_timeout(&self) -> time::Duration {
        self.shutdown_timeout
    }

    pub fn set_sync_update_timeout(mut self, secs: u64) -> Self {
        self.sync_update_timeout = time::Duration::from_secs(secs);
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

impl From<&LiveConfig> for LiveSignalConfig {
    fn from(value: &LiveConfig) -> Self {
        Self {
            sync_update_timeout: value.sync_update_timeout(),
            restart_interval: value.restart_interval(),
            shutdown_timeout: value.shutdown_timeout(),
        }
    }
}

pub struct LiveSignalEngine {
    config: LiveSignalConfig,
    db: Arc<DbContext>,
    sync_reader: Arc<dyn SyncReader>,
    evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    status_manager: Arc<LiveSignalStatusManager>,
    update_tx: LiveSignalTransmiter,
}

impl LiveSignalEngine {
    pub fn new(
        config: impl Into<LiveSignalConfig>,
        db: Arc<DbContext>,
        sync_reader: Arc<dyn SyncReader>,
        evaluators: Arc<Vec<ConfiguredSignalEvaluator>>,
    ) -> Result<Self> {
        if evaluators.is_empty() {
            return Err(SignalError::EmptyEvaluatorsVec);
        }

        let (update_tx, _) = broadcast::channel::<LiveSignalUpdate>(100);

        let status_manager = LiveSignalStatusManager::new(update_tx.clone());

        Ok(Self {
            config: config.into(),
            db,
            sync_reader,
            evaluators,
            status_manager,
            update_tx,
        })
    }

    pub fn reader(&self) -> Arc<dyn LiveSignalReader> {
        self.status_manager.clone()
    }

    pub fn update_receiver(&self) -> LiveSignalReceiver {
        self.status_manager.update_receiver()
    }

    pub fn status_snapshot(&self) -> LiveSignalStatus {
        self.status_manager.status_snapshot()
    }

    pub fn start(self) -> Arc<LiveSignalController> {
        // Internal channel for shutdown signal
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let handle = LiveSignalProcess::new(
            &self.config,
            self.db,
            self.evaluators,
            shutdown_tx.clone(),
            self.sync_reader,
            self.status_manager.clone(),
            self.update_tx,
        )
        .spawn_recovery_loop();

        LiveSignalController::new(&self.config, handle, shutdown_tx, self.status_manager)
    }
}
