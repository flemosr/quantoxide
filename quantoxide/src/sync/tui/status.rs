use std::sync::{Arc, Mutex};

use super::{
    super::{SyncError, error::Result},
    content::SyncTuiLogger,
};

#[derive(Debug, PartialEq)]
pub enum SyncTuiStatusStopped {
    Crashed(SyncError),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncTuiStatus {
    Running,
    ShutdownInitiated,
    Stopped(Arc<SyncTuiStatusStopped>),
}

impl SyncTuiStatus {
    pub fn is_crashed(&self) -> bool {
        if let SyncTuiStatus::Stopped(ref status_stopped) = *self {
            if let SyncTuiStatusStopped::Crashed(_) = status_stopped.as_ref() {
                return true;
            }
        }
        false
    }

    pub fn is_shutdown_initiated(&self) -> bool {
        SyncTuiStatus::ShutdownInitiated == *self
    }
}

impl From<SyncTuiStatusStopped> for SyncTuiStatus {
    fn from(value: SyncTuiStatusStopped) -> Self {
        Self::Stopped(Arc::new(value))
    }
}

impl From<Arc<SyncTuiStatusStopped>> for SyncTuiStatus {
    fn from(value: Arc<SyncTuiStatusStopped>) -> Self {
        Self::Stopped(value)
    }
}

pub struct SyncTuiStatusManager {
    logger: Arc<dyn SyncTuiLogger>,
    status: Mutex<SyncTuiStatus>,
}

impl SyncTuiStatusManager {
    pub fn new_running(logger: Arc<dyn SyncTuiLogger>) -> Arc<Self> {
        Arc::new(Self {
            logger,
            status: Mutex::new(SyncTuiStatus::Running),
        })
    }

    pub fn status(&self) -> SyncTuiStatus {
        self.status.lock().expect("not poisoned").clone()
    }

    fn set(&self, new_status: SyncTuiStatus) {
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

    pub fn set_crashed(&self, error: SyncError) -> Arc<SyncTuiStatusStopped> {
        let status_stopped = Arc::new(SyncTuiStatusStopped::Crashed(error));
        self.set(status_stopped.clone().into());

        status_stopped
    }

    pub fn set_shutdown_initiated(&self) {
        self.set(SyncTuiStatus::ShutdownInitiated.into());
    }

    pub fn set_shutdown(&self) {
        self.set(SyncTuiStatusStopped::Shutdown.into());
    }

    pub fn require_running(&self) -> Result<()> {
        match self.status() {
            SyncTuiStatus::Running => Ok(()),
            status_not_running => Err(SyncError::Generic(format!(
                "TUI is not running {:?}",
                status_not_running
            ))),
        }
    }
}
