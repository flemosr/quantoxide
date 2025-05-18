use std::sync::Arc;

use lnm_sdk::api::{
    ApiContext,
    websocket::models::{ConnectionState, LnmWebSocketChannel, WebSocketApiRes},
};
use tokio::sync::broadcast;

use crate::db::DbContext;

pub mod error;

use error::{RealTimeCollectionError, Result};

pub struct RealTimeCollectionTask {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    shutdown_tx: broadcast::Sender<()>,
}

impl RealTimeCollectionTask {
    pub fn new(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        shutdown_tx: broadcast::Sender<()>,
    ) -> Self {
        Self {
            db,
            api,
            shutdown_tx,
        }
    }

    pub async fn run(self) -> Result<()> {
        let ws = self.api.connect_ws().await?;

        let mut ws_rx = ws.receiver().await?;

        ws.subscribe(vec![LnmWebSocketChannel::FuturesBtcUsdLastPrice])
            .await?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                ws_res = ws_rx.recv() => {
                    match ws_res {
                        Ok(res) => match res {
                            WebSocketApiRes::PriceTick(tick) => {
                                self.db.price_ticks.add_tick(&tick).await?;
                            }
                            WebSocketApiRes::PriceIndex(_index) => {}
                            WebSocketApiRes::ConnectionUpdate(new_state) => match new_state.as_ref() {
                                ConnectionState::Connected => {}
                                ConnectionState::Disconnected | ConnectionState::Failed(_) => {
                                    return Err(RealTimeCollectionError::BadConnectionUpdate(new_state));
                                }
                            },
                        },
                        Err(err) => return Err(RealTimeCollectionError::Generic(err.to_string())),
                    }
                }
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        return Err(RealTimeCollectionError::Generic(format!("shutdown_rx error {e}")));
                    }
                    return Ok(());
                }
            }
        }
    }
}
