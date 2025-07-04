use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::broadcast;

use crate::{db::models::PriceTick, sync::process::PriceHistoryState};

use super::error::SyncError;

#[derive(Debug, PartialEq)]
pub enum SyncStateNotSynced {
    NotInitiated,
    Starting,
    InProgress(PriceHistoryState),
    WaitingForResync,
    Failed(SyncError),
    Restarting,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncState {
    NotSynced(Arc<SyncStateNotSynced>),
    Synced,
    ShutdownInitiated,
    Shutdown,
}

impl From<SyncStateNotSynced> for SyncState {
    fn from(value: SyncStateNotSynced) -> Self {
        Self::NotSynced(Arc::new(value))
    }
}

#[derive(Debug, Clone)]
pub enum SyncUpdate {
    StateChange(SyncState),
    PriceTick(PriceTick),
}

impl From<SyncState> for SyncUpdate {
    fn from(value: SyncState) -> Self {
        Self::StateChange(value)
    }
}

impl From<PriceTick> for SyncUpdate {
    fn from(value: PriceTick) -> Self {
        Self::PriceTick(value)
    }
}

pub type SyncTransmiter = broadcast::Sender<SyncUpdate>;
pub type SyncReceiver = broadcast::Receiver<SyncUpdate>;

pub trait SyncReader: Send + Sync + 'static {
    fn update_receiver(&self) -> SyncReceiver;
    fn state_snapshot(&self) -> SyncState;
}

#[derive(Debug)]
pub struct SyncStateManager {
    state: Mutex<SyncState>,
    update_tx: SyncTransmiter,
}

impl SyncStateManager {
    pub fn new(update_tx: SyncTransmiter) -> Arc<Self> {
        let state = Mutex::new(SyncStateNotSynced::NotInitiated.into());

        Arc::new(Self { state, update_tx })
    }

    fn update_state_guard(&self, mut state_guard: MutexGuard<'_, SyncState>, new_state: SyncState) {
        *state_guard = new_state.clone();
        drop(state_guard);

        // Ignore no-receivers errors
        let _ = self.update_tx.send(new_state.into());
    }

    pub fn update(&self, new_state: SyncState) {
        let state_guard = self
            .state
            .lock()
            .expect("`SyncStateManager` mutex can't be poisoned");

        self.update_state_guard(state_guard, new_state);
    }

    pub fn handle_price_history_state_update(&self, new_history_state: PriceHistoryState) {
        let state_guard = self
            .state
            .lock()
            .expect("`SyncStateManager` mutex can't be poisoned");

        let SyncState::NotSynced(sync_state_not_synced) = &*state_guard else {
            return;
        };

        if let SyncStateNotSynced::Starting
        | SyncStateNotSynced::InProgress(_)
        | SyncStateNotSynced::WaitingForResync = sync_state_not_synced.as_ref()
        {
            let new_state: SyncState = SyncStateNotSynced::InProgress(new_history_state).into();

            self.update_state_guard(state_guard, new_state);
        }
    }
}

impl SyncReader for SyncStateManager {
    fn update_receiver(&self) -> SyncReceiver {
        self.update_tx.subscribe()
    }

    fn state_snapshot(&self) -> SyncState {
        self.state
            .lock()
            .expect("`SyncStateManager` mutex can't be poisoned")
            .clone()
    }
}
