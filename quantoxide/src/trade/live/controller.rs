use std::{result, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future;
use tokio::{
    sync::{Mutex, MutexGuard, broadcast},
    task::JoinHandle,
};

use lnm_sdk::api::{
    ApiContext,
    rest::models::{
        BoundedPercentage, Leverage, LnmTrade, LowerBoundedPercentage, Price, Quantity,
        SATS_PER_BTC, Trade, TradeExecution, TradeSide,
    },
};

use crate::{
    db::DbContext,
    sync::{SyncReceiver, SyncState},
    trade::core::RiskParams,
    util::Never,
};

use super::{
    super::{
        core::{TradeController, TradeControllerState},
        error::Result,
    },
    error::{LiveError, Result as LiveResult},
};

fn calculate_quantity(
    balance: u64,
    market_price: Price,
    balance_perc: BoundedPercentage,
) -> Result<Quantity> {
    let balance_usd = balance as f64 * market_price.into_f64() / SATS_PER_BTC;
    let quantity_target = balance_usd * balance_perc.into_f64() / 100.;

    if quantity_target < 1. {
        return Err(LiveError::Generic("balance is too low".to_string()))?;
    }

    Ok(Quantity::try_from(quantity_target.floor()).map_err(LiveError::QuantityValidation)?)
}

#[derive(Clone)]
pub struct LiveTradeControllerStatus {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    last_trade_time: Option<DateTime<Utc>>,
    balance: u64,
    running: Vec<LnmTrade>,
    closed: Vec<LnmTrade>,
}

impl LiveTradeControllerStatus {
    async fn new(db: Arc<DbContext>, api: Arc<ApiContext>) -> LiveResult<Self> {
        let (running, user) = futures::try_join!(
            api.rest()
                .futures()
                .get_trades_running(None, None, 50.into()),
            api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        Ok(Self {
            db,
            api,
            last_trade_time: None,
            balance: user.balance(),
            running,
            closed: Vec::new(),
        })
    }

    // Returns true if the status was updated
    async fn reevaluate(&mut self) -> LiveResult<bool> {
        todo!()
    }

    fn register_trade(&mut self, trade: LnmTrade) -> LiveResult<()> {
        // state_guard.last_trade_time = Some(Utc::now());

        // let new_balance = state_guard.balance as i64
        //     - trade.margin().into_i64()
        //     - trade.maintenance_margin() as i64;
        // state_guard.balance = new_balance.min(0) as u64;

        // state_guard.running.push(trade);
        //
        todo!()
    }
}

pub enum LiveTradeControllerState {
    Starting,
    WaitingForSync(Arc<SyncState>),
    Ready(LiveTradeControllerStatus),
    NotViable(LiveError),
}

struct LockedLiveTradeControllerStatus<'a> {
    state_guard: MutexGuard<'a, Arc<LiveTradeControllerState>>,
}

impl<'a> LockedLiveTradeControllerStatus<'a> {
    fn as_status(&self) -> &LiveTradeControllerStatus {
        match self.state_guard.as_ref() {
            LiveTradeControllerState::Ready(status) => status,
            _ => panic!("state must be ready"),
        }
    }

    pub fn last_trade_time(&self) -> Option<DateTime<Utc>> {
        self.as_status().last_trade_time
    }

    pub fn balance(&self) -> u64 {
        self.as_status().balance
    }

    pub fn running(&self) -> &Vec<LnmTrade> {
        &self.as_status().running
    }

    pub fn closed(&self) -> &Vec<LnmTrade> {
        &self.as_status().closed
    }

    pub fn to_owned(&self) -> LiveTradeControllerStatus {
        self.as_status().clone()
    }
}

impl<'a> TryFrom<MutexGuard<'a, Arc<LiveTradeControllerState>>>
    for LockedLiveTradeControllerStatus<'a>
{
    type Error = LiveError;

    fn try_from(
        value: MutexGuard<'a, Arc<LiveTradeControllerState>>,
    ) -> result::Result<Self, Self::Error> {
        match value.as_ref() {
            LiveTradeControllerState::Ready(_) => Ok(Self { state_guard: value }),
            _ => Err(LiveError::ManagerNotReady),
        }
    }
}

pub type LiveTradeControllerTransmiter = broadcast::Sender<Arc<LiveTradeControllerState>>;
pub type LiveTradeControllerReceiver = broadcast::Receiver<Arc<LiveTradeControllerState>>;

struct LiveTradeControllerStateManager {
    state: Mutex<Arc<LiveTradeControllerState>>,
    state_tx: LiveTradeControllerTransmiter,
}

impl LiveTradeControllerStateManager {
    fn new() -> Arc<Self> {
        let state = Mutex::new(Arc::new(LiveTradeControllerState::Starting));
        let (state_tx, _) = broadcast::channel::<Arc<LiveTradeControllerState>>(100);

        Arc::new(Self { state, state_tx })
    }

    async fn try_lock_status(&self) -> LiveResult<LockedLiveTradeControllerStatus> {
        let state_guard = self.state.lock().await;
        LockedLiveTradeControllerStatus::try_from(state_guard)
    }

    pub async fn snapshot(&self) -> Arc<LiveTradeControllerState> {
        self.state.lock().await.clone()
    }

    pub fn receiver(&self) -> LiveTradeControllerReceiver {
        self.state_tx.subscribe()
    }

    async fn send_state_update(&self, new_state: Arc<LiveTradeControllerState>) {
        // We can safely ignore errors since they only mean that there are no
        // receivers.
        let _ = self.state_tx.send(new_state);
    }

    pub async fn update(&self, new_state: LiveTradeControllerState) {
        let new_state = Arc::new(new_state);

        let mut state_guard = self.state.lock().await;
        *state_guard = new_state.clone();
        drop(state_guard);

        self.send_state_update(new_state).await
    }

    pub async fn update_status(
        &self,
        mut locked_status: LockedLiveTradeControllerStatus<'_>,
        new_status: LiveTradeControllerStatus,
    ) {
        let new_state = Arc::new(LiveTradeControllerState::Ready(new_status));

        *locked_status.state_guard = new_state.clone();
        drop(locked_status);

        self.send_state_update(new_state).await
    }
}

pub struct LiveTradeController {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    start_time: DateTime<Utc>,
    start_balance: u64,
    state_manager: Arc<LiveTradeControllerStateManager>,
    handle: JoinHandle<()>,
}

impl LiveTradeController {
    fn handle_sync_updates(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
        state_manager: Arc<LiveTradeControllerStateManager>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let handler = async || -> LiveResult<Never> {
                let mut sync_rx = sync_rx;
                loop {
                    match sync_rx.recv().await {
                        Ok(sync_state) => match sync_state.as_ref() {
                            SyncState::NotInitiated
                            | SyncState::Starting
                            | SyncState::InProgress(_)
                            | SyncState::Failed(_)
                            | SyncState::Restarting => {
                                let new_state =
                                    LiveTradeControllerState::WaitingForSync(sync_state);
                                state_manager.update(new_state).await;
                            }
                            SyncState::Synced(_) => {
                                match state_manager.try_lock_status().await {
                                    Ok(locked_status) => {
                                        // update
                                        let mut status = locked_status.to_owned();

                                        // TODO: errors here would be recoverable
                                        status.reevaluate().await?;

                                        // let new_state = LiveTradeControllerState::Ready(status);
                                        state_manager.update_status(locked_status, status).await;
                                    }
                                    Err(_) => {
                                        // Manager wasn't ready

                                        // TODO: errors here would be recoverable
                                        let status =
                                            LiveTradeControllerStatus::new(db.clone(), api.clone())
                                                .await?;
                                        let new_state = LiveTradeControllerState::Ready(status);
                                        state_manager.update(new_state).await;
                                    }
                                };
                            }
                            SyncState::ShutdownInitiated | SyncState::Shutdown => {
                                return Err(LiveError::Generic(
                                    "sync process was shutdown".to_string(),
                                ));
                            }
                        },
                        Err(e) => {
                            return Err(LiveError::Generic(format!("sync_rx error {e}")));
                        }
                    }
                }
            };

            let Err(e) = handler().await;

            let new_state = LiveTradeControllerState::NotViable(e);
            state_manager.update(new_state).await;
        })
    }

    pub async fn new(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        sync_rx: SyncReceiver,
    ) -> Result<Self> {
        let start_time = Utc::now();

        let (_, _, user) = futures::try_join!(
            api.rest().futures().cancel_all_trades(),
            api.rest().futures().close_all_trades(),
            api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        let start_balance = user.balance();

        let state_manager = LiveTradeControllerStateManager::new();

        let handle =
            Self::handle_sync_updates(db.clone(), api.clone(), sync_rx, state_manager.clone());

        Ok(Self {
            db,
            api,
            start_time,
            start_balance,
            state_manager,
            handle,
        })
    }

    async fn get_estimated_market_price(&self) -> Result<Price> {
        // Assuming that the db is up-to-date

        let (_, last_entry_price) = self
            .db
            .price_ticks
            .get_latest_entry()
            .await
            .map_err(|e| LiveError::Generic(e.to_string()))?
            .ok_or(LiveError::Generic("db is empty".to_string()))?;

        let price =
            Price::round(last_entry_price).map_err(|e| LiveError::Generic(e.to_string()))?;

        Ok(price)
    }
}

