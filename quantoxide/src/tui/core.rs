use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyModifiers};
use tokio::{
    sync::{
        OnceCell,
        mpsc::{self, error::SendError},
    },
    task, time,
};

use crate::util::AbortOnDropHandle;

use super::{
    error::{Result, TuiError},
    status::{TuiStatus, TuiStatusManager},
    terminal::TuiTerminal,
    view::{TuiLogManager, TuiView},
};

pub(super) fn open_log_file(log_file_path: Option<&str>) -> Result<Option<File>> {
    log_file_path
        .map(|log_file_path| {
            if let Some(parent) = Path::new(log_file_path).parent() {
                fs::create_dir_all(parent).map_err(TuiError::LogFileOpen)?;
            }

            OpenOptions::new()
                .read(true)
                .append(true)
                .create(true)
                .open(log_file_path)
                .map_err(TuiError::LogFileOpen)
        })
        .transpose()
}

async fn run_ui<TView, TMessage>(
    event_check_interval: Duration,
    tui_view: Arc<TView>,
    tui_terminal: Arc<TuiTerminal>,
    mut ui_rx: mpsc::Receiver<TMessage>,
    shutdown_tx: mpsc::Sender<()>,
) -> Result<()>
where
    TView: TuiView<UiMessage = TMessage>,
    TMessage: Send + 'static,
{
    loop {
        task::yield_now().await;
        tui_terminal.draw(tui_view.as_ref())?;

        if let Ok(message) = ui_rx.try_recv() {
            let is_shutdown_completed = tui_view.handle_ui_message(message)?;
            if is_shutdown_completed {
                return Ok(());
            }
        }

        if event::poll(event_check_interval).map_err(TuiError::TerminalEventRead)? {
            if let Event::Key(key) = event::read().map_err(TuiError::TerminalEventRead)? {
                match key.code {
                    KeyCode::Char('c') | KeyCode::Char('C')
                        if key.modifiers.contains(KeyModifiers::CONTROL) =>
                    {
                        tui_view.add_log_entry("'Ctrl+C' pressed. Shutting down.".to_string())?;

                        shutdown_tx
                            .send(())
                            .await
                            .map_err(TuiError::SendShutdownFailed)?;

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
        tui_terminal.draw(tui_view.as_ref())?;
        time::sleep(event_check_interval).await;

        if let Ok(message) = ui_rx.try_recv() {
            let is_shutdown_completed = tui_view.handle_ui_message(message)?;
            if is_shutdown_completed {
                return Ok(());
            }
        }
    }
}

pub(super) fn spawn_ui_task<TView, TMessage>(
    event_check_interval: Duration,
    tui_view: Arc<TView>,
    status_manager: Arc<TuiStatusManager<TView>>,
    tui_terminal: Arc<TuiTerminal>,
    ui_rx: mpsc::Receiver<TMessage>,
    shutdown_tx: mpsc::Sender<()>,
) -> Arc<Mutex<Option<AbortOnDropHandle<()>>>>
where
    TView: TuiView<UiMessage = TMessage>,
    TMessage: Send + 'static,
{
    Arc::new(Mutex::new(Some(
        tokio::spawn(async move {
            if let Err(e) = run_ui(
                event_check_interval,
                tui_view,
                tui_terminal,
                ui_rx,
                shutdown_tx,
            )
            .await
            {
                status_manager.set_crashed(e);
            }
        })
        .into(),
    )))
}

#[async_trait]
pub(crate) trait TuiControllerShutdown: Sync + Send + 'static {
    async fn tui_shutdown(&self) -> Result<()>;
}

pub(super) async fn shutdown_inner<TView, TMessage, Fut, F>(
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<TView>>,
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    send_completed_signal: F,
    live_controller: Option<Arc<dyn TuiControllerShutdown>>,
) -> Result<()>
where
    TView: TuiLogManager,
    TMessage: Send + 'static,
    Fut: Future<Output = std::result::Result<(), SendError<TMessage>>>,
    F: FnOnce() -> Fut,
{
    let Some(mut handle) = ui_task_handle
        .lock()
        .expect("`ui_task_handle` mutex can't be poisoned")
        .take()
    else {
        return Err(TuiError::TuiAlreadyShutdown);
    };

    if handle.is_finished() {
        // Edge case. UI task crashed just after the shutdown signal
        // was sent, or just after the `LiveTui::shutdown` guard. It can be
        // assumed that the error state is available in `LiveTuiStatus`.

        let status_not_running = match status_manager.status() {
            // "Should Never Happen" case
            TuiStatus::Running => status_manager
                .set_crashed(TuiError::TuiCrashedWithoutStatusUpdate)
                .into(),
            status_not_running => status_not_running,
        };

        return Err(TuiError::TuiNotRunning(status_not_running));
    }

    status_manager.set_shutdown_initiated();

    let shutdown_procedure = async move || -> Result<()> {
        let shutdown_res = match live_controller {
            Some(controller) => controller.tui_shutdown().await,
            None => Ok(()),
        };

        let ui_message_res = send_completed_signal().await.map_err(|e| {
            handle.abort();
            TuiError::SendShutdownCompletedFailed(e.to_string())
        });

        shutdown_res.and(ui_message_res)?;

        tokio::select! {
            join_res = &mut handle => {
                join_res.map_err(TuiError::TaskJoin)?;
                Ok(())
            }
            _ = time::sleep(shutdown_timeout) => {
                handle.abort();
                Err(TuiError::ShutdownTimeout)
            }
        }
    };

    if let Err(e) = shutdown_procedure().await {
        let status_stopped = status_manager.set_crashed(e);
        Err(TuiError::ShutdownFailed(status_stopped.to_string()))
    } else {
        status_manager.set_shutdown();
        Ok(())
    }
}

pub(super) fn spawn_shutdown_signal_listener<TView, TMessage, Fut, F>(
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<TView>>,
    mut shutdown_rx: mpsc::Receiver<()>,
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    send_completed_signal: F,
    sync_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
) -> AbortOnDropHandle<()>
where
    TView: TuiLogManager,
    TMessage: Send + 'static,
    Fut: Future<Output = std::result::Result<(), SendError<TMessage>>> + Send,
    F: FnOnce() -> Fut + Send + 'static,
{
    tokio::spawn(async move {
        // If `shutdown_tx` is dropped, UI task is finished
        if let Some(_) = shutdown_rx.recv().await {
            let sync_controller = sync_controller.get().map(|inner_ref| inner_ref.clone());

            // Error handling via `TuiStatusManager`
            let _ = shutdown_inner(
                shutdown_timeout,
                status_manager,
                ui_task_handle,
                send_completed_signal,
                sync_controller,
            )
            .await;
        }
    })
    .into()
}

#[async_trait]
pub trait TuiLogger: Send + Sync {
    async fn log(&self, log_entry: String) -> Result<()>;
}
