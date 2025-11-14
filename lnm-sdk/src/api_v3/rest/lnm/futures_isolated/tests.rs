use std::{env, time::Instant};

use dotenv::dotenv;

use crate::shared::{config::RestClientConfig, models::quantity::Quantity};

use super::*;

fn init_repository_from_env() -> LnmFuturesIsolatedRepository {
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

    LnmFuturesIsolatedRepository::new(base)
}

async fn test_create_short_trade_quantity_limit(
    repo: &LnmFuturesIsolatedRepository,
    // ticker: &Ticker,
) -> Trade {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(1).unwrap();
    // let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    // let out_of_mkt_price = ticker
    //     .ask_price()
    //     .apply_discount(discount_percentage)
    //     .unwrap();
    let out_of_mkt_price = Price::try_from(90_000).unwrap(); // TEMPORARILY HARD CODED
    let execution = out_of_mkt_price.into();
    let stoploss = None;
    let takeprofit = None;
    let client_id = None;

    let created_trade = repo
        .new_trade(
            side,
            quantity.into(),
            leverage,
            execution,
            stoploss,
            takeprofit,
            client_id,
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

    assert!(created_trade.filled_at().is_none());
    assert!(created_trade.closed_at().is_none());
    assert!(created_trade.exit_price().is_none());

    created_trade
}

async fn test_get_trades_open(repo: &LnmFuturesIsolatedRepository, exp_open_trades: Vec<&Trade>) {
    let open_trades = repo.get_open_trades().await.expect("must get trades");

    assert_eq!(open_trades.len(), exp_open_trades.len());

    for open_trade in &open_trades {
        let ok = exp_open_trades
            .iter()
            .any(|exp| exp.id() == open_trade.id());
        assert!(ok, "open trade {} was not returned", open_trade.id());
    }
}

#[tokio::test]
async fn test_api() {
    let repo = init_repository_from_env();

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
        "cancel_all_trades (cleanup)",
        repo.cancel_all_trades().await.expect("must cancel trades")
    );

    // time_test!(
    //     "close_all_trades (cleanup)",
    //     repo.close_all_trades().await.expect("must close trades")
    // );

    // Start tests

    // let ticker = time_test!("test_ticker", test_ticker(&repo).await);

    let short_limit_trade_a = time_test!(
        "test_create_short_trade_quantity_limit",
        test_create_short_trade_quantity_limit(&repo).await
    );

    println!("short_limit_trade_a {:?}", short_limit_trade_a);

    // time_test!(
    //     "test_get_trade",
    //     test_get_trade(&repo, &short_limit_trade_a).await
    // );

    // let long_limit_trade_b = time_test!(
    //     "test_create_long_trade_margin_limit",
    //     test_create_long_trade_margin_limit(&repo, &ticker).await
    // );

    time_test!(
        "test_get_trades_open",
        test_get_trades_open(&repo, vec![&short_limit_trade_a]).await
    );

    // time_test!(
    //     "test_update_trade_stoploss",
    //     test_update_trade_stoploss(&repo, short_limit_trade_a.id(), short_limit_trade_a.price())
    //         .await
    // );

    // time_test!(
    //     "test_update_trade_takeprofit",
    //     test_update_trade_takeprofit(&repo, long_limit_trade_b.id(), long_limit_trade_b.price())
    //         .await
    // );

    // time_test!(
    //     "test_cancel_trade",
    //     test_cancel_trade(&repo, short_limit_trade_a.id()).await
    // );

    // time_test!(
    //     "test_cancel_all_trades",
    //     test_cancel_all_trades(&repo, vec![&long_limit_trade_b]).await
    // );

    // let long_market_trade_a = time_test!(
    //     "test_create_long_trade_quantity_market",
    //     test_create_long_trade_quantity_market(&repo, &ticker).await
    // );

    // time_test!(
    //     "test_get_trade",
    //     test_get_trade(&repo, &long_market_trade_a).await
    // );

    // let long_market_trade_a = time_test!(
    //     "test_add_margin",
    //     test_add_margin(&repo, long_market_trade_a).await
    // );

    // let long_market_trade_a = time_test!(
    //     "test_cash_in",
    //     test_cash_in(&repo, long_market_trade_a).await
    // );

    // let short_market_trade_b = time_test!(
    //     "test_create_short_trade_margin_market",
    //     test_create_short_trade_margin_market(&repo, &ticker).await
    // );

    // time_test!(
    //     "test_get_trades_running",
    //     test_get_trades_running(&repo, vec![&long_market_trade_a, &short_market_trade_b]).await
    // );

    // time_test!(
    //     "test_close_trade",
    //     test_close_trade(&repo, long_market_trade_a.id()).await
    // );

    // time_test!(
    //     "test_close_all_trades",
    //     test_close_all_trades(&repo, vec![&short_market_trade_b]).await
    // );

    // time_test!(
    //     "test_get_trades_closed",
    //     test_get_trades_closed(&repo, vec![&long_market_trade_a, &short_market_trade_b]).await
    // );
}
