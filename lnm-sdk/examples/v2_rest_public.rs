//! Basic example demonstrating how to create and use an API v2 REST public client.
//!
//! ## Prerequisites
//!
//! Set the following environment variables:
//! - `LNM_API_DOMAIN` - The LN Markets API domain
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example v2_rest_public
//! ```

#![allow(deprecated)]

use std::env;

use dotenv::dotenv;
use lnm_sdk::api_v2::{RestClient, RestClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    let rest = RestClient::new(RestClientConfig::default(), &domain)?;

    // Futures endpoints

    // Get the futures ticker
    let ticker = rest.futures.ticker().await?;
    println!(
        "Got the futures ticker. Last price: {}",
        ticker.last_price()
    );

    // Retrieve price history between two given timestamps
    let price_history = rest.futures.price_history(None, None, None).await?;
    println!("Got the price history. Len: {}", price_history.len());

    Ok(())
}
