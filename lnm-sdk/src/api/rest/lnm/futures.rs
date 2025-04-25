use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::api::rest::models::{Leverage, Price, Ticker, TradeExecution, TradeSize, TradeStatus};

use super::super::{
    error::{RestApiError, Result},
    models::{FuturesTradeRequestBody, PriceEntryLNM, Trade, TradeSide},
    repositories::FuturesRepository,
};
use super::base::LnmApiBase;

const PRICE_HISTORY_PATH: &str = "/v2/futures/history/price";
const FUTURES_TRADE_PATH: &str = "/v2/futures";
const FUTURES_TICKER_PATH: &str = "/v2/futures/ticker";
const FUTURES_CANCEL_TRADE_PATH: &str = "/v2/futures/cancel";

pub struct LnmFuturesRepository {
    base: Arc<LnmApiBase>,
}

impl LnmFuturesRepository {
    pub fn new(base: Arc<LnmApiBase>) -> Self {
        Self { base }
    }
}

#[async_trait]
impl FuturesRepository for LnmFuturesRepository {
    async fn get_trades(
        &self,
        status: TradeStatus,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<Trade>> {
        let mut query_params = Vec::new();

        query_params.push(("type", status.to_string()));

        if let Some(from) = from {
            query_params.push(("from", from.timestamp_millis().to_string()));
        }
        if let Some(to) = to {
            query_params.push(("to", to.timestamp_millis().to_string()));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let trades: Vec<Trade> = self
            .base
            .make_request_with_query_params(Method::GET, FUTURES_TRADE_PATH, query_params, true)
            .await?;

        Ok(trades)
    }

    async fn price_history(
        &self,
        from: Option<&DateTime<Utc>>,
        to: Option<&DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<PriceEntryLNM>> {
        let mut query_params = Vec::new();
        if let Some(from) = from {
            query_params.push(("from", from.timestamp_millis().to_string()));
        }
        if let Some(to) = to {
            query_params.push(("to", to.timestamp_millis().to_string()));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }

        let price_history: Vec<PriceEntryLNM> = self
            .base
            .make_request_with_query_params(Method::GET, PRICE_HISTORY_PATH, query_params, false)
            .await?;

        Ok(price_history)
    }

    async fn create_new_trade(
        &self,
        side: TradeSide,
        size: TradeSize,
        leverage: Leverage,
        execution: TradeExecution,
        stoploss: Option<Price>,
        takeprofit: Option<Price>,
    ) -> Result<Trade> {
        let body =
            FuturesTradeRequestBody::new(leverage, stoploss, takeprofit, side, size, execution)
                .map_err(|e| RestApiError::Generic(e.to_string()))?;

        let created_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, FUTURES_TRADE_PATH, body, true)
            .await?;

        Ok(created_trade)
    }

    async fn cancel_trade(&self, id: Uuid) -> Result<Trade> {
        let body = json!({"id": id.to_string()});

        let canceled_trade: Trade = self
            .base
            .make_request_with_body(Method::POST, FUTURES_CANCEL_TRADE_PATH, body, true)
            .await?;

        Ok(canceled_trade)
    }

    async fn close_trade(&self, id: Uuid) -> Result<Trade> {
        let query_params = vec![("id", id.to_string())];

        let deleted_trade: Trade = self
            .base
            .make_request_with_query_params(Method::DELETE, FUTURES_TRADE_PATH, query_params, true)
            .await?;

        Ok(deleted_trade)
    }

    async fn ticker(&self) -> Result<Ticker> {
        let ticker: Ticker = self
            .base
            .make_request_without_params(Method::GET, FUTURES_TICKER_PATH, true)
            .await?;

        Ok(ticker)
    }
}

#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use std::env;
    use tokio::time::{Duration, sleep};

    use super::super::super::models::{Margin, Quantity};
    use super::*;

    fn init_repository_from_env() -> LnmFuturesRepository {
        dotenv().ok();

        let domain =
            env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
        let key = env::var("LNM_API_KEY").expect("LNM_API_KEY environment variable must be set");
        let secret =
            env::var("LNM_API_SECRET").expect("LNM_API_SECRET environment variable must be set");
        let passphrase = env::var("LNM_API_PASSPHRASE")
            .expect("LNM_API_PASSPHRASE environment variable must be set");

        let base =
            LnmApiBase::new(domain, key, secret, passphrase).expect("Can create `LnmApiBase`");

        LnmFuturesRepository::new(base)
    }

    fn get_btc_out_of_market_price_from_env() -> Price {
        env::var("BTC_OUT_OF_MARKET_PRICE")
            .expect("BTC_OUT_OF_MARKET_PRICE environment variable must be set")
            .parse::<f64>()
            .expect("BTC_OUT_OF_MARKET_PRICE must be a valid number")
            .try_into()
            .expect("BTC_OUT_OF_MARKET_PRICE must be a valid `Price`")
    }

    #[tokio::test]
    async fn test_ticker() {
        let repo = init_repository_from_env();

        let ticker = repo.ticker().await.expect("must get ticker");

        assert!(!ticker.exchanges_weights().is_empty());
    }

    #[tokio::test]
    async fn test_create_new_trade_quantity_limit() {
        let repo = init_repository_from_env();

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1).unwrap();
        let leverage = Leverage::try_from(1).unwrap();
        let price = get_btc_out_of_market_price_from_env();
        let stoploss = Some(price.apply_change(-0.05).unwrap());
        let takeprofit = Some(price.apply_change(0.05).unwrap());
        let execution = price.into();

        let created_trade = repo
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                execution,
                stoploss,
                takeprofit,
            )
            .await
            .expect("must create trade");

