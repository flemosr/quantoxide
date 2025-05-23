use std::env;
use std::time::Instant;

use dotenv::dotenv;

use crate::api::rest::models::{BoundedPercentage, LowerBoundedPercentage};

use super::super::super::models::{Margin, Quantity, Trade};
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

async fn test_ticker(repo: &LnmFuturesRepository) -> Ticker {
    let ticker = repo.ticker().await.expect("must get ticker");

    assert!(!ticker.exchanges_weights().is_empty());

    ticker
}

async fn test_create_new_trade_quantity_limit(
    repo: &LnmFuturesRepository,
    ticker: &Ticker,
) -> LnmTrade {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(1).unwrap();
    let discount_percentage = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker
        .ask_price()
        .apply_discount(discount_percentage)
        .unwrap();
    let execution = out_of_mkt_price.into();
    let stoploss = None;
    let takeprofit = None;

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

    created_trade
}

async fn test_create_new_trade_quantity_market(
    repo: &LnmFuturesRepository,
    ticker: &Ticker,
) -> LnmTrade {
    let side = TradeSide::Buy;
    let quantity = Quantity::try_from(1).unwrap();
    let leverage = Leverage::try_from(2).unwrap();
    let est_price = ticker.ask_price();
    let range_percentage = BoundedPercentage::try_from(10).unwrap();
    let stoploss = Some(est_price.apply_discount(range_percentage).unwrap());
    let takeprofit = Some(est_price.apply_gain(range_percentage.into()).unwrap());
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

    created_trade
}

async fn test_create_new_trade_margin_limit(
    repo: &LnmFuturesRepository,
    ticker: &Ticker,
) -> LnmTrade {
    let side = TradeSide::Buy;
    let leverage = Leverage::try_from(1).unwrap();
    let discount = BoundedPercentage::try_from(30).unwrap();
    let out_of_mkt_price = ticker.ask_price().apply_discount(discount).unwrap();
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::try_calculate(implied_qtd, out_of_mkt_price, leverage).unwrap();
    let discount = BoundedPercentage::try_from(5).unwrap();
    let stoploss = Some(out_of_mkt_price.apply_discount(discount).unwrap());
    let takeprofit = None;
    let execution = out_of_mkt_price.into();

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

    created_trade
}

async fn test_create_new_trade_margin_market(
    repo: &LnmFuturesRepository,
    ticker: &Ticker,
) -> LnmTrade {
    let discount = BoundedPercentage::try_from(5).unwrap();
    let est_min_price = ticker.ask_price().apply_discount(discount).unwrap();

    let side = TradeSide::Buy;
    let leverage = Leverage::try_from(1).unwrap();
    let implied_qtd = Quantity::try_from(1).unwrap();
    let margin = Margin::try_calculate(implied_qtd, est_min_price, leverage).unwrap();
    let est_price = ticker.ask_price();
    let range = BoundedPercentage::try_from(10).unwrap();
    let stoploss = Some(est_price.apply_discount(range).unwrap());
    let takeprofit = Some(est_price.apply_gain(range.into()).unwrap());
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

    created_trade
}

async fn test_get_trade(repo: &LnmFuturesRepository, exp_trade: &LnmTrade) {
    let trade = repo
        .get_trade(exp_trade.id())
        .await
        .expect("must get trade");

    assert_eq!(trade.id(), exp_trade.id());
    assert_eq!(trade.uid(), exp_trade.uid());
    assert_eq!(trade.trade_type(), exp_trade.trade_type());
    assert_eq!(trade.side(), exp_trade.side());
    assert_eq!(trade.opening_fee(), exp_trade.opening_fee());
    assert_eq!(trade.closing_fee(), exp_trade.closing_fee());
    assert_eq!(trade.maintenance_margin(), exp_trade.maintenance_margin());
    assert_eq!(trade.quantity(), exp_trade.quantity());
    assert_eq!(trade.margin(), exp_trade.margin());
    assert_eq!(trade.leverage(), exp_trade.leverage());
    assert_eq!(trade.price(), exp_trade.price());
    assert_eq!(trade.liquidation(), exp_trade.liquidation());
    assert_eq!(trade.stoploss(), exp_trade.stoploss());
    assert_eq!(trade.takeprofit(), exp_trade.takeprofit());
    assert_eq!(trade.exit_price(), exp_trade.exit_price());
    assert_eq!(trade.creation_ts(), exp_trade.creation_ts());
    assert_eq!(trade.market_filled_ts(), exp_trade.market_filled_ts());
    assert_eq!(trade.closed_ts(), exp_trade.closed_ts());
    assert_eq!(trade.entry_price(), exp_trade.entry_price());
    assert_eq!(trade.entry_margin(), exp_trade.entry_margin());
    assert_eq!(trade.open(), exp_trade.open());
    assert_eq!(trade.running(), exp_trade.running());
    assert_eq!(trade.canceled(), exp_trade.canceled());
    assert_eq!(trade.closed(), exp_trade.closed());

    if trade.open() {
        // If the trade was never executed
        assert_eq!(trade.pl(), exp_trade.pl());
        assert_eq!(trade.sum_carry_fees(), exp_trade.sum_carry_fees());
    }
}

