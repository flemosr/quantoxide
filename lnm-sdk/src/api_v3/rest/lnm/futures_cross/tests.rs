use std::{env, time::Instant};

use dotenv::dotenv;

use crate::{
    api_v3::{
        FuturesDataRepository,
        models::{BoundedPercentage, CrossOrder, TradeExecutionType},
        rest::{lnm::futures_data::LnmFuturesDataRepository, models::ticker::Ticker},
    },
    shared::{config::RestClientConfig, models::quantity::Quantity},
};

use super::*;

fn init_repositories_from_env() -> (LnmFuturesCrossRepository, LnmFuturesDataRepository) {
    dotenv().ok();

    let domain =
        env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY environment variable must be set");
    let secret =
        env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET environment variable must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE")
        .expect("LNM_API_V3_PASSPHRASE environment variable must be set");

    let base = LnmRestBase::with_credentials(
        RestClientConfig::default(),
        domain,
        key,
        passphrase,
        SignatureGeneratorV3::new(secret),
    )
    .expect("Can create `LnmApiBase`");

    (
        LnmFuturesCrossRepository::new(base.clone()),
        LnmFuturesDataRepository::new(base),
    )
}

async fn test_create_long_order_limit(
    repo: &LnmFuturesCrossRepository,
    ticker: &Ticker,
) -> CrossOrder {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker
        .last_price()
        .apply_discount(discount_percentage)
        .unwrap();
    let execution = out_of_mkt_price.into();
    let client_id = None;

    let placed_order: CrossOrder = repo
        .place_order(side, quantity.into(), execution, client_id.clone())
        .await
        .expect("must place order");

    assert_eq!(placed_order.trade_type(), execution.to_type());
    assert_eq!(placed_order.side(), side);
    assert_eq!(placed_order.quantity(), quantity);
    assert_eq!(placed_order.price(), out_of_mkt_price);
    assert!(placed_order.open());
    assert!(!placed_order.filled());
    assert!(!placed_order.canceled());
    assert_eq!(placed_order.trading_fee(), 0);
    assert!(placed_order.filled_at().is_none());
    assert!(placed_order.canceled_at().is_none());
    assert!(placed_order.client_id().is_none());

    placed_order
}

async fn test_create_short_order_limit(
    repo: &LnmFuturesCrossRepository,
    ticker: &Ticker,
) -> CrossOrder {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1).unwrap();
    let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker
        .last_price()
        .apply_discount(discount_percentage)
        .unwrap();
    let execution = out_of_mkt_price.into();
    let client_id = None;

    let placed_order: CrossOrder = repo
        .place_order(side, quantity.into(), execution, client_id)
        .await
        .expect("must place order");

    assert_eq!(placed_order.trade_type(), execution.to_type());
    assert_eq!(placed_order.side(), side);
    assert_eq!(placed_order.quantity(), quantity);
    assert_eq!(placed_order.price(), out_of_mkt_price);
    assert!(placed_order.open());
    assert!(!placed_order.filled());
    assert!(!placed_order.canceled());
    assert_eq!(placed_order.trading_fee(), 0);
    assert!(placed_order.filled_at().is_none());
    assert!(placed_order.canceled_at().is_none());
    assert!(placed_order.client_id().is_none());

    placed_order
}

async fn test_create_long_order_market(repo: &LnmFuturesCrossRepository) -> CrossOrder {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(2).unwrap();
    let execution = TradeExecution::Market;
    let client_id = None;

    let placed_order: CrossOrder = repo
        .place_order(side, quantity.into(), execution, client_id)
        .await
        .expect("must place order");

    assert_eq!(placed_order.trade_type(), execution.to_type());
    assert_eq!(placed_order.side(), side);
    assert_eq!(placed_order.quantity(), quantity);
    assert!(!placed_order.open());
    assert!(placed_order.filled());
    assert!(!placed_order.canceled());
    assert!(placed_order.trading_fee() > 0);
    assert!(placed_order.filled_at().is_some());
    assert!(placed_order.canceled_at().is_none());
    assert!(placed_order.client_id().is_none());

    placed_order
}

