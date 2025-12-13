//! Basic example demonstrating how to create and use an API v3 REST authenticated client.
//!
//! ## Prerequisites
//!
//! Set the following environment variables:
//! - `LNM_API_DOMAIN` - The LN Markets API domain
//! - `LNM_API_V3_KEY` - Your API v3 key
//! - `LNM_API_V3_SECRET` - Your API v3 secret
//! - `LNM_API_V3_PASSPHRASE` - Your API v3 passphrase
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example v3_rest_auth
//! ```

use std::{env, num::NonZeroU64};

use dotenv::dotenv;
use lnm_sdk::api_v3::{
    RestClient, RestClientConfig,
    models::{
        CrossLeverage, Leverage, Margin, Percentage, PercentageCapped, Price, Quantity,
        TradeExecution, TradeSide, TradeSize,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY must be set");
    let secret = env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE").expect("LNM_API_V3_PASSPHRASE must be set");

    let rest = RestClient::with_credentials(
        RestClientConfig::default(),
        &domain,
        key,
        secret,
        passphrase,
    )?;

    // Account endpoints

    // Get account information
    let account = rest.account.get_account().await?;
    println!(
        "Got account information. Account username: {}",
        account.username()
    );

    // Futures Isolated endpoints

    // Get all open trades
    let open_trades = rest.futures_isolated.get_open_trades().await?;
    println!("Got open trades. Len: {}", open_trades.len());

    // Get all running trades
    let running_trades = rest.futures_isolated.get_running_trades().await?;
    println!("Got running trades. Len: {}", running_trades.len());

    // Get closed trades
    let closed_trades = rest
        .futures_isolated
        .get_closed_trades(None, None, None, None)
        .await?;
    println!("Got closed trades. Len: {}", closed_trades.data().len());

    // Get canceled trades
    let canceled_trades = rest
        .futures_isolated
        .get_canceled_trades(None, None, None, None)
        .await?;
    println!("Got canceled trades. Len: {}", canceled_trades.data().len());

    // Get funding fees for isolated trades
    let funding_fees = rest
        .futures_isolated
        .get_funding_fees(None, None, None, None)
        .await?;
    println!("Got funding fees. Len: {}", funding_fees.data().len());

    // Place a new isolated trade
    let new_trade = rest
        .futures_isolated
        .new_trade(
            TradeSide::Buy,
            TradeSize::from(Quantity::try_from(1)?), // 1 USD
            Leverage::try_from(30)?,                 // 30x leverage
            TradeExecution::Market,
            None, // stoploss
            None, // takeprofit
            None, // client trade id
        )
        .await?;
    println!("Created new trade. Trade ID: {}", new_trade.id());

    // Update takeprofit on the trade

    let tp_perc = Percentage::try_from(2)?;
    let new_takeprofit = new_trade.price().apply_gain(tp_perc)?; // 2% takeprofit above current price

    let updated_trade = rest
        .futures_isolated
        .update_takeprofit(new_trade.id(), Some(new_takeprofit))
        .await?;
    println!(
        "Updated takeprofit. New TP: {:?}",
        updated_trade.takeprofit()
    );

    // Update stoploss on the trade

    let sl_perc = PercentageCapped::try_from(2)?;
    let new_stoploss = updated_trade.price().apply_discount(sl_perc)?; // 2% stoploss below current price

    let updated_trade = rest
        .futures_isolated
        .update_stoploss(updated_trade.id(), Some(new_stoploss))
        .await?;
    println!("Updated stoploss. New SL: {:?}", updated_trade.stoploss());

    // Add margin to the trade
    let updated_trade = rest
        .futures_isolated
        .add_margin_to_trade(updated_trade.id(), NonZeroU64::try_from(10)?) // 10 sats
        .await?;
    println!("Added margin. New leverage: {}", updated_trade.leverage());

    // Cash-in from the trade
    let updated_trade = rest
        .futures_isolated
        .cash_in_trade(updated_trade.id(), NonZeroU64::try_from(10)?) // 10 sats
        .await?;
    println!("Cashed in. New leverage: {}", updated_trade.leverage());

    // Remove takeprofit from the trade
    let updated_trade = rest
        .futures_isolated
        .update_takeprofit(updated_trade.id(), None)
        .await?;
    println!(
        "Removed takeprofit. New TP: {:?}",
        updated_trade.takeprofit()
    );

    // Remove stoploss from the trade
    let updated_trade = rest
        .futures_isolated
        .update_stoploss(updated_trade.id(), None)
        .await?;
    println!("Removed stoploss. New SL: {:?}", updated_trade.stoploss());

    // Close the trade
    let closed_trade = rest
        .futures_isolated
        .close_trade(updated_trade.id())
        .await?;
    println!("Closed trade. Trade ID: {}", closed_trade.id());

    // Place a trade and then cancel it
    let cancelable_trade = rest
        .futures_isolated
        .new_trade(
            TradeSide::Buy,
            TradeSize::from(Margin::try_from(2_000)?), // 2000 satoshis (sats)
            Leverage::try_from(10)?,
            TradeExecution::Limit(Price::try_from(80_000)?),
            None, // stoploss
            None, // takeprofit
            None, // client trade id
        )
        .await?;
    println!(
        "Created cancelable trade. Trade ID: {}",
        cancelable_trade.id()
    );

    // Cancel the specific trade
    let _canceled_trade = rest
        .futures_isolated
        .cancel_trade(cancelable_trade.id())
        .await?;
    println!(
        "Canceled trade. Trade canceled: {}",
        cancelable_trade.canceled()
    );

    // Cancel all open trades (if any remain)
    let all_canceled = rest.futures_isolated.cancel_all_trades().await?;
    println!("Canceled all trades. Len: {}", all_canceled.len());

    // Futures Cross endpoints

    // Get all open cross orders
    let open_orders = rest.futures_cross.get_open_orders().await?;
    println!("Got open cross orders. Len: {}", open_orders.len());

    // Get filled cross orders
    let filled_orders = rest
        .futures_cross
        .get_filled_orders(None, None, None, None)
        .await?;
    println!(
        "Got filled cross orders. Len: {}",
        filled_orders.data().len()
    );

    // Get funding fees for cross position
    let funding_fees = rest
        .futures_cross
        .get_funding_fees(None, None, None, None)
        .await?;
    println!("Got cross funding fees. Len: {}", funding_fees.data().len());

    // Get transfer history for cross account
    let transfers = rest
        .futures_cross
        .get_transfers(None, None, None, None)
        .await?;
    println!("Got cross transfers. Len: {}", transfers.data().len());

    // Deposit funds to cross margin account
    let position = rest
        .futures_cross
        .deposit(NonZeroU64::try_from(1_000)?) // 1_000 sats
        .await?;
    println!(
        "Deposited to cross account. New margin: {}",
        position.margin()
    );

    // Set leverage for cross position
    let position = rest
        .futures_cross
        .set_leverage(CrossLeverage::try_from(30)?) // 30x leverage
        .await?;
    println!("Set cross leverage. New leverage: {}", position.leverage());

    // Place a new cross order
    let new_order = rest
        .futures_cross
        .place_order(
            TradeSide::Buy,
            Quantity::try_from(1)?, // 1 USD
            TradeExecution::Market,
            None, // client order id
        )
        .await?;
    println!("Placed new cross order. Order ID: {}", new_order.id());

    // Close the cross position
    let close_order = rest.futures_cross.close_position().await?;
    println!("Closed cross position. Order ID: {}", close_order.id());

    // Place a limit cross order and then cancel it
    let cancelable_order = rest
        .futures_cross
        .place_order(
            TradeSide::Sell,
            Quantity::try_from(1)?, // 1 USD
            TradeExecution::Limit(Price::try_from(80_000)?),
            None, // client order id
        )
        .await?;
    println!(
        "Placed cancelable cross order. Order ID: {}",
        cancelable_order.id()
    );

    // Cancel the specific order
    let canceled_order = rest
        .futures_cross
        .cancel_order(cancelable_order.id())
        .await?;
    println!(
        "Canceled cross order. Order canceled: {}",
        canceled_order.canceled()
    );

    // Cancel all open cross orders (if any remain)
    let all_canceled = rest.futures_cross.cancel_all_orders().await?;
    println!("Canceled all cross orders. Len: {}", all_canceled.len());

    // Get the current cross margin position
    let position = rest.futures_cross.get_position().await?;
    println!("Got cross position. Margin: {}", position.margin());

    // Withdraw funds from cross margin account
    let position = rest
        .futures_cross
        .withdraw(NonZeroU64::try_from(position.margin())?)
        .await?;
    println!(
        "Withdrew from cross account. New margin: {}",
        position.margin()
    );

    Ok(())
}
