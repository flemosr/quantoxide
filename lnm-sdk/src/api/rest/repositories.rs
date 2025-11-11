use std::num::NonZeroU64;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{
    error::Result,
    models::{
        leverage::Leverage,
        price::Price,
        price_history::PriceEntryLNM,
        ticker::Ticker,
        trade::{LnmTrade, TradeExecution, TradeSide, TradeSize, TradeStatus},
        user::User,
    },
};

/// Methods for interacting with [LNM's v2 API]'s REST Futures endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesRepository: crate::sealed::Sealed + Send + Sync {
    /// **Requires credentials**. Fetch the user’s trades by [`TradeStatus`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{LnmTrade, TradeStatus};
    /// let open_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades(TradeStatus::Open, None, None, None)
    ///     .await?;
    ///
    /// let running_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades(TradeStatus::Running, None, None, None)
    ///     .await?;
    ///
    /// let closed_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades(TradeStatus::Closed, None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    /// **Requires credentials**. Fetch the user’s open trades.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::LnmTrade;
    /// let open_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades_open(None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_trades_open(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    /// **Requires credentials**. Fetch the user’s running trades.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::LnmTrade;
    /// let running_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades_running(None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_trades_running(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    /// **Requires credentials**. Fetch the user’s closed trades.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::LnmTrade;
    /// let closed_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .get_trades_closed(None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_trades_closed(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<LnmTrade>>;

    /// Retrieve price history between two given timestamps. Limited to 1000 entries.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::PriceEntryLNM;
    /// let price_history: Vec<PriceEntryLNM> = api
    ///     .rest
    ///     .futures
    ///     .price_history(None, None, None)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn price_history(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>>;

    /// **Requires credentials**. Create a new trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Margin, Price, Quantity, TradeExecution,
    /// #     TradeSide, TradeSize
    /// # };
    /// // Create long market order with 10,000 sats of margin and no leverage,
    /// // stoploss or takeprofit.
    /// let trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Margin::try_from(10_000).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Market,
    ///         None,
    ///         None,
    ///     )
    ///     .await?;
    ///
    /// // Create long limit order at the price of 120,000 [USD/BTC] with 10 USD
    /// // of quantity and 2x leverage.
    /// // Stoploss at the price of 110,000 [USD/BTC] and takeprofit at the
    /// // price of 130,000 [USD/BTC].
    /// let trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Quantity::try_from(10).unwrap()),
    ///         Leverage::try_from(2).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(120_000).unwrap()),
    ///         Some(Price::try_from(110_000).unwrap()),
    ///         Some(Price::try_from(130_000).unwrap()),
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Create short limit order at the price of 130,000 [USD/BTC] with 10 USD
    /// // of quantity and 3x leverage.
    /// // Stoploss at the price of 140,000 [USD/BTC] and takeprofit at the
    /// // price of 120,000 [USD/BTC].
    /// let trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Sell,
    ///         TradeSize::from(Quantity::try_from(10).unwrap()),
    ///         Leverage::try_from(3).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(130_000).unwrap()),
    ///         Some(Price::try_from(140_000).unwrap()),
    ///         Some(Price::try_from(120_000).unwrap()),
    ///     )
    ///     .await
    ///     .unwrap();
    /// # Ok(())
    /// # }
    /// ```
    async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<LnmTrade>;

    /// **Requires credentials**. Get a trade by id.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Margin, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let running_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Margin::try_from(10_000).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Market,
    ///         None,
    ///         None,
    ///     )
    ///     .await?;
    ///
    /// let same_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .get_trade(running_trade.id()).await?;
    ///
    /// assert_eq!(running_trade.id(), same_trade.id());
    /// # Ok(())
    /// # }
    /// ```
    async fn get_trade(&self, id: Uuid) -> Result<LnmTrade>;

    /// **Requires credentials**. Cancel an open trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Price, Quantity, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let open_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Quantity::try_from(10).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(10_000).unwrap()),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Assuming trade is still open
    ///
    /// let cancelled_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .cancel_trade(open_trade.id()).await?;
    ///
    /// assert_eq!(cancelled_trade.id(), open_trade.id());
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_trade(&self, id: Uuid) -> Result<LnmTrade>;

    /// **Requires credentials**. Cancel all open trades.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::LnmTrade;
    /// let cancelled_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .cancel_all_trades().await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    async fn cancel_all_trades(&self) -> Result<Vec<LnmTrade>>;

    /// **Requires credentials**. Close a running trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Margin, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let running_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Margin::try_from(10_000).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Market,
    ///         None,
    ///         None,
    ///     )
    ///     .await?;
    ///
    /// let closed_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .close_trade(running_trade.id()).await?;
    ///
    /// assert_eq!(running_trade.id(), closed_trade.id());
    /// # Ok(())
    /// # }
    /// ```
    async fn close_trade(&self, id: Uuid) -> Result<LnmTrade>;

    /// **Requires credentials**. Close all running trades.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::LnmTrade;
    /// let closed_trades: Vec<LnmTrade> = api
    ///     .rest
    ///     .futures
    ///     .close_all_trades().await?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    async fn close_all_trades(&self) -> Result<Vec<LnmTrade>>;

    /// Get the futures ticker.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::Ticker;
    /// let ticker: Ticker = api.rest.futures.ticker().await?;
    /// # Ok(())
    /// # }
    async fn ticker(&self) -> Result<Ticker>;

    /// **Requires credentials**. Modify the stoploss of an open/running trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Price, Quantity, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let open_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Quantity::try_from(10).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(10_000).unwrap()),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Assuming trade is still open or running
    ///
    /// let new_stoploss = Price::try_from(9_000).unwrap();
    /// let updated_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .update_trade_stoploss(open_trade.id(), new_stoploss).await?;
    ///
    /// assert_eq!(updated_trade.stoploss().unwrap(), new_stoploss);
    /// # Ok(())
    /// # }
    /// ```
    async fn update_trade_stoploss(&self, id: Uuid, stoploss: Price) -> Result<LnmTrade>;

    /// **Requires credentials**. Modify the takeprofit of an open/running trade.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Price, Quantity, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let open_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Quantity::try_from(10).unwrap()),
    ///         Leverage::try_from(1).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(10_000).unwrap()),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Assuming trade is still open or running
    ///
    /// let new_takeprofit = Price::try_from(11_000).unwrap();
    /// let updated_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .update_trade_takeprofit(open_trade.id(), new_takeprofit).await?;
    ///
    /// assert_eq!(updated_trade.takeprofit().unwrap(), new_takeprofit);
    /// # Ok(())
    /// # }
    /// ```
    async fn update_trade_takeprofit(&self, id: Uuid, takeprofit: Price) -> Result<LnmTrade>;

    /// **Requires credentials**. Adds margin to an open/running trade, increasing the collateral
    /// and therefore reducing the leverage.
    ///
    /// The resulting [`Leverage`] must be valid (≥ 1) after the update. To target a specific
    /// liquidation price, see [`TradeRunning::est_collateral_delta_for_liquidation`].
    ///
    /// leverage = (quantity * SATS_PER_BTC) / (margin * price)
    ///
    /// Beware of potential rounding issues when evaluating the new leverage.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use std::num::NonZeroU64;
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Margin, Price, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let created_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Margin::try_from(10_000).unwrap()),
    ///         Leverage::try_from(2).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(10_000).unwrap()),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Assuming trade is still open or running
    ///
    /// let amount = NonZeroU64::try_from(1000).unwrap();
    /// let updated_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .add_margin(created_trade.id(), amount).await?;
    ///
    /// assert_eq!(updated_trade.id(), created_trade.id());
    /// assert_eq!(updated_trade.margin().into_u64(), 11_000);
    /// assert!(updated_trade.leverage() < created_trade.leverage());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`TradeRunning::est_collateral_delta_for_liquidation`]: crate::models::TradeRunning::est_collateral_delta_for_liquidation
    async fn add_margin(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade>;

    /// **Requires credentials**. Removes funds from a trade, decreasing the collateral and
    /// possibly increasing the leverage.
    ///
    /// Funds are first removed from the trade's PL, if positive, by adjusting the trade's entry
    /// price. Then, they are removed from the trade's margin.
    /// The resulting [`Leverage`] must be valid (≥ 1 and ≤ 100) after the update. To target a
    /// specific liquidation price, see [`TradeRunning::est_collateral_delta_for_liquidation`].
    ///
    /// leverage = (quantity * SATS_PER_BTC) / (margin * price)
    ///
    /// Beware of potential rounding issues when evaluating the new leverage.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use std::num::NonZeroU64;
    /// # use lnm_sdk::models::{
    /// #     Leverage, LnmTrade, Margin, Price, TradeExecution, TradeSide, TradeSize
    /// # };
    /// let created_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .create_new_trade(
    ///         TradeSide::Buy,
    ///         TradeSize::from(Margin::try_from(10_000).unwrap()),
    ///         Leverage::try_from(2).unwrap(),
    ///         TradeExecution::Limit(Price::try_from(10_000).unwrap()),
    ///         None,
    ///         None,
    ///     )
    ///     .await
    ///     .unwrap();
    ///
    /// // Assuming trade is still open (no PL)
    ///
    /// let amount = NonZeroU64::try_from(1000).unwrap();
    /// let updated_trade: LnmTrade = api
    ///     .rest
    ///     .futures
    ///     .cash_in(created_trade.id(), amount).await?;
    ///
    /// assert_eq!(updated_trade.id(), created_trade.id());
    /// assert_eq!(updated_trade.margin().into_u64(), 9_000);
    /// assert!(updated_trade.leverage() > created_trade.leverage());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`TradeRunning::est_collateral_delta_for_liquidation`]: crate::models::TradeRunning::est_collateral_delta_for_liquidation
    async fn cash_in(&self, id: Uuid, amount: NonZeroU64) -> Result<LnmTrade>;
}

/// Methods for interacting with [LNM's v2 API]'s REST User endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v2 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait UserRepository: crate::sealed::Sealed + Send + Sync {
    /// **Requires credentials**. Get user information.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example(api: lnm_sdk::ApiClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::models::User;
    /// let user: User = api.rest.user.get_user().await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn get_user(&self) -> Result<User>;
}
