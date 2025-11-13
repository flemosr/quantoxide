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
    pub use super::{
        rest::{
            error::RestApiError,
            models::error::{
                BoundedPercentageValidationError, FuturesTradeRequestValidationError,
                LeverageValidationError, LowerBoundedPercentageValidationError,
                MarginValidationError, PriceValidationError, TradeValidationError, ValidationError,
            },
        },
        websocket::{
            error::{WebSocketApiError, WebSocketConnectionError},
            models::{JsonRpcResponse, LnmJsonRpcRequest},
        },
    };
    pub use crate::shared::models::error::QuantityValidationError;
}

pub mod models {
    pub use uuid::Uuid;

    pub use crate::shared::models::quantity::Quantity;

    pub use super::{
        rest::models::{
            SATS_PER_BTC,
            leverage::Leverage,
            margin::Margin,
            price::{BoundedPercentage, LowerBoundedPercentage, Price},
            price_history::PriceEntry,
            ticker::Ticker,
            trade::{
                Trade, TradeExecution, TradeExecutionType, TradeSide, TradeSize, TradeStatus,
                util as trade_util,
            },
            user::{User, UserRole},
        },
        websocket::models::{
            LastTickDirection, PriceIndex, PriceTick, WebSocketChannel, WebSocketUpdate,
        },
    };
}
