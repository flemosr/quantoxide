use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crossterm::event::{self, Event, KeyCode};
use tokio::{sync::mpsc, task, time};

use crate::{tui::TuiStatusManager, util::AbortOnDropHandle};

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
