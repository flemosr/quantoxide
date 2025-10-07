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
    trade::{
        core::ClosedTradeHistory,
        live::{LiveEngine, LiveReceiver, LiveUpdate},
    },
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
    status_manager: Arc<TuiStatusManager<LiveTuiView>>,
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
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Arc<Self>> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<LiveUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(10);

        let tui_terminal = TuiTerminal::new()?;

        let tui_view = LiveTuiView::new(config.max_tui_log_len(), log_file);

        let status_manager = TuiStatusManager::new_running(tui_view.clone());

        let ui_task_handle = core::spawn_ui_task(
            config.event_check_interval(),
            tui_view,
            status_manager.clone(),
            tui_terminal.clone(),
            ui_rx,
            shutdown_tx,
        );

        let live_controller = Arc::new(OnceCell::new());

        let _shutdown_listener_handle = core::spawn_shutdown_signal_listener(
            config.shutdown_timeout(),
            status_manager.clone(),
            shutdown_rx,
            ui_task_handle.clone(),
            {
                let ui_tx = ui_tx.clone();
                || async move { ui_tx.send(LiveUiMessage::ShutdownCompleted).await }
            },
            live_controller.clone(),
        );

        Ok(Arc::new(Self {
            event_check_interval: config.event_check_interval(),
            shutdown_timeout: config.shutdown_timeout(),
            status_manager,
            _tui_terminal: tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            live_controller,
            live_update_listener_handle: OnceCell::new(),
        }))
    }

    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    fn spawn_live_update_listener(
        status_manager: Arc<TuiStatusManager<LiveTuiView>>,
        mut live_rx: LiveReceiver,
        ui_tx: mpsc::Sender<LiveUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let send_trades_update =
                async |running_trades_table: &str, closed_trades_table: &str| -> Result<()> {
                    let tables = format!("\nRunning Trades\n\n{running_trades_table}\n\n\nClosed Trades\n\n{closed_trades_table}");

                    ui_tx
                        .send(LiveUiMessage::TradesUpdate(tables))
                        .await
                        .map_err(|e| TuiError::Generic(e.to_string()))
                };

            let mut running_trades_table = "No running trades.".to_string();
            let mut closed_trade_history = ClosedTradeHistory::new();
            let mut closed_trades_table = closed_trade_history.to_table();

            let mut handle_live_update = async |live_update: LiveUpdate| -> Result<()> {
                match live_update {
                    LiveUpdate::Status(live_status) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(format!("Live status: {live_status}")))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Signal(signal) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(signal.to_string()))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::Order(order) => {
                        ui_tx
                            .send(LiveUiMessage::LogEntry(format!("Order: {order}")))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;
                    }
                    LiveUpdate::TradingState(trading_state) => {
                        ui_tx
                            .send(LiveUiMessage::SummaryUpdate(format!(
                                "\n{}",
                                trading_state.summary()
                            )))
                            .await
                            .map_err(|e| TuiError::Generic(e.to_string()))?;

                        running_trades_table = trading_state.running_trades_table();

                        send_trades_update(&running_trades_table, &closed_trades_table).await?;
                    }
                    LiveUpdate::ClosedTrade(closed_trade) => {
                        closed_trade_history.add(closed_trade).map_err(|e| TuiError::Generic(e.to_string()))?;

                        closed_trades_table = closed_trade_history.to_table();

                        send_trades_update(&running_trades_table, &closed_trades_table).await?;
                    }
                }

                Ok(())
            };

            loop {
                match live_rx.recv().await {
                    Ok(live_update) => {
                        if let Err(e) = handle_live_update(live_update).await {
                            status_manager.set_crashed(e);
                            return;
                        }
                    }
                    Err(RecvError::Lagged(skipped)) => {
                        let log_msg = format!("Live updates lagged by {skipped} messages");
                        if let Err(e) = ui_tx.send(LiveUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(TuiError::Generic(e.to_string()));
                            return;
                        }

                        // Keep trying to receive
                    }
                    Err(e) => {
                        // `live_rx` is expected to be dropped during shutdown

                        let status = status_manager.status();
                        if status.is_shutdown_initiated() || status.is_shutdown() {
                            return;
                        }

                        status_manager.set_crashed(TuiError::Generic(format!(
                            "`live_rx` returned err {:?}",
                            e
                        )));

                        return;
                    }
                }
            }
        })
        .into()
    }

    pub async fn couple(&self, engine: LiveEngine) -> Result<()> {
        if self.live_controller.initialized() {
            return Err(TuiError::Generic(
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
            .map_err(|e| TuiError::Generic(e.to_string()))?;

        self.live_controller
            .set(live_controller)
            .map_err(|_| TuiError::Generic("Failed to set `live_controller`".to_string()))?;

        self.live_update_listener_handle
            .set(live_update_listener_handle)
            .map_err(|_| {
                TuiError::Generic("Failed to set `live_update_listener_handle`".to_string())
            })?;

        Ok(())
    }

    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let live_controller = self
            .live_controller
            .get()
            .map(|inner_ref| inner_ref.clone());

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(LiveUiMessage::ShutdownCompleted),
            live_controller,
        )
        .await
    }

    pub async fn until_stopped(&self) -> Arc<TuiStatusStopped> {
        loop {
            if let TuiStatus::Stopped(status_stopped) = self.status() {
                return status_stopped;
            }

            time::sleep(self.event_check_interval).await;
        }
    }
}

#[async_trait]
impl TuiLogger for LiveTui {
    async fn log(&self, log_entry: String) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(LiveUiMessage::LogEntry(log_entry.into()))
            .await
            .map_err(|_| TuiError::Generic("TUI is not running".to_string()))
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
