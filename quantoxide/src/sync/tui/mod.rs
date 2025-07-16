use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    sync::{OnceCell, mpsc},
    time,
};

use crate::{
    tui::{self, Result, TuiControllerShutdown, TuiStatusManager, TuiTerminal},
    util::AbortOnDropHandle,
};

pub use crate::tui::{TuiConfig, TuiError as SyncTuiError, TuiStatus, TuiStatusStopped};

use super::{SyncEngine, SyncReceiver, SyncState, SyncStateNotSynced, SyncUpdate};

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
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Self> {
        let log_file = tui::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<SyncUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = SyncTuiView::new(config.max_tui_log_len(), log_file);

        let status_manager = TuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = tui::spawn_ui_task(
            config.event_check_interval(),
            tui_view,
            status_manager.clone(),
            tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let sync_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = tui::spawn_shutdown_signal_listener(
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

        Ok(Self {
            event_check_interval: config.event_check_interval(),
            shutdown_timeout: config.shutdown_timeout(),
            status_manager,
            _tui_terminal: tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            sync_controller,
            sync_update_listener_handle: OnceCell::new(),
        })
    }

    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(SyncUiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| SyncTuiError::Generic("TUI is not running".to_string()))
    }

    fn spawn_sync_update_listener(
        status_manager: Arc<TuiStatusManager<SyncTuiView>>,
        mut sync_rx: SyncReceiver,
        ui_tx: mpsc::Sender<SyncUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let handle_sync_update = async |sync_update: SyncUpdate| -> Result<()> {
                match sync_update {
                    SyncUpdate::StateChange(sync_state) => {
                        let log_str = match sync_state {
                            SyncState::NotSynced(sync_state_not_synced) => {
                                match sync_state_not_synced.as_ref() {
                                    SyncStateNotSynced::NotInitiated => {
                                        "Sync state: NotInitiated".to_string()
                                    }
                                    SyncStateNotSynced::Starting => {
                                        "Sync state: Starting".to_string()
                                    }
                                    SyncStateNotSynced::InProgress(price_history_state) => {
                                        ui_tx
                                            .send(SyncUiMessage::StateUpdate(
                                                price_history_state.to_string(),
                                            ))
                                            .await
                                            .map_err(|e| SyncTuiError::Generic(e.to_string()))?;

                                        "Sync state: InProgress".to_string()
                                    }
                                    SyncStateNotSynced::WaitingForResync => {
                                        "Sync state: WaitingForResync".to_string()
                                    }
                                    SyncStateNotSynced::Failed(e) => {
                                        format!("Sync state: Failed - {:?}", e)
                                    }
                                    SyncStateNotSynced::Restarting => {
                                        "Sync state: Restarting".to_string()
                                    }
                                }
                            }
                            SyncState::Synced => "Sync state: Synced".to_string(),
                            SyncState::ShutdownInitiated => {
                                "Sync state: ShutdownInitiated".to_string()
                            }
                            SyncState::Shutdown => "Sync state: Shutdown".to_string(),
                        };

                        ui_tx
                            .send(SyncUiMessage::LogEntry(log_str))
                            .await
                            .map_err(|e| SyncTuiError::Generic(e.to_string()))?;
                    }
                    SyncUpdate::PriceTick(tick) => {
                        ui_tx
                            .send(SyncUiMessage::LogEntry(format!("Price tick: {:?}", tick)))
                            .await
                            .map_err(|e| SyncTuiError::Generic(e.to_string()))?;
                    }
                }
                Ok(())
            };

            while let Ok(sync_update) = sync_rx.recv().await {
                if let Err(e) = handle_sync_update(sync_update).await {
                    status_manager.set_crashed(e);
                    return;
                }
            }

            // `sync_tx` was dropped, which is expected during shutdown

            let status = status_manager.status();
            if status.is_shutdown_initiated() || status.is_shutdown() {
                return;
            }

            status_manager.set_crashed(SyncTuiError::Generic(
                "`sync_tx` was unexpectedly dropped".to_string(),
            ));
        })
        .into()
    }

    pub fn couple(&self, engine: SyncEngine) -> Result<()> {
        if self.sync_controller.initialized() {
            return Err(SyncTuiError::Generic(
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
            .map_err(|_| SyncTuiError::Generic("Failed to set `sync_controller`".to_string()))?;

        self.sync_update_listener_handle
            .set(sync_update_listener_handle)
            .map_err(|_| {
                SyncTuiError::Generic("Failed to set `sync_update_listener_handle`".to_string())
            })?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let sync_controller = self
            .sync_controller
            .get()
            .map(|inner_ref| inner_ref.clone());

        tui::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(SyncUiMessage::ShutdownCompleted),
            sync_controller,
        )
        .await
    }

    pub async fn until_stopped(self) -> Arc<TuiStatusStopped> {
        loop {
            if let TuiStatus::Stopped(status_stopped) = self.status() {
                return status_stopped;
            }

            time::sleep(self.event_check_interval).await;
        }
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
