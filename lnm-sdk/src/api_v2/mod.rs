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
                FuturesTradeRequestValidationError, MarginValidationError, TradeValidationError,
                ValidationError,
            },
        },
        websocket::{
            error::{WebSocketApiError, WebSocketConnectionError},
            models::{JsonRpcResponse, LnmJsonRpcRequest},
        },
    };
    pub use crate::shared::models::error::{
        BoundedPercentageValidationError, LeverageValidationError,
        LowerBoundedPercentageValidationError, PriceValidationError, QuantityValidationError,
    };
}

pub mod models {
    pub use uuid::Uuid;

    pub use crate::shared::models::{
        SATS_PER_BTC,
        leverage::Leverage,
        price::{BoundedPercentage, LowerBoundedPercentage, Price},
        quantity::Quantity,
    };

    pub use super::{
        rest::models::{
            margin::Margin,
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
