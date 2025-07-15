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

use super::{SyncController, SyncEngine, SyncReceiver, SyncState, SyncStateNotSynced, SyncUpdate};

mod error;
mod status;
mod terminal;
mod view;

use error::Result;
use status::SyncTuiStatusManager;
use terminal::SyncTuiTerminal;
use view::{SyncTuiLogger, SyncTuiView};

pub use error::SyncTuiError;
pub use status::{SyncTuiStatus, SyncTuiStatusStopped};

#[derive(Clone, Debug)]
pub struct SyncTuiConfig {
    event_check_interval: Duration,
    max_tui_log_len: usize,
    shutdown_timeout: Duration,
}

impl Default for SyncTuiConfig {
    fn default() -> Self {
        Self {
            event_check_interval: Duration::from_millis(50),
            max_tui_log_len: 10_000,
            shutdown_timeout: Duration::from_secs(6),
        }
    }
}

impl SyncTuiConfig {
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
    StateUpdate(String),
    ShutdownCompleted,
}

pub struct SyncTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<SyncTuiStatusManager>,
    // Retain ownership to ensure `SyncTuiTerminal` destructor is executed when
    // `SyncTui` is dropped.
    _sync_tui_terminal: Arc<SyncTuiTerminal>,
    ui_tx: mpsc::Sender<UiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `SyncTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    sync_controller: Arc<OnceCell<Arc<SyncController>>>,
    sync_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl SyncTui {
    async fn run_ui(
        event_check_interval: Duration,
        tui_view: Arc<SyncTuiView>,
        terminal: Arc<SyncTuiTerminal>,
        mut ui_rx: mpsc::Receiver<UiMessage>,
        shutdown_tx: mpsc::Sender<()>,
    ) -> Result<()> {
        let handle_ui_message = |msg: UiMessage, content: &SyncTuiView| -> Result<bool> {
            match msg {
                UiMessage::LogEntry(entry) => {
                    content.add_log_entry(entry)?;
                    Ok(false)
                }
                UiMessage::StateUpdate(state) => {
                    content.update_sync_state(state);
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
                .map_err(|e| SyncTuiError::Generic(e.to_string()))?
            {
                if let Event::Key(key) =
                    event::read().map_err(|e| SyncTuiError::Generic(e.to_string()))?
                {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            tui_view.add_log_entry("'q' pressed".to_string())?;

                            shutdown_tx.send(()).await.map_err(|e| {
                                SyncTuiError::Generic(format!(
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
        tui_view: Arc<SyncTuiView>,
        status_manager: Arc<SyncTuiStatusManager>,
        terminal: Arc<SyncTuiTerminal>,
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
        status_manager: Arc<SyncTuiStatusManager>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
        sync_controller: Option<Arc<SyncController>>,
    ) -> Result<()> {
        let Some(mut handle) = ui_task_handle
            .lock()
            .expect("`ui_task_handle` mutex can't be poisoned")
            .take()
        else {
            return Err(SyncTuiError::Generic(
                "Sync TUI shutdown can only be run once".to_string(),
            ));
        };

        if handle.is_finished() {
            // Edge case. UI task crashed just after the shutdown signal
            // was sent, or just after the `SyncTui::shutdown` guard. It can be
            // assumed that the error state is available in `SyncTuiStatus`.

            let status_not_running = match status_manager.status() {
                // "Should Never Happen" case
                SyncTuiStatus::Running => status_manager
                    .set_crashed(SyncTuiError::Generic(
                        "UI task crashed without corresponding status update".to_string(),
                    ))
                    .into(),
                status_not_running => status_not_running,
            };

            return Err(SyncTuiError::Generic(format!(
                "Tried to shutdown TUI that is not running: {:?}",
                status_not_running
            )));
        }

        status_manager.set_shutdown_initiated();

        let shutdown_procedure = async move || -> Result<()> {
            let shutdown_res = match sync_controller {
                Some(controller) => controller
                    .shutdown()
                    .await
                    .map_err(|e| SyncTuiError::Generic(e.to_string())),
                None => Ok(()),
            };

            let ui_message_res = ui_tx.send(UiMessage::ShutdownCompleted).await.map_err(|e| {
                handle.abort();
                SyncTuiError::Generic(format!("Failed to send shutdown confirmation, {e}"))
            });

            shutdown_res.and(ui_message_res)?;

            tokio::select! {
                join_res = &mut handle => {
                    join_res.map_err(|e| SyncTuiError::Generic(e.to_string()))?;
                    Ok(())
                }
                _ = time::sleep(shutdown_timeout) => {
                    handle.abort();
                    Err(SyncTuiError::Generic("Shutdown timeout".to_string()))
                }
            }
        };

        if let Err(e) = shutdown_procedure().await {
            let status_stopped = status_manager.set_crashed(e);
            Err(SyncTuiError::Generic(format!(
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
        status_manager: Arc<SyncTuiStatusManager>,
        mut shutdown_rx: mpsc::Receiver<()>,
        ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
        ui_tx: mpsc::Sender<UiMessage>,
        sync_controller: Arc<OnceCell<Arc<SyncController>>>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            // If `shutdown_tx` is dropped, UI task is finished

            if let Some(_) = shutdown_rx.recv().await {
                let sync_controller = sync_controller.get().map(|inner_ref| inner_ref.clone());

                // Error handling via `SyncTuiStatus`
                let _ = Self::shutdown_inner(
                    shutdown_timeout,
                    status_manager,
                    ui_task_handle,
                    ui_tx.clone(),
                    sync_controller,
                )
                .await;
            }
        })
        .into()
    }

    pub async fn launch(config: SyncTuiConfig, log_file_path: Option<&str>) -> Result<Self> {
        let log_file = log_file_path
            .map(|log_file_path| {
                if let Some(parent) = Path::new(log_file_path).parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        SyncTuiError::Generic(format!(
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
                        SyncTuiError::Generic(format!(
                            "couldn't open the log file. {}",
                            e.to_string()
                        ))
                    })
            })
            .transpose()?;

        let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let sync_tui_terminal = SyncTuiTerminal::new()?;

        let tui_view = SyncTuiView::new(config.max_tui_log_len, log_file);

        let status_manager = SyncTuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = Self::spawn_ui_task(
            config.event_check_interval,
            tui_view,
            status_manager.clone(),
            sync_tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let sync_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = Self::spawn_shutdown_signal_listener(
            config.shutdown_timeout,
            status_manager.clone(),
            shutdown_rx,
            ui_task_handle.clone(),
            ui_tx.clone(),
            sync_controller.clone(),
        );

        Ok(Self {
            event_check_interval: config.event_check_interval,
            shutdown_timeout: config.shutdown_timeout,
            status_manager,
            _sync_tui_terminal: sync_tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            sync_controller,
            sync_update_listener_handle: OnceCell::new(),
        })
    }

    pub fn status(&self) -> SyncTuiStatus {
        self.status_manager.status()
    }

    pub async fn log(&self, log_entry: impl Into<String>) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(UiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| SyncTuiError::Generic("TUI is not running".to_string()))
    }

    fn spawn_sync_update_listener(
        status_manager: Arc<SyncTuiStatusManager>,
        mut sync_rx: SyncReceiver,
        ui_tx: mpsc::Sender<UiMessage>,
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
                                            .send(UiMessage::StateUpdate(
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
                            .send(UiMessage::LogEntry(log_str))
                            .await
                            .map_err(|e| SyncTuiError::Generic(e.to_string()))?;
                    }
                    SyncUpdate::PriceTick(tick) => {
                        ui_tx
                            .send(UiMessage::LogEntry(format!("Price tick: {:?}", tick)))
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

        Self::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            self.ui_tx.clone(),
            sync_controller,
        )
        .await
    }

    pub async fn until_stopped(self) -> Arc<SyncTuiStatusStopped> {
        loop {
            if let SyncTuiStatus::Stopped(status_stopped) = self.status() {
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
