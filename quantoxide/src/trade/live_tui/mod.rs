use std::{
    fs::{self, OpenOptions},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode};
use tokio::{
    sync::{OnceCell, mpsc},
    task, time,
};

use crate::util::AbortOnDropHandle;

use super::live_engine::{LiveController, LiveEngine, LiveReceiver, LiveUpdate};

mod error;
mod status;
mod terminal;
mod view;

use error::Result;
use status::LiveTuiStatusManager;
use terminal::LiveTuiTerminal;
use view::{LiveTuiLogger, LiveTuiView};

pub use error::LiveTuiError;
pub use status::{LiveTuiStatus, LiveTuiStatusStopped};

#[derive(Clone, Debug)]
pub struct LiveTuiConfig {
    event_check_interval: Duration,
    max_tui_log_len: usize,
    shutdown_timeout: Duration,
}

impl Default for LiveTuiConfig {
    fn default() -> Self {
        Self {
            event_check_interval: Duration::from_millis(50),
            max_tui_log_len: 10_000,
            shutdown_timeout: Duration::from_secs(6),
        }
    }
}

impl LiveTuiConfig {
    pub fn event_check_interval(&self) -> Duration {
        self.event_check_interval
    }

    pub fn max_tui_log_len(&self) -> usize {
        self.max_tui_log_len
    }

    pub fn shutdown_timeout(&self) -> Duration {
        self.shutdown_timeout
    }

    pub fn set_event_check_interval(mut self, millis: u64) -> Self {
        self.event_check_interval = Duration::from_millis(millis);
        self
    }

    pub fn set_max_tui_log_len(mut self, len: usize) -> Self {
        self.max_tui_log_len = len;
        self
    }

    pub fn set_shutdown_timeout(mut self, secs: u64) -> Self {
        self.shutdown_timeout = Duration::from_secs(secs);
        self
    }
}

#[derive(Debug)]
enum UiMessage {
    LogEntry(String),
    SummaryUpdate(String),
    TradesUpdate(String),
    ShutdownCompleted,
}

