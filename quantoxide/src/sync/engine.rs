use std::{
    fmt,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use tokio::{sync::broadcast, time};

use lnm_sdk::{api_v2::WebSocketClient, api_v3::RestClient};

use crate::{
    db::Database,
    shared::LookbackPeriod,
    sync::config::{SyncConfig, SyncControllerConfig},
    tui::{TuiControllerShutdown, TuiError, error::Result as TuiResult},
    util::AbortOnDropHandle,
};

use super::{
    error::{Result, SyncError},
    process::{SyncProcess, error::SyncProcessFatalError},
    state::{SyncReader, SyncReceiver, SyncStatus, SyncStatusManager, SyncTransmiter, SyncUpdate},
};

/// Controller for managing and monitoring a running synchronization process.
///
/// `SyncController` provides an interface to monitor the status of a sync process and perform
/// graceful shutdown operations. It holds a handle to the running sync task and coordinates
/// shutdown signals.
#[derive(Debug)]
pub struct SyncController {
    config: SyncControllerConfig,
    handle: Mutex<Option<AbortOnDropHandle<()>>>,
    shutdown_tx: broadcast::Sender<()>,
    status_manager: Arc<SyncStatusManager>,
}

impl SyncController {
    fn new(
        config: &SyncConfig,
        handle: AbortOnDropHandle<()>,
        shutdown_tx: broadcast::Sender<()>,
        status_manager: Arc<SyncStatusManager>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config: config.into(),
            handle: Mutex::new(Some(handle)),
            shutdown_tx,
            status_manager,
        })
    }

    /// Returns a [`SyncReader`] interface for accessing sync status and updates.
    pub fn reader(&self) -> Arc<dyn SyncReader> {
        self.status_manager.clone()
    }

    /// Returns the [`SyncMode`] of the sync process.
    pub fn mode(&self) -> SyncMode {
        self.status_manager.mode()
    }

    /// Creates a new [`SyncReceiver`] for subscribing to sync status updates.
    pub fn update_receiver(&self) -> SyncReceiver {
        self.status_manager.update_receiver()
    }

    /// Returns the current [`SyncStatus`] as a snapshot.
    pub fn status_snapshot(&self) -> SyncStatus {
        self.status_manager.status_snapshot()
    }

    fn try_consume_handle(&self) -> Option<AbortOnDropHandle<()>> {
        self.handle
            .lock()
            .expect("`SyncController` mutex can't be poisoned")
            .take()
    }

    /// Tries to perform a clean shutdown of the sync process and consumes the task handle. If a
    /// clean shutdown fails, the process is aborted.
    ///
    /// This method can only be called once per controller instance.
    ///
    /// Returns an error if the process had to be aborted, or if it the handle was already consumed.
    pub async fn shutdown(&self) -> Result<()> {
        let Some(mut handle) = self.try_consume_handle() else {
            return Err(SyncError::SyncAlreadyShutdown);
        };

        if handle.is_finished() {
            let status = self.status_manager.status_snapshot();
            return Err(SyncError::SyncAlreadyTerminated(status));
        }

        self.status_manager.update(SyncStatus::ShutdownInitiated);

        let shutdown_send_res = self.shutdown_tx.send(()).map_err(|e| {
            handle.abort();
            SyncProcessFatalError::SendShutdownSignalFailed(e)
        });

        let shutdown_res = match shutdown_send_res {
            Ok(_) => {
                tokio::select! {
                    join_res = &mut handle => {
                        join_res.map_err(SyncProcessFatalError::SyncProcessTaskJoin)
                    }
                    _ = time::sleep(self.config.shutdown_timeout()) => {
                        handle.abort();
                        Err(SyncProcessFatalError::ShutdownTimeout)
                    }
                }
            }
            Err(e) => Err(e),
        };

        if let Err(e) = shutdown_res {
            let e_ref = Arc::new(e);
            self.status_manager.update(e_ref.clone().into());

            return Err(SyncError::SyncShutdownFailed(e_ref));
        }

        self.status_manager.update(SyncStatus::Shutdown);
        Ok(())
    }
}

#[async_trait]
impl TuiControllerShutdown for SyncController {
    async fn tui_shutdown(&self) -> TuiResult<()> {
        self.shutdown().await.map_err(TuiError::SyncShutdownFailed)
    }
}

/// Synchronization mode that determines how price data is fetched and maintained.
///
/// The sync mode controls which data sources are used and how far back in time to fetch historical
/// data.
#[derive(Debug, Clone, Copy)]
pub enum SyncMode {
    /// Backfill mode: only fetches historical price data from REST API.
    ///
    /// This mode does not maintain live price feeds and is suitable for populating historical data
    /// in batch.
    Backfill,
    /// Live mode: maintains real-time price feeds via WebSocket.
    ///
    /// Optionally includes a lookback period to also fetch recent historical data before starting
    /// the live feed.
    Live(Option<LookbackPeriod>),
    /// Full mode: combines both backfill and live synchronization.
    ///
    /// Fetches complete historical data and then maintains real-time price feeds.
    Full,
}

impl SyncMode {
    /// Creates a backfill-only sync mode.
    pub fn backfill() -> Self {
        SyncMode::Backfill
    }

    /// Creates a live sync mode without historical lookback.
    pub fn live_no_lookback() -> Self {
        SyncMode::Live(None)
    }

    /// Creates a live sync mode with a specified lookback period in minutes.
    pub fn live_with_lookback(minutes: u64) -> Result<Self> {
        let lookback = LookbackPeriod::try_from(minutes)?;

        Ok(SyncMode::Live(Some(lookback)))
    }

