use std::{
    fmt,
    sync::{Arc, Mutex},
};

use super::{
    error::{Result, TuiError},
    view::TuiLogManager,
};

#[derive(Debug)]
pub enum TuiStatusStopped {
    Crashed(TuiError),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum TuiStatus {
    Running,
    ShutdownInitiated,
    Stopped(Arc<TuiStatusStopped>),
}

impl fmt::Display for TuiStatusStopped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Crashed(error) => write!(f, "Crashed: {}", error),
            Self::Shutdown => write!(f, "Shutdown"),
        }
    }
}

impl fmt::Display for TuiStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Running => write!(f, "Running"),
            Self::ShutdownInitiated => write!(f, "Shutdown Initiated"),
            Self::Stopped(status) => write!(f, "Stopped: {}", status),
        }
    }
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
        matches!(self, TuiStatus::ShutdownInitiated)
    }

    pub fn is_shutdown(&self) -> bool {
        if let TuiStatus::Stopped(ref status_stopped) = *self {
            return matches!(status_stopped.as_ref(), TuiStatusStopped::Shutdown);
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

pub struct TuiStatusManager<TView: TuiLogManager> {
    logger: Arc<TView>,
    status: Mutex<TuiStatus>,
}

impl<TView: TuiLogManager> TuiStatusManager<TView> {
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
            status_not_running => Err(TuiError::TuiNotRunning(status_not_running)),
        }
    }
}
