use std::{fmt, num::NonZeroU64, sync::Arc};

use tokio::sync::broadcast;
use uuid::Uuid;

use lnm_sdk::api_v2::{
    ApiClient,
    models::{Leverage, Price, Trade, TradeExecution, TradeSide, TradeSize, User},
};

use super::{
    super::super::core::TradingState,
    error::{ExecutorActionError, ExecutorActionResult},
    state::{LiveTradeExecutorStatus, live_trading_session::LiveTradingSession},
};

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorUpdateOrder {
    CreateNewTrade {
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    },
    UpdateTradeStoploss {
        id: Uuid,
        stoploss: Price,
    },
    AddMargin {
        id: Uuid,
        amount: NonZeroU64,
    },
    CashIn {
        id: Uuid,
        amount: NonZeroU64,
    },
    CloseTrade {
        id: Uuid,
    },
    CancelAllTrades,
    CloseAllTrades,
}

impl fmt::Display for LiveTradeExecutorUpdateOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateNewTrade {
                side,
                size,
                leverage,
                stoploss,
                takeprofit,
            } => {
                let fmt_price_opt = |price_opt: &Option<Price>| {
                    price_opt
                        .map(|price| format!("{:.1}", price))
                        .unwrap_or_else(|| "N/A".to_string())
                };

                write!(
                    f,
                    "Create New Trade:\n  side: {}\n  size: {}\n  leverage: {}\n  stoploss: {}\n  takeprofit: {}",
                    side,
                    size,
                    leverage,
                    fmt_price_opt(stoploss),
                    fmt_price_opt(takeprofit)
                )
            }
            Self::UpdateTradeStoploss { id, stoploss } => {
                write!(
                    f,
                    "Update Trade Stoploss:\n  id: {}\n  stoploss: {:.1}",
                    id, stoploss
                )
            }
            Self::AddMargin { id, amount } => {
                write!(f, "Add Margin:\n  id: {}\n  amount: {}", id, amount)
            }
            Self::CashIn { id, amount } => {
                write!(f, "Cash In:\n  id: {}\n  amount: {}", id, amount)
            }
            Self::CloseTrade { id } => {
                write!(f, "Close Trade:\n  id: {}", id)
            }
            Self::CancelAllTrades => write!(f, "Cancel All Trades"),
            Self::CloseAllTrades => write!(f, "Close All Trades"),
        }
    }
}

#[derive(Clone)]
pub enum LiveTradeExecutorUpdate {
    Order(LiveTradeExecutorUpdateOrder),
    Status(LiveTradeExecutorStatus),
    TradingState(TradingState),
    ClosedTrade(Trade),
}

impl From<LiveTradeExecutorUpdateOrder> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradeExecutorUpdateOrder) -> Self {
        Self::Order(value)
    }
}

impl From<LiveTradeExecutorStatus> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradeExecutorStatus) -> Self {
        LiveTradeExecutorUpdate::Status(value)
    }
}

impl From<LiveTradingSession> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradingSession) -> Self {
        LiveTradeExecutorUpdate::TradingState(value.into())
    }
}

pub(super) type LiveTradeExecutorTransmiter = broadcast::Sender<LiveTradeExecutorUpdate>;
pub type LiveTradeExecutorReceiver = broadcast::Receiver<LiveTradeExecutorUpdate>;

#[derive(Clone)]
pub(in crate::trade) struct WrappedApiContext {
    api: Arc<ApiClient>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl WrappedApiContext {
    pub fn new(api: Arc<ApiClient>, update_tx: LiveTradeExecutorTransmiter) -> Self {
        Self { api, update_tx }
    }

    pub async fn get_trades_running(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.api
            .rest
            .futures
            .get_trades_running(None, None, None)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn get_user(&self) -> ExecutorActionResult<User> {
        self.api
            .rest
            .user
            .get_user()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn get_trade(&self, id: Uuid) -> ExecutorActionResult<Trade> {
        self.api
            .rest
            .futures
            .get_trade(id)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    fn send_order_update(&self, order_update: LiveTradeExecutorUpdateOrder) {
        let _ = self.update_tx.send(order_update.into());
    }

    pub async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CreateNewTrade {
            side,
            size,
            leverage,
            stoploss,
            takeprofit,
        });

        self.api
            .rest
            .futures
            .create_new_trade(
                side,
                size,
                leverage,
                TradeExecution::Market,
                stoploss,
                takeprofit,
            )
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn update_trade_stoploss(
        &self,
        id: Uuid,
        stoploss: Price,
    ) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::UpdateTradeStoploss { id, stoploss });

        self.api
            .rest
            .futures
            .update_trade_stoploss(id, stoploss)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::AddMargin { id, amount });

        self.api
            .rest
            .futures
            .add_margin(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CashIn { id, amount });

        self.api
            .rest
            .futures
            .cash_in(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn close_trade(&self, id: Uuid) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseTrade { id });

        self.api
            .rest
            .futures
            .close_trade(id)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cancel_all_trades(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CancelAllTrades);

        self.api
            .rest
            .futures
            .cancel_all_trades()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn close_all_trades(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseAllTrades);

        self.api
            .rest
            .futures
            .close_all_trades()
            .await
            .map_err(ExecutorActionError::RestApi)
    }
}
