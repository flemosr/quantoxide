use std::sync::{Arc, Mutex};

use tokio::sync::broadcast::error::RecvError;

use crate::util::AbortOnDropHandle;

use super::super::{
    error::{BacktestError, Result},
    state::{
        BacktestParallelReceiver, BacktestParallelUpdate, BacktestStatus, BacktestStatusManager,
    },
};

/// Controller for managing and monitoring a running parallel backtest simulation.
///
/// Provides an interface to monitor backtest status, receive updates, and control the simulation
/// lifecycle including waiting for completion or aborting the process.
#[derive(Debug)]
pub struct BacktestParallelController {
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    status_manager: Arc<BacktestStatusManager<BacktestParallelUpdate>>,
}

impl BacktestParallelController {
    pub(super) fn new(
        handle: AbortOnDropHandle<()>,
        status_manager: Arc<BacktestStatusManager<BacktestParallelUpdate>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            handle: Mutex::new(Some(handle)),
            status_manager,
        })
    }

    /// Creates a new [`BacktestParallelReceiver`] for subscribing to backtest status and trading
    /// state updates.
    pub fn receiver(&self) -> BacktestParallelReceiver {
        self.status_manager.receiver()
    }

    /// Returns the current [`BacktestStatus`] as a snapshot.
    pub fn status_snapshot(&self) -> BacktestStatus {
        self.status_manager.snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`BacktestParallelController` mutex can't be poisoned")
            .take()
    }

    /// Waits until the backtest has stopped and returns the final status.
    ///
    /// This method blocks until the backtest reaches a stopped state (finished, failed, or
    /// aborted).
    pub async fn until_stopped(&self) -> BacktestStatus {
        let mut backtest_rx = self.receiver();

        let status = self.status_snapshot();
        if status.is_stopped() {
            return status;
        }

        loop {
            match backtest_rx.recv().await {
                Ok(backtest_update) => {
                    if let BacktestParallelUpdate::Status(status) = backtest_update
                        && status.is_stopped()
                    {
                        return status;
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    let status = self.status_snapshot();
                    if status.is_stopped() {
                        return status;
                    }
                }
                Err(RecvError::Closed) => return self.status_snapshot(),
            }
        }
    }

    /// Consumes the task handle and aborts the backtest. This method can only be called once per
    /// controller instance.
    pub async fn abort(&self) -> Result<()> {
        if let Some(handle) = self.try_consume_handle() {
            if !handle.is_finished() {
                handle.abort();
                self.status_manager.update(BacktestStatus::Aborted);
            }

            return handle.await.map_err(BacktestError::TaskJoin);
        }

        Err(BacktestError::ProcessAlreadyConsumed)
    }
}
