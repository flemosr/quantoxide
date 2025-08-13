use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tokio::{
    sync::{OnceCell, broadcast::error::RecvError, mpsc},
    time,
};

use crate::{
    sync::{SyncEngine, SyncReceiver, SyncUpdate},
    util::AbortOnDropHandle,
};

use super::{
    config::TuiConfig,
    core::{self, TuiControllerShutdown, TuiLogger},
    error::{Result, TuiError},
    status::{TuiStatus, TuiStatusManager, TuiStatusStopped},
    terminal::TuiTerminal,
};

mod view;

use view::SyncTuiView;

#[derive(Debug)]
pub enum SyncUiMessage {
    LogEntry(String),
    StateUpdate(String),
    ShutdownCompleted,
}

pub struct SyncTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<SyncTuiView>>,
    // Retain ownership to ensure `TuiTerminal` destructor is executed when
    // `SyncTui` is dropped.
    _tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<SyncUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `SyncTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    sync_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    sync_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl SyncTui {
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Arc<Self>> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<SyncUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = SyncTuiView::new(config.max_tui_log_len(), log_file);

        let status_manager = TuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = core::spawn_ui_task(
            config.event_check_interval(),
            tui_view,
            status_manager.clone(),
            tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let sync_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = core::spawn_shutdown_signal_listener(
            config.shutdown_timeout(),
            status_manager.clone(),
            shutdown_rx,
            ui_task_handle.clone(),
            {
                let ui_tx = ui_tx.clone();
                || async move { ui_tx.send(SyncUiMessage::ShutdownCompleted).await }
            },
            sync_controller.clone(),
        );

        Ok(Arc::new(Self {
            event_check_interval: config.event_check_interval(),
            shutdown_timeout: config.shutdown_timeout(),
            status_manager,
            _tui_terminal: tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            sync_controller,
            sync_update_listener_handle: OnceCell::new(),
        }))
    }

    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    fn spawn_sync_update_listener(
        status_manager: Arc<TuiStatusManager<SyncTuiView>>,
        mut sync_rx: SyncReceiver,
        ui_tx: mpsc::Sender<SyncUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let handle_sync_update = async |sync_update: SyncUpdate| -> Result<()> {
                match sync_update {
                    SyncUpdate::Status(sync_status) => {
                        ui_tx
                            .send(SyncUiMessage::LogEntry(format!(
                                "Sync status: {sync_status}"
                            )))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                    SyncUpdate::PriceTick(tick) => {
                        ui_tx
                            .send(SyncUiMessage::LogEntry(tick.to_string()))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                    SyncUpdate::PriceHistoryState(price_history_state) => {
                        ui_tx
                            .send(SyncUiMessage::StateUpdate(format!(
                                "\n{}",
                                price_history_state.summary()
                            )))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                }
                Ok(())
            };

            loop {
                match sync_rx.recv().await {
                    Ok(live_update) => {
                        if let Err(e) = handle_sync_update(live_update).await {
                            status_manager.set_crashed(e);
                            return;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        let log_msg = format!("Sync updates lagged by {skipped} messages");
                        if let Err(e) = ui_tx.send(SyncUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(TuiError::Generic(e.to_string()));
                            return;
                        }

                        // Keep trying to receive
                    }
                    Err(e) => {
                        // `sync_rx` is expected to be dropped during shutdown

                        let status = status_manager.status();
                        if status.is_shutdown_initiated() || status.is_shutdown() {
                            return;
                        }

                        status_manager.set_crashed(TuiError::Generic(format!(
                            "`sync_rx` returned err {:?}",
                            e
                        )));

                        return;
                    }
                }
            }
        })
        .into()
    }

    pub fn couple(&self, engine: SyncEngine) -> Result<()> {
        if self.sync_controller.initialized() {
            return Err(TuiError::Generic(
                "`sync_engine` was already coupled".to_string(),
            ));
        }

        let sync_rx = engine.update_receiver();

        let sync_update_listener_handle = Self::spawn_sync_update_listener(
            self.status_manager.clone(),
            sync_rx,
            self.ui_tx.clone(),
        );

        let sync_controller = engine.start();

        self.sync_controller
            .set(sync_controller)
            .map_err(|_| TuiError::Generic("Failed to set `sync_controller`".to_string()))?;

        self.sync_update_listener_handle
            .set(sync_update_listener_handle)
            .map_err(|_| {
                TuiError::Generic("Failed to set `sync_update_listener_handle`".to_string())
            })?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let sync_controller = self
            .sync_controller
            .get()
            .map(|inner_ref| inner_ref.clone());

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(SyncUiMessage::ShutdownCompleted),
            sync_controller,
        )
        .await
    }

    pub async fn until_stopped(&self) -> Arc<TuiStatusStopped> {
        loop {
            if let TuiStatus::Stopped(status_stopped) = self.status() {
                return status_stopped;
            }

            time::sleep(self.event_check_interval).await;
        }
    }
}

#[async_trait]
impl TuiLogger for SyncTui {
    async fn log(&self, log_entry: String) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(SyncUiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| TuiError::Generic("TUI is not running".to_string()))
    }
}

impl Drop for SyncTui {
    fn drop(&mut self) {
        if let Some(ui_handle) = self
            .ui_task_handle
            .lock()
            .expect("`ui_task_handle` mutex can't be poisoned")
            .take()
        {
            ui_handle.abort();
        };
    }
}
