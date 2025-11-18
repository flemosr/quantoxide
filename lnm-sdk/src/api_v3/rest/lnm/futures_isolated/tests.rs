use std::{env, time::Instant};

use dotenv::dotenv;

use crate::{
    api_v3::{
        FuturesDataRepository,
        models::{BoundedPercentage, LowerBoundedPercentage, Margin},
        rest::{lnm::futures_data::LnmFuturesDataRepository, models::ticker::Ticker},
    },
    shared::{config::RestClientConfig, models::quantity::Quantity},
};

use super::*;

fn init_repositories_from_env() -> (LnmFuturesIsolatedRepository, LnmFuturesDataRepository) {
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
        LnmFuturesIsolatedRepository::new(base.clone()),
        LnmFuturesDataRepository::new(base),
    )
}

async fn test_create_short_trade_quantity_limit(
    repo: &LnmFuturesIsolatedRepository,
    ticker: &Ticker,
) -> Trade {
    let side = TradeSide::Sell;
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(1).unwrap();
    let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker
        .last_price()
        .apply_discount(discount_percentage)
        .unwrap();
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

    assert_eq!(created_trade.opening_fee(), 0);
    assert_eq!(created_trade.closing_fee(), 0);

    assert!(created_trade.filled_at().is_none());
    assert!(created_trade.closed_at().is_none());
    assert!(created_trade.exit_price().is_none());

    created_trade
}