    /// Creates a full sync mode (both backfill and live).
    pub fn full() -> Self {
        SyncMode::Full
    }

    /// Returns whether this mode includes an active live price feed.
    ///
    /// Returns `true` for `Live` and `Full` modes, `false` for `Backfill`.
    pub fn live_feed_active(&self) -> bool {
        match self {
            SyncMode::Backfill => false,
            SyncMode::Live(_) => true,
            SyncMode::Full => true,
        }
    }
}

impl fmt::Display for SyncMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncMode::Backfill => write!(f, "Backfill"),
            SyncMode::Live(lookback_opt) => match lookback_opt {
                Some(lookback) => write!(f, "Live (lookback: {})", lookback.as_duration()),
                None => write!(f, "Live"),
            },
            SyncMode::Full => write!(f, "Full"),
        }
    }
}

pub(super) enum SyncModeInt {
    Backfill {
        api_rest: Arc<RestClient>,
    },
    LiveNoLookback {
        api_ws: Arc<WebSocketClient>,
    },
    LiveWithLookback {
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
        lookback: LookbackPeriod,
    },
    Full {
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
    },
}

impl From<&SyncModeInt> for SyncMode {
    fn from(value: &SyncModeInt) -> Self {
        match value {
            SyncModeInt::Backfill { .. } => Self::Backfill,
            SyncModeInt::LiveNoLookback { .. } => Self::Live(None),
            SyncModeInt::LiveWithLookback {
                api_rest: _,
                api_ws: _,
                lookback,
            } => Self::Live(Some(*lookback)),
            SyncModeInt::Full { .. } => Self::Full,
        }
    }
}

/// Builder for configuring and starting a synchronization engine.
///
/// `SyncEngine` encapsulates the configuration, database connection, API clients, and sync mode.
/// The sync process is spawned when [`start`](Self::start) is called, and a [`SyncController`] is
/// returned for monitoring and management.
pub struct SyncEngine {
    config: SyncConfig,
    db: Arc<Database>,
    mode_int: SyncModeInt,
    status_manager: Arc<SyncStatusManager>,
    update_tx: SyncTransmiter,
}

impl SyncEngine {
    fn with_mode_int(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        mode_int: SyncModeInt,
    ) -> Self {
        let (update_tx, _) = broadcast::channel::<SyncUpdate>(1_000);

        let mode = (&mode_int).into();
        let status_manager = SyncStatusManager::new(mode, update_tx.clone());

        Self {
            config: config.into(),
            db,
            mode_int,
            status_manager,
            update_tx,
        }
    }

    pub(crate) fn live_no_lookback(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_ws: Arc<WebSocketClient>,
    ) -> Self {
        let mode_int = SyncModeInt::LiveNoLookback { api_ws };

        Self::with_mode_int(config, db, mode_int)
    }

    pub(crate) fn live_with_lookback(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
        lookback: LookbackPeriod,
    ) -> Self {
        let mode_int = SyncModeInt::LiveWithLookback {
            api_rest,
            api_ws,
            lookback,
        };

        Self::with_mode_int(config, db, mode_int)
    }

    pub(crate) fn full(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_rest: Arc<RestClient>,
        api_ws: Arc<WebSocketClient>,
    ) -> Self {
        let mode_int = SyncModeInt::Full { api_rest, api_ws };

        Self::with_mode_int(config, db, mode_int)
    }

    /// Creates a new sync engine with the specified configuration and mode.
    ///
    /// This constructor automatically initializes the required API clients based on the sync mode.
    pub fn new(
        config: impl Into<SyncConfig>,
        db: Arc<Database>,
        api_domain: impl ToString,
        mode: SyncMode,
    ) -> Result<Self> {
        let config: SyncConfig = config.into();
        let domain = api_domain.to_string();

        let api_rest = RestClient::new(&config, domain.clone()).map_err(SyncError::RestApiInit)?;
        let api_ws = WebSocketClient::new(&config, domain);

        let mode = match mode {
            SyncMode::Backfill => SyncModeInt::Backfill { api_rest },
            SyncMode::Live(lookback_opt) => match lookback_opt {
                Some(lookback) => SyncModeInt::LiveWithLookback {
                    api_rest,
                    api_ws,
                    lookback,
                },
                None => SyncModeInt::LiveNoLookback { api_ws },
            },
            SyncMode::Full => SyncModeInt::Full { api_rest, api_ws },
        };

        Ok(Self::with_mode_int(config, db, mode))
    }

    /// Returns a reader interface for accessing sync status and updates.
    pub fn reader(&self) -> Arc<dyn SyncReader> {
        self.status_manager.clone()
    }

    /// Creates a new receiver for subscribing to sync status updates.
    pub fn update_receiver(&self) -> SyncReceiver {
        self.status_manager.update_receiver()
    }

    /// Returns the current synchronization status as a snapshot.
    pub fn status_snapshot(&self) -> SyncStatus {
        self.status_manager.status_snapshot()
    }

    /// Starts the synchronization process and returns a [`SyncController`] for managing it.
    ///
    /// This consumes the engine and spawns the sync task in the background.
    pub fn start(self) -> Arc<SyncController> {
        let (shutdown_tx, _) = broadcast::channel::<()>(1);

        let handle = SyncProcess::spawn(
            &self.config,
            self.db,
            self.mode_int,
            shutdown_tx.clone(),
            self.status_manager.clone(),
            self.update_tx,
        );

        SyncController::new(&self.config, handle, shutdown_tx, self.status_manager)
    }
}
