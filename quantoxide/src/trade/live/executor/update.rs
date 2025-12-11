use std::{fmt, num::NonZeroU64, sync::Arc};

use tokio::sync::broadcast;
use uuid::Uuid;

use lnm_sdk::api_v3::{
    RestClient,
    models::{Account, Leverage, Price, Trade, TradeExecution, TradeSide, TradeSize},
};

use super::{
    super::super::core::TradingState,
    error::{ExecutorActionError, ExecutorActionResult},
    state::{LiveTradeExecutorStatus, live_trading_session::LiveTradingSession},
};

/// Represents a trade order operation sent to the exchange API.
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

/// Update events emitted by the live trade executor including orders, status changes, trading
/// state, and closed trades.
#[derive(Clone)]
pub enum LiveTradeExecutorUpdate {
    /// A trade order operation was sent to the exchange.
    Order(LiveTradeExecutorUpdateOrder),
    /// The executor status changed.
    Status(LiveTradeExecutorStatus),
    /// The trading state was updated.
    TradingState(TradingState),
    /// A trade was closed.
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

/// Receiver for subscribing to [`LiveTradeExecutorUpdate`]s including orders, status changes, and
/// closed trades.
pub type LiveTradeExecutorReceiver = broadcast::Receiver<LiveTradeExecutorUpdate>;

#[derive(Clone)]
pub(in crate::trade) struct WrappedRestClient {
    api_rest: Arc<RestClient>,
    update_tx: LiveTradeExecutorTransmiter,
}

impl WrappedRestClient {
    pub fn new(api_rest: Arc<RestClient>, update_tx: LiveTradeExecutorTransmiter) -> Self {
        Self {
            api_rest,
            update_tx,
        }
    }

    pub async fn get_trades_running(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.api_rest
            .futures_isolated
            .get_running_trades()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn get_trades_closed(&self, limit: NonZeroU64) -> ExecutorActionResult<Vec<Trade>> {
        let trade_page = self
            .api_rest
            .futures_isolated
            .get_closed_trades(None, None, Some(limit), None)
            .await
            .map_err(ExecutorActionError::RestApi)?;

        Ok(trade_page.into())
    }

    pub async fn get_user(&self) -> ExecutorActionResult<Account> {
        self.api_rest
            .account
            .get_account()
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

        self.api_rest
            .futures_isolated
            .new_trade(
                side,
                size,
                leverage,
                TradeExecution::Market,
                stoploss,
                takeprofit,
                None,
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

        self.api_rest
            .futures_isolated
            .update_stoploss(id, Some(stoploss))
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::AddMargin { id, amount });

        self.api_rest
            .futures_isolated
            .add_margin_to_trade(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CashIn { id, amount });

        self.api_rest
            .futures_isolated
            .cash_in_trade(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn close_trade(&self, id: Uuid) -> ExecutorActionResult<Trade> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseTrade { id });

        self.api_rest
            .futures_isolated
            .close_trade(id)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cancel_all_trades(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CancelAllTrades);

        self.api_rest
            .futures_isolated
            .cancel_all_trades()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn close_all_trades(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_order_update(LiveTradeExecutorUpdateOrder::CloseAllTrades);

        let running_trades = self
            .api_rest
            .futures_isolated
            .get_running_trades()
            .await
            .map_err(ExecutorActionError::RestApi)?;

        let mut closed_trades = Vec::new();

        for trade in running_trades {
            let closed_trade = self
                .api_rest
                .futures_isolated
                .close_trade(trade.id())
                .await
                .map_err(ExecutorActionError::RestApi)?;

            closed_trades.push(closed_trade);
        }

        Ok(closed_trades)
    }
}
