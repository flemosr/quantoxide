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

use std::env;

use dotenv::dotenv;
use lnm_sdk::api_v2::{WebSocketChannel, WebSocketClient, WebSocketClientConfig, WebSocketUpdate};

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

    let max_messages = 10;
    let mut count = 0;

    while let Ok(ws_update) = ws_rx.recv().await {
        match ws_update {
            WebSocketUpdate::ConnectionStatus(status) => {
                println!("{status}");
            }
            WebSocketUpdate::PriceTick(price_tick) => {
                println!("{price_tick}");
                count += 1;
            }
            WebSocketUpdate::PriceIndex(price_index) => {
                println!("{price_index}");
                count += 1;
            }
        }

        if count >= max_messages {
            println!("Received {max_messages} messages, disconnecting...");
            break;
        }
    }

    ws.disconnect().await?;
    println!("Disconnected successfully.");

    Ok(())
}
