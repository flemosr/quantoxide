pub mod error;
mod lnm;
mod manager;
pub mod models;
mod repositories;

use error::Result;
use lnm::LnmWebSocketRepo;
use manager::ConnectionState;
use repositories::WebSocketRepository;

pub type WebSocketApiContext = Box<dyn WebSocketRepository>;

pub async fn new(api_domain: String) -> Result<WebSocketApiContext> {
    let lnm_websocket_repo = LnmWebSocketRepo::new(api_domain).await?;
    let ws_api_context = Box::new(lnm_websocket_repo);
    Ok(ws_api_context)
}
