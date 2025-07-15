use std::{
    fs::{self, OpenOptions},
    path::Path,
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

pub use crate::tui::{TuiConfig, TuiError as LiveTuiError, TuiStatus, TuiStatusStopped};

use super::live_engine::{LiveEngine, LiveReceiver, LiveUpdate};

mod view;

use view::LiveTuiView;

#[derive(Debug)]
pub enum LiveUiMessage {
    LogEntry(String),
    SummaryUpdate(String),
    TradesUpdate(String),
    ShutdownCompleted,
}

pub struct LiveTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager>,
    // Retain ownership to ensure `TuiTerminal` destructor is executed when
    // `LiveTui` is dropped.
    _tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<LiveUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `LiveTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    live_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    live_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl LiveTui {
    fn spawn_shutdown_signal_listener(
        shutdown_timeout: Duration,
        status_manager: Arc<TuiStatusManager>,
        mut shutdown_rx: mpsc::Receiver<()>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
        ui_tx: mpsc::Sender<LiveUiMessage>,
        live_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            // If `shutdown_tx` is dropped, UI task is finished

            if let Some(_) = shutdown_rx.recv().await {
                let live_controller = live_controller.get().map(|inner_ref| inner_ref.clone());

                // Error handling via `TuiStatus`
                let _ = tui::shutdown_inner(
                    shutdown_timeout,
                    status_manager,
                    ui_task_handle,
                    || ui_tx.send(LiveUiMessage::ShutdownCompleted),
                    live_controller,
                )
                .await;
            }
        })
        .into()
    }

    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Self> {
        let log_file = log_file_path
            .map(|log_file_path| {
                if let Some(parent) = Path::new(log_file_path).parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        LiveTuiError::Generic(format!(
                            "couldn't create log_file parent {}",
                            e.to_string()
                        ))
                    })?;
                }

                OpenOptions::new()
                    .read(true)
                    .append(true)
                    .create(true)
                    .open(log_file_path)
                    .map_err(|e| {
                        LiveTuiError::Generic(format!(
                            "couldn't open the log file. {}",
                            e.to_string()
                        ))
                    })
            })
            .transpose()?;

        let (ui_tx, ui_rx) = mpsc::channel::<LiveUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = LiveTuiView::new(config.max_tui_log_len(), log_file);

        let status_manager = TuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = tui::spawn_ui_task(
            config.event_check_interval(),
            tui_view,
            status_manager.clone(),
            tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let live_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = Self::spawn_shutdown_signal_listener(
            config.shutdown_timeout(),
            status_manager.clone(),
            shutdown_rx,
            ui_task_handle.clone(),
            ui_tx.clone(),
            live_controller.clone(),
        );

        Ok(Self {
            event_check_interval: config.event_check_interval(),
            shutdown_timeout: config.shutdown_timeout(),
            status_manager,
            _tui_terminal: tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            live_controller,
            live_update_listener_handle: OnceCell::new(),
        })
    }

    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(LiveUiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| LiveTuiError::Generic("TUI is not running".to_string()))
    }

    fn spawn_live_update_listener(
        status_manager: Arc<TuiStatusManager>,
        mut live_rx: LiveReceiver,
        ui_tx: mpsc::Sender<LiveUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let handle_live_update = async |live_update: LiveUpdate| -> Result<()> {
                match live_update {
                    LiveUpdate::State(live_state) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(format!("{:?}", live_state)))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Signal(signal) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(format!("{}", signal.to_string())))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Order(order) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(format!("Order: {:?}", order)))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::TradingState(trading_state) => {
                        ui_tx
                            .send(LiveUiMessage::SummaryUpdate(format!(
                                "\n{}",
                                trading_state.summary()
                            )))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;

                        let tables = vec![
                            "\nRunning Trades\n".to_string(),
                            trading_state.running_trades_table(),
                            "\n\nClosed Trades\n".to_string(),
                            trading_state.closed_trades_table(),
                        ]
                        .join("\n");

                        ui_tx
                            .send(LiveUiMessage::TradesUpdate(tables))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                }

                Ok(())
            };

            while let Ok(live_update) = live_rx.recv().await {
                if let Err(e) = handle_live_update(live_update).await {
                    status_manager.set_crashed(e);
                    return;
                }
            }

            // `live_tx` was dropped, which is expected during shutdown

            let status = status_manager.status();
            if status.is_shutdown_initiated() || status.is_shutdown() {
                return;
            }

            status_manager.set_crashed(LiveTuiError::Generic(
                "`live_tx` was unexpectedly dropped".to_string(),
            ));
        })
        .into()
    }

    pub async fn couple(&self, engine: LiveEngine) -> Result<()> {
        if self.live_controller.initialized() {
            return Err(LiveTuiError::Generic(
                "`live_engine` was already coupled".to_string(),
            ));
        }

        let live_rx = engine.update_receiver();

        let live_update_listener_handle = Self::spawn_live_update_listener(
            self.status_manager.clone(),
            live_rx,
            self.ui_tx.clone(),
        );

        let live_controller = engine
            .start()
            .await
            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;

        self.live_controller
            .set(live_controller)
            .map_err(|_| LiveTuiError::Generic("Failed to set `live_controller`".to_string()))?;

        self.live_update_listener_handle
            .set(live_update_listener_handle)
            .map_err(|_| {
                LiveTuiError::Generic("Failed to set `live_update_listener_handle`".to_string())
            })?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let live_controller = self
            .live_controller
            .get()
            .map(|inner_ref| inner_ref.clone());

        tui::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(LiveUiMessage::ShutdownCompleted),
            live_controller,
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

impl Drop for LiveTui {
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
