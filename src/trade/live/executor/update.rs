use std::{fmt, num::NonZeroU64, sync::Arc};

use tokio::sync::broadcast;
use uuid::Uuid;

use lnm_sdk::rest::v3::{
    RestClient,
    models::{
        Account, ClientId, CrossLeverage, CrossOrder, CrossPosition, Leverage, OrderQuantity,
        Price, Trade, TradeExecution, TradeSide, TradeSize,
    },
};

use super::{
    super::super::core::TradingState,
    error::{ExecutorActionError, ExecutorActionResult},
    state::{LiveTradeExecutorStatus, live_trading_session::LiveTradingSession},
};

/// Represents an executor action sent to the exchange API.
#[derive(Debug, Clone)]
pub enum LiveTradeExecutorAction {
    IsolatedOrder {
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    },
    IsolatedTradeUpdateStoploss {
        id: Uuid,
        stoploss: Price,
    },
    IsolatedTradeAddMargin {
        id: Uuid,
        amount: NonZeroU64,
    },
    IsolatedTradeCashIn {
        id: Uuid,
        amount: NonZeroU64,
    },
    IsolatedOrderClose {
        id: Uuid,
    },
    IsolatedOrderCancelAll,
    IsolatedOrderCloseAll,
    CrossDeposit {
        amount: NonZeroU64,
    },
    CrossWithdraw {
        amount: NonZeroU64,
    },
    CrossSetLeverage {
        leverage: CrossLeverage,
    },
    CrossOrder {
        side: TradeSide,
        quantity: OrderQuantity,
        client_id: Option<ClientId>,
    },
    CrossOrderCancelAll,
    CrossOrderClosePosition,
}

impl fmt::Display for LiveTradeExecutorAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IsolatedOrder {
                side,
                size,
                leverage,
                stoploss,
                takeprofit,
                client_id,
            } => {
                let fmt_price_opt = |price_opt: &Option<Price>| {
                    price_opt
                        .map(|price| format!("{:.1}", price))
                        .unwrap_or_else(|| "N/A".to_string())
                };

                let client_id_str = client_id.as_ref().map(|id| id.as_str()).unwrap_or("N/A");

                write!(
                    f,
                    "Isolated Order:\n  side: {}\n  size: {}\n  leverage: {}\n  stoploss: {}\n  takeprofit: {}\n  client_id: {}",
                    side,
                    size,
                    leverage,
                    fmt_price_opt(stoploss),
                    fmt_price_opt(takeprofit),
                    client_id_str
                )
            }
            Self::IsolatedTradeUpdateStoploss { id, stoploss } => {
                write!(
                    f,
                    "Isolated Trade Stoploss Update:\n  id: {}\n  stoploss: {:.1}",
                    id, stoploss
                )
            }
            Self::IsolatedTradeAddMargin { id, amount } => {
                write!(
                    f,
                    "Isolated Trade Add Margin:\n  id: {}\n  amount: {}",
                    id, amount
                )
            }
            Self::IsolatedTradeCashIn { id, amount } => {
                write!(
                    f,
                    "Isolated Trade Cash In:\n  id: {}\n  amount: {}",
                    id, amount
                )
            }
            Self::IsolatedOrderClose { id } => {
                write!(f, "Isolated Order Close:\n  id: {}", id)
            }
            Self::IsolatedOrderCancelAll => write!(f, "Cancel All Isolated Orders"),
            Self::IsolatedOrderCloseAll => write!(f, "Close All Isolated Orders"),
            Self::CrossDeposit { amount } => {
                write!(f, "Cross Deposit:\n  amount: {}", amount)
            }
            Self::CrossWithdraw { amount } => {
                write!(f, "Cross Withdraw:\n  amount: {}", amount)
            }
            Self::CrossSetLeverage { leverage } => {
                write!(f, "Cross Set Leverage:\n  leverage: {}", leverage)
            }
            Self::CrossOrder {
                side,
                quantity,
                client_id,
            } => {
                let client_id_str = client_id.as_ref().map(|id| id.as_str()).unwrap_or("N/A");

                write!(
                    f,
                    "Cross Order:\n  side: {}\n  quantity: {}\n  client_id: {}",
                    side, quantity, client_id_str
                )
            }
            Self::CrossOrderCancelAll => write!(f, "Cancel All Cross Orders"),
            Self::CrossOrderClosePosition => write!(f, "Cross Order Close Position"),
        }
    }
}

/// Update events emitted by the live trade executor including executor actions, status changes,
/// trading state, and closed trades.
#[derive(Clone)]
pub enum LiveTradeExecutorUpdate {
    /// An executor action was sent to the exchange.
    Action(LiveTradeExecutorAction),
    /// The executor status changed.
    Status(LiveTradeExecutorStatus),
    /// The trading state was updated.
    TradingState(TradingState),
    /// A trade was closed.
    ClosedTrade(Trade),
}

