//! Basic example demonstrating how to create and use an API v3 REST public client.
//!
//! ## Prerequisites
//!
//! Set the following environment variables:
//! - `LNM_API_DOMAIN` - The LN Markets API domain
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example v3_rest_public
//! ```

use std::env;

use dotenv::dotenv;
use lnm_sdk::api_v3::{RestClient, RestClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    let rest = RestClient::new(RestClientConfig::default(), &domain)?;

    // Utilities endpoints

    // Ping
    rest.utilities.ping().await?;
    println!("Pinged server successfully");

    // Get the server time
    let server_time = rest.utilities.time().await?;
    println!("Got server time: {}", server_time.time());

    // Futures Data endpoints

    // Get funding settlement history
    let funding_settlements = rest
        .futures_data
        .get_funding_settlements(None, None, None, None)
        .await?;
    println!(
        "Got funding settlements. Len: {}",
        funding_settlements.data().len()
    );

    // Get the futures ticker
    let ticker = rest.futures_data.get_ticker().await?;
    println!("Got futures ticker. Index: {}", ticker.index());

    // Get candles (OHLCs) history
    let candles = rest
        .futures_data
        .get_candles(None, None, None, None, None)
        .await?;
    println!("Got candles. Len: {}", candles.data().len());

    // Oracle endpoints

    // Get index history
    let index = rest.oracle.get_index(None, None, None, None).await?;
    println!("Got index history. Len: {}", index.len());

    // Get last price history
    let last_price = rest.oracle.get_last_price(None, None, None, None).await?;
    println!("Got last price history. Len: {}", last_price.len());

    Ok(())
}
