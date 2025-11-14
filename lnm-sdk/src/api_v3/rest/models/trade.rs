use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    api_v3::rest::models::error::FuturesIsolatedTradeRequestValidationError,
    shared::models::{
        leverage::Leverage,
        margin::Margin,
        price::Price,
        quantity::Quantity,
        serde_util,
        trade::{TradeExecution, TradeExecutionType, TradeSide, TradeSize},
    },
};

#[derive(Serialize, Debug)]
pub(in crate::api_v3) struct FuturesIsolatedTradeRequestBody {
    leverage: Leverage,
    side: TradeSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    stoploss: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    takeprofit: Option<Price>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(flatten)]
    size: TradeSize,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<Price>,
}

impl FuturesIsolatedTradeRequestBody {
    pub fn new(
        leverage: Leverage,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
        side: TradeSide,
        client_id: Option<String>,
        size: TradeSize,
        trade_execution: TradeExecution,
    ) -> Result<Self, FuturesIsolatedTradeRequestValidationError> {
        if let TradeExecution::Limit(price) = trade_execution {
            if let TradeSize::Margin(margin) = &size {
                // Implied `Quantity` must be valid
                let _ = Quantity::try_calculate(*margin, price, leverage)?;
            }

            if let Some(stoploss) = stoploss {
                if stoploss >= price {
                    return Err(
                        FuturesIsolatedTradeRequestValidationError::StopLossHigherThanPrice,
                    );
                }
            }

            if let Some(takeprofit) = takeprofit {
                if takeprofit <= price {
                    return Err(
                        FuturesIsolatedTradeRequestValidationError::TakeProfitLowerThanPrice,
                    );
                }
            }
        }

        let (trade_type, price) = match trade_execution {
            TradeExecution::Market => (TradeExecutionType::Market, None),
            TradeExecution::Limit(price) => (TradeExecutionType::Limit, Some(price)),
        };

        if client_id
            .as_ref()
            .map_or(false, |client_id| client_id.len() > 64)
        {
            return Err(FuturesIsolatedTradeRequestValidationError::ClientIdTooLong);
        }

        Ok(FuturesIsolatedTradeRequestBody {
            leverage,
            stoploss,
            takeprofit,
            side,
            client_id,
            size,
            trade_type,
            price,
        })
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    id: Uuid,
    #[serde(rename = "type")]
    trade_type: TradeExecutionType,
    side: TradeSide,
    opening_fee: u64,
    closing_fee: u64,
    maintenance_margin: i64,
    quantity: Quantity,
    margin: Margin,
    leverage: Leverage,
    price: Price,
    liquidation: Price,
    #[serde(with = "serde_util::price_option")]
    stoploss: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    takeprofit: Option<Price>,
    #[serde(with = "serde_util::price_option")]
    exit_price: Option<Price>,
    pl: i64,
    created_at: DateTime<Utc>,
    filled_at: Option<DateTime<Utc>>,
    closed_at: Option<DateTime<Utc>>,
    #[serde(with = "serde_util::price_option")]
    entry_price: Option<Price>,
    entry_margin: Option<Margin>,
    open: bool,
    running: bool,
    canceled: bool,
    closed: bool,
    sum_funding_fees: i64,
    client_id: Option<String>,
}

impl Trade {
    /// Returns the unique identifier for this trade.
    ///
    /// # Examples
    ///
    /// ```
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let trade_id = trade.id();
    ///
    /// println!("Trade ID: {}", trade_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the execution type (Market or Limit).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let exec_type = trade.trade_type();
    ///
    /// println!("Trade execution type: {:?}", exec_type);
    /// # Ok(())
    /// # }
    /// ```
    pub fn trade_type(&self) -> TradeExecutionType {
        self.trade_type
    }

    /// Returns the side of the trade (Buy or Sell).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let side = trade.side();
    ///
    /// println!("Trade side: {:?}", side);
    /// # Ok(())
    /// # }
    /// ```
    pub fn side(&self) -> TradeSide {
        self.side
    }

    /// Returns the opening fee charged when the trade was filled (in satoshis), or zero if the
    /// trade was not filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = trade.opening_fee();
    ///
    /// println!("Opening fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn opening_fee(&self) -> u64 {
        self.opening_fee
    }

    /// Returns the closing fee that was charged when the trade was closed (in satoshis), or zero
    /// if the trade was not closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee = trade.closing_fee();
    ///
    /// println!("Closing fee: {} sats", fee);
    /// # Ok(())
    /// # }
    /// ```
    pub fn closing_fee(&self) -> u64 {
        self.closing_fee
    }

