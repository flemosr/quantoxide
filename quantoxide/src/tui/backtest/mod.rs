use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use chrono::Utc;
use tokio::{
    sync::{OnceCell, broadcast::error::RecvError, mpsc},
    time,
};

use crate::{
    trade::{BacktestEngine, BacktestReceiver, BacktestUpdate, TradingState},
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

use view::BacktestTuiView;

#[derive(Debug)]
pub enum BacktestUiMessage {
    LogEntry(String),
    StateUpdate(TradingState),
    ShutdownCompleted,
}

/// Terminal user interface for backtest operations.
///
/// `BacktestTui` provides a visual interface for monitoring backtest execution, including trading
/// state, balance changes, and iteration progress. It must be coupled with a [`BacktestEngine`]
/// before the backtest begins.
pub struct BacktestTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<BacktestTuiView>>,
    // Ownership ensures the `TuiTerminal` destructor is executed when `BacktestTui` is dropped
    tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<BacktestUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `BacktestTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    backtest_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    backtest_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
    tui_view: Arc<BacktestTuiView>,
}

impl BacktestTui {
    /// Launches a new backtest TUI with the specified configuration.
    ///
    /// Optionally writes TUI logs to a file if `log_file_path` is provided.
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Arc<Self>> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<BacktestUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = BacktestTuiView::new(config.max_tui_log_len(), log_file);

        let status_manager = TuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = core::spawn_ui_task(
            config.event_check_interval(),
            tui_view.clone(),
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
                || async move { ui_tx.send(BacktestUiMessage::ShutdownCompleted).await }
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
            backtest_controller: sync_controller,
            backtest_update_listener_handle: OnceCell::new(),
            tui_view,
        }))
    }

    /// Returns the current [`TuiStatus`] as a snapshot.
    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    fn spawn_backtest_update_listener(
        status_manager: Arc<TuiStatusManager<BacktestTuiView>>,
        mut backtest_rx: BacktestReceiver,
        ui_tx: mpsc::Sender<BacktestUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let backtest_start = Utc::now();

            let send_ui_msg = async |ui_msg: BacktestUiMessage| -> Result<()> {
                ui_tx
                    .send(ui_msg)
                    .await
                    .map_err(|e| TuiError::BacktestTuiSendFailed(Box::new(e)))
            };

            let handle_backtest_update = async |backtest_update: BacktestUpdate| -> Result<()> {
                match backtest_update {
                    BacktestUpdate::Status(backtest_status) => {
                        let complement = if backtest_status.is_finished() {
                            let backtest_elapsed = Utc::now().signed_duration_since(backtest_start);
                            format!(
                                "\nIterations completed. Elapsed: {}m {}s",
                                backtest_elapsed.num_minutes(),
                                backtest_elapsed.num_seconds() % 60
                            )
                        } else {
                            String::new()
                        };

                        send_ui_msg(BacktestUiMessage::LogEntry(format!(
                            "Backtest status: {backtest_status}{complement}"
                        )))
                        .await?;
                    }
                    BacktestUpdate::TradingState(trading_state) => {
                        send_ui_msg(BacktestUiMessage::StateUpdate(trading_state)).await?;
                    }
                };

                Ok(())
            };

            loop {
                match backtest_rx.recv().await {
                    Ok(backtest_update) => {
                        if let Err(e) = handle_backtest_update(backtest_update).await {
                            status_manager.set_crashed(e);
                            return;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        let log_msg = format!("Backtest updates lagged by {skipped} messages");

                        if let Err(e) = send_ui_msg(BacktestUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(e);
                            return;
                        }

                        // Keep trying to receive
                    }
                    Err(e) => {
                        // `backtest_rx` is expected to be dropped during shutdown

                        let status = status_manager.status();
                        if status.is_shutdown_initiated() || status.is_shutdown() {
                            return;
                        }

                        status_manager.set_crashed(TuiError::BacktestRecv(e));

                        return;
                    }
                }
            }
        })
        .into()
    }

    /// Couples a [`BacktestEngine`] to this TUI instance.
    ///
    /// This method initializes the backtest visualization and starts listening for backtest
    /// updates. It can only be called once per TUI instance.
    ///
    /// Returns an error if a backtest engine has already been coupled.
    pub async fn couple(&self, engine: BacktestEngine) -> Result<()> {
        if self.backtest_controller.initialized() {
            return Err(TuiError::BacktestEngineAlreadyCoupled);
        }

        self.tui_view.initialize_chart(
            engine.start_time(),
            engine.end_time(),
            engine.start_balance(),
        );

        let backtest_rx = engine.receiver();

        let log_str = format!(
            "Starting iterations from {} to {}...",
            engine.start_time().format("%Y-%m-%d"),
            engine.end_time().format("%Y-%m-%d")
        );

        self.ui_tx
            .send(BacktestUiMessage::LogEntry(log_str))
            .await
            .map_err(|e| TuiError::BacktestTuiSendFailed(Box::new(e)))?;

        let backtest_update_listener_handle = Self::spawn_backtest_update_listener(
            self.status_manager.clone(),
            backtest_rx,
            self.ui_tx.clone(),
        );

        let backtest_controller = engine.start();

        self.backtest_controller
            .set(backtest_controller)
            .map_err(|_| TuiError::BacktestEngineAlreadyCoupled)?;

        self.backtest_update_listener_handle
            .set(backtest_update_listener_handle)
            .map_err(|_| TuiError::BacktestEngineAlreadyCoupled)?;

        Ok(())
    }

    /// Performs a graceful shutdown of the backtest TUI.
    ///
    /// This method shuts down the coupled backtest engine and stops the UI task. If shutdown does
    /// not complete within the configured timeout, the task is aborted.
    ///
    /// Returns an error if the TUI is not running or if shutdown fails.
    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let backtest_controller = self.backtest_controller.get().cloned();

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(BacktestUiMessage::ShutdownCompleted),
            backtest_controller,
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
}

#[async_trait]
impl TuiLogger for BacktestTui {
    async fn log(&self, log_entry: String) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(BacktestUiMessage::LogEntry(log_entry))
            .await
            .map_err(|e| TuiError::BacktestTuiSendFailed(Box::new(e)))
    }
}

impl Drop for BacktestTui {
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
