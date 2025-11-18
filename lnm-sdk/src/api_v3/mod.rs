pub use crate::shared::config::ApiClientConfig;

mod client;
pub(crate) mod rest;

pub use client::ApiClient;
pub use rest::{
    RestClient,
    repositories::{FuturesCrossRepository, FuturesDataRepository, FuturesIsolatedRepository},
};

pub mod error {
    pub use crate::shared::{
        models::error::{
            BoundedPercentageValidationError, LeverageValidationError,
            LowerBoundedPercentageValidationError, MarginValidationError, PriceValidationError,
            QuantityValidationError, TradeValidationError,
        },
        rest::error::RestApiError,
    };

    pub use super::rest::{
        error::RestApiV3Error,
        models::error::{
            FuturesCrossTradeOrderValidationError, FuturesIsolatedTradeRequestValidationError,
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
        trade::{
            TradeExecution, TradeExecutionType, TradeSide, TradeSize, TradeStatus,
            util as trade_util,
        },
    };

    pub use super::rest::models::{
        cross_leverage::CrossLeverage,
        funding::{CrossFunding, CrossFundingPage, IsolatedFunding, IsolatedFundingPage},
        trade::{CrossOrder, CrossOrderPage, CrossPosition, PaginatedTrades, Trade},
        transfer::{CrossTransfer, PaginatedCrossTransfers},
    };
}