    /// Returns the maintenance margin requirement (in satoshis).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let margin = trade.maintenance_margin();
    ///
    /// println!("Maintenance margin: {} sats", margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn maintenance_margin(&self) -> i64 {
        self.maintenance_margin
    }

    /// Returns the quantity (notional value in USD) of the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let quantity = trade.quantity();
    ///
    /// println!("Trade quantity: {}", quantity);
    /// # Ok(())
    /// # }
    /// ```
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }

    /// Returns the margin (collateral in satoshis) allocated to the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let margin = trade.margin();
    ///
    /// println!("Trade margin: {}", margin);
    /// # Ok(())
    /// # }
    /// ```
    pub fn margin(&self) -> Margin {
        self.margin
    }

    /// Returns the leverage multiplier applied to the trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let leverage = trade.leverage();
    ///
    /// println!("Trade leverage: {}", leverage);
    /// # Ok(())
    /// # }
    /// ```
    pub fn leverage(&self) -> Leverage {
        self.leverage
    }

    /// Returns the trade price.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let price = trade.price();
    ///
    /// println!("Trade price: {}", price);
    /// # Ok(())
    /// # }
    /// ```
    pub fn price(&self) -> Price {
        self.price
    }

    /// Returns the liquidation price at which the position will be automatically closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let liq_price = trade.liquidation();
    ///
    /// println!("Liquidation price: {}", liq_price);
    /// # Ok(())
    /// # }
    /// ```
    pub fn liquidation(&self) -> Price {
        self.liquidation
    }

    /// Returns the stop loss price, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(sl) = trade.stoploss() {
    ///     println!("Stop loss: {}", sl);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn stoploss(&self) -> Option<Price> {
        self.stoploss
    }

    /// Returns the take profit price, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(tp) = trade.takeprofit() {
    ///     println!("Take profit: {}", tp);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn takeprofit(&self) -> Option<Price> {
        self.takeprofit
    }

    /// Returns the price at which the trade was closed, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(exit) = trade.exit_price() {
    ///     println!("Exit price: {}", exit);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn exit_price(&self) -> Option<Price> {
        self.exit_price
    }

    /// Returns the realized profit/loss in satoshis.
    ///
    /// For running trades, this represents the current unrealized P/L. For closed trades, this is
    /// the final realized P/L.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let pl = trade.pl();
    ///
    /// if pl > 0 {
    ///     println!("Profit: {} sats", pl);
    /// } else {
    ///     println!("Loss: {} sats", pl.abs());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn pl(&self) -> i64 {
        self.pl
    }

    /// Returns the timestamp when the trade was created.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let created_at = trade.created_at();
    ///
    /// println!("Trade created at: {}", created_at);
    /// # Ok(())
    /// # }
    /// ```
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Returns the timestamp when the trade was filled, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(filled_at) = trade.filled_at() {
    ///     println!("Trade filled at: {}", filled_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn filled_at(&self) -> Option<DateTime<Utc>> {
        self.filled_at
    }

    /// Returns the timestamp when the trade was closed, if applicable.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(closed_at) = trade.closed_at() {
    ///     println!("Trade closed at: {}", closed_at);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed_at(&self) -> Option<DateTime<Utc>> {
        self.closed_at
    }

    /// Returns the actual entry price when the trade was filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(entry) = trade.entry_price() {
    ///     println!("Entry price: {}", entry);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn entry_price(&self) -> Option<Price> {
        self.entry_price
    }

    /// Returns the actual margin at entry, which may differ from the requested margin.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(entry_margin) = trade.entry_margin() {
    ///     println!("Entry margin: {}", entry_margin);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn entry_margin(&self) -> Option<Margin> {
        self.entry_margin
    }

    /// Returns `true` if the trade is open (limit order not yet filled).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.open() {
    ///     println!("Trade is open (limit order not filled)");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn open(&self) -> bool {
        self.open
    }

    /// Returns `true` if the trade is currently running (filled and active).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.running() {
    ///     println!("Trade is actively running");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn running(&self) -> bool {
        self.running
    }

    /// Returns `true` if the trade was canceled before being filled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.canceled() {
    ///     println!("Trade was canceled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn canceled(&self) -> bool {
        self.canceled
    }

    /// Returns `true` if the trade has been closed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if trade.closed() {
    ///     println!("Trade has been closed");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// Returns the sum of all funding fees paid on this trade in satoshis.
    ///
    /// Funding fees are periodic payments charged on open positions.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// let total_fees = trade.sum_funding_fees();
    ///
    /// println!("Total funding fees paid: {} sats", total_fees);
    /// # Ok(())
    /// # }
    /// ```
    pub fn sum_funding_fees(&self) -> i64 {
        self.sum_funding_fees
    }

    /// Returns the client-provided identifier for this trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(trade: lnm_sdk::api_v3::models::Trade) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(client_id) = trade.client_id() {
    ///     println!("Client ID: {}", client_id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn client_id(&self) -> Option<&String> {
        self.client_id.as_ref()
    }
}