        assert_eq!(created_trade.trade_type(), execution.to_type());
        assert_eq!(created_trade.side(), side);
        assert_eq!(created_trade.quantity(), quantity);
        assert_eq!(created_trade.leverage(), leverage);
        assert_eq!(created_trade.price(), price);
        assert_eq!(created_trade.stoploss(), stoploss);
        assert_eq!(created_trade.takeprofit(), takeprofit);

        assert!(created_trade.open());
        assert!(!created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        assert!(created_trade.market_filled_ts().is_none());
        assert!(created_trade.closed_ts().is_none());
        assert!(created_trade.exit_price().is_none());
    }

    #[tokio::test]
    async fn test_create_new_trade_quantity_market() {
        let repo = init_repository_from_env();

        let side = TradeSide::Buy;
        let quantity = Quantity::try_from(1).unwrap();
        let leverage = Leverage::try_from(1).unwrap();
        let stoploss = None;
        let takeprofit = None;
        let execution = TradeExecution::Market;

        let created_trade = repo
            .create_new_trade(
                side,
                quantity.into(),
                leverage,
                execution,
                stoploss,
                takeprofit,
            )
            .await
            .expect("must create trade");

        assert_eq!(created_trade.trade_type(), execution.to_type());
        assert_eq!(created_trade.side(), side);
        assert_eq!(created_trade.quantity(), quantity);
        assert_eq!(created_trade.leverage(), leverage);
        assert_eq!(created_trade.stoploss(), stoploss);
        assert_eq!(created_trade.takeprofit(), takeprofit);

        assert!(!created_trade.open());
        assert!(created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        assert!(created_trade.market_filled_ts().is_some());
        assert!(created_trade.closed_ts().is_none());
        assert!(created_trade.exit_price().is_none());
    }

    #[tokio::test]
    async fn test_create_new_trade_margin_limit() {
        let repo = init_repository_from_env();

        let side = TradeSide::Buy;
        let leverage = Leverage::try_from(1).unwrap();
        let price = get_btc_out_of_market_price_from_env();
        let implied_quantity = Quantity::try_from(1).unwrap();
        let margin = Margin::try_calculate(implied_quantity, price, leverage).unwrap();
        let stoploss = Some(price.apply_change(-0.05).unwrap());
        let takeprofit = None;
        let execution = price.into();

        let created_trade = repo
            .create_new_trade(
                side,
                margin.into(),
                leverage,
                execution,
                stoploss,
                takeprofit,
            )
            .await
            .expect("must create trade");

        assert_eq!(created_trade.trade_type(), execution.to_type());
        assert_eq!(created_trade.side(), side);
        assert_eq!(created_trade.margin(), margin);
        assert_eq!(created_trade.leverage(), leverage);
        assert_eq!(created_trade.price(), price);
        assert_eq!(created_trade.stoploss(), stoploss);
        assert_eq!(created_trade.takeprofit(), takeprofit);

        assert!(created_trade.open());
        assert!(!created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        assert!(created_trade.market_filled_ts().is_none());
        assert!(created_trade.closed_ts().is_none());
        assert!(created_trade.exit_price().is_none());
    }

    #[tokio::test]
    async fn test_create_new_trade_margin_market() {
        let repo = init_repository_from_env();

        let est_min_price = {
            let ticker = repo.ticker().await.expect("must get ticker");
            ticker.ask_price().apply_change(-0.05).unwrap()
        };

        let side = TradeSide::Buy;
        let leverage = Leverage::try_from(1).unwrap();
        let implied_quantity = Quantity::try_from(1).unwrap();
        let margin = Margin::try_calculate(implied_quantity, est_min_price, leverage).unwrap();
        let stoploss = None;
        let takeprofit = None;
        let execution = TradeExecution::Market;

        let created_trade = repo
            .create_new_trade(
                side,
                margin.into(),
                leverage,
                execution,
                stoploss,
                takeprofit,
            )
            .await
            .expect("must create trade");

        assert_eq!(created_trade.trade_type(), execution.to_type());
        assert_eq!(created_trade.side(), side);
        assert_eq!(created_trade.margin(), margin);
        assert_eq!(created_trade.leverage(), leverage);
        assert_eq!(created_trade.stoploss(), stoploss);
        assert_eq!(created_trade.takeprofit(), takeprofit);

        assert!(!created_trade.open());
        assert!(created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        assert!(created_trade.market_filled_ts().is_some());
        assert!(created_trade.closed_ts().is_none());
        assert!(created_trade.exit_price().is_none());
    }

    #[tokio::test]
    async fn test_get_trades() {
        let repo = init_repository_from_env();

        let to = Utc::now();
        let from = to - chrono::Duration::days(1);
        let limit = Some(1);

        let _ = repo
            .get_trades(TradeStatus::Open, Some(&from), Some(&to), limit)
            .await
            .expect("must get trades");
    }

    #[tokio::test]
    async fn test_cancel_trade() {
        let repo = init_repository_from_env();

        let price = get_btc_out_of_market_price_from_env();
        let stoploss = Some(price.apply_change(-0.05).unwrap());
        let takeprofit = Some(price.apply_change(0.05).unwrap());
        let execution = price.into();

        let created_trade = repo
            .create_new_trade(
                TradeSide::Buy,
                Quantity::try_from(1).unwrap().into(),
                Leverage::try_from(1).unwrap(),
                execution,
                stoploss,
                takeprofit,
            )
            .await
            .expect("must create trade");

        assert!(created_trade.open());
        assert!(!created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        sleep(Duration::from_secs(1)).await;

        let canceled_trade = repo
            .cancel_trade(created_trade.id())
            .await
            .expect("must cancel trade");

        assert_eq!(canceled_trade.id(), created_trade.id());
        assert!(!canceled_trade.open());
        assert!(!canceled_trade.running());
        assert!(!canceled_trade.closed());
        assert!(canceled_trade.canceled());
    }

    #[tokio::test]
    async fn test_close_trade() {
        let repo = init_repository_from_env();

        let created_trade = repo
            .create_new_trade(
                TradeSide::Buy,
                Quantity::try_from(1).unwrap().into(),
                Leverage::try_from(1).unwrap(),
                TradeExecution::Market,
                None,
                None,
            )
            .await
            .expect("must create trade");

        assert!(!created_trade.open());
        assert!(created_trade.running());
        assert!(!created_trade.closed());
        assert!(!created_trade.canceled());

        sleep(Duration::from_secs(1)).await;

        let closed_trade = repo
            .close_trade(created_trade.id())
            .await
            .expect("must close trade");

        assert_eq!(closed_trade.id(), created_trade.id());
        assert!(!created_trade.open());
        assert!(!created_trade.running());
        assert!(created_trade.closed());
        assert!(!created_trade.canceled());
    }
}