impl From<LiveTradeExecutorAction> for LiveTradeExecutorUpdate {
    fn from(value: LiveTradeExecutorAction) -> Self {
        Self::Action(value)
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

pub(super) type LiveTradeExecutorTransmitter = broadcast::Sender<LiveTradeExecutorUpdate>;

/// Receiver for subscribing to [`LiveTradeExecutorUpdate`]s including executor actions, status
/// changes, and closed trades.
pub type LiveTradeExecutorReceiver = broadcast::Receiver<LiveTradeExecutorUpdate>;

#[derive(Clone)]
pub(in crate::trade) struct WrappedRestClient {
    api_rest: Arc<RestClient>,
    update_tx: LiveTradeExecutorTransmitter,
}

impl WrappedRestClient {
    pub fn new(api_rest: Arc<RestClient>, update_tx: LiveTradeExecutorTransmitter) -> Self {
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

    fn send_action_update(&self, action: LiveTradeExecutorAction) {
        let _ = self.update_tx.send(action.into());
    }

    pub async fn isolated_order(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        client_id: Option<ClientId>,
    ) -> ExecutorActionResult<Trade> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedOrder {
            side,
            size,
            leverage,
            stoploss,
            takeprofit,
            client_id: client_id.clone(),
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
                client_id,
            )
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_trade_update_stoploss(
        &self,
        id: Uuid,
        stoploss: Price,
    ) -> ExecutorActionResult<Trade> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedTradeUpdateStoploss {
            id,
            stoploss,
        });

        self.api_rest
            .futures_isolated
            .update_stoploss(id, Some(stoploss))
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_trade_add_margin(
        &self,
        id: Uuid,
        amount: NonZeroU64,
    ) -> ExecutorActionResult<Trade> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedTradeAddMargin { id, amount });

        self.api_rest
            .futures_isolated
            .add_margin_to_trade(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_trade_cash_in(
        &self,
        id: Uuid,
        amount: NonZeroU64,
    ) -> ExecutorActionResult<Trade> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedTradeCashIn { id, amount });

        self.api_rest
            .futures_isolated
            .cash_in_trade(id, amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_order_close(&self, id: Uuid) -> ExecutorActionResult<Trade> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedOrderClose { id });

        self.api_rest
            .futures_isolated
            .close_trade(id)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_order_cancel_all(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedOrderCancelAll);

        self.api_rest
            .futures_isolated
            .cancel_all_trades()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn isolated_order_close_all(&self) -> ExecutorActionResult<Vec<Trade>> {
        self.send_action_update(LiveTradeExecutorAction::IsolatedOrderCloseAll);

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

    pub async fn cross_get_position(&self) -> ExecutorActionResult<CrossPosition> {
        self.api_rest
            .futures_cross
            .get_position()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_deposit(&self, amount: NonZeroU64) -> ExecutorActionResult<CrossPosition> {
        self.send_action_update(LiveTradeExecutorAction::CrossDeposit { amount });

        self.api_rest
            .futures_cross
            .deposit(amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_withdraw(&self, amount: NonZeroU64) -> ExecutorActionResult<CrossPosition> {
        self.send_action_update(LiveTradeExecutorAction::CrossWithdraw { amount });

        self.api_rest
            .futures_cross
            .withdraw(amount)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_set_leverage(
        &self,
        leverage: CrossLeverage,
    ) -> ExecutorActionResult<CrossPosition> {
        self.send_action_update(LiveTradeExecutorAction::CrossSetLeverage { leverage });

        self.api_rest
            .futures_cross
            .set_leverage(leverage)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_order(
        &self,
        side: TradeSide,
        quantity: OrderQuantity,
        client_id: Option<ClientId>,
    ) -> ExecutorActionResult<CrossOrder> {
        self.send_action_update(LiveTradeExecutorAction::CrossOrder {
            side,
            quantity,
            client_id: client_id.clone(),
        });

        self.api_rest
            .futures_cross
            .place_order(side, quantity, TradeExecution::Market, client_id)
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_cancel_all_orders(&self) -> ExecutorActionResult<Vec<CrossOrder>> {
        self.send_action_update(LiveTradeExecutorAction::CrossOrderCancelAll);

        self.api_rest
            .futures_cross
            .cancel_all_orders()
            .await
            .map_err(ExecutorActionError::RestApi)
    }

    pub async fn cross_order_close_position(&self) -> ExecutorActionResult<CrossOrder> {
        self.send_action_update(LiveTradeExecutorAction::CrossOrderClosePosition);

        self.api_rest
            .futures_cross
            .close_position()
            .await
            .map_err(ExecutorActionError::RestApi)
    }
}
