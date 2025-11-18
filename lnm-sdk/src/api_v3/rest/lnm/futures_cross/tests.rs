use std::{env, time::Instant};

use dotenv::dotenv;

use crate::{
    api_v3::{
        FuturesDataRepository,
        models::{BoundedPercentage, CrossLeverage, CrossOrder, TradeExecutionType},
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

async fn test_get_position(repo: &LnmFuturesCrossRepository) -> CrossPosition {
    let cross_position: CrossPosition = repo.get_position().await.expect("must get position");

    assert_eq!(cross_position.quantity(), 0);

    cross_position
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

async fn test_set_leverage(repo: &LnmFuturesCrossRepository, leverage: CrossLeverage) {
    let cross_position: CrossPosition = repo
        .set_leverage(leverage)
        .await
        .expect("must set leverage");

    assert_eq!(cross_position.leverage(), leverage);
}

async fn test_get_open_orders(repo: &LnmFuturesCrossRepository, exp_open_orders: Vec<&CrossOrder>) {
    let open_orders: Vec<CrossOrder> = repo.get_open_orders().await.expect("must get open orders");

    assert_eq!(open_orders.len(), exp_open_orders.len());

    for order in &open_orders {
        let ok = exp_open_orders.iter().any(|exp| exp.id() == order.id());
        assert!(ok, "open order {} was not returned", order.id());
    }
}

async fn test_get_filled_orders(
    repo: &LnmFuturesCrossRepository,
    exp_filled_orders: Vec<&CrossOrder>,
) {
    let limit = NonZeroU64::try_from(exp_filled_orders.len() as u64).unwrap();
    let filled_orders: CrossOrderPage = repo
        .get_filled_orders(None, None, Some(limit), None)
        .await
        .expect("must get open orders");

    assert_eq!(filled_orders.data().len(), exp_filled_orders.len());

    for order in filled_orders.data() {
        let ok = exp_filled_orders.iter().any(|exp| exp.id() == order.id());
        assert!(ok, "filled order {} was not returned", order.id());
    }
}

async fn test_deposit(
    repo: &LnmFuturesCrossRepository,
    cross_position: CrossPosition,
    deposit_amount: u64,
) -> CrossPosition {
    let updated_cross_position: CrossPosition = repo
        .deposit(NonZeroU64::try_from(deposit_amount).unwrap())
        .await
        .expect("must make deposit");

    assert_eq!(
        updated_cross_position.margin(),
        cross_position.margin() + deposit_amount
    );

    updated_cross_position
}

async fn test_withdrawal(
    repo: &LnmFuturesCrossRepository,
    cross_position: CrossPosition,
    withdrawal_amount: u64,
) -> CrossPosition {
    let updated_cross_position: CrossPosition = repo
        .withdraw(NonZeroU64::try_from(withdrawal_amount).unwrap())
        .await
        .expect("must make deposit");

    assert_eq!(
        updated_cross_position.margin(),
        cross_position.margin() - withdrawal_amount
    );

    updated_cross_position
}

async fn test_get_transfers(
    repo: &LnmFuturesCrossRepository,
    deposit_amount: u64,
    withdrawal_amount: u64,
) {
    let limit = NonZeroU64::try_from(2).unwrap();
    let transfers: PaginatedCrossTransfers = repo
        .get_transfers(None, None, Some(limit), None)
        .await
        .expect("must get transfers");

    let withdrawal = transfers.data().first().expect("must have withdrawal");
    let deposit = transfers.data().last().expect("must have deposit");

    assert_eq!(withdrawal.amount(), withdrawal_amount as i64 * -1);
    assert_eq!(deposit.amount(), deposit_amount as i64);
}

async fn test_get_funding_fees(repo: &LnmFuturesCrossRepository) {
    let _ = repo
        .get_funding_fees(None, None, None, None)
        .await
        .expect("must get funding fees");
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

    let cross_position: CrossPosition = time_test!(
        "get_position (cleanup)",
        repo.get_position().await.expect("must get position")
    );

    if cross_position.quantity() > 0 {
        time_test!(
            "close_position (cleanup)",
            repo.close_position().await.expect("must close position")
        );
    }

    time_test!(
        "set_leverage (cleanup)",
        repo.set_leverage(CrossLeverage::try_from(1).unwrap())
            .await
            .expect("must set leverage")
    );

    // Start tests

    time_test!("test_get_position", test_get_position(&repo).await);

    let ticker: Ticker = repo_data.get_ticker().await.expect("must get ticker");

    let long_order_limit = time_test!(
        "test_create_long_order_limit",
        test_create_long_order_limit(&repo, &ticker).await
    );

    time_test!(
        "test_cancel_order",
        test_cancel_order(&repo, long_order_limit.id()).await
    );

    let short_order_limit = time_test!(
        "test_create_short_order_limit",
        test_create_short_order_limit(&repo, &ticker).await
    );

    time_test!(
        "test_get_open_orders",
        test_get_open_orders(&repo, vec![&short_order_limit]).await
    );

    time_test!(
        "test_cancel_all_orders",
        test_cancel_all_orders(&repo, vec![&short_order_limit]).await
    );

    let long_order_market = time_test!(
        "test_create_long_order_market",
        test_create_long_order_market(&repo).await
    );

    time_test!(
        "test_set_leverage",
        test_set_leverage(&repo, CrossLeverage::try_from(2).unwrap()).await
    );

    let short_order_market = time_test!(
        "test_create_short_order_market",
        test_create_short_order_market(&repo).await
    );

    time_test!(
        "test_get_filled_orders",
        test_get_filled_orders(&repo, vec![&long_order_market, &short_order_market]).await
    );

    time_test!("test_close_position", test_close_position(&repo).await);

    let cross_position: CrossPosition = time_test!(
        "get_position",
        repo.get_position().await.expect("must get position")
    );

    let deposit_amount = 100;
    let cross_position = time_test!(
        "test_deposit",
        test_deposit(&repo, cross_position, deposit_amount).await
    );

    let withdrawal_amount = 100;
    time_test!(
        "test_withdrawal",
        test_withdrawal(&repo, cross_position, withdrawal_amount).await
    );

    time_test!(
        "test_get_transfers",
        test_get_transfers(&repo, deposit_amount, withdrawal_amount).await
    );

    time_test!("test_get_funding_fees", test_get_funding_fees(&repo).await);
}
