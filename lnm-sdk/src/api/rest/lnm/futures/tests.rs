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

    let base = LnmApiBase::new(domain, key, secret, passphrase).expect("Can create `LnmApiBase`");

    LnmFuturesRepository::new(base)
}

async fn get_ask_price(repo: &LnmFuturesRepository) -> Price {
    let ticker = repo.ticker().await.expect("must get ticker");
    ticker.ask_price()
}

async fn get_out_of_market_price(repo: &LnmFuturesRepository) -> Price {
    let ask = get_ask_price(repo).await;
    ask.apply_change(-0.3).unwrap()
}

#[tokio::test]
async fn test_cancel_all_trades() {
    let repo = init_repository_from_env();

    let trades = repo.cancel_all_trades().await.expect("must cancel trades");

    assert!(trades.is_empty());
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
    let out_of_mkt_price = get_out_of_market_price(&repo).await;
    let stoploss = Some(out_of_mkt_price.apply_change(-0.05).unwrap());
    let takeprofit = Some(out_of_mkt_price.apply_change(0.05).unwrap());
    let execution = out_of_mkt_price.into();

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
    assert_eq!(created_trade.price(), out_of_mkt_price);
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
    let out_of_mkt_price = get_out_of_market_price(&repo).await;
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::try_calculate(implied_qtd, out_of_mkt_price, leverage).unwrap();
    let stoploss = Some(out_of_mkt_price.apply_change(-0.05).unwrap());
    let takeprofit = None;
    let execution = out_of_mkt_price.into();

    println!(
        "out_of_mkt_price {:?} margin {:?}",
        out_of_mkt_price, margin
    );

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
    assert_eq!(created_trade.price(), out_of_mkt_price);
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

    let est_min_price = get_ask_price(&repo).await.apply_change(-0.1).unwrap();

    let side = TradeSide::Buy;
    let leverage = Leverage::try_from(1).unwrap();
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::try_calculate(implied_qtd, est_min_price, leverage).unwrap();
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

    let price = get_out_of_market_price(&repo).await;
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
    assert!(!closed_trade.open());
    assert!(!closed_trade.running());
    assert!(closed_trade.closed());
    assert!(!closed_trade.canceled());
}