async fn test_get_trades_open(repo: &LnmFuturesRepository, exp_open_trades: Vec<&LnmTrade>) {
    let open_trades = repo
        .get_trades(TradeStatus::Open, None, None, None)
        .await
        .expect("must get trades");

    assert_eq!(open_trades.len(), exp_open_trades.len());

    for open_trade in &open_trades {
        let ok = exp_open_trades
            .iter()
            .any(|exp| exp.id() == open_trade.id());
        assert!(ok, "open trade {} was not returned", open_trade.id());
    }
}

async fn test_get_trades_running(repo: &LnmFuturesRepository, exp_running_trades: Vec<&LnmTrade>) {
    let running_trades = repo
        .get_trades(TradeStatus::Running, None, None, None)
        .await
        .expect("must get trades");

    assert_eq!(running_trades.len(), exp_running_trades.len());

    for running_trade in &running_trades {
        let ok = exp_running_trades
            .iter()
            .any(|exp| exp.id() == running_trade.id());
        assert!(ok, "running trade {} was not returned", running_trade.id());
    }
}

async fn test_get_trades_closed(repo: &LnmFuturesRepository, exp_closed_trades: Vec<&LnmTrade>) {
    let closed_trades = repo
        .get_trades(
            TradeStatus::Closed,
            None,
            None,
            Some(exp_closed_trades.len()),
        )
        .await
        .expect("must get trades");

    assert_eq!(closed_trades.len(), exp_closed_trades.len());

    for closed_trade in &closed_trades {
        let ok = exp_closed_trades
            .iter()
            .any(|exp| exp.id() == closed_trade.id());
        assert!(ok, "closed trade {} was not returned", closed_trade.id());
    }
}

async fn test_cancel_trade(repo: &LnmFuturesRepository, id: Uuid) {
    let canceled_trade = repo.cancel_trade(id).await.expect("must cancel trade");

    assert_eq!(canceled_trade.id(), id);
    assert!(!canceled_trade.open());
    assert!(!canceled_trade.running());
    assert!(!canceled_trade.closed());
    assert!(canceled_trade.canceled());
}

async fn test_close_trade(repo: &LnmFuturesRepository, id: Uuid) {
    let closed_trade = repo.close_trade(id).await.expect("must close trade");

    assert_eq!(closed_trade.id(), id);
    assert!(!closed_trade.open());
    assert!(!closed_trade.running());
    assert!(closed_trade.closed());
    assert!(!closed_trade.canceled());
}

async fn test_cancel_all_trades(repo: &LnmFuturesRepository, exp_open_trades: Vec<&LnmTrade>) {
    let cancelled_trades = repo.cancel_all_trades().await.expect("must cancel trades");

    assert_eq!(cancelled_trades.len(), exp_open_trades.len());

    for open_trade in &exp_open_trades {
        let cancelled = cancelled_trades
            .iter()
            .any(|cancelled| cancelled.id() == open_trade.id());
        assert!(cancelled, "trade {} was not cancelled", open_trade.id());
    }
}

async fn test_close_all_trades(repo: &LnmFuturesRepository, exp_running_trades: Vec<&LnmTrade>) {
    let closed_trades = repo.close_all_trades().await.expect("must close trades");

    assert_eq!(closed_trades.len(), exp_running_trades.len());

    for running_trade in &exp_running_trades {
        let closed = closed_trades
            .iter()
            .any(|closed| closed.id() == running_trade.id());
        assert!(closed, "trade {} was not closed", running_trade.id());
    }
}

async fn test_update_trade_stoploss(repo: &LnmFuturesRepository, id: Uuid, price: Price) {
    let discount = BoundedPercentage::try_from(5).unwrap();
    let stoploss = price.apply_discount(discount).unwrap();
    let updated_trade = repo
        .update_trade_stoploss(id, stoploss)
        .await
        .expect("must update trade");

    assert_eq!(updated_trade.id(), id);
    assert_eq!(updated_trade.stoploss(), Some(stoploss));
}

