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
    state::{LiveTradeExecutorReadyStatus, LiveTradeExecutorStateNotReady},
};

#[derive(Debug, Clone)]
pub enum LiveTradeExecutorUpdateRunning {
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
    State(TradingState),
}

impl From<TradingState> for LiveTradeExecutorUpdateRunning {
    fn from(value: TradingState) -> Self {
        Self::State(value)
    }
}

impl From<Arc<LiveTradeExecutorReadyStatus>> for LiveTradeExecutorUpdateRunning {
    fn from(value: Arc<LiveTradeExecutorReadyStatus>) -> Self {
        Self::from(TradingState::from(value.as_ref()))
    }
}

impl fmt::Display for LiveTradeExecutorUpdateRunning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateNewTrade {
                side,
                quantity,
                leverage,
                stoploss,
                takeprofit,
            } => write!(
                f,
                "CreateNewTrade(side: {}, quantity: {}, leverage: {}, stoploss: {}, takeprofit: {})",
                side, quantity, leverage, stoploss, takeprofit
            ),
            Self::UpdateTradeStoploss { id, stoploss } => {
                write!(f, "UpdateTradeStoploss(id: {}, stoploss: {})", id, stoploss)
            }
            Self::CloseTrade { id } => {
                write!(f, "CloseTrade(id: {})", id)
            }
            Self::CancelAllTrades => write!(f, "CancelAllTrades"),
            Self::CloseAllTrades => write!(f, "CloseAllTrades"),
            Self::State(_) => {
                write!(f, "State(...)")
            }
        }
    }
}

#[derive(Clone)]
pub enum LiveTradeExecutorUpdate {
    NotReady(Arc<LiveTradeExecutorStateNotReady>),
    Ready(LiveTradeExecutorUpdateRunning),
}

impl From<LiveTradeExecutorUpdateRunning> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradeExecutorUpdateRunning) -> Self {
        Self::Ready(value)
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

    pub async fn create_new_trade(
        &self,
        side: TradeSide,
        quantity: Quantity,
        leverage: Leverage,
        stoploss: Price,
        takeprofit: Price,
    ) -> LiveResult<LnmTrade> {
        let update = LiveTradeExecutorUpdateRunning::CreateNewTrade {
            side,
            quantity,
            leverage,
            stoploss,
            takeprofit,
        };

        let _ = self.update_tx.send(update.into());

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
        let update = LiveTradeExecutorUpdateRunning::UpdateTradeStoploss { id, stoploss };

        let _ = self.update_tx.send(update.into());

        self.api
            .rest
            .futures
            .update_trade_stoploss(id, stoploss)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_trade(&self, id: Uuid) -> LiveResult<LnmTrade> {
        let update = LiveTradeExecutorUpdateRunning::CloseTrade { id };

        let _ = self.update_tx.send(update.into());

        self.api
            .rest
            .futures
            .close_trade(id)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn cancel_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        let update = LiveTradeExecutorUpdateRunning::CancelAllTrades;

        let _ = self.update_tx.send(update.into());

        self.api
            .rest
            .futures
            .cancel_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        let update = LiveTradeExecutorUpdateRunning::CloseAllTrades;

        let _ = self.update_tx.send(update.into());

        self.api
            .rest
            .futures
            .close_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }
}
