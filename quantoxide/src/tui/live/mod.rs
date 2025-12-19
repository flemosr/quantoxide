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
    trade::{ClosedTradeHistory, LiveTradeEngine, LiveTradeReceiver, LiveTradeUpdate},
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

/// Terminal user interface for live trading operations.
///
/// `LiveTui` provides a visual interface for monitoring live trading activity, including signals,
/// orders, trading state, and position updates. It must be coupled with a [`LiveTradeEngine`]
/// before trading begins.
pub struct LiveTui {
    event_check_interval: Duration,
    shutdown_timeout: Duration,
    status_manager: Arc<TuiStatusManager<LiveTuiView>>,
    // Ownership ensures the `TuiTerminal` destructor is executed when `LiveTui` is dropped
    tui_terminal: Arc<TuiTerminal>,
    ui_tx: mpsc::Sender<LiveUiMessage>,
    // Explicitly aborted on drop, to ensure the terminal is restored before
    // `LiveTui`'s drop is completed.
    ui_task_handle: Arc<Mutex<Option<AbortOnDropHandle<()>>>>,
    _shutdown_listener_handle: AbortOnDropHandle<()>,
    live_controller: Arc<OnceCell<Arc<dyn TuiControllerShutdown>>>,
    live_update_listener_handle: OnceCell<AbortOnDropHandle<()>>,
}

impl LiveTui {
    /// Launches a new live trading TUI with the specified configuration.
    ///
    /// Optionally writes TUI logs to a file if `log_file_path` is provided.
    pub async fn launch(config: TuiConfig, log_file_path: Option<&str>) -> Result<Arc<Self>> {
        let log_file = core::open_log_file(log_file_path)?;

        let (ui_tx, ui_rx) = mpsc::channel::<LiveUiMessage>(100);
        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

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
            tui_terminal,
            ui_tx,
            ui_task_handle,
            _shutdown_listener_handle,
            live_controller,
            live_update_listener_handle: OnceCell::new(),
        }))
    }

    /// Returns the current [`TuiStatus`] as a snapshot.
    pub fn status(&self) -> TuiStatus {
        self.status_manager.status()
    }

    fn spawn_live_update_listener(
        status_manager: Arc<TuiStatusManager<LiveTuiView>>,
        mut live_rx: LiveTradeReceiver,
        ui_tx: mpsc::Sender<LiveUiMessage>,
    ) -> AbortOnDropHandle<()> {
        tokio::spawn(async move {
            let send_ui_msg = async |ui_msg: LiveUiMessage| -> Result<()> {
                ui_tx
                    .send(ui_msg)
                    .await
                    .map_err(|e| TuiError::LiveTuiSendFailed(Box::new(e)))
            };

            let send_trades_update =
                async |running_trades_table: &str, closed_trades_table: &str| -> Result<()> {
                    let tables = format!("\nRunning Trades\n\n{running_trades_table}\n\n\nClosed Trades\n\n{closed_trades_table}");

                    send_ui_msg(LiveUiMessage::TradesUpdate(tables)).await
                };

            let mut running_trades_table = "No running trades.".to_string();
            let mut closed_trade_history = ClosedTradeHistory::new();
            let mut closed_trades_table = closed_trade_history.to_table();

            let mut handle_live_update = async |live_update: LiveTradeUpdate| -> Result<()> {
                match live_update {
                    LiveTradeUpdate::Status(live_status) => {
                        send_ui_msg(LiveUiMessage::LogEntry(format!("Live status: {live_status}"))).await?;

                    }
                    LiveTradeUpdate::Signal(signal) => {
                        send_ui_msg(LiveUiMessage::LogEntry(signal.to_string())).await?;
                    }
                    LiveTradeUpdate::Order(order) => {
                        send_ui_msg(LiveUiMessage::LogEntry(format!("Order: {order}"))).await?;
                    }
                    LiveTradeUpdate::TradingState(trading_state) => {
                        send_ui_msg(LiveUiMessage::SummaryUpdate(format!(
                            "\n{}",
                            trading_state.summary()
                        ))).await?;

                        running_trades_table = trading_state.running_trades_table();

                        send_trades_update(&running_trades_table, &closed_trades_table).await?;
                    }
                    LiveTradeUpdate::ClosedTrade(closed_trade) => {
                        closed_trade_history.add(closed_trade).map_err(TuiError::LiveHandleClosedTradeFailed)?;

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

                        if let Err(e) = send_ui_msg(LiveUiMessage::LogEntry(log_msg)).await {
                            status_manager.set_crashed(e);
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

                        status_manager.set_crashed(TuiError::LiveRecv(e));

                        return;
                    }
                }
            }
        })
        .into()
    }

    /// Couples a [`LiveTradeEngine`] to this TUI instance.
    ///
    /// This method starts the live trade engine and begins listening for trading updates. It can
    /// only be called once per TUI instance.
    ///
    /// Returns an error if a live trade engine has already been coupled or if the engine fails to
    /// start.
    pub async fn couple(&self, engine: LiveTradeEngine) -> Result<()> {
        if self.live_controller.initialized() {
            return Err(TuiError::LiveTradeEngineAlreadyCoupled);
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
            .map_err(TuiError::LiveTradeEngineStartFailed)?;

        self.live_controller
            .set(live_controller)
            .map_err(|_| TuiError::LiveTradeEngineAlreadyCoupled)?;

        self.live_update_listener_handle
            .set(live_update_listener_handle)
            .map_err(|_| TuiError::LiveTradeEngineAlreadyCoupled)?;

        Ok(())
    }

    /// Performs a graceful shutdown of the live trading TUI.
    ///
    /// This method shuts down the coupled live trade engine and stops the UI task. If shutdown
    /// does not complete within the configured timeout, the task is aborted.
    ///
    /// Returns an error if the TUI is not running or if shutdown fails.
    pub async fn shutdown(&self) -> Result<()> {
        self.status_manager.require_running()?;

        let live_controller = self.live_controller.get().cloned();

        core::shutdown_inner(
            self.shutdown_timeout,
            self.status_manager.clone(),
            self.ui_task_handle.clone(),
            || self.ui_tx.send(LiveUiMessage::ShutdownCompleted),
            live_controller,
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

    /// Returns this TUI as a [`TuiLogger`] trait object.
    ///
    /// This is useful for passing the TUI to components that accept a generic logger.
    pub fn as_logger(self: &Arc<Self>) -> Arc<dyn TuiLogger> {
        self.clone()
    }
}

#[async_trait]
impl TuiLogger for LiveTui {
    async fn log(&self, log_entry: String) -> Result<()> {
        self.status_manager.require_running()?;

        // An error here would be an edge case

        self.ui_tx
            .send(LiveUiMessage::LogEntry(log_entry))
            .await
            .map_err(|e| TuiError::LiveTuiSendFailed(Box::new(e)))
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
