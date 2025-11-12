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
                MarginValidationError, PriceValidationError, QuantityValidationError,
                TradeValidationError, ValidationError,
            },
        },
        websocket::{
            error::{WebSocketApiError, WebSocketConnectionError},
            models::{JsonRpcResponse, LnmJsonRpcRequest},
        },
    };
}

pub mod models {
    pub use uuid::Uuid;

    pub use super::{
        rest::models::{
            SATS_PER_BTC,
            leverage::Leverage,
            margin::Margin,
            price::{BoundedPercentage, LowerBoundedPercentage, Price},
            price_history::PriceEntry,
            quantity::Quantity,
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
