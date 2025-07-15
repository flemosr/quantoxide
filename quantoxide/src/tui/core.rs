use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use crossterm::event::{self, Event, KeyCode};
use tokio::{
    sync::{
        OnceCell,
        mpsc::{self, error::SendError},
    },
    task, time,
};

use crate::{
    tui::{TuiStatus, TuiStatusManager},
    util::AbortOnDropHandle,
};

use super::{Result, TuiError, TuiTerminal, TuiView};

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
        tui_terminal.draw(tui_view.clone())?;

        if let Ok(message) = ui_rx.try_recv() {
            let is_shutdown_completed = tui_view.handle_ui_message(message)?;
            if is_shutdown_completed {
                return Ok(());
            }
        }

        if event::poll(event_check_interval).map_err(|e| TuiError::Generic(e.to_string()))? {
            if let Event::Key(key) = event::read().map_err(|e| TuiError::Generic(e.to_string()))? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        tui_view.add_log_entry("'q' pressed".to_string())?;

                        shutdown_tx.send(()).await.map_err(|e| {
                            TuiError::Generic(format!("Failed to send TUI shutdown signal {:?}", e))
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
        tui_terminal.draw(tui_view.clone())?;
        time::sleep(event_check_interval).await;

        if let Ok(message) = ui_rx.try_recv() {
            let is_shutdown_completed = tui_view.handle_ui_message(message)?;
            if is_shutdown_completed {
                return Ok(());
            }
        }
    }
}

pub fn spawn_ui_task<TView, TMessage>(
    event_check_interval: Duration,
    tui_view: Arc<TView>,
    status_manager: Arc<TuiStatusManager>,
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
pub trait TuiControllerShutdown: Sync + Send + 'static {
    async fn tui_shutdown(&self) -> Result<()>;
}

pub async fn shutdown_inner<TMessage, Fut, F>(
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager>,
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    send_completed_signal: F,
    live_controller: Option<Arc<dyn TuiControllerShutdown>>,
) -> Result<()>
where
    TMessage: Send + 'static,
    Fut: Future<Output = std::result::Result<(), SendError<TMessage>>>,
    F: FnOnce() -> Fut,
{
    let Some(mut handle) = ui_task_handle
        .lock()
        .expect("`ui_task_handle` mutex can't be poisoned")
        .take()
    else {
        return Err(TuiError::Generic(
            "Live TUI shutdown can only be run once".to_string(),
        ));
    };

    if handle.is_finished() {
        // Edge case. UI task crashed just after the shutdown signal
        // was sent, or just after the `LiveTui::shutdown` guard. It can be
        // assumed that the error state is available in `LiveTuiStatus`.

        let status_not_running = match status_manager.status() {
            // "Should Never Happen" case
            TuiStatus::Running => status_manager
                .set_crashed(TuiError::Generic(
                    "UI task crashed without corresponding status update".to_string(),
                ))
                .into(),
            status_not_running => status_not_running,
        };

        return Err(TuiError::Generic(format!(
            "Tried to shutdown TUI that is not running: {:?}",
            status_not_running
        )));
    }

    status_manager.set_shutdown_initiated();

    let shutdown_procedure = async move || -> Result<()> {
        let shutdown_res = match live_controller {
            Some(controller) => controller.tui_shutdown().await,
            None => Ok(()),
        };

        let ui_message_res = send_completed_signal().await.map_err(|e| {
            handle.abort();
            TuiError::Generic(format!("Failed to send shutdown completed signal, {e}"))
        });

        shutdown_res.and(ui_message_res)?;

        tokio::select! {
            join_res = &mut handle => {
                join_res.map_err(|e| TuiError::Generic(e.to_string()))?;
                Ok(())
            }
            _ = time::sleep(shutdown_timeout) => {
                handle.abort();
                Err(TuiError::Generic("Shutdown timeout".to_string()))
            }
        }
    };

    if let Err(e) = shutdown_procedure().await {
        let status_stopped = status_manager.set_crashed(e);
        Err(TuiError::Generic(format!(
            "Shutdown failed: {:?}",
            status_stopped
        )))
    } else {
        status_manager.set_shutdown();
        Ok(())
    }
}

pub fn spawn_shutdown_signal_listener<TMessage, Fut, F>(
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager>,
    mut shutdown_rx: mpsc::Receiver<()>,
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    send_completed_signal: F,
    sync_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
) -> AbortOnDropHandle<()>
where
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