pub struct LiveTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<LiveTuiStatusManager>,
    // Retain ownership to ensure `LiveTuiTerminal` destructor is executed when
    // `LiveTui` is dropped.
    _live_tui_terminal: Arc<LiveTuiTerminal>,
    ui_tx: mpsc::Sender<UiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `LiveTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    live_controller: Arc<OnceCell<Arc<LiveController>>>,
    live_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl LiveTui {
    async fn run_ui(
        event_check_interval: Duration,
        tui_view: Arc<LiveTuiView>,
        terminal: Arc<LiveTuiTerminal>,
        mut ui_rx: mpsc::Receiver<UiMessage>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Result<()> {
        let handle_ui_message = |msg: UiMessage, content: &LiveTuiView| -> Result<bool> {
            match msg {
                UiMessage::LogEntry(entry) => {
                    content.add_log_entry(entry)?;
                    Ok(false)
                }
                UiMessage::SummaryUpdate(summary) => {
                    content.update_summary(summary);
                    Ok(false)
                }
                UiMessage::TradesUpdate(trades_table) => {
                    content.update_trades(trades_table);
                    Ok(false)
                }
                UiMessage::ShutdownCompleted => Ok(true),
            }
        };

        loop {
            task::yield_now().await;
            terminal.draw(&tui_view)?;

            if let Ok(message) = ui_rx.try_recv() {
                let is_shutdown_completed = handle_ui_message(message, &tui_view)?;
                if is_shutdown_completed {
                    return Ok(());
                }
            }

            if event::poll(event_check_interval)
                .map_err(|e| LiveTuiError::Generic(e.to_string()))?
            {
                if let Event::Key(key) =
                    event::read().map_err(|e| LiveTuiError::Generic(e.to_string()))?
                {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            tui_view.add_log_entry("'q' pressed".to_string())?;

                            shutdown_tx.send(()).await.map_err(|e| {
                                LiveTuiError::Generic(format!(
                                    "Failed to send TUI shutdown signal {:?}",
                                    e
                                ))
                            })?;

                            break;
                        }
                        KeyCode::Up => tui_view.scroll_up(),
                        KeyCode::Down => tui_view.scroll_down(),
                        KeyCode::Left => tui_view.scroll_left(),
                        KeyCode::Right => tui_view.scroll_right(),
                        KeyCode::Char('t') | KeyCode::Char('T') => tui_view.reset_scroll(),
                        KeyCode::Char('b') | KeyCode::Char('B') => tui_view.scroll_to_bottom(),
                        KeyCode::Tab => tui_view.switch_pane(),
                        _ => {}
                    }
                }
            }
        }

        loop {
            terminal.draw(&tui_view)?;
            time::sleep(event_check_interval).await;

            if let Ok(message) = ui_rx.try_recv() {
                let is_shutdown_completed = handle_ui_message(message, &tui_view)?;
                if is_shutdown_completed {
                    return Ok(());
                }
            }
        }
    }

    fn spawn_ui_task(
        event_check_interval: Duration,
        tui_view: Arc<LiveTuiView>,
        status_manager: Arc<LiveTuiStatusManager>,
        terminal: Arc<LiveTuiTerminal>,
        ui_rx: mpsc::Receiver<UiMessage>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Arc<Mutex<Option<AbortOnDropHandle<()>>>> {
        Arc::new(Mutex::new(Some(
            tokio::spawn(async move {
                if let Err(e) =
                    Self::run_ui(event_check_interval, tui_view, terminal, ui_rx, shutdown_tx).await
                {
                    status_manager.set_crashed(e);
                }
            })
            .into(),
        )))
    }

    async fn shutdown_inner(
        shutdown_timeout: Duration,
        status_manager: Arc<LiveTuiStatusManager>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
        live_controller: Option<Arc<LiveController>>,
    ) -> Result<()> {
        let Some(mut handle) = ui_task_handle
            .lock()
            .expect("`ui_task_handle` mutex can't be poisoned")
            .take()
        else {
            return Err(LiveTuiError::Generic(
                "Live TUI shutdown can only be run once".to_string(),
            ));
        };

        if handle.is_finished() {
            // Edge case. UI task crashed just after the shutdown signal
            // was sent, or just after the `LiveTui::shutdown` guard. It can be
            // assumed that the error state is available in `LiveTuiStatus`.

            let status_not_running = match status_manager.status() {
                // "Should Never Happen" case
                LiveTuiStatus::Running => status_manager
                    .set_crashed(LiveTuiError::Generic(
                        "UI task crashed without corresponding status update".to_string(),
                    ))
                    .into(),
                status_not_running => status_not_running,
            };

            return Err(LiveTuiError::Generic(format!(
                "Tried to shutdown TUI that is not running: {:?}",
                status_not_running
            )));
        }

        status_manager.set_shutdown_initiated();

        let shutdown_procedure = async move || -> Result<()> {
            let shutdown_res = match live_controller {
                Some(controller) => controller
                    .shutdown()
                    .await
                    .map_err(|e| LiveTuiError::Generic(e.to_string())),
                None => Ok(()),
            };

            let ui_message_res = ui_tx.send(UiMessage::ShutdownCompleted).await.map_err(|e| {
                handle.abort();
                LiveTuiError::Generic(format!("Failed to send shutdown confirmation, {e}"))
            });

            shutdown_res.and(ui_message_res)?;

            tokio::select! {
                join_res = &mut handle => {
                    join_res.map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    Ok(())
                }
                _ = time::sleep(shutdown_timeout) => {
                    handle.abort();
                    Err(LiveTuiError::Generic("Shutdown timeout".to_string()))
                }
            }
        };

        if let Err(e) = shutdown_procedure().await {
            let status_stopped = status_manager.set_crashed(e);
            Err(LiveTuiError::Generic(format!(
                "Shutdown failed: {:?}",
                status_stopped
            )))
        } else {
            status_manager.set_shutdown();
            Ok(())
        }
    }

    fn spawn_shutdown_signal_listener(
        shutdown_timeout: Duration,
        status_manager: Arc<LiveTuiStatusManager>,
        mut shutdown_rx: mpsc::Receiver<()>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
        live_controller: Arc<OnceCell<Arc<LiveController>>>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            // If `shutdown_tx` is dropped, UI task is finished

            if let Some(_) = shutdown_rx.recv().await {
                let live_controller = live_controller.get().map(|inner_ref| inner_ref.clone());

                // Error handling via `LiveTuiStatus`
                let _ = Self::shutdown_inner(
                    shutdown_timeout,
                    status_manager,
                    ui_task_handle,
                    ui_tx.clone(),
                    live_controller,
                )
                .await;
            }
        })
        .into()
    }

    pub async fn launch(config: LiveTuiConfig, log_file_path: Option<&str>) -> Result<Self> {
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

        let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let live_tui_terminal = LiveTuiTerminal::new()?;

        let tui_view = LiveTuiView::new(config.max_tui_log_len, log_file);

        let status_manager = LiveTuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = Self::spawn_ui_task(
            config.event_check_interval,
            tui_view,
            status_manager.clone(),
            live_tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let live_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = Self::spawn_shutdown_signal_listener(
            config.shutdown_timeout,
            status_manager.clone(),
            shutdown_rx,
            ui_task_handle.clone(),
            ui_tx.clone(),
            live_controller.clone(),
        );

        Ok(Self {
            event_check_interval: config.event_check_interval,
            shutdown_timeout: config.shutdown_timeout,
            status_manager,
            _live_tui_terminal: live_tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            live_controller,
            live_update_listener_handle: OnceCell::new(),
        })
    }

    pub fn status(&self) -> LiveTuiStatus {
        self.status_manager.status()
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(UiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| LiveTuiError::Generic("TUI is not running".to_string()))
    }

    fn spawn_live_update_listener(
        status_manager: Arc<LiveTuiStatusManager>,
        mut live_rx: LiveReceiver,
        ui_tx: mpsc::Sender<UiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let handle_live_update = async |live_update: LiveUpdate| -> Result<()> {
                match live_update {
                    LiveUpdate::State(live_state) => {
                        ui_tx
                            .send(UiMessage::LogEntry(format!("{:?}", live_state)))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Signal(signal) => {
                        ui_tx
                            .send(UiMessage::LogEntry(format!("{}", signal.to_string())))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Order(order) => {
                        ui_tx
                            .send(UiMessage::LogEntry(format!("Order: {:?}", order)))
                            .await
                            .map_err(|e| LiveTuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::TradingState(trading_state) => {
                        ui_tx
                            .send(UiMessage::SummaryUpdate(format!(
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
                            .send(UiMessage::TradesUpdate(tables))
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

        Self::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            self.ui_tx.clone(),
            live_controller,
        )
        .await
    }

    pub async fn until_stopped(self) -> Arc<LiveTuiStatusStopped> {
        loop {
            if let LiveTuiStatus::Stopped(status_stopped) = self.status() {
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
