use std::{fmt, sync::Arc};

use lnm_sdk::api::{
    ApiContext,
    rest::models::{Leverage, LnmTrade, Price, Quantity, TradeExecution, TradeSide, User},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    super::{
        super::core::TradingState,
        error::{LiveError, Result as LiveResult},
    },
    state::{LiveTradeExecutorStatus, LiveTradingSession},
};

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorUpdateOrder {
    CreateNewTrade {
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        stoploss: Price,
        takeprofit: Price,
    },
    UpdateTradeStoploss {
        id: Uuid,
        stoploss: Price,
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
                quantity,
                leverage,
                stoploss,
                takeprofit,
            } => {
                write!(
                    f,
                    "CreateNewTrade:\n  side: {}\n  quantity: {}\n  leverage: {}\n  stoploss: {:.1}\n  takeprofit: {:.1}",
                    side, quantity, leverage, stoploss, takeprofit
                )
            }
            Self::UpdateTradeStoploss { id, stoploss } => {
                write!(
                    f,
                    "UpdateTradeStoploss:\n  id: {}\n  stoploss: {:.1}",
                    id, stoploss
                )
            }
            Self::CloseTrade { id } => {
                write!(f, "CloseTrade:\n  id: {}", id)
            }
            Self::CancelAllTrades => write!(f, "CancelAllTrades"),
            Self::CloseAllTrades => write!(f, "CloseAllTrades"),
        }
    }
}

#[derive(Clone)]
pub enum LiveTradeExecutorUpdate {
    Order(LiveTradeExecutorUpdateOrder),
    Status(LiveTradeExecutorStatus),
    TradingState(TradingState),
    ClosedTrade(LnmTrade),
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

pub type LiveTradeExecutorTransmiter = broadcast::Sender<LiveTradeExecutorUpdate>;
pub type LiveTradeExecutorReceiver = broadcast::Receiver<LiveTradeExecutorUpdate>;

#[derive(Clone)]
pub struct WrappedApiContext {
    api: Arc<ApiContext>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl WrappedApiContext {
    pub fn new(api: Arc<ApiContext>, update_tx: LiveTradeExecutorTransmiter) -> Self {
        Self { api, update_tx }
    }

    pub async fn get_trades_running(&self) -> LiveResult<Vec<LnmTrade>> {
        self.api
            .rest
            .futures
            .get_trades_running(None, None, None)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn get_user(&self) -> LiveResult<User> {
        self.api
            .rest
            .user
            .get_user()
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn get_trade(&self, id: Uuid) -> LiveResult<LnmTrade> {
        self.api
            .rest
            .futures
            .get_trade(id)
            .await
            .map_err(LiveError::RestApi)
    }

    fn send_order_update(&self, order_update: LiveTradeExecutorUpdateOrder) {
        let _ = self.update_tx.send(order_update.into());
    }

    pub async fn create_new_trade(
        &self,
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        stoploss: Price,
        takeprofit: Price,
    ) -> LiveResult<LnmTrade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CreateNewTrade {
            side,
            quantity,
            leverage,
            stoploss,
            takeprofit,
        });

        self.api
            .rest
            .futures
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                TradeExecution::Market,
                Some(stoploss),
                Some(takeprofit),
            )
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> LiveResult<LnmTrade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::UpdateTradeStoploss { id, stoploss });

        self.api
            .rest
            .futures
            .update_trade_stoploss(id, stoploss)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_trade(&self, id: Uuid) -> LiveResult<LnmTrade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseTrade { id });

        self.api
            .rest
            .futures
            .close_trade(id)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn cancel_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CancelAllTrades);

        self.api
            .rest
            .futures
            .cancel_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseAllTrades);

        self.api
            .rest
            .futures
            .close_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }
}
