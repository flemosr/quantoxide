use std::sync::{Arc, Mutex};

use super::{
    error::{LiveTuiError, Result},
    view::LiveTuiLogger,
};

#[derive(Debug, PartialEq)]
pub enum LiveTuiStatusStopped {
    Crashed(LiveTuiError),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveTuiStatus {
    Running,
    ShutdownInitiated,
    Stopped(Arc<LiveTuiStatusStopped>),
}

impl LiveTuiStatus {
    pub fn is_crashed(&self) -> bool {
        if let LiveTuiStatus::Stopped(ref status_stopped) = *self {
            if let LiveTuiStatusStopped::Crashed(_) = status_stopped.as_ref() {
                return true;
            }
        }
        false
    }

    pub fn is_shutdown_initiated(&self) -> bool {
        LiveTuiStatus::ShutdownInitiated == *self
    }
}

impl From<LiveTuiStatusStopped> for LiveTuiStatus {
    fn from(value: LiveTuiStatusStopped) -> Self {
        Self::Stopped(Arc::new(value))
    }
}

impl From<Arc<LiveTuiStatusStopped>> for LiveTuiStatus {
    fn from(value: Arc<LiveTuiStatusStopped>) -> Self {
        Self::Stopped(value)
    }
}

pub struct LiveTuiStatusManager {
    logger: Arc<dyn LiveTuiLogger>,
    status: Mutex<LiveTuiStatus>,
}

impl LiveTuiStatusManager {
    pub fn new_running(logger: Arc<dyn LiveTuiLogger>) -> Arc<Self> {
        Arc::new(Self {
            logger,
            status: Mutex::new(LiveTuiStatus::Running),
        })
    }

    pub fn status(&self) -> LiveTuiStatus {
        self.status.lock().expect("not poisoned").clone()
    }

    fn set(&self, new_status: LiveTuiStatus) {
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

    pub fn set_crashed(&self, error: LiveTuiError) -> Arc<LiveTuiStatusStopped> {
        let status_stopped = Arc::new(LiveTuiStatusStopped::Crashed(error));
        self.set(status_stopped.clone().into());

        status_stopped
    }

    pub fn set_shutdown_initiated(&self) {
        self.set(LiveTuiStatus::ShutdownInitiated.into());
    }

    pub fn set_shutdown(&self) {
        self.set(LiveTuiStatusStopped::Shutdown.into());
    }

    pub fn require_running(&self) -> Result<()> {
        match self.status() {
            LiveTuiStatus::Running => Ok(()),
            status_not_running => Err(LiveTuiError::Generic(format!(
                "TUI is not running {:?}",
                status_not_running
            ))),
        }
    }
}
