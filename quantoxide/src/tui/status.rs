use std::sync::{Arc, Mutex};

use super::{
    error::{Result, TuiError},
    view::TuiLogger,
};

#[derive(Debug, PartialEq)]
pub enum TuiStatusStopped {
    Crashed(TuiError),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TuiStatus {
    Running,
    ShutdownInitiated,
    Stopped(Arc<TuiStatusStopped>),
}

impl TuiStatus {
    pub fn is_crashed(&self) -> bool {
        if let TuiStatus::Stopped(ref status_stopped) = *self {
            if let TuiStatusStopped::Crashed(_) = status_stopped.as_ref() {
                return true;
            }
        }
        false
    }

    pub fn is_shutdown_initiated(&self) -> bool {
        TuiStatus::ShutdownInitiated == *self
    }

    pub fn is_shutdown(&self) -> bool {
        if let TuiStatus::Stopped(ref status_stopped) = *self {
            return TuiStatusStopped::Shutdown == **status_stopped;
        }
        false
    }
}

impl From<TuiStatusStopped> for TuiStatus {
    fn from(value: TuiStatusStopped) -> Self {
        Self::Stopped(Arc::new(value))
    }
}

impl From<Arc<TuiStatusStopped>> for TuiStatus {
    fn from(value: Arc<TuiStatusStopped>) -> Self {
        Self::Stopped(value)
    }
}

pub struct TuiStatusManager<TView: TuiLogger> {
    logger: Arc<TView>,
    status: Mutex<TuiStatus>,
}

impl<TView: TuiLogger> TuiStatusManager<TView> {
    pub fn new_running(logger: Arc<TView>) -> Arc<Self> {
        Arc::new(Self {
            logger,
            status: Mutex::new(TuiStatus::Running),
        })
    }

    pub fn status(&self) -> TuiStatus {
        self.status.lock().expect("not poisoned").clone()
    }

    fn set(&self, new_status: TuiStatus) {
        let mut status = self.status.lock().expect("not poisoned");

        if status.is_crashed() {
            // Don't overwrite 'crashed' status
            return;
        }

        // TODO: Improve this log entry
        let _ = self
            .logger
            .add_log_entry(format!("TUI Status: {:?}", new_status));

        *status = new_status
    }

    pub fn set_crashed(&self, error: TuiError) -> Arc<TuiStatusStopped> {
        let status_stopped = Arc::new(TuiStatusStopped::Crashed(error));
        self.set(status_stopped.clone().into());

        status_stopped
    }

    pub fn set_shutdown_initiated(&self) {
        self.set(TuiStatus::ShutdownInitiated.into());
    }

    pub fn set_shutdown(&self) {
        self.set(TuiStatusStopped::Shutdown.into());
    }

    pub fn require_running(&self) -> Result<()> {
        match self.status() {
            TuiStatus::Running => Ok(()),
            status_not_running => Err(TuiError::Generic(format!(
                "TUI is not running {:?}",
                status_not_running
            ))),
        }
    }
}
