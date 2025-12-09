pub use crate::shared::config::RestClientConfig;

pub(crate) mod rest;

pub use rest::{
    RestClient,
    repositories::{
        AccountRepository, FuturesCrossRepository, FuturesDataRepository,
        FuturesIsolatedRepository, OracleRepository, UtilitiesRepository,
    },
};

pub mod error {
    pub use crate::shared::{
        models::error::{
            BoundedPercentageValidationError, LeverageValidationError, MarginValidationError,
            PercentageValidationError, PriceValidationError, QuantityValidationError,
            TradeValidationError,
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
        price::{BoundedPercentage, Percentage, Price},
        quantity::Quantity,
        trade::{
            TradeExecution, TradeExecutionType, TradeSide, TradeSize, TradeStatus,
            util as trade_util,
        },
    };

    pub use super::rest::models::{
        account::Account,
        cross_leverage::CrossLeverage,
        funding::{CrossFunding, FundingSettlement, IsolatedFunding},
        futures_data::{OhlcCandle, OhlcRange},
        oracle::{Index, LastPrice},
        page::Page,
        trade::{CrossOrder, CrossPosition, Trade},
        transfer::CrossTransfer,
    };
}