async fn test_update_trade_takeprofit(repo: &LnmFuturesRepository, id: Uuid, price: Price) {
    let gain = LowerBoundedPercentage::try_from(5).unwrap();
    let takeprofit = price.apply_gain(gain).unwrap();
    let updated_trade = repo
        .update_trade_takeprofit(id, takeprofit)
        .await
        .expect("must update trade");

    assert_eq!(updated_trade.id(), id);
    assert_eq!(updated_trade.takeprofit(), Some(takeprofit));
}

async fn test_add_margin(repo: &LnmFuturesRepository, trade: LnmTrade) -> LnmTrade {
    assert!(trade.leverage().into_f64() > 1.6);

    let target_leverage = Leverage::try_from(1.5).unwrap();
    let target_margin =
        Margin::try_calculate(trade.quantity(), trade.price(), target_leverage).unwrap();
    let amount = target_margin.into_u64() - trade.margin().into_u64();
    let amount = amount.try_into().unwrap();

    let updated_trade = repo
        .add_margin(trade.id(), amount)
        .await
        .expect("must add margin");

    assert_eq!(updated_trade.id(), trade.id());
    assert_eq!(updated_trade.margin(), target_margin);

    updated_trade
}

async fn test_cash_in(repo: &LnmFuturesRepository, trade: LnmTrade) -> LnmTrade {
    assert!(trade.leverage().into_f64() < 1.9);

    let target_leverage = Leverage::try_from(2).unwrap();
    let target_margin =
        Margin::try_calculate(trade.quantity(), trade.price(), target_leverage).unwrap();
    let amount = trade.margin().into_u64() - target_margin.into_u64() + trade.pl().max(0) as u64;
    let amount = amount.try_into().unwrap();

    let updated_trade = repo
        .cash_in(trade.id(), amount)
        .await
        .expect("must cash-in");

    assert_eq!(updated_trade.id(), trade.id());
    assert!(updated_trade.leverage() > trade.leverage());

    updated_trade
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

    time_test!(
        "close_all_trades (cleanup)",
        repo.close_all_trades().await.expect("must close trades")
    );

    // Start tests

    let ticker = time_test!("test_ticker", test_ticker(&repo).await);

    let limit_trade_a = time_test!(
        "test_create_new_trade_quantity_limit",
        test_create_new_trade_quantity_limit(&repo, &ticker).await
    );

    time_test!(
        "test_get_trade",
        test_get_trade(&repo, &limit_trade_a).await
    );

    let limit_trade_b = time_test!(
        "test_create_new_trade_margin_limit",
        test_create_new_trade_margin_limit(&repo, &ticker).await
    );

    time_test!(
        "test_get_trades_open",
        test_get_trades_open(&repo, vec![&limit_trade_a, &limit_trade_b]).await
    );

    time_test!(
        "test_update_trade_stoploss",
        test_update_trade_stoploss(&repo, limit_trade_a.id(), limit_trade_a.price()).await
    );

    time_test!(
        "test_update_trade_takeprofit",
        test_update_trade_takeprofit(&repo, limit_trade_a.id(), limit_trade_a.price()).await
    );

    time_test!(
        "test_cancel_trade",
        test_cancel_trade(&repo, limit_trade_a.id()).await
    );

    time_test!(
        "test_cancel_all_trades",
        test_cancel_all_trades(&repo, vec![&limit_trade_b]).await
    );

    let market_trade_a = time_test!(
        "test_create_new_trade_quantity_market",
        test_create_new_trade_quantity_market(&repo, &ticker).await
    );

    time_test!(
        "test_get_trade",
        test_get_trade(&repo, &market_trade_a).await
    );

    let market_trade_a = time_test!(
        "test_add_margin",
        test_add_margin(&repo, market_trade_a).await
    );

    let market_trade_a = time_test!("test_cash_in", test_cash_in(&repo, market_trade_a).await);

    let market_trade_b = time_test!(
        "test_create_new_trade_margin_market",
        test_create_new_trade_margin_market(&repo, &ticker).await
    );

    time_test!(
        "test_get_trades_running",
        test_get_trades_running(&repo, vec![&market_trade_a, &market_trade_b]).await
    );

    time_test!(
        "test_close_trade",
        test_close_trade(&repo, market_trade_a.id()).await
    );

    time_test!(
        "test_close_all_trades",
        test_close_all_trades(&repo, vec![&market_trade_b]).await
    );

    time_test!(
        "test_get_trades_closed",
        test_get_trades_closed(&repo, vec![&market_trade_a, &market_trade_b]).await
    );
}