async fn test_create_long_trade_margin_limit(
    repo: &LnmFuturesIsolatedRepository,
    ticker: &Ticker,
) -> Trade {
    let side = TradeSide::Buy;

    let leverage = Leverage::try_from(1).unwrap();
    let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker
        .last_price()
        .apply_discount(discount_percentage)
        .unwrap();
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::calculate(implied_qtd, out_of_mkt_price, leverage);
    let discount = BoundedPercentage::try_from(5).unwrap();
    let stoploss = Some(out_of_mkt_price.apply_discount(discount).unwrap());
    let takeprofit = None;
    let execution = out_of_mkt_price.into();
    let client_id = None;

    let created_trade = repo
        .new_trade(
            side,
            margin.into(),
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
    assert_eq!(created_trade.margin(), margin);
    assert_eq!(created_trade.leverage(), leverage);
    assert_eq!(created_trade.price(), out_of_mkt_price);
    assert_eq!(created_trade.stoploss(), stoploss);
    assert_eq!(created_trade.takeprofit(), takeprofit);

    assert!(created_trade.open());
    assert!(!created_trade.running());
    assert!(!created_trade.closed());
    assert!(!created_trade.canceled());

    assert_eq!(created_trade.opening_fee(), 0);
    assert_eq!(created_trade.closing_fee(), 0);

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

async fn test_update_trade_stoploss(repo: &LnmFuturesIsolatedRepository, id: Uuid, price: Price) {
    let gain = LowerBoundedPercentage::try_from(5).unwrap();
    let stoploss = Some(price.apply_gain(gain).unwrap());
    let updated_trade = repo
        .update_stoploss(id, stoploss)
        .await
        .expect("must update trade");

    assert_eq!(updated_trade.id(), id);
    assert_eq!(updated_trade.stoploss(), stoploss);
}

async fn test_update_trade_takeprofit(repo: &LnmFuturesIsolatedRepository, id: Uuid, price: Price) {
    let gain = LowerBoundedPercentage::try_from(5).unwrap();
    let takeprofit = Some(price.apply_gain(gain).unwrap());
    let updated_trade = repo
        .update_takeprofit(id, takeprofit)
        .await
        .expect("must update trade");

    assert_eq!(updated_trade.id(), id);
    assert_eq!(updated_trade.takeprofit(), takeprofit);
}

async fn test_cancel_trade(repo: &LnmFuturesIsolatedRepository, id: Uuid) {
    let canceled_trade = repo.cancel_trade(id).await.expect("must cancel trade");

    assert_eq!(canceled_trade.id(), id);
    assert!(!canceled_trade.open());
    assert!(!canceled_trade.running());
    assert!(!canceled_trade.closed());
    assert!(canceled_trade.canceled());
}

async fn test_cancel_all_trades(repo: &LnmFuturesIsolatedRepository, exp_open_trades: Vec<&Trade>) {
    let cancelled_trades = repo.cancel_all_trades().await.expect("must cancel trades");

    assert_eq!(cancelled_trades.len(), exp_open_trades.len());

    for open_trade in &exp_open_trades {
        let cancelled = cancelled_trades
            .iter()
            .any(|cancelled| cancelled.id() == open_trade.id());
        assert!(cancelled, "trade {} was not cancelled", open_trade.id());
    }
}

async fn test_get_trades_canceled(
    repo: &LnmFuturesIsolatedRepository,
    exp_canceled_trades: Vec<&Trade>,
) {
    let limit = Some(NonZeroU64::try_from(exp_canceled_trades.len() as u64).unwrap());
    let canceled_trades = repo
        .get_canceled_trades(None, None, limit, None)
        .await
        .expect("must get trades");

    assert_eq!(canceled_trades.data().len(), exp_canceled_trades.len());

    for trade in canceled_trades.data() {
        let ok = exp_canceled_trades.iter().any(|exp| exp.id() == trade.id());
        assert!(ok, "canceled trade {} was not returned", trade.id());
    }
}

async fn test_create_long_trade_quantity_market(
    repo: &LnmFuturesIsolatedRepository,
    ticker: &Ticker,
) -> Trade {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(2).unwrap();
    let est_price = ticker.last_price();
    let range_percentage = BoundedPercentage::try_from(10).unwrap();
    let stoploss = Some(est_price.apply_discount(range_percentage).unwrap());
    let takeprofit = Some(est_price.apply_gain(range_percentage.into()).unwrap());
    let execution = TradeExecution::Market;
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
    assert_eq!(created_trade.stoploss(), stoploss);
    assert_eq!(created_trade.takeprofit(), takeprofit);

    assert!(!created_trade.open());
    assert!(created_trade.running());
    assert!(!created_trade.closed());
    assert!(!created_trade.canceled());

    assert!(created_trade.opening_fee() > 0);
    assert_eq!(created_trade.closing_fee(), 0);

    assert!(created_trade.filled_at().is_some());
    assert!(created_trade.closed_at().is_none());
    assert!(created_trade.exit_price().is_none());

    created_trade
}

async fn test_add_margin(repo: &LnmFuturesIsolatedRepository, trade: Trade) -> Trade {
    assert!(trade.leverage().into_f64() > 1.6);

    let target_leverage = Leverage::try_from(1.5).unwrap();
    let target_margin = Margin::calculate(trade.quantity(), trade.price(), target_leverage);
    let amount = target_margin.into_u64() - trade.margin().into_u64();
    let amount = amount.try_into().unwrap();

    let updated_trade = repo
        .add_margin_to_trade(trade.id(), amount)
        .await
        .expect("must add margin");

    assert_eq!(updated_trade.id(), trade.id());
    assert_eq!(updated_trade.margin(), target_margin);
    assert!(updated_trade.leverage() < trade.leverage());

    updated_trade
}

async fn test_cash_in(repo: &LnmFuturesIsolatedRepository, trade: Trade) -> Trade {
    assert!(trade.leverage().into_f64() < 1.9);

    let target_leverage = Leverage::try_from(2).unwrap();
    let target_margin = Margin::calculate(trade.quantity(), trade.price(), target_leverage);
    let amount = trade.margin().into_u64() - target_margin.into_u64() + trade.pl().max(0) as u64;
    let amount = amount.try_into().unwrap();

    let updated_trade = repo
        .cash_in_trade(trade.id(), amount)
        .await
        .expect("must cash-in");

    assert_eq!(updated_trade.id(), trade.id());
    assert!(updated_trade.leverage() > trade.leverage());

    updated_trade
}

async fn test_create_short_trade_margin_market(
    repo: &LnmFuturesIsolatedRepository,
    ticker: &Ticker,
) -> Trade {
    let discount = BoundedPercentage::try_from(5).unwrap();
    let est_min_price = ticker.last_price().apply_discount(discount).unwrap();

    let side = TradeSide::Sell;
    let leverage = Leverage::try_from(1).unwrap();
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::calculate(implied_qtd, est_min_price, leverage);
    let est_price = ticker.last_price();
    let range = BoundedPercentage::try_from(10).unwrap();
    let stoploss = Some(est_price.apply_gain(range.into()).unwrap());
    let takeprofit = Some(est_price.apply_discount(range).unwrap());
    let execution = TradeExecution::Market;
    let client_id = None;

    let created_trade = repo
        .new_trade(
            side,
            margin.into(),
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
    assert_eq!(created_trade.margin(), margin);
    assert_eq!(created_trade.leverage(), leverage);
    assert_eq!(created_trade.stoploss(), stoploss);
    assert_eq!(created_trade.takeprofit(), takeprofit);

    assert!(!created_trade.open());
    assert!(created_trade.running());
    assert!(!created_trade.closed());
    assert!(!created_trade.canceled());

    assert!(created_trade.opening_fee() > 0);
    assert_eq!(created_trade.closing_fee(), 0);

    assert!(created_trade.filled_at().is_some());
    assert!(created_trade.closed_at().is_none());
    assert!(created_trade.exit_price().is_none());

    created_trade
}

async fn test_get_trades_running(
    repo: &LnmFuturesIsolatedRepository,
    exp_running_trades: Vec<&Trade>,
) {
    let running_trades = repo.get_running_trades().await.expect("must get trades");

    assert_eq!(running_trades.len(), exp_running_trades.len());

    for trade in &running_trades {
        let ok = exp_running_trades.iter().any(|exp| exp.id() == trade.id());
        assert!(ok, "running trade {} was not returned", trade.id());
    }
}

async fn test_close_trade(repo: &LnmFuturesIsolatedRepository, id: Uuid) {
    let closed_trade = repo.close_trade(id).await.expect("must close trade");

    assert_eq!(closed_trade.id(), id);
    assert!(!closed_trade.open());
    assert!(!closed_trade.running());
    assert!(closed_trade.closed());
    assert!(!closed_trade.canceled());
}

async fn test_get_trades_closed(
    repo: &LnmFuturesIsolatedRepository,
    exp_closed_trades: Vec<&Trade>,
) {
    let limit = Some(NonZeroU64::try_from(exp_closed_trades.len() as u64).unwrap());
    let closed_trades = repo
        .get_closed_trades(None, None, limit, None)
        .await
        .expect("must get trades");

    assert_eq!(closed_trades.data().len(), exp_closed_trades.len());

    for trade in closed_trades.data() {
        let ok = exp_closed_trades.iter().any(|exp| exp.id() == trade.id());
        assert!(ok, "closed trade {} was not returned", trade.id());
    }
}

async fn test_get_funding_fees(repo: &LnmFuturesIsolatedRepository) {
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
        "cancel_all_trades (cleanup)",
        repo.cancel_all_trades().await.expect("must cancel trades")
    );

    // Start tests

    let ticker: Ticker = repo_data.get_ticker().await.expect("must get ticker");

    let short_limit_trade_a = time_test!(
        "test_create_short_trade_quantity_limit",
        test_create_short_trade_quantity_limit(&repo, &ticker).await
    );

    let long_limit_trade_b = time_test!(
        "test_create_long_trade_margin_limit",
        test_create_long_trade_margin_limit(&repo, &ticker).await
    );

    time_test!(
        "test_get_trades_open",
        test_get_trades_open(&repo, vec![&short_limit_trade_a, &long_limit_trade_b]).await
    );

    time_test!(
        "test_update_trade_stoploss",
        test_update_trade_stoploss(&repo, short_limit_trade_a.id(), short_limit_trade_a.price())
            .await
    );

    time_test!(
        "test_update_trade_takeprofit",
        test_update_trade_takeprofit(&repo, long_limit_trade_b.id(), long_limit_trade_b.price())
            .await
    );

    time_test!(
        "test_cancel_trade",
        test_cancel_trade(&repo, short_limit_trade_a.id()).await
    );

    time_test!(
        "test_cancel_all_trades",
        test_cancel_all_trades(&repo, vec![&long_limit_trade_b]).await
    );

    time_test!(
        "test_get_trades_canceled",
        test_get_trades_canceled(&repo, vec![&short_limit_trade_a, &long_limit_trade_b]).await
    );

    let long_market_trade_a = time_test!(
        "test_create_long_trade_quantity_market",
        test_create_long_trade_quantity_market(&repo, &ticker).await
    );

    let long_market_trade_a = time_test!(
        "test_add_margin",
        test_add_margin(&repo, long_market_trade_a).await
    );

    let long_market_trade_a = time_test!(
        "test_cash_in",
        test_cash_in(&repo, long_market_trade_a).await
    );

    let short_market_trade_b = time_test!(
        "test_create_short_trade_margin_market",
        test_create_short_trade_margin_market(&repo, &ticker).await
    );

    time_test!(
        "test_get_trades_running",
        test_get_trades_running(&repo, vec![&long_market_trade_a, &short_market_trade_b]).await
    );

    time_test!(
        "test_close_trade",
        test_close_trade(&repo, long_market_trade_a.id()).await
    );

    time_test!(
        "test_close_trade",
        test_close_trade(&repo, short_market_trade_b.id()).await
    );

    time_test!(
        "test_get_trades_closed",
        test_get_trades_closed(&repo, vec![&long_market_trade_a, &short_market_trade_b]).await
    );

    time_test!("test_get_funding_fees", test_get_funding_fees(&repo).await);
}
