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
    sync::{SyncEngine, SyncMode, SyncReader, SyncUpdate},
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
    PriceHistoryStateUpdate(String),
    FundingSettlementsStateUpdate(String),
    ShutdownCompleted,
}

/// Terminal user interface for synchronization operations.
///
/// `SyncTui` provides a visual interface for monitoring price data synchronization, including sync
/// status, price ticks, and price history state. It must be coupled with a [`SyncEngine`] before
/// synchronization begins.
pub struct SyncTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<SyncTuiView>>,
    // Ownership ensures the `TuiTerminal` destructor is executed when `SyncTui` is dropped
    tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<SyncUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `SyncTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    sync_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    sync_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl SyncTui {
    /// Launches a new sync TUI with the specified configuration.
    ///
    /// Optionally writes TUI logs to a file if `log_file_path` is provided.
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Arc<Self>> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<SyncUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

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
            tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            sync_controller,
            sync_update_listener_handle: OnceCell::new(),
        }))
    }

    /// Returns the current [`TuiStatus`] as a snapshot.
    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    fn spawn_sync_update_listener(
        status_manager: Arc<TuiStatusManager<SyncTuiView>>,
        sync_reader: Arc<dyn SyncReader>,
        ui_tx: mpsc::Sender<SyncUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let send_ui_msg = async |ui_msg: SyncUiMessage| -> Result<()> {
                ui_tx
                    .send(ui_msg)
                    .await
                    .map_err(|e| TuiError::SyncTuiSendFailed(Box::new(e)))
            };

            let handle_sync_update = async |sync_update: SyncUpdate| -> Result<()> {
                match sync_update {
                    SyncUpdate::Status(sync_status) => {
                        send_ui_msg(SyncUiMessage::LogEntry(format!(
                            "Sync status: {sync_status}"
                        )))
                        .await?;
                    }
                    SyncUpdate::PriceTick(tick) => {
                        send_ui_msg(SyncUiMessage::LogEntry(tick.to_string())).await?;
                    }
                    SyncUpdate::PriceHistoryState(price_history_state) => {
                        send_ui_msg(SyncUiMessage::PriceHistoryStateUpdate(
                            price_history_state.summary(),
                        ))
                        .await?;
                    }
                    SyncUpdate::FundingSettlementsState(funding_state) => {
                        send_ui_msg(SyncUiMessage::FundingSettlementsStateUpdate(
                            funding_state.summary(),
                        ))
                        .await?;
                    }
                }
                Ok(())
            };

            if matches!(sync_reader.mode(), SyncMode::Live(None)) {
                if let Err(e) = send_ui_msg(SyncUiMessage::PriceHistoryStateUpdate(
                    "Not evaluated.".to_string(),
                ))
                .await
                {
                    status_manager.set_crashed(e);
                    return;
                }

                if let Err(e) = send_ui_msg(SyncUiMessage::FundingSettlementsStateUpdate(
                    "Not evaluated.".to_string(),
                ))
                .await
                {
                    status_manager.set_crashed(e);
                    return;
                }
            }

            let mut sync_rx = sync_reader.update_receiver();

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

                        if let Err(e) = send_ui_msg(SyncUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(e);
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

                        status_manager.set_crashed(TuiError::SyncRecv(e));

                        return;
                    }
                }
            }
        })
        .into()
    }

    /// Couples a [`SyncEngine`] to this TUI instance.
    ///
    /// This method starts the sync engine and begins listening for sync updates. It can only be
    /// called once per TUI instance.
    ///
    /// Returns an error if a sync engine has already been coupled.
    pub fn couple(&self, engine: SyncEngine) -> Result<()> {
        if self.sync_controller.initialized() {
            return Err(TuiError::SyncEngineAlreadyCoupled);
        }

        let sync_update_listener_handle = Self::spawn_sync_update_listener(
            self.status_manager.clone(),
            engine.reader(),
            self.ui_tx.clone(),
        );

        let sync_controller = engine.start();

        self.sync_controller
            .set(sync_controller)
            .map_err(|_| TuiError::SyncEngineAlreadyCoupled)?;

        self.sync_update_listener_handle
            .set(sync_update_listener_handle)
            .map_err(|_| TuiError::SyncEngineAlreadyCoupled)?;

        Ok(())
    }

    /// Performs a graceful shutdown of the sync TUI.
    ///
    /// This method shuts down the coupled sync engine and stops the UI task. If shutdown does not
    /// complete within the configured timeout, the task is aborted.
    ///
    /// Returns an error if the TUI is not running or if shutdown fails.
    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let sync_controller = self.sync_controller.get().cloned();

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(SyncUiMessage::ShutdownCompleted),
            sync_controller,
        )
        .await
    }

    /// Waits until the TUI has stopped and returns the final stopped status.
    ///
    /// This method blocks until the TUI reaches a stopped state, either through graceful shutdown
    /// or a crash.
    ///
    /// The terminal is automatically restored before this method returns.
    pub async fn until_stopped(&self) -> Arc<TuiStatusStopped> {
        loop {
            if let TuiStatus::Stopped(status_stopped) = self.status() {
                let _ = self.tui_terminal.restore();
                return status_stopped;
            }

            time::sleep(self.event_check_interval).await;
        }
    }

    /// Logs a message to the TUI.
    ///
    /// Returns an error if the TUI is not running or if sending the log entry fails.
    pub async fn log(&self, text: String) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(SyncUiMessage::LogEntry(text))
            .await
            .map_err(|e| TuiError::SyncTuiSendFailed(Box::new(e)))
    }

    /// Returns this TUI as a [`TuiLogger`] trait object.
    ///
    /// This is useful for passing the TUI to components that accept a generic logger.
    pub fn as_logger(self: &Arc<Self>) -> Arc<dyn TuiLogger> {
        self.clone()
    }
}

#[async_trait]
impl TuiLogger for SyncTui {
    async fn log(&self, log_entry: String) -> Result<()> {
        self.log(log_entry).await
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
