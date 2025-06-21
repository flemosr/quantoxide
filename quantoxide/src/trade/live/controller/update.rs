use std::sync::Arc;

use lnm_sdk::api::{
    ApiContext,
    rest::models::{Leverage, LnmTrade, Price, Quantity, TradeExecution, TradeSide, User},
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    super::{
        super::core::TradeControllerState,
        error::{LiveError, Result as LiveResult},
    },
    state::LiveTradeControllerStateNotReady,
};

#[derive(Debug, Clone)]
pub enum LiveTradeControllerUpdateRunning {
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
    State(TradeControllerState),
}

#[derive(Debug, Clone)]
pub enum LiveTradeControllerUpdate {
    NotReady(Arc<LiveTradeControllerStateNotReady>),
    Ready(LiveTradeControllerUpdateRunning),
}

impl From<LiveTradeControllerUpdateRunning> for LiveTradeControllerUpdate {
    fn from(value: LiveTradeControllerUpdateRunning) -> Self {
        Self::Ready(value)
    }
}

pub type LiveTradeControllerTransmiter = broadcast::Sender<LiveTradeControllerUpdate>;
pub type LiveTradeControllerReceiver = broadcast::Receiver<LiveTradeControllerUpdate>;

#[derive(Clone)]
pub struct WrappedApiContext {
    api: Arc<ApiContext>,
    controller_tx: LiveTradeControllerTransmiter,
}

impl WrappedApiContext {
    pub fn new(api: Arc<ApiContext>, controller_tx: LiveTradeControllerTransmiter) -> Self {
        Self { api, controller_tx }
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
        let tc_update = LiveTradeControllerUpdateRunning::CreateNewTrade {
            side,
            quantity,
            leverage,
            stoploss,
            takeprofit,
        };

        let _ = self.controller_tx.send(tc_update.into());

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
        let tc_update = LiveTradeControllerUpdateRunning::UpdateTradeStoploss { id, stoploss };

        let _ = self.controller_tx.send(tc_update.into());

        self.api
            .rest
            .futures
            .update_trade_stoploss(id, stoploss)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_trade(&self, id: Uuid) -> LiveResult<LnmTrade> {
        let tc_update = LiveTradeControllerUpdateRunning::CloseTrade { id };

        let _ = self.controller_tx.send(tc_update.into());

        self.api
            .rest
            .futures
            .close_trade(id)
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn cancel_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        self.api
            .rest
            .futures
            .cancel_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }

    pub async fn close_all_trades(&self) -> LiveResult<Vec<LnmTrade>> {
        self.api
            .rest
            .futures
            .close_all_trades()
            .await
            .map_err(LiveError::RestApi)
    }
}
