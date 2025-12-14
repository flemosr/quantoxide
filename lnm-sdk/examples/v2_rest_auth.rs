//! Example demonstrating how to use the API v2 REST authenticated client.

#![allow(deprecated)]

use std::{env, num::NonZeroU64};

use dotenv::dotenv;
use lnm_sdk::api_v2::{
    RestClient, RestClientConfig,
    models::{
        Leverage, Margin, Percentage, PercentageCapped, Price, Quantity, TradeExecution, TradeSide,
        TradeSize,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    let key = env::var("LNM_API_V2_KEY").expect("LNM_API_V2_KEY must be set");
    let secret = env::var("LNM_API_V2_SECRET").expect("LNM_API_V2_SECRET must be set");
    let passphrase = env::var("LNM_API_V2_PASSPHRASE").expect("LNM_API_V2_PASSPHRASE must be set");

    let rest = RestClient::with_credentials(
        RestClientConfig::default(),
        &domain,
        key,
        secret,
        passphrase,
    )?;

    // User endpoints

    // Get user information
    let user = rest.user.get_user().await?;
    println!("Got user information. Username: {}", user.username());

    // Futures endpoints

    // Get all open trades
    let open_trades = rest.futures.get_trades_open(None, None, None).await?;
    println!("Got open trades. Len: {}", open_trades.len());

    // Get all running trades
    let running_trades = rest.futures.get_trades_running(None, None, None).await?;
    println!("Got running trades. Len: {}", running_trades.len());

    // Get closed trades
    let closed_trades = rest.futures.get_trades_closed(None, None, None).await?;
    println!("Got closed trades. Len: {}", closed_trades.len());

    // Create a new market trade
    let new_trade = rest
        .futures
        .create_new_trade(
            TradeSide::Buy,
            TradeSize::from(Quantity::try_from(1)?), // 1 USD
            Leverage::try_from(30)?,                 // 30x leverage
            TradeExecution::Market,
            None, // stoploss
            None, // takeprofit
        )
        .await?;
    println!("Created new trade. Trade ID: {}", new_trade.id());

    // Get the trade by ID
    let trade = rest.futures.get_trade(new_trade.id()).await?;
    println!("Got trade by ID. Trade running: {}", trade.running());

    // Update takeprofit on the trade

    let tp_perc = Percentage::try_from(2)?;
    let new_takeprofit = new_trade.price().apply_gain(tp_perc)?; // 2% takeprofit above current price

    let updated_trade = rest
        .futures
        .update_trade_takeprofit(new_trade.id(), new_takeprofit)
        .await?;
    println!(
        "Updated takeprofit. New TP: {:?}",
        updated_trade.takeprofit()
    );

    // Update stoploss on the trade

    let sl_perc = PercentageCapped::try_from(2)?;
    let new_stoploss = updated_trade.price().apply_discount(sl_perc)?; // 2% stoploss below current price

    let updated_trade = rest
        .futures
        .update_trade_stoploss(updated_trade.id(), new_stoploss)
        .await?;
    println!("Updated stoploss. New SL: {:?}", updated_trade.stoploss());

    // Add margin to the trade
    let updated_trade = rest
        .futures
        .add_margin(updated_trade.id(), NonZeroU64::try_from(10)?) // 10 sats
        .await?;
    println!("Added margin. New leverage: {}", updated_trade.leverage());

    // Cash-in from the trade
    let updated_trade = rest
        .futures
        .cash_in(updated_trade.id(), NonZeroU64::try_from(10)?) // 10 sats
        .await?;
    println!("Cashed in. New leverage: {}", updated_trade.leverage());

    // Close the trade
    let closed_trade = rest.futures.close_trade(updated_trade.id()).await?;
    println!("Closed trade. Trade closed: {}", closed_trade.closed());

    // Create a limit trade and then cancel it
    let cancelable_trade = rest
        .futures
        .create_new_trade(
            TradeSide::Buy,
            TradeSize::from(Margin::try_from(2_000)?), // 2000 satoshis (sats)
            Leverage::try_from(10)?,
            TradeExecution::Limit(Price::try_from(80_000)?),
            None, // stoploss
            None, // takeprofit
        )
        .await?;
    println!(
        "Created cancelable trade. Trade ID: {}",
        cancelable_trade.id()
    );

    // Cancel the specific trade
    let canceled_trade = rest.futures.cancel_trade(cancelable_trade.id()).await?;
    println!(
        "Canceled trade. Trade canceled: {}",
        canceled_trade.canceled()
    );

    // Cancel all open trades (if any remain)
    let all_canceled = rest.futures.cancel_all_trades().await?;
    println!("Canceled all trades. Len: {}", all_canceled.len());

    // Close all running trades (if any remain)
    let all_closed = rest.futures.close_all_trades().await?;
    println!("Closed all trades. Len: {}", all_closed.len());

    Ok(())
}
