use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    sync::{OnceCell, broadcast::error::RecvError, mpsc},
    time,
};

use crate::{
    trade::backtest::{BacktestEngine, BacktestReceiver, BacktestState},
    util::AbortOnDropHandle,
};

use super::{
    config::TuiConfig,
    core::{self, TuiControllerShutdown},
    error::{Result, TuiError},
    status::{TuiStatus, TuiStatusManager, TuiStatusStopped},
    terminal::TuiTerminal,
};

mod view;

use view::BacktestTuiView;

#[derive(Debug)]
pub enum BacktestUiMessage {
    LogEntry(String),
    StateUpdate(String),
    ShutdownCompleted,
}

pub struct BacktestTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<BacktestTuiView>>,
    // Retain ownership to ensure `TuiTerminal` destructor is executed when
    // `BacktestTui` is dropped.
    _tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<BacktestUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `BacktestTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    backtest_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    backtest_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl BacktestTui {
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Self> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<BacktestUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = BacktestTuiView::new(config.max_tui_log_len(), log_file);

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
                || async move { ui_tx.send(BacktestUiMessage::ShutdownCompleted).await }
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
            backtest_controller: sync_controller,
            backtest_update_listener_handle: OnceCell::new(),
        })
    }

    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(BacktestUiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| TuiError::Generic("TUI is not running".to_string()))
    }

    fn spawn_backtest_update_listener(
        status_manager: Arc<TuiStatusManager<BacktestTuiView>>,
        mut backtest_rx: BacktestReceiver,
        ui_tx: mpsc::Sender<BacktestUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let handle_backtest_update = async |backtest_update: Arc<BacktestState>| -> Result<()> {
                let log_str = match backtest_update.as_ref() {
                    BacktestState::NotInitiated => "BacktestState::NotInitiated".to_string(),
                    BacktestState::Starting => "BacktestState::Starting".to_string(),
                    BacktestState::Running(trading_state) => {
                        ui_tx
                            .send(BacktestUiMessage::StateUpdate(trading_state.to_string()))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;

                        "BacktestState::Running".to_string()
                    }
                    BacktestState::Finished(trading_state) => {
                        ui_tx
                            .send(BacktestUiMessage::StateUpdate(trading_state.to_string()))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;

                        "BacktestState::Finished".to_string()
                    }
                    BacktestState::Failed(err) => {
                        format!("BacktestState::Failed with error {err}")
                    }
                    BacktestState::Aborted => "BacktestState::Aborted".to_string(),
                };

                ui_tx
                    .send(BacktestUiMessage::LogEntry(log_str))
                    .await
                    .map_err(|e| TuiError::Generic(e.to_string()))?;

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
                        while let Err(_) = backtest_rx.recv().await {}

                        let log_msg = format!("Backtest updates lagged by {skipped} messages");
                        if let Err(e) = ui_tx.send(BacktestUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(TuiError::Generic(e.to_string()));
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

                        status_manager.set_crashed(TuiError::Generic(format!(
                            "`backtest_rx` returned err {:?}",
                            e
                        )));

                        return;
                    }
                }
            }
        })
        .into()
    }

    pub fn couple(&self, engine: BacktestEngine) -> Result<()> {
        if self.backtest_controller.initialized() {
            return Err(TuiError::Generic(
                "`backtest_engine` was already coupled".to_string(),
            ));
        }

        let backtest_rx = engine.receiver();

        let backtest_update_listener_handle = Self::spawn_backtest_update_listener(
            self.status_manager.clone(),
            backtest_rx,
            self.ui_tx.clone(),
        );

        let backtest_controller = engine.start();

        self.backtest_controller
            .set(backtest_controller)
            .map_err(|_| TuiError::Generic("Failed to set `backtest_controller`".to_string()))?;

        self.backtest_update_listener_handle
            .set(backtest_update_listener_handle)
            .map_err(|_| {
                TuiError::Generic("Failed to set `backtest_update_listener_handle`".to_string())
            })?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let backtest_controller = self
            .backtest_controller
            .get()
            .map(|inner_ref| inner_ref.clone());

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(BacktestUiMessage::ShutdownCompleted),
            backtest_controller,
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
