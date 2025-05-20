use chrono::{DateTime, Duration, SubsecRound, Utc};
use tokio::{sync::broadcast, task::JoinHandle};

/// A type that can not be instantiated
pub enum Never {}

pub trait DateTimeExt {
    fn ceil_sec(&self) -> DateTime<Utc>;

    fn is_round(&self) -> bool;
}

impl DateTimeExt for DateTime<Utc> {
    fn ceil_sec(&self) -> DateTime<Utc> {
        let trunc_time_sec = self.trunc_subsecs(0);
        if trunc_time_sec == *self {
            trunc_time_sec
        } else {
            trunc_time_sec + Duration::seconds(1)
        }
    }

    fn is_round(&self) -> bool {
        *self == self.trunc_subsecs(0)
    }
}

pub struct ShutdownForwarder(JoinHandle<()>);

impl ShutdownForwarder {
    /// Creates a new forwarder that pipes signals from `external_rx` to
    /// `internal_tx`.
    ///
    /// **Important**: An `internal_tx` signal is sent if an `external_rx` is
    /// received or if the `external_rx` channel is closed.
    pub fn new(
        mut external_rx: broadcast::Receiver<()>,
        internal_tx: broadcast::Sender<()>,
    ) -> Self {
        let handle = tokio::spawn(async move {
            let _ = external_rx.recv().await;

            // Send signal if a shutdown is received, or if `external_rx` is closed
            let _ = internal_tx.send(());
        });

        Self(handle)
    }
}

impl Drop for ShutdownForwarder {
    fn drop(&mut self) {
        self.0.abort();
    }
}
