mod api;

pub use api::{ApiContext, ApiContextConfig, websocket::state::ConnectionStatus};

pub mod error {
    pub use super::api::{
        rest::{
            error::RestApiError,
            models::error::{
                BoundedPercentageValidationError, FuturesTradeRequestValidationError,
                LeverageValidationError, LowerBoundedPercentageValidationError,
                MarginValidationError, PriceValidationError, QuantityValidationError,
                TradeValidationError, ValidationError,
            },
        },
        websocket::error::{WebSocketApiError, WebSocketConnectionError},
    };
}

pub mod models {
    pub use uuid::Uuid;

    pub use super::api::{
        rest::models::{
            leverage::Leverage,
            margin::Margin,
            price::{BoundedPercentage, LowerBoundedPercentage, Price},
            price_history::PriceEntryLNM,
            quantity::Quantity,
            ticker::Ticker,
            trade::{
                LnmTrade, Trade, TradeClosed, TradeExecution, TradeExecutionType, TradeRunning,
                TradeSide, TradeSize, TradeStatus, util as trade_util,
            },
            user::{User, UserRole},
        },
        websocket::models::{
            JsonRpcResponse, LastTickDirection, LnmJsonRpcReqMethod, LnmJsonRpcRequest,
            LnmJsonRpcResponse, LnmWebSocketChannel, PriceIndexLNM, PriceTickLNM, SubscriptionData,
            WebSocketUpdate,
        },
    };
}
