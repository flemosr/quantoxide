mod client;
mod rest;
mod websocket;

pub use client::{ApiClient, ApiClientConfig};
pub use rest::{
    RestClient,
    repositories::{FuturesRepository, UserRepository},
};
pub use websocket::{
    WebSocketClient, repositories::WebSocketRepository, state::WsConnectionStatus,
};

pub mod error {
    pub use crate::shared::models::error::{
        BoundedPercentageValidationError, LeverageValidationError,
        LowerBoundedPercentageValidationError, MarginValidationError, PriceValidationError,
        QuantityValidationError, TradeValidationError,
    };

    pub use super::{
        rest::{
            error::RestApiError,
            models::error::{FuturesTradeRequestValidationError, ValidationError},
        },
        websocket::{
            error::{WebSocketApiError, WebSocketConnectionError},
            models::{JsonRpcResponse, LnmJsonRpcRequest},
        },
    };
}

pub mod models {
    pub use uuid::Uuid;

    pub use crate::shared::models::{
        SATS_PER_BTC,
        leverage::Leverage,
        margin::Margin,
        price::{BoundedPercentage, LowerBoundedPercentage, Price},
        quantity::Quantity,
        trade::{TradeSide, util as trade_util},
    };

    pub use super::{
        rest::models::{
            price_history::PriceEntry,
            ticker::Ticker,
            trade::{Trade, TradeExecution, TradeExecutionType, TradeSize, TradeStatus},
            user::{User, UserRole},
        },
        websocket::models::{
            LastTickDirection, PriceIndex, PriceTick, WebSocketChannel, WebSocketUpdate,
        },
    };
}
