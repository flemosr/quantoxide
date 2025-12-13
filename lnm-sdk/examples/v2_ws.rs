//! Basic example demonstrating how to create and use an API v2 WebSocket client.
//!
//! ## Prerequisites
//!
//! Set the following environment variables:
//! - `LNM_API_DOMAIN` - The LN Markets API domain
//!
//! ## Usage
//!
//! ```bash
//! cargo run --example v2_ws
//! ```

use std::{env, time::Duration};

use dotenv::dotenv;
use lnm_sdk::api_v2::{WebSocketChannel, WebSocketClient, WebSocketClientConfig, WebSocketUpdate};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let domain = env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");

    let client = WebSocketClient::new(WebSocketClientConfig::default(), domain);
    let ws = client.connect().await?;

    println!("Connected to WebSocket successfully.");

    let mut ws_rx = ws.receiver().await?;

    ws.subscribe(vec![
        WebSocketChannel::FuturesBtcUsdIndex,
        WebSocketChannel::FuturesBtcUsdLastPrice,
    ])
    .await?;

    println!("Subscribed to channels.");

    let timeout = sleep(Duration::from_secs(10));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            res = ws_rx.recv() => {
                match res {
                    Ok(ws_update) => match ws_update {
                        WebSocketUpdate::ConnectionStatus(status) => println!("{status}"),
                        WebSocketUpdate::PriceTick(price_tick) => println!("{price_tick}"),
                        WebSocketUpdate::PriceIndex(price_index) => println!("{price_index}"),
                    }
                    Err(e) => {
                        println!("error: {:?}", e);
                        break;
                    }
                }
            }
            _ = &mut timeout => {
                println!("10 seconds elapsed, stop receiving");
                break;
            }
        }
    }

    ws.disconnect().await?;

    println!("Disconnected to WebSocket successfully.");

    Ok(())
}