#[async_trait]
impl TradeController for LiveTradeController {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        let risk_params = RiskParams::Long {
            stoploss_perc,
            takeprofit_perc,
        };

        let est_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(est_price)?;

        let quantity = calculate_quantity(locked_status.balance(), est_price, balance_perc)?;

        let trade = self
            .api
            .rest()
            .futures()
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)?;

        let mut new_status = locked_status.to_owned();

        new_status.register_trade(trade)?;

        self.state_manager
            .update_status(locked_status, new_status)
            .await;

        Ok(())
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        let risk_params = RiskParams::Short {
            stoploss_perc,
            takeprofit_perc,
        };

        let est_price = self.get_estimated_market_price().await?;

        let (side, stoploss, takeprofit) = risk_params.into_trade_params(est_price)?;

        let quantity = calculate_quantity(locked_status.balance(), est_price, balance_perc)?;

        let trade = self
            .api
            .rest()
            .futures()
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)?;

        let mut new_status = locked_status.to_owned();

        new_status.register_trade(trade)?;

        self.state_manager
            .update_status(locked_status, new_status)
            .await;

        Ok(())
    }

    async fn close_longs(&self) -> Result<()> {
        let locked_status = self.state_manager.try_lock_status().await?;

        // let running = self
        //     .api
        //     .rest()
        //     .futures()
        //     .get_trades_running(None, None, 1000.into())
        //     .await
        //     .map_err(LiveError::RestApi)?;

        let long_trades = locked_status
            .running()
            .iter()
            .filter(|trade| trade.side() == TradeSide::Buy)
            .collect::<Vec<_>>();

        // Process in batches of 5
        for chunk in long_trades.chunks(5) {
            let close_futures = chunk
                .iter()
                .map(|trade| {
                    let rest_futures = self.api.rest().futures();
                    async move { rest_futures.close_trade(trade.id()).await }
                })
                .collect::<Vec<_>>();

            future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)?;
        }

        Ok(())
    }

    async fn close_shorts(&self) -> Result<()> {
        // let mut state_guard = self.state.lock().await;
        // state_guard.last_trade_time = Some(Utc::now());

        let running = self
            .api
            .rest()
            .futures()
            .get_trades_running(None, None, 1000.into())
            .await
            .map_err(LiveError::RestApi)?;

        let short_trades = running
            .into_iter()
            .filter(|trade| trade.side() == TradeSide::Sell)
            .collect::<Vec<_>>();

        // Process in batches of 5
        for chunk in short_trades.chunks(5) {
            let close_futures = chunk
                .iter()
                .map(|trade| {
                    let rest_futures = self.api.rest().futures();
                    async move { rest_futures.close_trade(trade.id()).await }
                })
                .collect::<Vec<_>>();

            future::join_all(close_futures)
                .await
                .into_iter()
                .collect::<result::Result<Vec<_>, _>>()
                .map_err(LiveError::RestApi)?;
        }

        Ok(())
    }

    async fn close_all(&self) -> Result<()> {
        // let mut state_guard = self.state.lock().await;
        // state_guard.last_trade_time = Some(Utc::now());

        let (_, _) = futures::try_join!(
            self.api.rest().futures().cancel_all_trades(),
            self.api.rest().futures().close_all_trades(),
        )
        .map_err(LiveError::RestApi)?;

        Ok(())
    }

    async fn state(&self) -> Result<TradeControllerState> {
        let status = self.state_manager.try_lock_status().await?;

        // TODO

        let (running_trades, closed_trades, ticker, user) = futures::try_join!(
            self.api
                .rest()
                .futures()
                .get_trades_running(None, None, 1000.into()),
            self.api
                .rest()
                .futures()
                .get_trades_closed(Some(&self.start_time), None, 1000.into()),
            self.api.rest().futures().ticker(),
            self.api.rest().user().get_user()
        )
        .map_err(LiveError::RestApi)?;

        let mut running_long_qtd: usize = 0;
        let mut running_long_margin: u64 = 0;
        let mut running_short_qtd: usize = 0;
        let mut running_short_margin: u64 = 0;
        let mut running_pl: i64 = 0;
        let mut running_fees: u64 = 0;
        let mut running_maintenance_margin: u64 = 0;

        for trade in running_trades.iter() {
            match trade.side() {
                TradeSide::Buy => {
                    running_long_qtd += 1;
                    running_long_margin += trade.margin().into_u64();
                }
                TradeSide::Sell => {
                    running_short_qtd += 1;
                    running_short_margin += trade.margin().into_u64();
                }
            };

            running_pl += trade.pl();
            running_fees += trade.opening_fee();
            running_maintenance_margin += trade.maintenance_margin();
        }

        let mut closed_pl: i64 = 0;
        let mut closed_fees: u64 = 0;

        for trade in closed_trades.iter() {
            closed_pl += trade.pl();
            closed_fees += trade.opening_fee() + trade.closing_fee();
        }

        let trades_state = TradeControllerState::new(
            self.start_time,
            self.start_balance,
            Utc::now(),
            user.balance(),
            ticker.last_price().into_f64(),
            status.last_trade_time(),
            running_long_qtd,
            running_long_margin,
            running_short_qtd,
            running_short_margin,
            running_pl,
            running_fees,
            running_maintenance_margin,
            closed_trades.len(),
            closed_pl,
            closed_fees,
        );

        Ok(trades_state)
    }
}