async fn test_create_short_order_market(repo: &LnmFuturesCrossRepository) -> CrossOrder {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1).unwrap();
    let execution = TradeExecution::Market;
    let client_id = None;

    let placed_order: CrossOrder = repo
        .place_order(side, quantity.into(), execution, client_id)
        .await
        .expect("must place order");

    assert_eq!(placed_order.trade_type(), execution.to_type());
    assert_eq!(placed_order.side(), side);
    assert_eq!(placed_order.quantity(), quantity);
    assert!(!placed_order.open());
    assert!(placed_order.filled());
    assert!(!placed_order.canceled());
    assert!(placed_order.trading_fee() > 0);
    assert!(placed_order.filled_at().is_some());
    assert!(placed_order.canceled_at().is_none());
    assert!(placed_order.client_id().is_none());

    placed_order
}

async fn test_cancel_order(repo: &LnmFuturesCrossRepository, id: Uuid) {
    let canceled_order = repo.cancel_order(id).await.expect("must cancel order");

    assert_eq!(canceled_order.id(), id);
    assert!(!canceled_order.open());
    assert!(!canceled_order.filled());
    assert!(canceled_order.canceled());
}

async fn test_cancel_all_orders(
    repo: &LnmFuturesCrossRepository,
    exp_open_orders: Vec<&CrossOrder>,
) {
    let cancelled_orders = repo.cancel_all_orders().await.expect("must cancel orders");

    assert_eq!(cancelled_orders.len(), exp_open_orders.len());

    for open_order in &exp_open_orders {
        let cancelled = cancelled_orders
            .iter()
            .any(|cancelled| cancelled.id() == open_order.id());
        assert!(cancelled, "order {} was not cancelled", open_order.id());
    }
}

async fn test_close_position(repo: &LnmFuturesCrossRepository) {
    let short_order_market: CrossOrder = repo.close_position().await.expect("must close position");

    assert_eq!(short_order_market.trade_type(), TradeExecutionType::Market);
    assert_eq!(short_order_market.side(), TradeSide::Sell);
    assert_eq!(short_order_market.quantity(), Quantity::MIN);
    assert!(short_order_market.trading_fee() > 0);
    assert!(!short_order_market.open());
    assert!(short_order_market.filled());
    assert!(short_order_market.filled_at().is_some());
    assert!(!short_order_market.canceled());
    assert!(short_order_market.canceled_at().is_none());
    assert!(short_order_market.client_id().is_none());
}

#[tokio::test]
async fn test_api() {
    let (repo, repo_data) = init_repositories_from_env();

    macro_rules! time_test {
        ($test_name: expr, $test_block: expr) => {{
            println!("Starting test: {}", $test_name);
            let start = Instant::now();
            let result = $test_block;
            let elapsed = start.elapsed();
            println!("Test '{}' took: {:?}", $test_name, elapsed);
            result
        }};
    }

    // Initial clean-up

    time_test!(
        "cancel_all_orders (cleanup)",
        repo.cancel_all_orders().await.expect("must cancel orders")
    );

    // Start tests

    let ticker: Ticker = repo_data.get_ticker().await.expect("must get ticker");

    let long_order_limit = time_test!(
        "test_create_long_order_limit",
        test_create_long_order_limit(&repo, &ticker).await
    );

    println!("long_order_limit {:?}", long_order_limit);

    let long_order_limit = time_test!(
        "test_cancel_order",
        test_cancel_order(&repo, long_order_limit.id()).await
    );

    println!("long_order_limit {:?}", long_order_limit);

    let short_order_limit = time_test!(
        "test_create_short_order_limit",
        test_create_short_order_limit(&repo, &ticker).await
    );

    println!("short_limit_trade_a {:?}", short_order_limit);

    time_test!(
        "test_cancel_all_orders",
        test_cancel_all_orders(&repo, vec![&short_order_limit]).await
    );

    let long_order_market = time_test!(
        "test_create_long_order_market",
        test_create_long_order_market(&repo).await
    );

    println!("long_order_market {:?}", long_order_market);

    let short_order_market = time_test!(
        "test_create_short_order_market",
        test_create_short_order_market(&repo).await
    );

    println!("short_order_market {:?}", short_order_market);

    time_test!("test_close_position", test_close_position(&repo).await);
}
